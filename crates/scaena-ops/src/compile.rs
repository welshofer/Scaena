//! Compile `.scn` source into a deck (SPEC §4, PLAN 1.5), checked in the bundle it is for,
//! and say where in the source each finding is about. `scaena compile` prints it; the web
//! editor underlines it (PLAN 2.3).

use scaena_core::dsl::{self, DslError, SourceMap};
use scaena_core::validate::BundleFiles;
use scaena_core::{Deck, Finding, Severity};
use serde_json::Value;
use std::collections::BTreeMap;

/// A source compiled: the deck it says, where each part of it came from, and what
/// `validate` finds in it.
#[derive(Debug, Clone)]
pub struct Compiled {
    /// The deck as the source says it, unchecked.
    pub json: Value,
    pub map: SourceMap,
    /// What `validate` finds in the deck, in its bundle: both schemas, every reference to a
    /// file or a theme name, each state resolved (SPEC §7.1).
    pub findings: Vec<Finding>,
}

/// `source` compiled and validated in the bundle whose files are `files`. A source that
/// does not parse is a [`DslError`], placed in it.
pub fn compile(source: &str, files: &dyn BundleFiles) -> Result<Compiled, DslError> {
    compile_renamed(source, files, &BTreeMap::new())
}

/// `source` compiled and validated as [`compile`] does, each file it names by a name a save
/// gave up (`renamed`: that name, and the one the file has now) named as it is now: a source
/// from before the save, which an undo makes the deck again, opens in the bundle the save
/// wrote (PLAN 1.4).
pub fn compile_renamed(
    source: &str,
    files: &dyn BundleFiles,
    renamed: &BTreeMap<String, String>,
) -> Result<Compiled, DslError> {
    let (mut json, map) = dsl::compile_json(source)?;
    rename_files(&mut json, renamed);
    let text = serde_json::to_string(&json).expect("a compiled deck is JSON");
    let findings = scaena_core::validate::validate_bundle(&text, files).expect("a compiled deck parses");
    Ok(Compiled { json, map, findings })
}

/// Each file `json`, a deck or a theme, names by a name `renamed` maps, named by the name it
/// maps to, wherever a save names a file anew: a font's `file`, an image's `src`, and a beat's
/// `evidence`.
pub fn rename_files(json: &mut Value, renamed: &BTreeMap<String, String>) {
    fn walk(value: &mut Value, key: Option<&str>, renamed: &BTreeMap<String, String>) {
        match value {
            Value::String(name) if matches!(key, Some("file" | "src" | "evidence")) => {
                if let Some(now) = renamed.get(name.as_str()) {
                    now.clone_into(name);
                }
            }
            Value::Array(items) => items.iter_mut().for_each(|item| walk(item, key, renamed)),
            Value::Object(fields) => fields.iter_mut().for_each(|(key, field)| walk(field, Some(key), renamed)),
            _ => {}
        }
    }
    if !renamed.is_empty() {
        walk(json, None, renamed);
    }
}

impl Compiled {
    /// The deck, once `validate` finds no error in it.
    pub fn deck(&self) -> Option<Deck> {
        if self.findings.iter().any(|f| f.severity == Severity::Error) {
            return None;
        }
        Deck::from_json(&self.json.to_string()).ok()
    }

    /// The source a finding about this deck is about, as a byte offset and length: the part
    /// its pointer names, or the nearest part the source wrote. A finding about a whole node
    /// in a state is about that state's line for it, where the state has one. A finding
    /// about another file (the theme) is about none of the source.
    pub fn span(&self, f: &Finding) -> Option<(usize, usize)> {
        if f.file.is_some() {
            return None;
        }
        let index = f.state.as_deref().and_then(|id| self.state_index(id));
        if let (Some(i), Some(node)) = (index, &f.node) {
            let node = node.replace('~', "~0").replace('/', "~1");
            let whole = f.path.as_deref().is_none_or(|p| p == format!("/nodes/{node}"));
            if let (true, Some(span)) = (whole, self.map.exact(&format!("/states/{i}/props/{node}"))) {
                return Some(span);
            }
        }
        match (&f.path, index) {
            (Some(path), _) => self.map.locate(path),
            (None, Some(i)) => self.map.locate(&format!("/states/{i}")),
            (None, None) => None,
        }
    }

    /// The source each state's declaration starts at, by state id, in the deck's order.
    pub fn states(&self) -> Vec<(String, usize)> {
        let states = self.json.get("states").and_then(Value::as_array).map_or(&[][..], Vec::as_slice);
        states
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let id = s.get("id")?.as_str()?.to_string();
                Some((id, self.map.exact(&format!("/states/{i}"))?.0))
            })
            .collect()
    }

    fn state_index(&self, id: &str) -> Option<usize> {
        let states = self.json.get("states")?.as_array()?;
        states.iter().position(|s| s.get("id").and_then(Value::as_str) == Some(id))
    }
}

/// 1-based line and column (in characters) of a byte offset into `source`.
pub fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..source.floor_char_boundary(offset)];
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (before.matches('\n').count() + 1, col)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoFiles;

    impl BundleFiles for NoFiles {
        fn exists(&self, _: &str) -> bool {
            false
        }
        fn read_text(&self, _: &str) -> Option<String> {
            None
        }
    }

    const SOURCE: &str = "deck \"T\" canvas:1920x1080\n\nstate a\n  title text \"One\"\n\nstate b\n  title \"Two\"\n";

    #[test]
    fn a_finding_about_a_node_in_a_state_is_about_that_states_line() {
        let c = compile(SOURCE, &NoFiles).unwrap();
        let line = |f: &Finding| line_col(SOURCE, c.span(f).unwrap().0).0;
        let about = |state: &str| Finding::new("E100", Severity::Error, "").state(state).node("title");
        assert_eq!(line(&about("a").at("/nodes/title")), 4);
        assert_eq!(line(&about("b").at("/nodes/title")), 7);
        // A pointer into the node's own props stays where the node says them.
        assert_eq!(line(&about("b").at("/nodes/title/text")), 4);
        // Each state's declaration starts at its id.
        let at = |id: &str| SOURCE.find(&format!("state {id}")).unwrap() + "state ".len();
        assert_eq!(c.states(), vec![("a".to_string(), at("a")), ("b".to_string(), at("b"))]);
    }

    #[test]
    fn line_and_column_count_characters() {
        assert_eq!(line_col("ab\ncé d", 7), (2, 4));
        assert_eq!(line_col("ab", 99), (1, 3));
    }
}

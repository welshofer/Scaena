//! New bundles, and data for them (SPEC §3.1, §3.10): what an agent starts from. A bundle is
//! made from a theme, its fonts, a deck, and data files. It is checked as `validate` checks
//! a bundle before anything is written, and written only if valid.

use crate::lint::{View, errors, lint, lint_in, write_deck};
use crate::{Bundle, Context, OpsError};
use indexmap::IndexMap;
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::validate_bundle;
use scaena_core::{Deck, Finding};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A data file to attach: copied into the bundle's `data/`, declared as a source.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Attach {
    /// The source's id, which charts and tables name as `@id`.
    pub id: String,
    /// The CSV or JSON file (an array of objects) to copy in.
    pub file: PathBuf,
    /// Each column's type: `number`, `string`, `date`, or `boolean`. Without it, each is
    /// typed as narrowly as all its values allow, a date in ISO 8601.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<IndexMap<String, String>>,
    /// A date column's format, for dates not in ISO 8601 (`docs/spec/format.md`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse: Option<IndexMap<String, String>>,
}

/// A data file attached.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Attached {
    /// Whether it was written: not when it would make the deck invalid (in `added`).
    pub attached: bool,
    pub id: String,
    /// Its path in the bundle.
    pub source: String,
    /// Each column's type, as the deck declares it.
    pub schema: IndexMap<String, String>,
    pub rows: usize,
    /// What `validate` and `lint` find after it that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not after.
    pub removed: Vec<Finding>,
    /// The findings that are errors, after.
    pub errors: usize,
}

/// Attach the data file `req.file` to the bundle as source `req.id`: copy it into `data/`
/// and declare it, typed. Written only if the deck it makes validates no worse.
pub fn attach(b: &Bundle, req: &Attach) -> Result<Attached, OpsError> {
    let Source { path: source, bytes, decl: value, rows } = source(req)?;
    if b.deck.data.contains_key(&req.id) {
        return Err(OpsError::new(format!(
            "the deck has a data source `{}` already; `bind_data` with a `source` replaces one (SPEC §7.3)",
            req.id
        )));
    }
    if b.files.read(&source).is_ok_and(|there| there != bytes) {
        return Err(OpsError::new(format!("the bundle has another `{source}`; name the file differently")));
    }
    let schema = schema_of(&value);
    let mut doc = serde_json::to_value(&b.deck)?;
    match doc.get_mut("data").and_then(Value::as_object_mut) {
        Some(data) => drop(data.insert(req.id.clone(), value)),
        None => doc["data"] = json!({ req.id.clone(): value }),
    }
    let next = Deck::from_json(&doc.to_string()).context("the deck with its data")?;
    let view = View::of(b).with(source.clone(), bytes.clone());
    // Checked as `validate` checks a bundle: a validation finding it adds refuses it.
    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let invalid = validate_bundle(&b.deck.to_json()?, &b.files)?;
    let invalid_after = validate_bundle(&next.to_json()?, &view)?;
    let broken = delta(&invalid, &states, &invalid_after, &states, &[]).added;
    if !broken.is_empty() {
        let added = broken.into_iter().cloned().collect();
        let errors = errors(&invalid_after);
        return Ok(Attached {
            attached: false,
            id: req.id.clone(),
            source,
            schema,
            rows,
            added,
            removed: vec![],
            errors,
        });
    }
    let before = lint(b)?.findings;
    let after = lint_in(&next, &view)?.findings;
    write_deck(b, &next, BTreeMap::from([(source.clone(), bytes)]))?;
    let Delta { added, removed } = delta(&before, &states, &after, &states, &[]);
    Ok(Attached {
        attached: true,
        id: req.id.clone(),
        source,
        schema,
        rows,
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        errors: errors(&after),
    })
}

/// A data file to attach, read and typed.
struct Source {
    /// Its path in the bundle.
    path: String,
    bytes: Vec<u8>,
    /// The data source declaring it, as `deck.json` holds it.
    decl: Value,
    rows: usize,
}

/// The data file `req` names.
fn source(req: &Attach) -> Result<Source, OpsError> {
    if !scaena_core::ids::is_valid_id(&req.id) {
        return Err(OpsError::new(format!(
            "`{}` is not an id: a lowercase letter, then lowercase letters, digits, `-`, and `_`",
            req.id
        )));
    }
    let bytes = std::fs::read(&req.file).with_context(|| format!("reading {}", req.file.display()))?;
    let name = req.file.file_name().and_then(|n| n.to_str()).context("the data file has no name")?;
    let path = format!("data/{name}");
    let inferred = scaena_core::data::infer(&path, &bytes).map_err(|e| OpsError::new(e.to_string()))?;
    let schema: IndexMap<String, String> = match &req.schema {
        Some(schema) => schema.clone(),
        None => inferred.columns.iter().zip(&inferred.types).map(|(c, t)| (c.clone(), t.name().to_string())).collect(),
    };
    let mut value = json!({ "source": path, "schema": schema });
    if let Some(parse) = &req.parse {
        value["parse"] = json!(parse);
    }
    Ok(Source { path, bytes, decl: value, rows: inferred.rows })
}

fn schema_of(source: &Value) -> IndexMap<String, String> {
    source["schema"]
        .as_object()
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string()))
        .collect()
}

/// A new bundle.
#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Create {
    /// The theme file to start from, copied to `themes/`. The fonts its families name are
    /// copied to `fonts/` from beside it, or above it: from the bundle it belongs to.
    pub theme: PathBuf,
    /// The deck, as `deck.json` holds it. Its `theme` and `fonts` are set to the bundle's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<Value>,
    /// The deck as `.scn` source (SPEC §4), instead of `deck`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scn: Option<String>,
    /// The title, for a deck made here: without `deck` or `scn`, one state with nothing
    /// on screen, on a 1920 × 1080 canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Data files to attach.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data: Vec<Attach>,
}

/// A bundle made.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Created {
    /// Whether it was written: it is only if it validates.
    pub created: bool,
    /// Every file in it, by its path in the bundle.
    pub files: Vec<String>,
    /// What `validate` and `lint` find in it.
    pub findings: Vec<Finding>,
    /// The findings that are errors.
    pub errors: usize,
}

/// Make a bundle at `path`, a directory not there yet or empty: the theme and its fonts,
/// the data files, and the deck, pointed at them. Written only if it validates.
pub fn create(path: &Path, req: &Create) -> Result<Created, OpsError> {
    if path.exists() && std::fs::read_dir(path).map_or(true, |mut d| d.next().is_some()) {
        return Err(OpsError::new(format!(
            "`{}` is there and is not an empty directory: a new bundle needs a place of its own",
            path.display()
        )));
    }
    let mut files: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    // The theme, and the fonts its families name.
    let text = std::fs::read_to_string(&req.theme).with_context(|| format!("reading {}", req.theme.display()))?;
    let theme: Value = serde_json::from_str(&text).with_context(|| format!("{} is not JSON", req.theme.display()))?;
    let name = req.theme.file_name().and_then(|n| n.to_str()).context("the theme has no file name")?;
    let rel = format!("themes/{name}");
    let mut fonts = Vec::new();
    for family in theme.pointer("/type/families").and_then(Value::as_object).into_iter().flat_map(|f| f.values()) {
        let (Some(file), Some(name)) = (family["file"].as_str(), family["family"].as_str()) else { continue };
        let found =
            req.theme.ancestors().skip(1).map(|dir| dir.join(file)).find(|p| p.is_file()).with_context(|| {
                format!("the theme's font `{file}` is not beside {} or above it", req.theme.display())
            })?;
        files.insert(file.to_string(), std::fs::read(&found)?);
        let mut font = json!({ "family": name, "file": file });
        if let Some(axes) = family.get("axes") {
            font["axes"] = axes.clone();
        }
        fonts.push(font);
    }
    files.insert(rel.clone(), text.into_bytes());
    // The deck: given, compiled, or made here.
    let mut doc = match (&req.deck, &req.scn) {
        (Some(_), Some(_)) => return Err(OpsError::new("give the deck as `deck` or as `scn`, not both")),
        (Some(deck), None) => deck.clone(),
        (None, Some(scn)) => {
            let compiled = scaena_core::dsl::compile_json(scn);
            compiled.map_err(|e| OpsError::new(format!("scn line {}, col {}: {}", e.line, e.col, e.message)))?.0
        }
        (None, None) => json!({
            "scaena": scaena_core::FORMAT_VERSION,
            "meta": { "title": req.title.as_deref().unwrap_or("Untitled") },
            "canvas": { "width": 1920, "height": 1080 },
            "nodes": {},
            "states": [{ "id": "start" }],
        }),
    };
    let Some(fields) = doc.as_object_mut() else { return Err(OpsError::new("the deck is a JSON object")) };
    fields.insert("theme".into(), json!(rel));
    fields.insert("fonts".into(), Value::Array(fonts));
    for attach in &req.data {
        let Source { path, bytes, decl, .. } = source(attach)?;
        files.insert(path, bytes);
        let data = fields.entry("data").or_insert_with(|| json!({}));
        data[attach.id.as_str()] = decl;
    }
    // Checked before anything is written.
    let base = scaena_store::Files::Dir(path.to_path_buf());
    let view = View { base: &base, pending: files.clone() };
    let deck_json = serde_json::to_string_pretty(&doc)?;
    let invalid = validate_bundle(&deck_json, &view)?;
    let mut listed: Vec<String> = files.keys().cloned().chain(["deck.json".to_string()]).collect();
    listed.sort();
    if invalid.iter().any(|f| f.severity == scaena_core::Severity::Error) {
        return Ok(Created { created: false, files: listed, errors: errors(&invalid), findings: invalid });
    }
    let deck = Deck::from_json(&deck_json).context("the deck")?;
    let findings = lint_in(&deck, &view)?.findings;
    std::fs::create_dir_all(path).with_context(|| format!("making {}", path.display()))?;
    let b =
        Bundle { root: path.to_path_buf(), deck_file: "deck.json".into(), deck, theme_json: None, files: base.clone() };
    write_deck(&b, &b.deck, files)?;
    Ok(Created { created: true, files: listed, errors: errors(&findings), findings })
}

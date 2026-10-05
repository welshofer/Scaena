//! Find and replace across the deck's texts, in every state (PLAN 2.47, ADR-0013).
//!
//! A text node's text is written in one place for each state that shows it: the deck's
//! `overrides`, which hold it in every state; else the latest delta that sets it, from the
//! state back along what it tracks; else the node's own. [`find`] gives each text the deck
//! shows once for each place it is written, with the states that show it from there and
//! where a query matches it. [`replacing`] makes the patch that replaces those matches: a
//! `replace_text` for each, made in the first state that shows the text, so it is written
//! where the text lives, as typing writes it.

use super::esc;
use crate::document::{Deck, Props};
use crate::tracking::{Lives, lives, merge_props, resolve_states};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What is sought in the deck's texts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Query {
    /// The characters sought, as typed.
    pub find: String,
    /// Upper and lower case apart; without it, alike.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub case: bool,
    /// Whole words only: a match that neither begins nor ends inside a word.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub words: bool,
}

/// A text the deck shows, as written in one place, and where a query matches it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Found {
    /// The text node.
    pub node: String,
    /// The first state that shows the text as written there: where `replace_text` is made.
    pub state: String,
    /// Every state that shows it as written there, in the deck's order.
    pub states: Vec<String>,
    /// Where it is written, a JSON pointer into the deck: the node's own `text` or `runs`
    /// (`/nodes/title/text`), a state's delta (`/states/2/props/title/text`), or the deck's
    /// `overrides`, which hold it in every state (`/overrides/title/text`).
    pub lives: String,
    /// The text: its `text`, or its runs' texts end to end.
    pub text: String,
    /// Each match, `[from, to]` in characters (Unicode scalar values), as `replace_text`
    /// counts them, in order; none overlap.
    pub matches: Vec<[u32; 2]>,
}

/// Each text the deck shows that `query` matches, once for each place it is written, in the
/// order the deck first shows them. An empty query matches nothing.
pub fn find(doc: &Value, query: &Query) -> Result<Vec<Found>, String> {
    let deck =
        Deck::from_value(doc).map_err(|e| format!("the deck must parse to be searched, and it does not: {e}"))?;
    let snapshots = resolve_states(&deck).map_err(|e| e.to_string())?;
    let overrides = doc.get("overrides").and_then(Value::as_object);
    let kind =
        |node: &str| doc.get("nodes").and_then(|n| n.get(node)).and_then(|n| n.get("type")).and_then(Value::as_str);
    // Each place a shown text is written, matched or not, so every state that shows it is
    // counted with it.
    let mut places: IndexMap<(String, String), Found> = IndexMap::new();
    for (i, snapshot) in snapshots.iter().enumerate() {
        for (node, props) in &snapshot.nodes {
            if kind(node) != Some("text") {
                continue;
            }
            let over = overrides.and_then(|o| o.get(node)).and_then(Value::as_object);
            let (prop, text) = shown(props, over);
            let lives = match over.is_some_and(|o| o.contains_key(prop)) {
                true => format!("/overrides/{}/{prop}", esc(node)),
                false => match lives(&deck, i, node, prop, &[]) {
                    Lives::State(j) => format!("/states/{j}/props/{}/{prop}", esc(node)),
                    Lives::Node => format!("/nodes/{}/{prop}", esc(node)),
                },
            };
            let state = &snapshot.state_id;
            places.entry((node.clone(), lives.clone())).and_modify(|f| f.states.push(state.clone())).or_insert_with(
                || Found {
                    node: node.clone(),
                    state: state.clone(),
                    states: vec![state.clone()],
                    lives,
                    matches: matches(&text, query),
                    text,
                },
            );
        }
    }
    Ok(places.into_values().filter(|f| !f.matches.is_empty()).collect())
}

/// The patch that replaces every match of `found` with `with`: a `replace_text` for each, in
/// the first state that shows its text, so it is written where the text lives. Within a text,
/// the last match goes first, so that the offsets of those before it still stand.
pub fn replacing(found: &[Found], with: &str) -> Vec<Value> {
    let each = |f: &Found, &[from, to]: &[u32; 2]| json!({ "op": "replace_text", "node": f.node, "from": from, "to": to, "text": with, "state": f.state });
    found.iter().flat_map(|f| f.matches.iter().rev().map(move |m| each(f, m))).collect()
}

/// A text node's text as props show it, under the deck's overrides for it: the property it
/// is (`runs` where it has some, else `text`), and the text, its runs' texts end to end.
fn shown(props: &Props, over: Option<&serde_json::Map<String, Value>>) -> (&'static str, String) {
    let mut props = props.clone();
    if let Some(over) = over {
        merge_props(&mut props, &over.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
    }
    match props.get("runs").and_then(Value::as_array).filter(|runs| !runs.is_empty()) {
        Some(runs) => ("runs", runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect()),
        None => ("text", props.get("text").and_then(Value::as_str).unwrap_or_default().to_string()),
    }
}

/// Where `query` matches `text`, in characters, from the start, none overlapping.
fn matches(text: &str, query: &Query) -> Vec<[u32; 2]> {
    let hay: Vec<char> = text.chars().collect();
    let needle: Vec<char> = query.find.chars().collect();
    let n = needle.len();
    if n == 0 {
        return Vec::new();
    }
    let same = |a: char, b: char| a == b || (!query.case && a.to_lowercase().eq(b.to_lowercase()));
    let word = |at: usize| hay.get(at).is_some_and(|c| c.is_alphanumeric() || *c == '_');
    let mut out = Vec::new();
    let mut i = 0;
    while i + n <= hay.len() {
        let here = (0..n).all(|k| same(hay[i + k], needle[k]));
        let whole = !query.words || ((i == 0 || !word(i - 1)) && !word(i + n));
        if here && whole {
            out.push([i as u32, (i + n) as u32]);
            i += n;
        } else {
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate::BundleFiles;

    fn query(find: &str) -> Query {
        Query { find: find.into(), ..Query::default() }
    }

    /// A bundle with nothing in it but the deck.
    struct Bare;

    impl BundleFiles for Bare {
        fn exists(&self, _: &str) -> bool {
            false
        }

        fn read_text(&self, _: &str) -> Option<String> {
            None
        }
    }

    fn apply(doc: &mut Value, ops: &[Value]) {
        *doc = crate::patch::compile(doc, ops, &Bare).unwrap().doc;
    }

    /// A deck where `title` is written in its own props, shown in `a` and in `c`, which
    /// branches from it, and in `b`'s delta; `note` reads in runs; `badge`'s text is set by
    /// the deck's overrides.
    fn deck() -> Value {
        json!({
            "scaena": crate::FORMAT_VERSION,
            "canvas": { "width": 1920, "height": 1080 },
            "nodes": {
                "title": { "type": "text", "text": "Revenue in Q3", "role": "headline" },
                "note": { "type": "text", "runs": [{ "text": "Q3 was " }, { "text": "strong", "emphasis": "strong" }, { "text": " in q3." }] },
                "badge": { "type": "text", "text": "draft" },
                "box": { "type": "shape", "shape": "rect" }
            },
            "states": [
                { "id": "a", "props": { "title": {}, "box": {} } },
                { "id": "b", "props": { "title": { "text": "Q3, by region" }, "note": {}, "badge": {} } },
                { "id": "c", "from": "a", "props": { "note": {}, "badge": {} } }
            ],
            "overrides": { "badge": { "text": "Q3 draft" } }
        })
    }

    #[test]
    fn each_text_is_found_once_for_each_place_it_is_written() {
        let found = find(&deck(), &query("q3")).unwrap();
        let places: Vec<(&str, &str, Vec<&str>)> = found
            .iter()
            .map(|f| (f.node.as_str(), f.lives.as_str(), f.states.iter().map(String::as_str).collect()))
            .collect();
        assert_eq!(
            places,
            [
                ("title", "/nodes/title/text", vec!["a", "c"]),
                ("title", "/states/1/props/title/text", vec!["b"]),
                ("note", "/nodes/note/runs", vec!["b", "c"]),
                ("badge", "/overrides/badge/text", vec!["b", "c"]),
            ],
            "{found:#?}"
        );
        assert_eq!(found[0].state, "a");
        assert_eq!(found[2].state, "b");
        // A box is no text, and nothing is found in it.
        assert!(found.iter().all(|f| f.node != "box"));
    }

    #[test]
    fn matches_count_characters_and_take_case_and_words_as_asked() {
        let found = find(&deck(), &query("q3")).unwrap();
        // Runs read end to end: "Q3 was strong in q3.".
        assert_eq!(found[2].text, "Q3 was strong in q3.");
        assert_eq!(found[2].matches, [[0, 2], [17, 19]]);
        let cased = find(&deck(), &Query { case: true, ..query("q3") }).unwrap();
        assert_eq!(cased.iter().map(|f| f.matches.len()).sum::<usize>(), 1, "only `q3.` in the note");
        // Characters, not bytes; and whole words only where asked.
        assert_eq!(matches("Ünïcode Q3x Q3", &query("q3")), [[8, 10], [12, 14]]);
        assert_eq!(matches("Ünïcode Q3x Q3", &Query { words: true, ..query("q3") }), [[12, 14]]);
        assert_eq!(matches("ÉTÉ été", &query("été")), [[0, 3], [4, 7]], "case alike, letter by letter");
        assert_eq!(matches("aaaa", &query("aa")), [[0, 2], [2, 4]], "none overlap");
        assert!(matches("anything", &query("")).is_empty(), "an empty query matches nothing");
        assert!(find(&deck(), &query("")).unwrap().is_empty());
    }

    #[test]
    fn replacing_every_match_is_one_patch_written_where_each_text_lives() {
        let mut doc = deck();
        let found = find(&doc, &query("q3")).unwrap();
        let ops = replacing(&found, "Q4");
        assert_eq!(ops.len(), 5);
        assert_eq!(
            ops[1],
            json!({ "op": "replace_text", "node": "title", "from": 0, "to": 2, "text": "Q4", "state": "b" })
        );
        // The note's two matches, the last first, so the first one's offsets stand.
        assert_eq!((ops[2]["from"].as_u64(), ops[3]["from"].as_u64()), (Some(17), Some(0)));
        apply(&mut doc, &ops);
        assert_eq!(doc["nodes"]["title"]["text"], "Revenue in Q4");
        assert_eq!(doc["states"][1]["props"]["title"]["text"], "Q4, by region");
        // Each run keeps its look.
        assert_eq!(
            doc["nodes"]["note"]["runs"],
            json!([{ "text": "Q4 was " }, { "text": "strong", "emphasis": "strong" }, { "text": " in Q4." }])
        );
        assert_eq!(doc["overrides"]["badge"]["text"], "Q4 draft");
        assert!(find(&doc, &query("q3")).unwrap().is_empty(), "nothing is left to find");
        // Taken away, as replaced by nothing.
        let found = find(&doc, &Query { words: true, ..query("in") }).unwrap();
        apply(&mut doc, &replacing(&found, ""));
        assert_eq!(doc["nodes"]["title"]["text"], "Revenue  Q4");
    }
}

//! Find and replace across the deck's words, in every state (PLAN 2.47, 2.83, ADR-0013): its
//! texts, each node's description (`alt`), each state's notes, each beat's claim and notes, the
//! words a chart or a table draws that the deck writes (an axis's or a channel's title, a legend's
//! title, an annotation's text, a column's title, and the values a highlight picks out), and the
//! words in the cells of a data source written inline (dogfood pass 3).
//!
//! A text node's text is written in one place for each state that shows it: the deck's
//! `overrides`, which hold it in every state; else the latest delta that sets it, from the
//! state back along what it tracks; else the node's own. A node's `alt` is written as a text
//! is. [`find`] gives each of them the deck shows once for each place it is written, with the
//! states that show it from there and where a query matches it; and each state's notes, and
//! each beat's claim and notes, where the state or the spine writes them. [`replacing`] makes
//! the patch that replaces those matches: for a text, a `replace_text` for each, made in the
//! first state that shows the text, so it is written where the text lives, as typing writes it;
//! for the rest, a JSON Patch `replace` of the words where they are written.

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

/// What words a match is in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// A text node's text.
    #[default]
    Text,
    /// A node's description, its `alt`.
    Alt,
    /// A state's speaker notes, or a beat's.
    Notes,
    /// A beat's claim.
    Claim,
    /// Words a chart or a table draws that the deck writes: an axis's or a channel's `title`, a
    /// legend's `title`, an annotation's `text`, a column's `title`, or a value an annotation's
    /// `at` picks out (`x`, `series`), which names a value of the data it is drawn from.
    Chart,
    /// A cell of a data source written inline in the deck (`data.<name>.source.inline`).
    Data,
}

/// Words the deck holds, as written in one place, and where a query matches them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Found {
    /// What the words are: a text, a description, notes, or a claim.
    #[serde(default)]
    pub kind: Kind,
    /// The node: the text's, the one the description describes, or the chart or table that draws
    /// the words. None for notes, a claim, or a data source's cell.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<String>,
    /// The data source a cell is of, by its name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// The file a cell is in, for a data source read from one: found, and left as it is by a
    /// replacement, which writes the deck alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The beat, for its claim or its notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub beat: Option<String>,
    /// The first state that shows them as written there: where `replace_text` is made. For
    /// notes, the state they are of, or the beat's first; for a claim, the beat's first state.
    pub state: String,
    /// Every state that shows them as written there, in the deck's order.
    pub states: Vec<String>,
    /// Where they are written, a JSON pointer into the deck: the node's own `text`, `runs`, or
    /// `alt` (`/nodes/title/text`), a state's delta (`/states/2/props/title/text`), or the deck's
    /// `overrides`, which hold it in every state (`/overrides/title/text`); a state's notes
    /// (`/states/2/notes`); or a beat's claim or notes (`/spine/sections/0/beats/1/claim`).
    pub lives: String,
    /// The words: a text's `text`, or its runs' texts end to end.
    pub text: String,
    /// Each match, `[from, to]` in characters (Unicode scalar values), as `replace_text`
    /// counts them, in order; none overlap.
    pub matches: Vec<[u32; 2]>,
}

/// Each of the deck's words that `query` matches, once for each place it is written, in the
/// order the deck first shows them: in each state, a beat's claim and notes where it begins,
/// the state's notes, then each node's text and description. An empty query matches nothing.
pub fn find(doc: &Value, query: &Query) -> Result<Vec<Found>, String> {
    let deck =
        Deck::from_value(doc).map_err(|e| format!("the deck must parse to be searched, and it does not: {e}"))?;
    let snapshots = resolve_states(&deck).map_err(|e| e.to_string())?;
    let overrides = doc.get("overrides").and_then(Value::as_object);
    let kind =
        |node: &str| doc.get("nodes").and_then(|n| n.get(node)).and_then(|n| n.get("type")).and_then(Value::as_str);
    // Each beat, by where the spine writes it.
    let beats: Vec<(String, &Value)> = doc
        .pointer("/spine/sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .enumerate()
        .flat_map(|(s, section)| {
            let beats = section.get("beats").and_then(Value::as_array).into_iter().flatten();
            beats.enumerate().map(move |(b, beat)| (format!("/spine/sections/{s}/beats/{b}"), beat))
        })
        .collect();
    let states_of = |beat: &Value| -> Vec<String> {
        let ids = beat.get("states").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str);
        ids.map(str::to_string).collect()
    };
    // Each place words are written, matched or not, so every state that shows them is counted.
    let mut places: IndexMap<String, Found> = IndexMap::new();
    let add = |places: &mut IndexMap<String, Found>,
               kind,
               node: Option<&str>,
               beat,
               state: &str,
               lives: String,
               text: &str| {
        places.entry(lives.clone()).and_modify(|f| f.states.push(state.to_string())).or_insert_with(|| Found {
            kind,
            node: node.map(str::to_string),
            source: None,
            file: None,
            beat,
            state: state.to_string(),
            states: vec![state.to_string()],
            lives,
            matches: matches(text, query),
            text: text.to_string(),
        });
    };
    // A node's words as written for a state: where they live, and what they are.
    let written = |i: usize, node: &str, prop: &str, over: Option<&serde_json::Map<String, Value>>| match over
        .is_some_and(|o| o.contains_key(prop))
    {
        true => format!("/overrides/{}/{prop}", esc(node)),
        false => match lives(&deck, i, node, prop, &[]) {
            Lives::State(j) => format!("/states/{j}/props/{}/{prop}", esc(node)),
            Lives::Node => format!("/nodes/{}/{prop}", esc(node)),
        },
    };
    for (i, snapshot) in snapshots.iter().enumerate() {
        let state = snapshot.state_id.as_str();
        // A beat's claim and notes, in the states it spans; first where it begins.
        for (at, beat) in &beats {
            if !states_of(beat).iter().any(|s| s == state) {
                continue;
            }
            let id = beat.get("id").and_then(Value::as_str).map(str::to_string);
            for (field, kind) in [("claim", Kind::Claim), ("notes", Kind::Notes)] {
                if let Some(text) = beat.get(field).and_then(Value::as_str) {
                    add(&mut places, kind, None, id.clone(), state, format!("{at}/{field}"), text);
                }
            }
        }
        if let Some(notes) = doc.pointer(&format!("/states/{i}/notes")).and_then(Value::as_str) {
            add(&mut places, Kind::Notes, None, None, state, format!("/states/{i}/notes"), notes);
        }
        for (node, props) in &snapshot.nodes {
            let over = overrides.and_then(|o| o.get(node)).and_then(Value::as_object);
            let mut props = props.clone();
            if let Some(over) = over {
                merge_props(&mut props, &over.iter().map(|(k, v)| (k.clone(), v.clone())).collect());
            }
            if kind(node) == Some("text") {
                let (prop, text) = shown(&props);
                add(&mut places, Kind::Text, Some(node), None, state, written(i, node, prop, over), &text);
            }
            if let Some(alt) = props.get("alt").and_then(Value::as_str) {
                add(&mut places, Kind::Alt, Some(node), None, state, written(i, node, "alt", over), alt);
            }
            if matches!(kind(node), Some("chart" | "table")) {
                for (prop, path, words) in drawn(&props) {
                    let lives = format!("{}{path}", written(i, node, prop, over));
                    add(&mut places, Kind::Chart, Some(node), None, state, lives, &words);
                }
            }
        }
    }
    // Each cell of a data source written inline, in the states that draw a chart or a table of it.
    for (name, source) in doc.get("data").and_then(Value::as_object).into_iter().flatten() {
        let Some(rows) = source.pointer("/source/inline").and_then(Value::as_array) else { continue };
        let reading = format!("@{name}");
        let states: Vec<String> = (snapshots.iter())
            .filter(|s| s.nodes.values().any(|p| p.get("data").and_then(Value::as_str) == Some(reading.as_str())))
            .map(|s| s.state_id.clone())
            .collect();
        for (r, row) in rows.iter().enumerate() {
            for (column, cell) in row.as_object().into_iter().flatten() {
                let Some(text) = cell.as_str() else { continue };
                let lives = format!("/data/{}/source/inline/{r}/{}", esc(name), esc(column));
                places.insert(
                    lives.clone(),
                    Found {
                        kind: Kind::Data,
                        node: None,
                        source: Some(name.clone()),
                        file: None,
                        beat: None,
                        state: states.first().cloned().unwrap_or_else(|| first_state(&snapshots)),
                        states: states.clone(),
                        lives,
                        matches: matches(text, query),
                        text: text.to_string(),
                    },
                );
            }
        }
    }
    // A beat no state shows holds the deck's words all the same: made at its first state, or
    // the deck's, and shown in none.
    let first = first_state(&snapshots);
    for (at, beat) in &beats {
        for (field, kind) in [("claim", Kind::Claim), ("notes", Kind::Notes)] {
            let (lives, text) = (format!("{at}/{field}"), beat.get(field).and_then(Value::as_str));
            let Some(text) = text.filter(|_| !places.contains_key(&lives)) else { continue };
            let found = Found {
                kind,
                node: None,
                source: None,
                file: None,
                beat: beat.get("id").and_then(Value::as_str).map(str::to_string),
                state: states_of(beat).into_iter().next().unwrap_or_else(|| first.clone()),
                states: Vec::new(),
                lives: lives.clone(),
                matches: matches(text, query),
                text: text.to_string(),
            };
            places.insert(lives, found);
        }
    }
    Ok(places.into_values().filter(|f| !f.matches.is_empty()).collect())
}

/// The id of the deck's first state, which words no state shows are made at.
fn first_state(snapshots: &[crate::tracking::Snapshot]) -> String {
    snapshots.first().map(|s| s.state_id.clone()).unwrap_or_default()
}

/// The words a chart or a table draws that the deck writes, as `props` show them: each with the
/// property it is in and the path in it, `("y", "/title", "Revenue ($M)")`. An annotation's `at`
/// values are among them: one names a value of the data, and a replacement that changes the data
/// changes it too, so a highlight still picks out what it did.
fn drawn(props: &Props) -> Vec<(&'static str, String, String)> {
    let mut out = Vec::new();
    for prop in ["x", "y", "series", "color", "sizeEncoding", "facet", "legend"] {
        if let Some(title) = props.get(prop).and_then(|p| p.get("title")).and_then(Value::as_str) {
            out.push((prop, "/title".to_string(), title.to_string()));
        }
    }
    for (k, note) in props.get("annotations").and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(text) = note.get("text").and_then(Value::as_str) {
            out.push(("annotations", format!("/{k}/text"), text.to_string()));
        }
        for axis in ["x", "series"] {
            match note.pointer(&format!("/at/{axis}")) {
                Some(Value::String(v)) => out.push(("annotations", format!("/{k}/at/{axis}"), v.clone())),
                Some(Value::Array(vs)) => {
                    for (j, v) in vs.iter().enumerate() {
                        if let Some(v) = v.as_str() {
                            out.push(("annotations", format!("/{k}/at/{axis}/{j}"), v.to_string()));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    for (k, column) in props.get("columns").and_then(Value::as_array).into_iter().flatten().enumerate() {
        if let Some(title) = column.get("title").and_then(Value::as_str) {
            out.push(("columns", format!("/{k}/title"), title.to_string()));
        }
    }
    out
}

/// The patch that replaces every match of `found` with `with`. In a text, a `replace_text` for
/// each, in the first state that shows it, so it is written where the text lives; within a text,
/// the last match goes first, so that the offsets of those before it still stand. Other words, a
/// JSON Patch `replace` of them where they are written, every match in them replaced. A cell of a
/// data source read from a file is left as it is: the file is the bundle's, not the deck's.
pub fn replacing(found: &[Found], with: &str) -> Vec<Value> {
    let each = |f: &Found, &[from, to]: &[u32; 2]| json!({ "op": "replace_text", "node": f.node, "from": from, "to": to, "text": with, "state": f.state });
    found
        .iter()
        .filter(|f| f.file.is_none())
        .flat_map(|f| match f.kind {
            Kind::Text => f.matches.iter().rev().map(|m| each(f, m)).collect(),
            _ => vec![json!({ "op": "replace", "path": f.lives, "value": replaced(&f.text, &f.matches, with) })],
        })
        .collect()
}

/// `text` with each of `matches`, in characters, replaced by `with`.
fn replaced(text: &str, matches: &[[u32; 2]], with: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut at = 0;
    for &[from, to] in matches {
        out.extend(&chars[at..from as usize]);
        out.push_str(with);
        at = to as usize;
    }
    out.extend(&chars[at..]);
    out
}

/// A text node's text as props show it, under the deck's overrides for it: the property it
/// is (`runs` where it has some, else `text`), and the text, its runs' texts end to end.
fn shown(props: &Props) -> (&'static str, String) {
    match props.get("runs").and_then(Value::as_array).filter(|runs| !runs.is_empty()) {
        Some(runs) => ("runs", runs.iter().filter_map(|r| r.get("text").and_then(Value::as_str)).collect()),
        None => ("text", props.get("text").and_then(Value::as_str).unwrap_or_default().to_string()),
    }
}

/// Where `query` matches `text`, in characters, from the start, none overlapping.
pub fn matches(text: &str, query: &Query) -> Vec<[u32; 2]> {
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
            .map(|f| {
                (f.node.as_deref().unwrap_or_default(), f.lives.as_str(), f.states.iter().map(String::as_str).collect())
            })
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
        assert!(found.iter().all(|f| f.node.as_deref() != Some("box")));
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

    /// The deck with words beyond its texts (PLAN 2.83): a description, a state's notes, and a
    /// beat's claim and notes, each saying Q3 once.
    fn worded() -> Value {
        let mut doc = deck();
        doc["nodes"]["box"]["alt"] = json!("A box for Q3");
        doc["states"][1]["notes"] = json!("Say Q3 slowly.");
        doc["spine"] = json!({ "sections": [{ "id": "results", "title": "Results", "beats": [
            { "id": "growth", "claim": "Q3 grew.", "states": ["b"], "notes": "Pause after Q3." },
            { "id": "later", "claim": "Q3 again, unshown.", "states": [] }
        ] }] });
        doc
    }

    #[test]
    fn notes_claims_and_descriptions_are_found_where_they_are_written() {
        let found = find(&worded(), &query("q3")).unwrap();
        // What each is in, its node, its beat, the state it is made in, and where it is written.
        type Place<'a> = (Kind, Option<&'a str>, Option<&'a str>, &'a str, &'a str);
        let places: Vec<Place> = found
            .iter()
            .map(|f| (f.kind, f.node.as_deref(), f.beat.as_deref(), f.state.as_str(), f.lives.as_str()))
            .collect();
        assert_eq!(
            places,
            [
                (Kind::Text, Some("title"), None, "a", "/nodes/title/text"),
                (Kind::Alt, Some("box"), None, "a", "/nodes/box/alt"),
                (Kind::Claim, None, Some("growth"), "b", "/spine/sections/0/beats/0/claim"),
                (Kind::Notes, None, Some("growth"), "b", "/spine/sections/0/beats/0/notes"),
                (Kind::Notes, None, None, "b", "/states/1/notes"),
                (Kind::Text, Some("title"), None, "b", "/states/1/props/title/text"),
                (Kind::Text, Some("note"), None, "b", "/nodes/note/runs"),
                (Kind::Text, Some("badge"), None, "b", "/overrides/badge/text"),
                // A beat no state shows: at the deck's first state, shown in none.
                (Kind::Claim, None, Some("later"), "a", "/spine/sections/0/beats/1/claim"),
            ],
            "{found:#?}"
        );
        assert_eq!(found[1].states, ["a", "b", "c"], "the box's description, in each state that shows the box");
        assert!(found[8].states.is_empty());
    }

    #[test]
    fn replacing_words_beyond_texts_is_one_patch_too() {
        let mut doc = worded();
        let found = find(&doc, &query("q3")).unwrap();
        let ops = replacing(&found, "Q4");
        assert!(
            ops.contains(&json!({ "op": "replace", "path": "/states/1/notes", "value": "Say Q4 slowly." })),
            "{ops:#?}"
        );
        apply(&mut doc, &ops);
        assert_eq!(doc["nodes"]["box"]["alt"], "A box for Q4");
        assert_eq!(doc["states"][1]["notes"], "Say Q4 slowly.");
        assert_eq!(doc["spine"]["sections"][0]["beats"][0]["claim"], "Q4 grew.");
        assert_eq!(doc["spine"]["sections"][0]["beats"][0]["notes"], "Pause after Q4.");
        assert_eq!(doc["spine"]["sections"][0]["beats"][1]["claim"], "Q4 again, unshown.");
        assert!(find(&doc, &query("q3")).unwrap().is_empty(), "nothing is left to find");
        // Every match in the words, characters counted as `replace_text` counts them.
        assert_eq!(replaced("Ünï Q3 q3", &[[4, 6], [7, 9]], "x"), "Ünï x x");
    }
    #[test]
    fn a_charts_words_and_its_inline_cells_are_found_and_replaced_where_written() {
        let mut doc = json!({
            "scaena": crate::FORMAT_VERSION,
            "canvas": { "width": 1920, "height": 1080 },
            "data": { "mix": { "source": { "inline": [
                { "region": "North", "sales": 4 },
                { "region": "Northwest", "sales": 2 }
            ] } } },
            "nodes": {
                "c": { "type": "chart", "kind": "bar", "data": "@mix",
                    "x": { "field": "region", "type": "nominal", "title": "North and south" },
                    "y": { "field": "sales", "type": "quantitative" },
                    "annotations": [{ "at": { "x": "North" }, "text": "North leads" }] },
                "t": { "type": "table", "data": "@mix", "columns": [{ "field": "region", "title": "North?" }] }
            },
            "states": [{ "id": "a", "props": { "c": {}, "t": {} } }]
        });
        let found = find(&doc, &Query { words: true, ..query("North") }).unwrap();
        let lives: Vec<(&str, Kind)> = found.iter().map(|f| (f.lives.as_str(), f.kind)).collect();
        for want in [
            ("/nodes/c/x/title", Kind::Chart),
            ("/nodes/c/annotations/0/text", Kind::Chart),
            ("/nodes/c/annotations/0/at/x", Kind::Chart),
            ("/nodes/t/columns/0/title", Kind::Chart),
            ("/data/mix/source/inline/0/region", Kind::Data),
        ] {
            assert!(lives.contains(&want), "{want:?} in {lives:?}");
        }
        assert!(!lives.iter().any(|(l, _)| l.ends_with("/1/region")), "whole words only: {lives:?}");
        let cell = found.iter().find(|f| f.kind == Kind::Data).unwrap();
        assert_eq!((cell.source.as_deref(), cell.states.as_slice()), (Some("mix"), &["a".to_string()][..]));

        // Replaced in one patch, each where it is written: the highlight still picks out its row.
        apply(&mut doc, &replacing(&found, "Nord"));
        assert_eq!(doc["nodes"]["c"]["x"]["title"], "Nord and south");
        assert_eq!(doc["nodes"]["c"]["annotations"][0], json!({ "at": { "x": "Nord" }, "text": "Nord leads" }));
        assert_eq!(doc["nodes"]["t"]["columns"][0]["title"], "Nord?");
        assert_eq!(doc["data"]["mix"]["source"]["inline"][0]["region"], "Nord");
        assert_eq!(doc["data"]["mix"]["source"]["inline"][1]["region"], "Northwest");
        assert_eq!(doc["nodes"]["c"]["x"]["field"], "region", "a field names a column, never words");
    }
}

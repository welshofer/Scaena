//! The deck as a CRDT (PLAN 1.23, SPEC §8). Every deck in the repository goes in and comes
//! out as deck.json, byte for byte. A change is the least it can be and says who made it.
//! Branches merge, a renamed node is still the node it was, and undo undoes one's own.

use scaena_core::Deck;
use scaena_store::crdt::{CrdtError, DeckDoc, Edit, FS, OUTSIDE, Recorded};
use serde_json::{Value, json};
use std::path::Path;

/// Every deck in the repository that reads as one, by path.
fn decks() -> Vec<(String, Deck)> {
    fn walk(dir: &Path, out: &mut Vec<(String, Deck)>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap().path()).collect();
        entries.sort();
        for path in entries {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if path.is_dir() {
                walk(&path, out);
            } else if name == "deck.json" || name.ends_with(".deck.json") {
                let text = std::fs::read_to_string(&path).unwrap();
                if let Ok(deck) = Deck::from_json(&text) {
                    out.push((path.display().to_string(), deck));
                }
            }
        }
    }
    let mut out = Vec::new();
    for dir in ["../../docs", "../../tests"] {
        walk(Path::new(dir), &mut out);
    }
    out
}

fn deck(v: Value) -> Deck {
    serde_json::from_value(v).unwrap()
}

fn json_of(deck: &Deck) -> Value {
    serde_json::to_value(deck).unwrap()
}

fn base() -> Value {
    json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "meta": { "title": "Q3", "lang": "en-US", "client": "Acme" },
        "canvas": { "width": 1920, "height": 1080 },
        "nodes": {
            "title": { "type": "text", "role": "title", "text": "Revenue doubled" },
            "chart": { "type": "chart", "at": { "col": [1, 6], "row": [2, 3] } },
            "panel": { "type": "stack", "gap": 24 },
            "caption": { "type": "text", "role": "caption", "text": "Q3", "at": { "parent": "panel" } },
            "rich": { "type": "text", "role": "body",
                      "runs": [{ "text": "Pro ", "emphasis": "high" }, { "text": "drove it." }] }
        },
        "states": [
            { "id": "a", "hold": 2000, "notes": "Pause." },
            { "id": "b",
              "props": { "title": { "text": "Revenue doubled again" }, "chart": { "opacity": 0.5 } },
              "remove": ["caption"],
              "choreography": [{ "target": "title", "enter": "fade" }] }
        ],
        "overrides": { "title": { "style": { "size": 96, "color": "#fff" } } },
        "spine": { "sections": [
            { "id": "open", "title": "Open", "beats": [{ "id": "hello", "claim": "Hello.", "states": ["a"] }] },
            { "id": "growth", "beats": [{ "id": "doubled", "claim": "It doubled.", "states": ["b"], "notes": "Slowly." }] }
        ] }
    })
}

fn user() -> Edit<'static> {
    Edit { timestamp: Some(1_000), ..Edit::by("user") }
}

/// Both documents take in the other's changes, and then hold the same deck.
fn merged(a: &DeckDoc, b: &DeckDoc) -> Value {
    a.merge(b).unwrap();
    b.merge(a).unwrap();
    let (x, y) = (json_of(&a.deck().unwrap()), json_of(&b.deck().unwrap()));
    assert_eq!(x, y, "both sides of a merge hold one deck");
    x
}

#[test]
fn every_deck_in_the_repository_comes_out_as_it_went_in() {
    let decks = decks();
    assert!(decks.len() > 40, "{} decks", decks.len());
    for (path, deck) in decks {
        let canonical = deck.to_json().unwrap();
        let doc = DeckDoc::from_deck(&deck, &user()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(doc.deck().unwrap().to_json().unwrap(), canonical, "{path}");
        let loaded = DeckDoc::load(&doc.save().unwrap()).unwrap();
        assert_eq!(loaded.deck().unwrap().to_json().unwrap(), canonical, "{path}, saved and loaded");
        assert!(!loaded.apply(&deck, &Edit::by("user")).unwrap(), "{path}: the same deck again is no change");
    }
}

#[test]
fn a_change_is_the_least_it_can_be_and_says_who_made_it() {
    let doc = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let mut next = base();
    next["nodes"]["title"]["text"] = json!("Revenue doubled in Q3");
    let edit = Edit { message: Some("Say when."), timestamp: Some(2_000), ..Edit::by("agent:claude") };
    assert!(doc.apply(&deck(next.clone()), &edit).unwrap());
    assert_eq!(json_of(&doc.deck().unwrap()), json_of(&deck(next)));
    let changes = doc.changes();
    assert_eq!(changes.len(), 2);
    let last = &changes[1];
    assert_eq!(
        (last.author.as_deref(), last.message.as_deref(), last.timestamp),
        (Some("agent:claude"), Some("Say when."), 2_000)
    );
    // Six characters typed, and nothing else touched.
    assert_eq!(last.ops, " in Q3".len());
    assert_eq!((changes[0].author.as_deref(), changes[0].message.as_deref()), (Some("user"), None));
    // The history is saved with the document.
    assert_eq!(DeckDoc::load(&doc.save().unwrap()).unwrap().changes(), changes);
}

#[test]
fn a_renamed_node_is_still_the_node_it_was() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    // On one branch, `title` becomes `headline`, everywhere it is named.
    let renamed = serde_json::to_string(&base())
        .unwrap()
        .replace("\"title\":{", "\"headline\":{")
        .replace("\"target\":\"title\"", "\"target\":\"headline\"");
    let renamed: Value = serde_json::from_str(&renamed).unwrap();
    assert!(renamed["nodes"].get("headline").is_some() && renamed["overrides"].get("headline").is_some());
    let hint = [("title".to_string(), "headline".to_string())];
    a.apply(&deck(renamed), &Edit { renamed_nodes: &hint, ..Edit::by("user") }).unwrap();
    // On the other, by its old id: its text, its delta in `b`, and its override.
    let mut edited = base();
    edited["nodes"]["title"]["text"] = json!("Revenue doubled, at last");
    edited["states"][1]["props"]["title"]["text"] = json!("And again");
    edited["overrides"]["title"]["style"]["size"] = json!(120);
    b.apply(&deck(edited), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    assert!(both["nodes"].get("title").is_none(), "{both:#}");
    assert_eq!(both["nodes"]["headline"]["text"], "Revenue doubled, at last");
    assert_eq!(both["states"][1]["props"]["headline"]["text"], "And again");
    assert_eq!(both["states"][1]["choreography"][0]["target"], "headline");
    assert_eq!(both["overrides"]["headline"]["style"], json!({ "size": 120, "color": "#fff" }));
    // It kept its place in paint order.
    assert_eq!(both["nodes"].as_object().unwrap().keys().next().map(String::as_str), Some("headline"));
}

#[test]
fn concurrent_edits_to_one_deck_merge() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    let mut one = base();
    one["nodes"]["title"]["text"] = json!("Revenue doubled in Q3");
    one["nodes"]["chart"]["opacity"] = json!(0.8);
    // `chart` paints first.
    let nodes = one["nodes"].as_object().unwrap().clone();
    let mut reordered = serde_json::Map::new();
    reordered.insert("chart".into(), nodes["chart"].clone());
    for (k, v) in nodes.into_iter().filter(|(k, _)| k != "chart") {
        reordered.insert(k, v);
    }
    one["nodes"] = Value::Object(reordered);
    a.apply(&deck(one), &Edit::by("user")).unwrap();
    let mut other = base();
    other["nodes"]["title"]["text"] = json!("Look: Revenue doubled");
    other["nodes"]["chart"]["at"] = json!({ "col": [1, 12] });
    other["nodes"]["logo"] = json!({ "type": "image", "src": "assets/logo.png" });
    other["states"][0]["notes"] = json!("Pause. Breathe.");
    b.apply(&deck(other), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    // Text merges by character; props by prop; order by move.
    assert_eq!(both["nodes"]["title"]["text"], "Look: Revenue doubled in Q3");
    assert_eq!(both["nodes"]["chart"], json!({ "type": "chart", "at": { "col": [1, 12] }, "opacity": 0.8 }));
    let order: Vec<&str> = both["nodes"].as_object().unwrap().keys().map(String::as_str).collect();
    assert_eq!(order, ["chart", "title", "panel", "caption", "rich", "logo"]);
    assert_eq!(both["states"][0]["notes"], "Pause. Breathe.");
}

#[test]
fn beats_move_between_sections_and_keep_their_edits() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    let mut moved = base();
    let hello = moved["spine"]["sections"][0]["beats"][0].clone();
    moved["spine"]["sections"][0]["beats"] = json!([]);
    moved["spine"]["sections"][1]["beats"].as_array_mut().unwrap().push(hello);
    a.apply(&deck(moved), &Edit::by("user")).unwrap();
    let mut edited = base();
    edited["spine"]["sections"][0]["beats"][0]["claim"] = json!("Hello, Q3.");
    edited["spine"]["sections"][1]["beats"][0]["notes"] = json!("Slowly, then fast.");
    b.apply(&deck(edited), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    let growth = &both["spine"]["sections"][1]["beats"];
    assert_eq!(growth[0]["notes"], "Slowly, then fast.");
    assert_eq!((growth[1]["id"].as_str(), growth[1]["claim"].as_str()), (Some("hello"), Some("Hello, Q3.")));
    assert_eq!(both["spine"]["sections"][0]["beats"], json!([]));
}

#[test]
fn runs_are_rich_text_and_each_stays_a_run() {
    let mut v = base();
    // Two runs alike are still two runs; a style keeps the order of its keys.
    v["nodes"]["rich"]["runs"] = json!([
        { "text": "Pro ", "emphasis": "high", "style": { "weight": 700, "color": "accent" } },
        { "text": "drove " },
        { "text": "it." }
    ]);
    let a = DeckDoc::from_deck(&deck(v.clone()), &user()).unwrap();
    assert_eq!(a.deck().unwrap().to_json().unwrap(), deck(v.clone()).to_json().unwrap());
    let b = a.fork();
    // Typing inside one run while another is restyled: both stand. (Text typed exactly at
    // the edge of a run being restyled may land on either side: that is rich text's one
    // ambiguity, settled the same way on every side.)
    let mut typed = v.clone();
    typed["nodes"]["rich"]["runs"][1]["text"] = json!("drive ");
    typed["nodes"]["rich"]["runs"][0]["text"] = json!("Pros ");
    a.apply(&deck(typed), &Edit::by("user")).unwrap();
    let mut styled = v.clone();
    styled["nodes"]["rich"]["runs"][2]["emphasis"] = json!("low");
    b.apply(&deck(styled), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    assert_eq!(
        both["nodes"]["rich"]["runs"],
        json!([
            { "text": "Pros ", "emphasis": "high", "style": { "weight": 700, "color": "accent" } },
            { "text": "drive " },
            { "text": "it.", "emphasis": "low" }
        ])
    );
}

#[test]
fn nodes_made_apart_under_one_id_are_told_apart() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    for (doc, text) in [(&a, "A"), (&b, "B")] {
        let mut v = base();
        v["nodes"]["note"] = json!({ "type": "text", "role": "caption", "text": text });
        v["states"][0]["props"] = json!({ "note": { "opacity": 1 } });
        doc.apply(&deck(v), &Edit::by("user")).unwrap();
    }
    let both = merged(&a, &b);
    let texts: Vec<&str> = ["note", "note-2"].iter().map(|id| both["nodes"][id]["text"].as_str().unwrap()).collect();
    assert!(texts == ["A", "B"] || texts == ["B", "A"], "{texts:?}");
    // Each keeps its own delta, under the id it now has.
    assert_eq!(both["states"][0]["props"], json!({ "note": { "opacity": 1 }, "note-2": { "opacity": 1 } }));
    // Written back, the suffix sticks, and the two stay two.
    let doc = DeckDoc::load(&a.save().unwrap()).unwrap();
    let written = deck(both.clone());
    assert!(doc.apply(&written, &Edit::by("user")).unwrap());
    assert_eq!(json_of(&doc.deck().unwrap()), both);
}

#[test]
fn references_to_nodes_that_are_not_there_stay_as_written() {
    let mut v = base();
    v["states"][1]["remove"] = json!(["caption", "ghost"]);
    v["states"][1]["props"]["phantom"] = json!({ "opacity": 0 });
    v["nodes"]["chart"]["at"] = json!({ "parent": "nowhere" });
    let doc = DeckDoc::from_deck(&deck(v.clone()), &user()).unwrap();
    assert_eq!(doc.deck().unwrap().to_json().unwrap(), deck(v).to_json().unwrap());
}

#[test]
fn undo_undoes_ones_own_change_and_leaves_the_files_be() {
    let doc = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let mut undo = doc.undo_manager();
    let mut mine = base();
    mine["nodes"]["title"]["text"] = json!("Revenue tripled");
    doc.apply(&deck(mine.clone()), &Edit::by("user")).unwrap();
    // Then the file changes on disk.
    let mut theirs = mine.clone();
    theirs["nodes"]["chart"]["opacity"] = json!(0.25);
    doc.apply(&deck(theirs), &Edit::by(FS)).unwrap();
    assert!(undo.undo(&doc, "user").unwrap());
    let now = json_of(&doc.deck().unwrap());
    assert_eq!(now["nodes"]["title"]["text"], "Revenue doubled", "my change is undone");
    assert_eq!(now["nodes"]["chart"]["opacity"], 0.25, "the file's stays");
    assert!(undo.redo(&doc, "user").unwrap());
    assert_eq!(json_of(&doc.deck().unwrap())["nodes"]["title"]["text"], "Revenue tripled");
    let last = doc.changes().pop().unwrap();
    assert_eq!((last.author.as_deref(), last.message.as_deref()), (Some("user"), Some("redo")));
}

#[test]
fn a_key_the_crdt_keeps_for_itself_is_refused() {
    let mut v = base();
    v["nodes"]["title"]["\u{0}order"] = json!(1);
    let err = DeckDoc::from_deck(&deck(v), &user()).unwrap_err();
    assert!(matches!(err, CrdtError::Reserved(_)), "{err}");
}

#[test]
fn states_keep_who_they_are_through_moves_and_renames() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    // One branch moves `b` first and renames `a` to `intro`, wherever it is named.
    let mut moved = base();
    let states = moved["states"].as_array().unwrap().clone();
    moved["states"] = json!([states[1], states[0]]);
    moved["states"][1]["id"] = json!("intro");
    moved["spine"]["sections"][0]["beats"][0]["states"] = json!(["intro"]);
    let hint = [("a".to_string(), "intro".to_string())];
    a.apply(&deck(moved), &Edit { renamed_states: &hint, ..Edit::by("user") }).unwrap();
    // The other changes both, by their old places and ids.
    let mut edited = base();
    edited["states"][0]["hold"] = json!(3000);
    edited["states"][1]["hold"] = json!(1500);
    b.apply(&deck(edited), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    let ids: Vec<&str> = both["states"].as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["b", "intro"]);
    assert_eq!((both["states"][0]["hold"].as_f64(), both["states"][1]["hold"].as_f64()), (Some(1500.0), Some(3000.0)));
    assert_eq!(both["states"][1]["notes"], "Pause.", "its notes came with it");
}

#[test]
fn text_becomes_runs_and_back() {
    let doc = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let mut v = base();
    v["nodes"]["title"].as_object_mut().unwrap().remove("text");
    v["nodes"]["title"]["runs"] = json!([{ "text": "Revenue " }, { "text": "doubled", "emphasis": "high" }]);
    doc.apply(&deck(v.clone()), &Edit::by("user")).unwrap();
    assert_eq!(doc.deck().unwrap().to_json().unwrap(), deck(v).to_json().unwrap());
    doc.apply(&deck(base()), &Edit::by("user")).unwrap();
    assert_eq!(doc.deck().unwrap().to_json().unwrap(), deck(base()).to_json().unwrap());
}

#[test]
fn a_node_removed_on_one_branch_stays_removed() {
    let a = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let b = a.fork();
    let mut removed = base();
    removed["nodes"].as_object_mut().unwrap().remove("chart");
    removed["states"][1]["props"].as_object_mut().unwrap().remove("chart");
    a.apply(&deck(removed), &Edit::by("user")).unwrap();
    let mut edited = base();
    edited["nodes"]["chart"]["opacity"] = json!(0.9);
    b.apply(&deck(edited), &Edit::by("agent:claude")).unwrap();
    let both = merged(&a, &b);
    assert!(both["nodes"].get("chart").is_none(), "{:#}", both["nodes"]);
    assert!(both["states"][1]["props"].get("chart").is_none());
}

#[test]
fn recorded_changes_go_in_one_by_one_by_their_authors() {
    let doc = DeckDoc::from_deck(&deck(base()), &user()).unwrap();
    let text = |v: &Value| deck(v.clone()).to_json().unwrap();
    let mut typed = base();
    typed["nodes"]["title"]["text"] = json!("Revenue doubled in Q3");
    let mut renamed = serde_json::to_string(&typed)
        .unwrap()
        .replace("\"title\":{", "\"headline\":{")
        .replace("\"target\":\"title\"", "\"target\":\"headline\"");
    renamed = renamed.replace("Revenue doubled in Q3", "Revenue doubled in Q3, again");
    let renamed: Value = serde_json::from_str(&renamed).unwrap();
    let changes = [
        // The deck as it was: no change, and none recorded.
        Recorded { deck: text(&base()), author: FS.into(), message: Some(OUTSIDE.into()), ..at(2_000) },
        Recorded { deck: text(&typed), author: "user".into(), message: Some("edit".into()), ..at(3_000) },
        Recorded {
            deck: text(&renamed),
            author: "agent:scripted".into(),
            message: Some("patch: rename_node, set_text".into()),
            renamed_nodes: vec![("title".into(), "headline".into())],
            ..at(4_000)
        },
    ];
    // As one module hands them to another.
    let wire = serde_json::to_string(&changes).unwrap();
    assert!(wire.contains("\"renamedNodes\":[[\"title\",\"headline\"]]") && !wire.contains("renamedStates"), "{wire}");
    let changes: Vec<Recorded> = serde_json::from_str(&wire).unwrap();
    assert_eq!(doc.record(&changes).unwrap(), 2);
    assert_eq!(doc.deck().unwrap().to_json().unwrap(), text(&renamed));
    let said: Vec<_> =
        doc.changes().into_iter().map(|c| (c.author.unwrap(), c.message.unwrap_or_default(), c.timestamp)).collect();
    assert_eq!(
        said[1..],
        [
            ("user".to_string(), "edit".to_string(), 3_000),
            ("agent:scripted".to_string(), "patch: rename_node, set_text".to_string(), 4_000)
        ]
    );
    // The rename kept the node: its id changed, and its text was edited by character, not
    // written again.
    assert_eq!(doc.changes()[2].ops, 1 + ", again".len());
}

fn at(timestamp: i64) -> Recorded {
    Recorded {
        deck: String::new(),
        author: String::new(),
        message: None,
        timestamp: Some(timestamp),
        renamed_nodes: Vec::new(),
        renamed_states: Vec::new(),
    }
}

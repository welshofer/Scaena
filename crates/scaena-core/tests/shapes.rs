//! Words in a shape (PLAN 2.108): a text whose `at.parent` names a shape is its words. A shape
//! holds one text, which takes its place from the shape; a line or an arrow holds none; and the
//! words go where the shape goes, entering with it and leaving with it.

use scaena_core::validate::{BundleFiles, validate_bundle};
use scaena_core::{Deck, resolve_states};
use serde_json::{Value, json};

/// A bundle of no files: the deck alone.
struct Bare;

impl BundleFiles for Bare {
    fn exists(&self, _: &str) -> bool {
        false
    }

    fn read_text(&self, _: &str) -> Option<String> {
        None
    }
}

/// A deck of `nodes` whose `states` list what each shows, by id: a list of ids is a state's
/// nodes, entered there; `"-id"` takes one away.
fn deck(nodes: Value, states: Value) -> Deck {
    let states: Vec<Value> = (states.as_array().unwrap().iter().enumerate())
        .map(|(i, s)| {
            let (id, mode, ids) = (s["id"].as_str().unwrap(), s["mode"].as_str(), s["shows"].as_array().unwrap());
            let props: serde_json::Map<String, Value> = ids
                .iter()
                .filter_map(Value::as_str)
                .filter(|n| !n.starts_with('-'))
                .map(|n| (n.to_string(), json!({})))
                .collect();
            let remove: Vec<&str> = ids.iter().filter_map(Value::as_str).filter_map(|n| n.strip_prefix('-')).collect();
            let mut state = json!({ "id": id, "props": props, "remove": remove });
            if let Some(mode) = mode.or((i == 0).then_some("absolute")) {
                state["mode"] = json!(mode);
            }
            state
        })
        .collect();
    let doc = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "nodes": nodes,
        "states": states,
    });
    Deck::from_json(&doc.to_string()).unwrap()
}

/// Each finding about where a node is placed, as `code path: message`, sorted.
fn findings(deck: &Deck) -> Vec<String> {
    let json = serde_json::to_string(deck).unwrap();
    let mut out: Vec<String> = (validate_bundle(&json, &Bare).unwrap().into_iter())
        .filter(|f| f.path.as_deref().is_some_and(|p| p.contains("/at/")))
        .map(|f| format!("{} {}: {}", f.code, f.path.unwrap_or_default(), f.message))
        .collect();
    out.sort();
    out
}

fn star() -> Value {
    json!({ "type": "shape", "kind": "polygon", "points": [[0.5, 0], [1, 1], [0, 1]], "at": { "rect": [0, 0, 400, 300] } })
}

#[test]
fn a_shape_holds_one_text_which_it_places() {
    // One state that shows every node but the words, which enter with their shape.
    let one = |nodes: Value| {
        let shows: Vec<&String> = nodes.as_object().unwrap().keys().filter(|k| !k.ends_with("words")).collect();
        findings(&deck(nodes.clone(), json!([{ "id": "a", "shows": shows }])))
    };
    // Words in a shape, which enter with it.
    let ok = one(json!({ "star": star(), "words": { "type": "text", "text": "Hi", "at": { "parent": "star" } } }));
    assert_eq!(ok, Vec::<String>::new());
    // A second text, a picture, a place of its own, and words in a line are each E106.
    let two = one(json!({
        "star": star(),
        "words": { "type": "text", "text": "Hi", "at": { "parent": "star" } },
        "more": { "type": "text", "text": "There", "at": { "parent": "star" } }
    }));
    assert_eq!(two, ["E106 /nodes/more/at/parent: shape `star` holds the words of `words`; a shape holds one text"]);
    let pic = one(json!({ "star": star(), "pic": { "type": "image", "src": "a.png", "at": { "parent": "star" } } }));
    assert_eq!(pic.len(), 1);
    assert!(
        pic[0].starts_with("E106 /nodes/pic/at/parent: node `pic`, an image node, is placed in shape `star`"),
        "{pic:?}"
    );
    let placed = one(
        json!({ "star": star(), "words": { "type": "text", "text": "Hi", "at": { "parent": "star", "rect": [0, 0, 9, 9] } } }),
    );
    assert_eq!(
        placed,
        [
            "E106 /nodes/words/at/rect: node `words` is the words in shape `star`, which places them: take `at.rect` away"
        ]
    );
    let line = one(json!({
        "rule": { "type": "shape", "kind": "line", "at": { "rect": [0, 0, 400, 10] } },
        "words": { "type": "text", "text": "Hi", "at": { "parent": "rule" } }
    }));
    assert_eq!(
        line,
        ["E106 /nodes/words/at/parent: node `words` is placed in `rule`, a line; a line or an arrow holds no words"]
    );
}

#[test]
fn the_words_go_where_their_shape_goes() {
    let nodes = json!({ "star": star(), "words": { "type": "text", "text": "Hi", "at": { "parent": "star" } }, "title": { "type": "text", "text": "T" } });
    // The shape enters on the first slide, stays a step, and leaves; it enters again on the next.
    let d = deck(
        nodes.clone(),
        json!([
            { "id": "one", "shows": ["title", "star"] },
            { "id": "two", "shows": [] },
            { "id": "three", "shows": ["-star"] },
            { "id": "next", "mode": "absolute", "shows": ["star"] }
        ]),
    );
    let shown = |d: &Deck| -> Vec<String> {
        (resolve_states(d).unwrap().iter())
            .map(|s| format!("{}: {}", s.state_id, s.nodes.keys().cloned().collect::<Vec<_>>().join(" ")))
            .collect()
    };
    assert_eq!(shown(&d), ["one: star words title", "two: star words title", "three: title", "next: star words"]);
    assert_eq!(findings(&d), Vec::<String>::new());
    // Taken away on their own, they stay away until their shape enters again.
    let d = deck(
        nodes,
        json!([
            { "id": "one", "shows": ["title", "star"] },
            { "id": "two", "shows": ["-words"] },
            { "id": "three", "shows": [] },
            { "id": "next", "mode": "absolute", "shows": ["star"] }
        ]),
    );
    assert_eq!(shown(&d), ["one: star words title", "two: star title", "three: star title", "next: star words"]);
}

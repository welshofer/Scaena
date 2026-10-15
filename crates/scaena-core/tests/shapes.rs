//! Words in a shape (PLAN 2.108): a text whose `at.parent` names a shape is its words. A shape
//! holds one text, which takes its place from the shape; a line or an arrow holds none; and the
//! words go where the shape goes, entering with it and leaving with it. And connectors (PLAN
//! 2.109): a line or an arrow that joins two nodes, placed by them, which leaves with either.

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

/// Each finding about what a connector joins (PLAN 2.109), as `code path: message`, sorted.
fn joined(deck: &Deck) -> Vec<String> {
    let json = serde_json::to_string(deck).unwrap();
    let mut out: Vec<String> = (validate_bundle(&json, &Bare).unwrap().into_iter())
        .filter(|f| f.path.as_deref().is_some_and(|p| ["/connect", "/at", "/points"].iter().any(|k| p.contains(k))))
        .map(|f| format!("{} {}: {}", f.code, f.path.unwrap_or_default(), f.message))
        .collect();
    out.sort();
    out
}

#[test]
fn a_connector_joins_two_nodes_of_their_own_and_is_placed_by_them() {
    let two = || json!({ "a": { "type": "text", "text": "A" }, "b": { "type": "text", "text": "B" } });
    let one = |more: Value| {
        let mut nodes = two();
        nodes.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        let shows: Vec<String> = nodes.as_object().unwrap().keys().cloned().collect();
        joined(&deck(nodes, json!([{ "id": "s", "shows": shows }])))
    };
    let arrow = |connect: Value| json!({ "type": "shape", "kind": "arrow", "connect": connect });
    assert_eq!(one(json!({ "ab": arrow(json!({ "from": "a", "to": "b" })) })), Vec::<String>::new());
    assert_eq!(one(json!({ "ab": arrow(json!({ "from": "a", "to": "b", "route": "elbow" })) })), Vec::<String>::new());
    assert_eq!(
        one(json!({ "ab": { "type": "shape", "kind": "rect", "connect": { "from": "a", "to": "b" } } })),
        ["E106 /nodes/ab/connect: node `ab`, a rect, joins nothing: a line or an arrow joins two nodes"]
    );
    assert_eq!(
        one(json!({ "ab": arrow(json!({ "from": "a", "to": "zz" })) })),
        ["E102 /nodes/ab/connect/to: node `ab` joins `zz`, which the deck lacks"]
    );
    assert_eq!(
        one(json!({ "ab": arrow(json!({ "from": "a", "to": "a" })) })),
        ["E106 /nodes/ab/connect/to: node `ab` joins a node to itself"]
    );
    assert_eq!(
        one(json!({ "ab": arrow(json!({ "from": "ab", "to": "b" })) })),
        ["E106 /nodes/ab/connect/from: node `ab` joins itself"]
    );
    assert_eq!(
        one(json!({ "ab": arrow(json!({ "from": "a", "to": "b" })), "ba": arrow(json!({ "from": "b", "to": "ab" })) })),
        ["E106 /nodes/ba/connect/to: node `ba` joins `ab`, another connector"]
    );
    assert_eq!(
        one(json!({ "g": { "type": "group" }, "ag": arrow(json!({ "from": "a", "to": "g" })) })),
        ["E106 /nodes/ag/connect/to: node `ag` joins group `g`, which has no box of its own: join one of its members"]
    );
    let mut placed = arrow(json!({ "from": "a", "to": "b" }));
    placed["at"] = json!({ "rect": [0, 0, 9, 9] });
    assert_eq!(
        one(json!({ "ab": placed })),
        ["E106 /nodes/ab/at: node `ab` is placed by what it joins: take `at` away"]
    );
    // What it joins is its own, in every state (E104).
    let mut nodes = two();
    nodes["ab"] = arrow(json!({ "from": "a", "to": "b" }));
    let mut d = deck(nodes, json!([{ "id": "s", "shows": ["a", "b", "ab"] }]));
    d.states[0]
        .props
        .insert("ab".into(), serde_json::from_value(json!({ "connect": { "from": "b", "to": "a" } })).unwrap());
    assert_eq!(
        joined(&d),
        ["E104 /states/0/props/ab/connect: `ab`'s ends are the node's own; a state cannot set `connect`"]
    );
}

/// A line's ends and a curved route (PLAN 2.128), and the paragraphs a text draws (PLAN 2.129):
/// ends only on a line or an arrow, wherever they are written, and paragraphs that run forward.
#[test]
fn ends_are_a_lines_and_paragraphs_run_forward() {
    let found = |nodes: Value, delta: Value| -> Vec<String> {
        let ids: Vec<String> = nodes.as_object().unwrap().keys().cloned().collect();
        let mut d = deck(nodes, json!([{ "id": "s", "shows": ids }]));
        for (id, props) in delta.as_object().unwrap() {
            d.states[0].props.insert(id.clone(), serde_json::from_value(props.clone()).unwrap());
        }
        let json = serde_json::to_string(&d).unwrap();
        let mut out: Vec<String> = (validate_bundle(&json, &Bare).unwrap().into_iter())
            .filter(|f| f.code == "E106")
            .map(|f| format!("{}: {}", f.path.unwrap_or_default(), f.message))
            .collect();
        out.sort();
        out
    };
    let ends = json!({ "start": "circle", "end": "open" });
    let nodes = json!({
        "a": { "type": "text", "text": "A\nB\nC" },
        "b": { "type": "text", "text": "B" },
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "b", "route": "curved" }, "ends": ends },
        "rule": { "type": "shape", "kind": "line", "ends": { "end": "diamond" } }
    });
    let none = Vec::<String>::new();
    assert_eq!(found(nodes.clone(), json!({ "a": { "paragraphs": [1, 2] }, "b": { "paragraphs": 1 } })), none);
    assert_eq!(
        found(nodes.clone(), json!({ "a": { "paragraphs": [3, 2] } })),
        ["/states/0/props/a/paragraphs: `a` draws paragraphs 3 to 2, which run backward: write [2, 3]"]
    );
    let boxed = json!({ "box": { "type": "shape", "kind": "rect", "ends": ends } });
    assert_eq!(
        found(boxed, json!({})),
        ["/nodes/box/ends: `box`, a rect, has no ends to draw: only a line or an arrow has ends"]
    );
    // Written on a text, it is another type's property too.
    let mut words = nodes;
    words["b"]["ends"] = json!({ "end": "arrow" });
    let said = found(words, json!({}));
    assert!(said.iter().any(|f| f.starts_with("/nodes/b/ends: `b`,")), "{said:?}");
}

#[test]
fn a_connector_leaves_wherever_either_end_does() {
    let nodes = json!({
        "a": { "type": "text", "text": "A" },
        "b": { "type": "text", "text": "B" },
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "b" } }
    });
    let d = deck(
        nodes,
        json!([
            { "id": "one", "shows": ["a", "b", "ab"] },
            { "id": "two", "shows": ["-b"] },
            { "id": "three", "shows": ["b"] }
        ]),
    );
    let shown: Vec<String> = (resolve_states(&d).unwrap().iter())
        .map(|s| format!("{}: {}", s.state_id, s.nodes.keys().cloned().collect::<Vec<_>>().join(" ")))
        .collect();
    // It goes with b, and b coming back does not bring it: a state enters it, as any node.
    assert_eq!(shown, ["one: a b ab", "two: a", "three: a b"]);
}

/// Words that entered with their shape (PLAN 2.108) leave by `hide_node` where they entered: the
/// state takes them away, the shape staying.
#[test]
fn words_hidden_where_they_entered_with_their_shape_leave_there() {
    let nodes = json!({ "star": star(), "words": { "type": "text", "text": "Hi", "at": { "parent": "star" } } });
    let d = deck(nodes, json!([{ "id": "a", "shows": ["star"] }, { "id": "b", "shows": [] }]));
    let doc = serde_json::to_value(&d).unwrap();
    let hide = [json!({ "op": "hide_node", "node": "words", "state": "a" })];
    let hidden = scaena_core::patch::compile(&doc, &hide, &Bare).unwrap();
    assert!(!hidden.patch.is_empty(), "a patch that does something");
    let d: Deck = serde_json::from_value(hidden.doc).unwrap();
    let shown: Vec<Vec<String>> =
        resolve_states(&d).unwrap().iter().map(|s| s.nodes.keys().cloned().collect()).collect();
    assert_eq!(shown, [vec!["star".to_string()], vec!["star".to_string()]]);
}

/// Where a shape's words get a property (ADR-0013, PLAN 2.108): a state that takes their shape away
/// takes them with it, so they come back with their own, not with what a state before it gave them.
/// A connector likewise leaves with either end (PLAN 2.109).
#[test]
fn words_that_left_with_their_shape_come_back_with_their_own() {
    use scaena_core::tracking::{Lives, lives};
    let nodes = json!({
        "star": star(),
        "words": { "type": "text", "text": "Hi", "at": { "parent": "star" } },
        "a": { "type": "text", "text": "A" },
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "star" } }
    });
    let mut d = deck(
        nodes,
        json!([
            { "id": "a1", "shows": ["star", "a", "ab"] },
            { "id": "a2", "shows": [] },
            { "id": "b1", "mode": "delta", "shows": ["-star"] },
            { "id": "b2", "mode": "delta", "shows": ["star", "ab"] }
        ]),
    );
    // A step gives the words their text, and the arrow its stroke.
    d.states[1].props.insert("words".into(), serde_json::from_value(json!({ "text": "Step" })).unwrap());
    d.states[1].props.insert("ab".into(), serde_json::from_value(json!({ "stroke": "accent" })).unwrap());
    assert_eq!(lives(&d, 1, "words", "text", &[]), Lives::State(1));
    assert_eq!(lives(&d, 3, "words", "text", &[]), Lives::Node, "the words came back with the star");
    assert_eq!(lives(&d, 3, "ab", "stroke", &[]), Lives::Node, "the arrow came back by a state of its own");
}

/// A connector's ends renamed (PLAN 2.109): `rename_node` renames what each connector joins, and
/// the deck stays valid.
#[test]
fn a_connector_joins_what_its_ends_are_renamed_to() {
    let nodes = json!({
        "a": { "type": "text", "text": "A" },
        "b": { "type": "text", "text": "B" },
        "ab": { "type": "shape", "kind": "arrow", "connect": { "from": "a", "to": "b" } },
        "ba": { "type": "shape", "kind": "line", "connect": { "from": "b", "to": "a" } }
    });
    let d = deck(nodes, json!([{ "id": "s", "shows": ["a", "b", "ab", "ba"] }]));
    let doc = serde_json::to_value(&d).unwrap();
    let renamed = scaena_core::patch::compile(&doc, &[json!({ "op": "rename_node", "id": "a", "to": "z" })], &Bare);
    let renamed = renamed.unwrap().doc;
    assert_eq!(renamed["nodes"]["ab"]["connect"], json!({ "from": "z", "to": "b" }));
    assert_eq!(renamed["nodes"]["ba"]["connect"], json!({ "from": "b", "to": "z" }));
    assert_eq!(joined(&serde_json::from_value(renamed).unwrap()), Vec::<String>::new());
}

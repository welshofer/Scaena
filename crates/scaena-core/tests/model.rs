//! The typed model holds every document in the repository (PLAN 1.1): each node, at rest
//! and in every state that shows it, has its type's view, and the view writes back what it
//! read. So does each theme.

use scaena_core::document::{Deck, Node};
use scaena_core::model::Theme;
use scaena_core::resolve_states;
use serde_json::Value;

const DECKS: &[(&str, &str)] = &[
    ("revenue", include_str!("../../../docs/examples/revenue.deck.json")),
    ("authorability", include_str!("../../../docs/examples/authorability/deck.json")),
    ("torture", include_str!("../../../tests/fixtures/torture.scaena/deck.json")),
    ("b1", include_str!("../../../tests/bench/b1.scaena/deck.json")),
];

const THEMES: &[(&str, &str)] = &[
    ("dusk", include_str!("../../../docs/examples/themes/dusk.theme.json")),
    ("authorability dusk", include_str!("../../../docs/examples/authorability/themes/dusk.theme.json")),
    ("authorability daybreak", include_str!("../../../docs/examples/authorability/themes/daybreak.theme.json")),
    ("torture", include_str!("../../../tests/fixtures/torture.scaena/theme.json")),
    ("b1", include_str!("../../../tests/bench/b1.scaena/theme.json")),
];

/// Every number as an `f64`, so `1` and `1.0` compare equal.
fn numbers_as_floats(v: Value) -> Value {
    match v {
        Value::Number(n) => Value::from(n.as_f64().expect("a finite number")),
        Value::Array(items) => items.into_iter().map(numbers_as_floats).collect(),
        Value::Object(map) => map.into_iter().map(|(k, v)| (k, numbers_as_floats(v))).collect(),
        other => other,
    }
}

#[test]
fn every_node_in_every_state_has_its_types_view() {
    for (name, json) in DECKS {
        let deck = Deck::from_json(json).unwrap();
        for (id, node) in &deck.nodes {
            let typed = node.typed().unwrap_or_else(|e| panic!("{name}: node `{id}`: {e}"));
            assert_eq!(
                numbers_as_floats(serde_json::to_value(&typed).unwrap()),
                numbers_as_floats(serde_json::to_value(node).unwrap()),
                "{name}: node `{id}` writes back other than it reads"
            );
        }
        for snapshot in resolve_states(&deck).unwrap() {
            for (id, props) in &snapshot.nodes {
                let node = Node { node_type: deck.nodes[id].node_type, props: props.clone() };
                node.typed().unwrap_or_else(|e| panic!("{name}: node `{id}` in state `{}`: {e}", snapshot.state_id));
            }
        }
    }
}

#[test]
fn every_theme_has_a_typed_view() {
    for (name, json) in THEMES {
        let value: Value = serde_json::from_str(json).unwrap();
        let theme: Theme = serde_json::from_value(value.clone()).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(
            numbers_as_floats(serde_json::to_value(&theme).unwrap()),
            numbers_as_floats(value),
            "{name} writes back other than it reads"
        );
    }
}

#[test]
fn a_property_of_another_type_has_no_view() {
    let deck = Deck::from_json(DECKS[0].1).unwrap();
    let mut chart = deck.nodes["rev"].clone();
    chart.props.insert("role".into(), "body".into());
    assert!(chart.typed().is_err(), "`role` is a text node's");
}

#[test]
fn an_annotation_stands_where_its_kind_can() {
    use scaena_core::model::values::Annotation;
    let check = |note: Value, donut: bool| {
        let note: Annotation = serde_json::from_value(note).expect("an annotation");
        note.check(donut)
    };
    let ok = |note: Value| assert_eq!(check(note.clone(), false), Ok(()), "{note}");
    let err = |note: Value, donut: bool, says: &str| {
        let message = check(note.clone(), donut).expect_err(&note.to_string());
        assert!(message.contains(says), "{note}: {message}");
    };
    ok(serde_json::json!({ "kind": "rule", "at": { "y": 30 }, "text": "Target" }));
    ok(serde_json::json!({ "kind": "rule", "at": { "x": "Q3" } }));
    ok(serde_json::json!({ "kind": "band", "at": { "x": ["Q2", "Q3"] } }));
    ok(serde_json::json!({ "kind": "band", "at": { "y": [10, 20] }, "text": "Range" }));
    ok(serde_json::json!({ "kind": "callout", "at": { "x": "Q4", "series": "Cloud" }, "text": "Record" }));
    ok(serde_json::json!({ "kind": "callout", "at": { "x": 2.5, "y": 10 }, "text": "Here" }));
    ok(serde_json::json!({ "kind": "highlight", "at": { "x": ["Q1", "Q2"], "series": "Cloud" } }));
    err(serde_json::json!({ "kind": "rule", "at": { "y": "high" } }), false, "a number");
    err(serde_json::json!({ "kind": "rule", "at": {} }), false, "`at.x` or `at.y`");
    err(serde_json::json!({ "kind": "rule", "at": { "y": [1, 2] } }), false, "one value");
    err(serde_json::json!({ "kind": "rule", "at": { "y": 1, "series": "A" } }), false, "no `at.series`");
    err(serde_json::json!({ "kind": "band", "at": { "x": ["Q1", "Q2", "Q3"] } }), false, "from one value to another");
    err(serde_json::json!({ "kind": "callout", "at": { "x": "Q4" } }), false, "give it `text`");
    err(serde_json::json!({ "kind": "callout", "at": { "y": 3 }, "text": "?" }), false, "stands at `at.x`");
    err(
        serde_json::json!({ "kind": "callout", "at": { "x": "Q4", "y": 3, "series": "A" }, "text": "?" }),
        false,
        "not both",
    );
    err(serde_json::json!({ "kind": "callout", "at": { "x": ["Q1", "Q2"] }, "text": "?" }), false, "one place");
    err(serde_json::json!({ "kind": "highlight", "at": {} }), false, "`at.x`, `at.series`, or both");
    err(serde_json::json!({ "kind": "highlight", "at": { "y": 3 } }), false, "no `at.y`");
    // A donut has slices: no axes, no series.
    assert_eq!(check(serde_json::json!({ "kind": "highlight", "at": { "x": "Direct" } }), true), Ok(()));
    err(serde_json::json!({ "kind": "rule", "at": { "y": 3 } }), true, "a donut has no axes");
    err(serde_json::json!({ "kind": "highlight", "at": { "series": "A" } }), true, "a donut has no series");
    // Unknown places break the schema.
    assert!(serde_json::from_value::<Annotation>(serde_json::json!({ "kind": "rule", "at": { "z": 1 } })).is_err());
}

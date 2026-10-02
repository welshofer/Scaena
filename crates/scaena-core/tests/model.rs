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

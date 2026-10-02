//! The typography torture deck (PLAN 0.2, benchmark B4) is a valid, lint-clean
//! document whose states are isolated cases, so the parity harness (PLAN 0.9)
//! can report per case without one case's nodes leaking into another. The one
//! exception is the chart's build (PLAN 0.10): its states track from the one before,
//! because the transitions between them are what it tests.

use scaena_core::document::StateMode;
use scaena_core::lint::lint_document;
use scaena_core::{Deck, resolve_states};
use std::collections::{BTreeSet, HashSet};

const TORTURE: &str = include_str!("../../../tests/fixtures/torture.scaena/deck.json");

#[test]
fn torture_deck_is_valid_and_lint_clean() {
    let deck = Deck::from_json(TORTURE).expect("torture deck parses");
    let findings = lint_document(&deck);
    assert!(findings.is_empty(), "{findings:#?}");
}

/// States that track from the state before them, as (state, that state): the chart's
/// build, where the transitions are what is tested.
const MORPHS: [(&str, &str); 2] = [("chart", "chart-intro"), ("chart-next", "chart")];

#[test]
fn every_torture_state_is_an_isolated_case() {
    let deck = Deck::from_json(TORTURE).unwrap();
    let snapshots = resolve_states(&deck).unwrap();
    assert_eq!(snapshots.len(), 28);
    let mut specimens_seen = HashSet::new();
    for (i, (state, snap)) in deck.states.iter().zip(&snapshots).enumerate() {
        if let Some((_, from)) = MORPHS.iter().find(|(s, _)| *s == state.id) {
            assert_eq!((state.mode, deck.states[i - 1].id.as_str()), (StateMode::Delta, *from), "{}", state.id);
            let before = &snapshots[i - 1].nodes;
            assert!(before.keys().all(|k| snap.nodes.contains_key(k)), "{}: builds on `{from}`", state.id);
            continue;
        }
        assert_eq!(state.mode, StateMode::Absolute, "{} must not track from the previous case", state.id);
        assert_eq!(snap.layout.as_deref(), Some("specimen"), "{}", state.id);
        let visible: BTreeSet<&str> = snap.nodes.keys().map(String::as_str).collect();
        let declared: BTreeSet<&str> = state.props.keys().map(String::as_str).collect();
        assert_eq!(visible, declared, "{}: only the case label and its own specimens are visible", state.id);
        assert!(visible.contains("case"), "{} has no case label", state.id);
        for id in declared.iter().filter(|id| **id != "case") {
            assert!(specimens_seen.insert(*id), "specimen `{id}` appears in more than one case");
        }
    }
}

//! The typography torture deck (PLAN 0.2, benchmark B4) is a valid document whose
//! states are isolated cases, so the parity harness (PLAN 0.9)
//! can report per case without one case's nodes leaking into another. The exceptions
//! are the chart's build (PLAN 0.10), the chart gallery's motion (PLAN 1.9), and the
//! morphs (PLAN 1.12): those states track from the one before, because the transitions
//! between them are what they test.

use scaena_core::document::StateMode;
use scaena_core::lint::{Severity, lint_document};
use scaena_core::{Deck, resolve_states};
use std::collections::{BTreeSet, HashSet};

const TORTURE: &str = include_str!("../../../tests/fixtures/torture.scaena/deck.json");
const THEME: &str = include_str!("../../../tests/fixtures/torture.scaena/theme.json");

/// Valid, so every case renders. Its document-level warnings are its cases' (a deck laid
/// out in 9:16 for one case places the rest by grid cells, W302); the CLI's lint golden
/// (`tests/golden/lint/torture.txt`) holds every finding, layout's included.
#[test]
fn torture_deck_is_valid() {
    let deck = Deck::from_json(TORTURE).expect("torture deck parses");
    let theme = serde_json::from_str(THEME).expect("torture theme parses");
    let errors: Vec<_> =
        lint_document(&deck, Some(&theme)).into_iter().filter(|f| f.severity == Severity::Error).collect();
    assert!(errors.is_empty(), "{errors:#?}");
}

/// States that track from the state before them, as (state, that state): the chart's
/// build and the gallery's next states, where the transitions are what is tested.
const MORPHS: [(&str, &str); 8] = [
    ("chart", "chart-intro"),
    ("chart-next", "chart"),
    ("chart-kinds-next", "chart-kinds"),
    ("chart-kinds-2-next", "chart-kinds-2"),
    ("regroup-stacked", "regroup"),
    ("annotations-next", "annotations"),
    ("morph", "morph-from"),
    ("forecast-next", "forecast"),
];

#[test]
fn every_torture_state_is_an_isolated_case() {
    let deck = Deck::from_json(TORTURE).unwrap();
    let snapshots = resolve_states(&deck).unwrap();
    assert_eq!(snapshots.len(), 49);
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

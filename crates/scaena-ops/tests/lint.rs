//! A lint of one state (PLAN 2.3), the editor's answer while the state is being edited:
//! on every torture state, it finds in that state what the whole lint finds there, and
//! more only where the whole lint reports the same thing once, at another state: where a
//! collision starts, or where text reads worst. What a finding measures across states
//! (every state it holds in) counts only the states laid out.

use scaena_core::Finding;
use scaena_ops::lint::{View, lint_in, lint_state_in};
use std::path::Path;

const TORTURE: &str = "../../tests/fixtures/torture.scaena";

/// What a finding is about.
fn what(f: &Finding) -> String {
    format!("{} {:?} {:?} {:?}", f.code, f.format, f.node, f.path)
}

/// What it is about and what it says there.
fn said(f: &Finding) -> String {
    format!("{} {}", what(f), f.message)
}

#[test]
fn a_state_alone_lints_as_it_does_in_the_whole_deck() {
    let b = scaena_ops::open(Path::new(TORTURE)).unwrap();
    let files = View::of(&b);
    let whole = lint_in(&b.deck, &files).unwrap();
    assert!(whole.laid);
    let order: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let at = |f: &Finding| f.state.as_deref().and_then(|s| order.iter().position(|o| *o == s));
    for (i, state) in order.iter().enumerate() {
        let one = lint_state_in(&b.deck, &files, state).unwrap();
        assert!(one.laid, "{state}");
        let here = |l: &[Finding]| -> Vec<Finding> {
            l.iter().filter(|f| f.state.is_none() || f.state.as_deref() == Some(state)).cloned().collect()
        };
        let (mine, theirs) = (here(&one.findings), here(&whole.findings));
        for f in &theirs {
            let found = mine.iter().any(|g| said(g) == said(f));
            assert!(found, "{state}: the whole lint finds {} here, and the state alone does not", said(f));
        }
        for f in mine.iter().filter(|f| !theirs.iter().any(|g| said(g) == said(f))) {
            let elsewhere = whole.findings.iter().any(|g| what(g) == what(f) && at(g).is_some_and(|j| j != i));
            assert!(elsewhere, "{state}: alone it finds {}, which the whole lint does not", said(f));
        }
    }
}

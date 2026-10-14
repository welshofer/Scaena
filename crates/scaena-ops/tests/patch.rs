//! A patch made in an editor (SPEC §7.3, ADR-0021): checked and refused as any patch is, and
//! linted on the states it reaches, each it changes and each whose cue starts from one, so an
//! edit costs what those states cost however long the deck. What it finds there is what the
//! whole deck's lint finds there.

use scaena_core::Finding;
use scaena_ops::lint::{View, data_in, engine_in, layout_rules_on};
use scaena_ops::patch::{editing, patching};
use serde_json::{Value, json};
use std::path::Path;

const HIGHER_ED: &str = "../../docs/examples/higher-ed.deck.json";

/// `ops` made on `b` as an editor makes them: the states each lint was asked to lay out, before
/// the patch and after, with what it did.
fn edited(b: &scaena_ops::Bundle, ops: &Value) -> (scaena_ops::patch::Patched, Vec<Vec<String>>) {
    let mut asked = Vec::new();
    let layout = |deck: &scaena_core::Deck, theme: &scaena_engine::theme::Theme, states: &[&str]| {
        asked.push(states.iter().map(|s| s.to_string()).collect());
        let files = View::of(b);
        let mut assets = scaena_paint::Assets::new();
        let mut engine = engine_in(deck, &files, theme, Some(&mut assets))?;
        layout_rules_on(&mut engine, deck, theme, &data_in(deck, &files)?, &assets, states)
    };
    let (patched, _) = editing(b, ops, None, None, layout).unwrap();
    (patched, asked)
}

fn said(findings: &[Finding]) -> Vec<String> {
    findings.iter().map(|f| format!("{} {:?} {:?} {}", f.code, f.state, f.node, f.message)).collect()
}

/// A move on the cover of a deck of 38 states lays out the cover and the state whose cue starts
/// from it, before and after, and no other; and what it adds is what the whole deck's lint adds.
#[test]
fn an_edit_lints_the_states_it_reaches_and_finds_what_the_whole_lint_finds_there() {
    let b = scaena_ops::open(Path::new(HIGHER_ED)).unwrap();
    assert_eq!(b.deck.states.len(), 38);
    let (first, second) = (b.deck.states[0].id.clone(), b.deck.states[1].id.clone());
    // The cover's subtitle set in a word too long for its box: E100 there, and nowhere else.
    let ops = json!([{ "op": "set_text", "node": "cover-sub", "text": "W".repeat(400) }]);
    let (scoped, asked) = edited(&b, &ops);
    assert_eq!(asked, [vec![first.clone(), second.clone()], vec![first.clone(), second.clone()]]);
    assert!(scoped.applied && scoped.states == [first.clone()], "{:?}", scoped.states);
    let (whole, _) = patching(&b, &ops, None).unwrap();
    assert!(said(&whole.added).iter().any(|f| f.starts_with("E100")), "{:?}", said(&whole.added));
    assert_eq!(said(&scoped.added), said(&whole.added));
    assert_eq!(said(&scoped.removed), said(&whole.removed));

    // A patch that would make the deck invalid is refused as before, and lays nothing out.
    let refused = json!([{ "op": "choose", "node": "cover-art", "prop": "src", "value": "assets/nowhere.png" }]);
    let (scoped, asked) = edited(&b, &refused);
    assert!(scoped.refused && !scoped.applied, "{scoped:?}");
    assert!(asked.is_empty());
    assert!(said(&scoped.added).iter().any(|f| f.starts_with("E102")), "{:?}", said(&scoped.added));
}

/// What every state is drawn from, the deck's furniture here, reaches every state.
#[test]
fn a_change_to_what_every_state_is_drawn_from_reaches_every_state() {
    let b = scaena_ops::open(Path::new(HIGHER_ED)).unwrap();
    let (_, asked) = edited(&b, &json!([{ "op": "furnish", "footer": "The State of Higher Education" }]));
    assert_eq!(asked.len(), 2);
    assert!(asked.iter().all(|states| states.len() == 38), "{:?}", asked.iter().map(Vec::len).collect::<Vec<_>>());
}

/// A state taken away is judged before the patch, and the state after it, whose cue now starts
/// from another, before and after.
#[test]
fn a_state_removed_is_judged_before_and_the_state_after_it_on_both_sides() {
    let b = scaena_ops::open(Path::new(HIGHER_ED)).unwrap();
    let ids: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let at = ids.iter().position(|s| *s == "era-4").unwrap();
    let (gone, next) = (ids[at], ids[at + 1]);
    let (patched, asked) = edited(&b, &json!([{ "op": "remove_state", "id": gone }]));
    assert!(patched.applied);
    assert!(asked[0].contains(&gone.to_string()) && asked[0].contains(&next.to_string()), "{:?}", asked[0]);
    assert!(asked[1].contains(&next.to_string()) && !asked[1].contains(&gone.to_string()), "{:?}", asked[1]);
    assert!(asked.iter().all(|states| states.len() < 5), "{asked:?}");
}

/// Whether a deck runs on its own is the deck's: a state an edit leaves showing for no time in a
/// deck whose other states hold is W323 in what the edit adds, as the whole deck's lint finds it,
/// though the edit lays out that state alone.
#[test]
fn a_state_an_edit_leaves_showing_for_no_time_is_found_where_the_whole_lint_finds_it() {
    let b = scaena_ops::open(Path::new("../../docs/examples/revenue.deck.json")).unwrap();
    let ops = json!([{ "op": "set_state", "id": "close", "prop": "hold", "value": null }]);
    let (scoped, asked) = edited(&b, &ops);
    assert!(asked.iter().all(|states| states == &["close"]), "{asked:?}");
    let (whole, _) = patching(&b, &ops, None).unwrap();
    assert!(said(&whole.added).iter().any(|f| f.starts_with("W323")), "{:?}", said(&whole.added));
    assert_eq!(said(&scoped.added), said(&whole.added));
}

/// A hold given to a deck that held nowhere makes it run on its own, which every state is judged
/// by: the edit reaches every state, and each other that shows for no time is W323 in what it
/// adds, as the whole deck's lint finds it.
#[test]
fn a_hold_that_makes_the_deck_run_on_its_own_reaches_every_state() {
    let b = scaena_ops::open(Path::new("../../docs/examples/revenue.deck.json")).unwrap();
    let unheld: Vec<Value> = (b.deck.states.iter())
        .map(|s| json!({ "op": "set_state", "id": s.id, "prop": "hold", "value": null }))
        .collect();
    let (_, write) = patching(&b, &Value::Array(unheld), None).unwrap();
    let live = scaena_ops::Bundle { deck: write.unwrap().deck, ..b.clone() };
    let ops = json!([{ "op": "set_state", "id": "intro", "prop": "hold", "value": 4000 }]);
    let (scoped, asked) = edited(&live, &ops);
    assert!(asked.iter().all(|states| states.len() == live.deck.states.len()), "{asked:?}");
    let (whole, _) = patching(&live, &ops, None).unwrap();
    assert!(
        said(&whole.added).iter().any(|f| f.starts_with("W323") && f.contains("close")),
        "{:?}",
        said(&whole.added)
    );
    assert_eq!(said(&scoped.added), said(&whole.added));
}

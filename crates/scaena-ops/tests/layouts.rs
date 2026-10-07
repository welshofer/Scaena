//! The layouts an editor suggests for a state (PLAN 2.92), on the trails example: each of the
//! theme's layouts the state may take, judged by lint with its patch made and drawn, best first,
//! those that draw it alike folded into one.

use scaena_core::Severity;
use scaena_ops::layouts::suggest;
use scaena_ops::lint::{View, lint_state_in};
use serde_json::json;
use std::path::Path;

const TRAILS: &str = "../../docs/examples/trails.deck.json";

#[test]
fn a_state_is_offered_each_layout_that_draws_it_otherwise_judged_by_lint_best_first() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    // The budget slide places its note in a slot, which four of the theme's layouts have: three
    // set it at the slide's foot, where it reads alike, and narrow-figure over the table and the
    // aside, where they collide (E101).
    let suggested = suggest(&b, "budget", None).unwrap();
    let names: Vec<&str> = suggested.iter().map(|s| s.layout.as_str()).collect();
    assert_eq!(names, ["figure", "narrow-figure"], "{suggested:#?}");
    // The layout it takes now first, with nothing to patch, and those it draws alike beside it.
    let now = &suggested[0];
    assert!(now.current && now.patch.is_empty() && now.reach.is_empty(), "{now:?}");
    assert_eq!(now.alike, ["poster", "full"]);
    // The other is one `set_state`, changes this state alone, and makes the slide's errors.
    let other = &suggested[1];
    assert!(!other.current && other.alike.is_empty() && other.errors > 0, "{other:?}");
    assert_eq!(other.patch, [json!({ "op": "set_state", "id": "budget", "prop": "layout", "value": "narrow-figure" })]);
    assert_eq!(other.reach, ["budget"]);
    // Each count is what lint finds in the state with the patch made, the folded ones' too.
    let files = View::of(&b);
    let counted = |layout: &str| {
        let patch = [json!({ "op": "set_state", "id": "budget", "prop": "layout", "value": layout })];
        let doc = scaena_core::patch::compile(&b.deck.to_value().unwrap(), &patch, &files).unwrap().doc;
        let deck = scaena_core::Deck::from_value(&doc).unwrap();
        let found = lint_state_in(&deck, &files, "budget").unwrap().findings;
        let count =
            |severity| found.iter().filter(|f| f.state.as_deref() == Some("budget") && f.severity == severity).count();
        (count(Severity::Error), count(Severity::Warning))
    };
    for s in &suggested {
        assert_eq!((s.errors, s.warnings), counted(&s.layout), "{}", s.layout);
        for like in &s.alike {
            assert_eq!(counted(like), (s.errors, s.warnings), "{like} is judged as {} is", s.layout);
        }
    }
}

#[test]
fn a_clean_layout_that_draws_the_state_otherwise_is_suggested() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    // The storm slide's caption is its one node in a slot: three layouts set it at the foot of the
    // photo, alike, and narrow-figure higher up and to the right, where lint finds nothing.
    let suggested = suggest(&b, "storm", None).unwrap();
    let judged: Vec<_> = suggested.iter().map(|s| (s.layout.as_str(), s.current, s.errors, s.warnings)).collect();
    assert_eq!(judged, [("full", true, 0, 0), ("narrow-figure", false, 0, 0)], "{suggested:#?}");
    assert_eq!(suggested[0].alike, ["poster", "figure"]);
}

#[test]
fn a_state_every_layout_draws_alike_is_offered_the_one_it_takes() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    // The agenda's kicker is the one node in a slot, and each layout puts it in the same place.
    let suggested = suggest(&b, "agenda", None).unwrap();
    assert_eq!(suggested.len(), 1, "{suggested:#?}");
    assert!(suggested[0].current && suggested[0].layout == "full");
    assert_eq!(suggested[0].alike, ["figure", "narrow-figure", "art-left", "art-right"]);
    // As the cover is, which one layout holds.
    let cover = suggest(&b, "cover", None).unwrap();
    assert_eq!(cover.len(), 1, "{cover:#?}");
    assert!(cover[0].current && cover[0].layout == "title" && cover[0].alike.is_empty());
}

#[test]
fn a_state_that_places_no_node_in_a_slot_is_offered_none() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    // The before and after cards stand on the grid: every layout would draw them alike.
    assert_eq!(suggest(&b, "change", None).unwrap(), []);
}

#[test]
fn inspect_names_the_state_it_suggests_layouts_for() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    let views = scaena_ops::inspect::Views { layouts: true, ..Default::default() };
    let err = scaena_ops::inspect::inspect(&b, None, views.clone()).unwrap_err();
    assert!(err.message.contains("name the state"), "{}", err.message);
    let inspected = scaena_ops::inspect::inspect(&b, Some("budget"), views).unwrap();
    assert_eq!(inspected[0].layouts.as_ref().map(Vec::len), Some(2));
}

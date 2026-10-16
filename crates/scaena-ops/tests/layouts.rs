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
    // The budget slide places its note in a slot, which a dozen of the theme's layouts have: five
    // set it at the slide's foot, where it reads alike; the rest elsewhere, where lint finds the
    // table and the note colliding (E101), fewest first: narrow-figure sets the note over the table
    // and the aside.
    let suggested = suggest(&b, "budget", None).unwrap();
    let names: Vec<&str> = suggested.iter().map(|s| s.layout.as_str()).collect();
    assert_eq!(
        names,
        [
            "figure",
            "venn",
            "risk-matrix",
            "two-figures",
            "bar-chart",
            "line-chart",
            "metric-trend",
            "panorama",
            "narrow-figure"
        ],
        "{suggested:#?}"
    );
    // The layout it takes now first, with nothing to patch, and those it draws alike beside it.
    let now = &suggested[0];
    assert!(now.current && now.patch.is_empty() && now.reach.is_empty(), "{now:?}");
    assert_eq!(now.alike, ["poster", "table", "forecast", "full", "photo-caption"]);
    // Each other is one `set_state`, changes this state alone, and makes the slide's errors.
    // Risk Matrix and Positioning Map set the note beside what the slide holds, alike, and
    // Pictograph sets it under its figures as Two Figures does, and Map sets its headline and
    // its note where Two Figures does (PLAN O.1e).
    assert_eq!(suggested[2].alike, ["positioning-map"]);
    assert_eq!(suggested[3].layout, "two-figures");
    assert_eq!(suggested[3].alike, ["pictograph", "map"]);
    for other in &suggested[1..] {
        assert!(!other.current && other.errors > 0, "{other:?}");
        assert!(other.alike.is_empty() || ["risk-matrix", "two-figures"].contains(&other.layout.as_str()), "{other:?}");
        assert_eq!(
            other.patch,
            [json!({ "op": "set_state", "id": "budget", "prop": "layout", "value": other.layout })]
        );
        assert_eq!(other.reach, ["budget"]);
    }
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
    // The storm slide's caption is its one node in a slot: five layouts set it at the foot of the
    // photo, alike, and the rest elsewhere: four where lint finds nothing, two-figures under its
    // figures and narrow-figure higher up and to the right among them, then those it finds
    // something in.
    let suggested = suggest(&b, "storm", None).unwrap();
    let judged: Vec<_> = suggested.iter().map(|s| (s.layout.as_str(), s.current, s.errors, s.warnings)).collect();
    assert_eq!(
        judged,
        [
            ("full", true, 0, 0),
            ("two-figures", false, 0, 0),
            ("metric-trend", false, 0, 0),
            ("narrow-figure", false, 0, 0),
            ("panorama", false, 0, 0),
            ("venn", false, 0, 1),
            ("bar-chart", false, 1, 0),
            ("line-chart", false, 1, 0),
            ("risk-matrix", false, 2, 0)
        ],
        "{suggested:#?}"
    );
    assert_eq!(suggested[0].alike, ["poster", "figure", "table", "forecast", "photo-caption"]);
}

#[test]
fn the_layouts_that_draw_a_state_alike_fold_into_the_one_it_takes() {
    let b = scaena_ops::open(Path::new(TRAILS)).unwrap();
    // The agenda's header is the one node in a slot. Most layouts put it in the same place, folded
    // into the one the agenda takes; five put it elsewhere: beside a picture off either edge, as
    // `photo-grid-text` does `photo-left`'s, over a map across the slide, and beside an agenda's
    // topics, with a picture or without (PLAN 3.30, 3.31, O.1e).
    let suggested = suggest(&b, "agenda", None).unwrap();
    let names: Vec<&str> = suggested.iter().map(|s| s.layout.as_str()).collect();
    assert_eq!(names, ["full", "agenda-photo", "photo-left", "map-full", "photo-right", "agenda"], "{suggested:#?}");
    assert!(suggested[0].current && suggested[1..].iter().all(|s| !s.current));
    assert_eq!(suggested[2].alike, ["photo-grid-text"]);
    assert_eq!(suggested[0].alike[..4], ["title-only", "manifesto", "bullets", "numbered"]);
    assert!(
        ["cards", "three-up", "number-grid", "swot", "org-chart"]
            .iter()
            .all(|l| suggested[0].alike.iter().any(|a| a == l))
    );
    // The cover's title and subtitle: in `title`, over a picture in `photo-title`, centered in
    // `title-center`, and beside a picture in `title-split`.
    let cover = suggest(&b, "cover", None).unwrap();
    let names: Vec<(&str, bool)> = cover.iter().map(|s| (s.layout.as_str(), s.current)).collect();
    assert_eq!(
        names,
        [("title", true), ("photo-title", false), ("title-center", false), ("title-split", false)],
        "{cover:#?}"
    );
    assert!(cover.iter().all(|s| s.alike.is_empty()));
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
    assert_eq!(inspected[0].layouts.as_ref().map(Vec::len), Some(9));
}

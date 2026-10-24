//! Text fit policies (SPEC §3.4, PLAN 1.8): `shrink` and `grow` find the largest size that
//! fits the box between the text's bounds, `clip` cuts it to the box, and `error` refuses
//! to draw it. Each reports whether the text, as set, overflows (E100 and W203 read this).

use scaena_core::Deck;
use scaena_core::displaylist::Op;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest, PlacedText};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck's fonts and theme with one state, `t`, showing `nodes`.
fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "t", "layout": "specimen", "props": props }]);
    d["data"] = json!({});
    serde_json::from_value(d).unwrap()
}

fn engine(deck: &Deck) -> Engine {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    Engine::new(fonts)
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn placed(deck: &Deck, node: &str) -> PlacedText {
    let (theme, data) = (theme(), DataFiles::new());
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    engine(deck).text_layout(&req, node).unwrap()
}

/// The trimmed height of the text as set: what must fit the box.
fn height(p: &PlacedText) -> f32 {
    let lines = &p.text.lines;
    lines.last().map_or(0.0, |l| l.top + l.height) - lines.first().map_or(0.0, |l| l.top)
}

const LONG: &str = "A headline that says far too much for the little box it was given on this slide";

#[test]
fn shrink_sets_the_text_at_the_largest_size_that_fits() {
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "text": LONG,
                                "at": { "col": [1, 4], "row": 2 } } }));
    let p = placed(&d, "n");
    assert!(p.scale < 1.0 && p.scale >= 0.5 && !p.overflow, "{} {}", p.scale, p.overflow);
    assert!(height(&p) <= p.cell[3] + 1e-3);
    // A taller box keeps more of the size.
    let roomier = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "text": LONG,
                                      "at": { "col": [1, 4], "row": [2, 3] } } }));
    assert!(placed(&roomier, "n").scale > p.scale);
}

/// A text that shrinks is held under its own `maxSize` (deck format 0.36), so a set of them side
/// by side reads at one size: a short one that fits at its size is set at the cap. A deck saved
/// before that reads without the cap and draws as it did.
#[test]
fn shrink_is_held_under_the_texts_own_max_size_from_format_0_36() {
    let saved = |version: &str| {
        let mut d = serde_json::to_value(deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink",
            "maxSize": 30, "text": "Short", "at": { "col": [1, 4], "row": [2, 3] } } })))
        .unwrap();
        d["scaena"] = json!(version);
        Deck::from_json(&d.to_string()).unwrap()
    };
    let capped = placed(&saved(scaena_core::FORMAT_VERSION), "n");
    assert_eq!(capped.scale, 30.0 / 40.0, "body is 40; maxSize 30");
    assert!(!capped.overflow);
    let before = placed(&saved("0.35"), "n");
    assert_eq!(before.scale, 1.0, "as it drew");
    // A long one that fits only smaller than the cap is set smaller, as before.
    let long = |max: f64| {
        deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "maxSize": max, "text": LONG,
                            "at": { "col": [1, 4], "row": 2 } } }))
    };
    let (under, free) = (placed(&long(39.0), "n").scale, placed(&long(400.0), "n").scale);
    assert!(free < 39.0 / 40.0 && (under - free).abs() < 1e-3, "{under} {free}");
}

#[test]
fn shrink_stops_at_the_minimum_size_and_reports_the_overflow() {
    let text = LONG.repeat(4);
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "minSize": 30, "text": text,
                                "at": { "col": [1, 4], "row": 2 } } }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 30.0 / 40.0, "body is 40; minSize 30");
    assert!(p.overflow, "still too tall at its minimum: W203");
}

/// A text placed in a slot that shrinks what is set in it (theme format 0.13), as a number too
/// wide for a Stat's slot is, shrinks as one whose own `fit` says so; a text that says how it
/// fits keeps its own.
#[test]
fn a_slot_that_shrinks_what_is_set_in_it_shrinks_a_text_that_does_not_say() {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["layouts"]["specimen"]["slots"]["narrow"] = json!({ "col": [1, 4], "row": 2, "fit": "shrink" });
    let theme = Theme::from_json(&t.to_string()).unwrap();
    let at = |d: &Deck, node: &str| {
        let data = DataFiles::new();
        let req = FrameRequest { deck: d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
        engine(d).text_layout(&req, node).unwrap()
    };
    let d = deck(json!({
        "n": { "type": "text", "role": "body", "text": LONG, "at": { "in": "narrow" } },
        "w": { "type": "text", "role": "body", "fit": "wrap", "text": LONG, "at": { "in": "narrow" } },
    }));
    let shrunk = at(&d, "n");
    assert!(shrunk.scale < 1.0 && !shrunk.overflow, "{} {}", shrunk.scale, shrunk.overflow);
    let own = at(&d, "w");
    assert!(own.scale == 1.0 && own.overflow, "its own `wrap` wins: {} {}", own.scale, own.overflow);
}

#[test]
fn shrink_also_holds_the_text_to_max_lines() {
    let text = "Three short lines of body text";
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "maxLines": 1, "text": text,
                                "at": { "col": [1, 3], "row": [2, 8] } } }));
    let p = placed(&d, "n");
    assert_eq!(p.text.lines.len(), 1, "{}", p.scale);
    assert!(p.scale < 1.0 && !p.overflow);
}

#[test]
fn grow_sets_the_text_at_the_largest_size_that_still_fits() {
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "grow", "maxSize": 400, "text": "Grow",
                                "at": { "col": [1, 6], "row": [2, 4] } } }));
    let p = placed(&d, "n");
    assert!(p.scale > 1.0 && p.scale < 10.0 && !p.overflow, "{}", p.scale);
    assert_eq!(p.text.lines.len(), 1);
    // Bounded by maxSize when the box would take more.
    let capped = deck(json!({ "n": { "type": "text", "role": "body", "fit": "grow", "maxSize": 60, "text": "Grow",
                                     "at": { "col": [1, 6], "row": [2, 4] } } }));
    assert_eq!(placed(&capped, "n").scale, 1.5);
}

#[test]
fn clip_cuts_the_text_to_its_box_and_error_refuses_it() {
    let at = json!({ "col": [1, 3], "row": 2 });
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "clip", "text": LONG, "at": at } }));
    let p = placed(&d, "n");
    assert!(p.overflow && p.clip == Some(p.cell), "{:?}", p.clip);
    let (theme, data) = (theme(), DataFiles::new());
    let req = FrameRequest { deck: &d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    let dl = engine(&d).frame(&req).unwrap().display_list;
    let clipped = dl.ops.iter().any(|op| matches!(op, Op::Layer { node: Some(n), clip: Some(_), .. } if n == "n"));
    assert!(clipped, "the text's layer carries the clip");

    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "error", "text": LONG, "at": at } }));
    let req = FrameRequest { deck: &d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    let err = engine(&d).frame(&req).unwrap_err().to_string();
    assert!(err.contains("node `n`") && err.contains("fit: error"), "{err}");
}

/// Layout and lint read one fit (PLAN 2.131, ADR-0029): a text's own, else its slot's, else
/// `shrink` for a shape's words and for code, else `wrap`. A slot's shrinking text and a shape's
/// words that do not fit at their smallest are W203, as a text that says `shrink` is, not E100.
#[test]
fn lint_and_layout_read_one_fit() {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    t["layouts"]["specimen"]["slots"]["narrow"] = json!({ "col": [1, 4], "row": 2, "fit": "shrink" });
    let theme = Theme::from_json(&t.to_string()).unwrap();
    let long = LONG.repeat(4);
    let d = deck(json!({
        "slotted": { "type": "text", "role": "body", "text": long, "at": { "in": "narrow" } },
        "card": { "type": "shape", "kind": "rect", "at": { "col": [6, 7], "row": 2 } },
        "words": { "type": "text", "role": "body", "text": long, "at": { "parent": "card" } },
        "wraps": { "type": "text", "role": "body", "text": long, "at": { "col": [9, 12], "row": 2 } },
    }));
    let data = DataFiles::new();
    let req = FrameRequest { deck: &d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    let laid = engine(&d).text_layouts(&req, &["slotted", "words", "wraps"]).unwrap();
    let fits: Vec<_> = laid.iter().map(|p| (p.fit, p.overflow)).collect();
    use scaena_core::model::nodes::TextFit;
    assert_eq!(fits, [(TextFit::Shrink, true), (TextFit::Shrink, true), (TextFit::Wrap, true)]);
    let found = scaena_engine::lint::lint(&mut engine(&d), &d, &theme, &data, None).unwrap();
    // In the deck's own format: the torture deck lays out in 9:16 too.
    let said = |node: &str| {
        let ours = found.iter().filter(|f| f.node.as_deref() == Some(node) && f.format.is_none());
        let mut codes: Vec<&str> = ours.map(|f| f.code.as_str()).collect();
        codes.retain(|c| ["E100", "W203"].contains(c));
        codes
    };
    assert_eq!(said("slotted"), ["W203"]);
    assert_eq!(said("words"), ["W203"]);
    assert_eq!(said("wraps"), ["E100"]);
}

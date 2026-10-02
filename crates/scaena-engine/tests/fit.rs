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
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY };
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

#[test]
fn shrink_stops_at_the_minimum_size_and_reports_the_overflow() {
    let text = LONG.repeat(4);
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "minSize": 30, "text": text,
                                "at": { "col": [1, 4], "row": 2 } } }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 30.0 / 40.0, "body is 40; minSize 30");
    assert!(p.overflow, "still too tall at its minimum: W203");
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
    let req = FrameRequest { deck: &d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY };
    let dl = engine(&d).frame(&req).unwrap().display_list;
    let clipped = dl.ops.iter().any(|op| matches!(op, Op::Layer { node: Some(n), clip: Some(_), .. } if n == "n"));
    assert!(clipped, "the text's layer carries the clip");

    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "error", "text": LONG, "at": at } }));
    let req = FrameRequest { deck: &d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY };
    let err = engine(&d).frame(&req).unwrap_err().to_string();
    assert!(err.contains("node `n`") && err.contains("fit: error"), "{err}");
}

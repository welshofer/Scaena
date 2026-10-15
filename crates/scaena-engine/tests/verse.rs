//! Verse set as written (PLAN O.4b): a text that wraps `none` breaks its lines only where it
//! breaks them, and under `fit: shrink` a line too wide shrinks the whole poem, worked out from
//! one layout as code is, never under its `minSize`, where W203 says it does not fit.

use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{TextBox, Theme, Wrap};
use scaena_engine::{Engine, FrameRequest, PlacedText};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

const POEM: &str = "An old silent pond, still beneath the winter moon\nA frog jumps into the pond\nSplash! Silence again";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "t", "layout": "specimen", "props": props }]);
    d["data"] = json!({});
    serde_json::from_value(d).unwrap()
}

fn fonts(deck: &Deck) -> BundleFonts {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn placed(deck: &Deck, node: &str) -> PlacedText {
    let (theme, data) = (theme(), DataFiles::new());
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    Engine::new(fonts(deck)).text_layout(&req, node).unwrap()
}

fn lint(deck: &Deck) -> Vec<(String, String, String)> {
    let (theme, data) = (theme(), DataFiles::new());
    let found = scaena_engine::lint::lint(&mut Engine::new(fonts(deck)), deck, &theme, &data, None).unwrap();
    found.into_iter().map(|f| (f.code, f.node.unwrap_or_default(), f.message)).collect()
}

/// A poem in the torture theme's `body` role (40 cu), wrapping `none`, shrunk to fit, placed `at`.
fn verse(text: &str, at: Value) -> Value {
    json!({ "type": "text", "role": "body", "wrap": "none", "fit": "shrink", "text": text, "at": at })
}

/// `text` in `body`, wrapping `none`, laid out across `width` at `scale`.
fn set(text: &str, width: f32, scale: f32) -> TextLayout {
    let (theme, deck) = (theme(), deck(json!({})));
    let mut spec = TextSpec::plain(theme.text_role("body").unwrap(), text);
    spec.wrap = Some(Wrap::None);
    TextEngine::new().layout(&mut fonts(&deck), &theme, &spec.scaled(scale), width).unwrap()
}

#[test]
fn a_verse_is_set_as_written() {
    // In four columns, far narrower than its first line.
    let d = deck(json!({ "n": verse(POEM, json!({ "col": [1, 4], "row": [2, 10] })) }));
    let p = placed(&d, "n");
    // No soft break: three lines, as written, set whole and smaller.
    assert_eq!(p.text.lines.len(), 3, "{:?}", p.text.lines);
    assert_eq!(p.text.wrap, Wrap::None);
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
    // Wide enough, it keeps its size and its lines.
    let wide = placed(&deck(json!({ "n": verse(POEM, json!({ "col": [1, 12], "row": [2, 10] })) })), "n");
    assert_eq!((wide.scale, wide.text.lines.len()), (1.0, 3));
    // A text that wraps breaks the same words into more lines.
    let mut prose = verse(POEM, json!({ "col": [1, 4], "row": [2, 10] }));
    prose["wrap"] = json!("greedy");
    prose["fit"] = json!("wrap");
    assert!(placed(&deck(json!({ "n": prose })), "n").text.lines.len() > 3);
    // Centered, each line stands in the middle on its own.
    let mut centered = verse(POEM, json!({ "col": [1, 12], "row": [2, 10] }));
    centered["align"] = json!({ "x": "center" });
    let t = placed(&deck(json!({ "n": centered })), "n").text;
    let middles: Vec<f32> = t.lines.iter().map(|l| l.x + 0.5 * l.width).collect();
    assert!(middles.windows(2).all(|m| (m[0] - m[1]).abs() < 1.0), "{:?}", t.lines);
    assert!(t.lines[0].x < t.lines[1].x, "{:?}", t.lines);
}

#[test]
fn a_verse_shrinks_within_1_64_of_bisection() {
    let d = deck(json!({ "n": verse(POEM, json!({ "col": [1, 5], "row": [2, 10] })) }));
    let p = placed(&d, "n");
    let [w, h] = [p.cell[2], p.cell[3]];
    let fits = |scale: f32| {
        let laid = set(POEM, w, scale);
        let (top, bottom) = laid.trimmed(TextBox::Line);
        bottom - top <= h + 1.0 / 64.0 && laid.lines.iter().all(|l| l.width <= w + 1.0 / 64.0)
    };
    let (mut lo, mut hi) = (0.5_f32, 1.0_f32);
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if fits(mid) { lo = mid } else { hi = mid }
    }
    assert!((p.scale - lo).abs() <= 1.0 / 64.0, "worked out {}, bisected {lo}", p.scale);
    assert!(fits(p.scale));
}

#[test]
fn a_verse_shrinks_no_further_than_its_min_size() {
    let mut node = verse(POEM, json!({ "col": [1, 2], "row": [2, 10] }));
    node["minSize"] = json!(30);
    let d = deck(json!({ "n": node }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 0.75);
    assert!(p.overflow);
    assert_eq!(p.text.lines.len(), 3);
    let found = lint(&d);
    assert!(found.iter().any(|(c, n, _)| c == "W203" && n == "n"), "{found:?}");
    // Its lines are its own: no measure to run past, and no paragraph of the slide's to align.
    assert!(!found.iter().any(|(c, n, _)| (c == "W201" || c == "E100") && n == "n"), "{found:?}");
    let mut centered = verse(POEM, json!({ "col": [1, 12], "row": [2, 6] }));
    centered["align"] = json!({ "x": "center" });
    let prose = json!({ "type": "text", "role": "body", "text": "Two lines of words that run across the box and on, and on, and on again, so that they break.", "at": { "col": [1, 6], "row": [7, 10] } });
    let found = lint(&deck(json!({ "n": centered, "p": prose })));
    assert!(!found.iter().any(|(c, _, _)| c == "W220"), "{found:?}");
}

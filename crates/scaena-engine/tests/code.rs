//! A code text, set as written (PLAN 2.131, ADR-0029), with the torture deck's fonts: its lines
//! break only where it breaks them, its tabs stand to stops, nothing hangs, and it runs left to
//! right whatever its first word is. It grows its box first, then shrinks, worked out from one
//! layout, and by itself never under 12 pt at presentation size.

use scaena_core::Deck;
use scaena_core::model::values::Range;
use scaena_engine::code::CodeSpec;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::{TextBox, Theme};
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

/// A code text in the torture theme's `body` role (40 cu, leading 1.35), placed `at`.
fn code(text: &str, at: Value) -> Value {
    json!({ "type": "text", "role": "body", "code": { "language": "rust" }, "text": text, "at": at })
}

/// `text` as code in `body`, laid out across `width` at `scale`: what the engine lays out for
/// [`code`]'s node.
fn set(text: &str, width: f32, scale: f32) -> TextLayout {
    let (theme, deck) = (theme(), deck(json!({})));
    let mut spec = TextSpec::plain(theme.text_role("body").unwrap(), text);
    spec.code = Some(CodeSpec { language: "rust".into(), tab: 4 });
    TextEngine::new().layout(&mut fonts(&deck), &theme, &spec.scaled(scale), width).unwrap()
}

/// The x of the glyph that sets the character `written` bytes into `layout`'s text as written.
fn glyph_x(layout: &TextLayout, written: usize) -> f32 {
    let laid = layout.offsets.iter().find(|(w, _)| *w == written).unwrap().1;
    let at = layout.runs.iter().flat_map(|r| r.glyphs.iter().zip(&r.clusters)).find(|(_, c)| **c == laid);
    at.unwrap().0.x
}

/// What lint finds of `code` in `deck`, laid out, as (code, node).
fn lint(deck: &Deck) -> Vec<(String, String, String)> {
    let (theme, data) = (theme(), DataFiles::new());
    let found = scaena_engine::lint::lint(&mut Engine::new(fonts(deck)), deck, &theme, &data, None).unwrap();
    found.into_iter().map(|f| (f.code, f.node.unwrap_or_default(), f.message)).collect()
}

const SNIPPET: &str = "// مرحبا\nfn main() {\n\tlet greeting = \"hello\";\n\"quoted\" opens a line\n}";

#[test]
fn a_code_text_is_set_as_written() {
    // In three columns, far narrower than its widest line.
    let d = deck(json!({ "n": code(SNIPPET, json!({ "col": [1, 3], "row": [2, 8] })) }));
    let p = placed(&d, "n");
    let t = &p.text;
    // Lines break only where the text breaks them, though its widest runs past its box.
    assert_eq!(t.lines.len(), 5, "{:?}", t.lines);
    assert!(p.natural.unwrap()[0] > p.cell[2], "{:?} in {:?}", p.natural, p.cell);
    // An Arabic comment on the first line leaves the block left to right: every line starts at
    // its left. Prose that opens so runs right to left.
    assert!(!t.rtl);
    assert!(t.lines.iter().all(|l| l.x == 0.0), "{:?}", t.lines);
    let prose = |text: &str| {
        let d = deck(
            json!({ "n": { "type": "text", "role": "body", "text": text, "at": { "col": [1, 12], "row": [2, 8] } } }),
        );
        placed(&d, "n").text
    };
    assert!(prose(SNIPPET).rtl, "prose takes its first strong character's direction");
    // A quote that opens a line hangs in prose and not in code: it sits on the block's edge.
    let quoted = SNIPPET.find("\"quoted").unwrap();
    assert_eq!(t.lines[3].hang, 0.0);
    assert_eq!(glyph_x(t, quoted), 0.0);
    let hung = prose(SNIPPET.split_once('\n').unwrap().1);
    assert!(hung.lines.iter().any(|l| l.hang > 0.0), "{:?}", hung.lines);
    // A tab stands to its stop: `let` where four spaces would put it, and a caret steps over the
    // tab whole, from the line's start to the `l`.
    let tab = SNIPPET.find('\t').unwrap();
    let spaced = set(&SNIPPET.replace('\t', "    "), 2000.0, 1.0);
    let tabbed = set(SNIPPET, 2000.0, 1.0);
    assert_eq!(glyph_x(&tabbed, tab + 1), glyph_x(&spaced, tab + 4));
    let carets = tabbed.carets([0.0, 0.0]);
    let line = &carets.lines[2];
    assert_eq!((line.chars[0].offset, line.chars[1].offset), (tab, tab + 1));
    assert_eq!((line.chars[0].lead, line.chars[0].trail), (0.0, glyph_x(&tabbed, tab + 1)));
    assert_eq!(line.chars[1].lead, line.chars[0].trail);
    // The mark that runs it left to right is drawn by no glyph and said by none.
    assert!(tabbed.runs.iter().all(|r| r.clusters.iter().all(|&c| c >= '\u{200E}'.len_utf8())));
    assert_eq!(tabbed.written, SNIPPET);
}

#[test]
fn a_code_texts_shrink_is_worked_out_within_1_64_of_bisection() {
    let text =
        (1..=12).map(|i| format!("    let value_{i:02} = compute(input, {i}, options.clone());")).collect::<Vec<_>>();
    let text = text.join("\n");
    let d = deck(json!({ "n": code(&text, json!({ "col": [1, 6], "row": [2, 5] })) }));
    let p = placed(&d, "n");
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
    // The largest scale that fits, by bisection over the same layout: 12 steps, as `fit:
    // shrink` sets prose.
    let [w, h] = [p.cell[2], p.cell[3]];
    let fits = |scale: f32| {
        let laid = set(&text, w, scale);
        let (top, bottom) = laid.trimmed(TextBox::Line);
        bottom - top <= h + 1.0 / 64.0 && laid.lines.iter().all(|l| l.width <= w + 1.0 / 64.0)
    };
    let (mut lo, mut hi) = (24.0 / 40.0, 1.0_f32);
    for _ in 0..12 {
        let mid = 0.5 * (lo + hi);
        if fits(mid) { lo = mid } else { hi = mid }
    }
    assert!((p.scale - lo).abs() <= 1.0 / 64.0, "worked out {}, bisected {lo}", p.scale);
    assert!(fits(p.scale));
}

#[test]
fn code_shrinks_no_further_than_12_pt() {
    let long = (1..=30).map(|i| format!("step_{i}();")).collect::<Vec<_>>().join("\n");
    // Body is 40 cu: by itself it stops at 24, 12 pt at presentation size, and overflows: W203.
    let d = deck(json!({ "n": code(&long, json!({ "col": [1, 6], "row": 2 })) }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 0.6);
    assert!(p.overflow);
    assert!(p.text.runs.iter().all(|r| (r.size - 24.0).abs() < 1e-3));
    let found = lint(&d);
    let w203 = found.iter().find(|(c, n, _)| c == "W203" && n == "n").expect("W203");
    assert!(w203.2.contains("code `n` shrank to 24 cu"), "{}", w203.2);
    assert!(!found.iter().any(|(c, n, _)| (c == "E100" || c == "W205") && n == "n"), "{found:?}");
    // A `minSize` of its own lets it go smaller, which W205 says.
    let mut small = code(&long, json!({ "col": [1, 6], "row": 2 }));
    small["minSize"] = json!(12);
    let d = deck(json!({ "n": small }));
    let p = placed(&d, "n");
    assert_eq!(p.scale, 0.3);
    let found = lint(&d);
    let w205 = found.iter().find(|(c, n, _)| c == "W205" && n == "n").expect("W205");
    assert!(w205.2.contains("drawn at 12 cu") && w205.2.contains("30 lines"), "{}", w205.2);
    // On a canvas twice as large, 12 pt is 48 cu: past body's own 40, which it keeps.
    let mut big = deck(json!({ "n": code(&long, json!({ "col": [1, 6], "row": 2 })) }));
    (big.canvas.width, big.canvas.height) = (3840.0, 2160.0);
    assert_eq!(placed(&big, "n").scale, 1.0);
}

#[test]
fn a_code_text_grows_its_box_before_it_shrinks() {
    let lines = (1..=8).map(|i| format!("println!(\"{i}\");")).collect::<Vec<_>>().join("\n");
    let (theme, data) = (theme(), DataFiles::new());
    let grown = |d: &Deck| {
        let req = FrameRequest { deck: d, theme: &theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
        Engine::new(fonts(d)).grown(&req, "n").unwrap()
    };
    // By cells: its rows grow down to the first that holds its eight lines at its own size.
    let d = deck(json!({ "n": code(&lines, json!({ "col": [1, 6], "row": 2 })) }));
    let p = placed(&d, "n");
    let natural = p.natural.expect("code keeps its size at its own")[1];
    assert!(p.scale < 1.0 && natural > p.cell[3], "it shrinks where it stands");
    let target = grown(&d).expect("it grows");
    assert!(target.cell[3] >= natural - 0.5, "{:?} holds {natural}", target.cell);
    let spot = &target.spots[0].1;
    assert_eq!(spot.row, Some(Range::Span([2, 5])));
    let mut d2 = deck(json!({ "n": code(&lines, json!({ "col": [1, 6], "row": [2, 5] })) }));
    assert_eq!(placed(&d2, "n").scale, 1.0, "grown, it fits at its own size");
    assert!(grown(&d2).is_none(), "nothing to grow");
    // By `rect`: its height grows to hold it, no further than the grid's margin.
    d2 = deck(json!({ "n": code(&lines, json!({ "rect": [96, 200, 852, 100] })) }));
    let target = grown(&d2).expect("it grows");
    assert_eq!(target.cell, [96.0, 200.0, 852.0, natural.ceil()]);
    let low = deck(json!({ "n": code(&lines, json!({ "rect": [96, 700, 852, 100] })) }));
    let target = grown(&low).expect("it grows to the margin");
    assert_eq!(target.cell[3], 984.0 - 700.0);
    // And then shrinks what is left.
    let mut placed_low = low.clone();
    placed_low.nodes["n"].props.insert("at".into(), json!({ "rect": [96, 700, 852, 284] }));
    let p = placed(&placed_low, "n");
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
}

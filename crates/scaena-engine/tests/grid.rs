//! The baseline grid (SPEC §3.4, PLAN 1.25): a role with `snap: baseline` sets every
//! baseline on one of the grid's lines, its lines whole grid lines apart, and one with
//! `snap: cap` sets its first line's cap height on one. The torture theme's grid has a
//! line every 8 cu from its 96 cu top margin; its rows do not start on them.

use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest, PlacedText};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

/// The torture grid's baseline grid: its top margin, and the distance between lines.
const ORIGIN: f32 = 96.0;
const PITCH: f32 = 8.0;

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck's fonts with one state, `t`, showing `nodes`.
fn deck(nodes: Value) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    d["nodes"] = nodes;
    d["states"] = json!([{ "id": "t", "layout": "specimen", "props": props }]);
    d["data"] = json!({});
    serde_json::from_value(d).unwrap()
}

/// The torture theme with `snap` set on the roles named, as each says.
fn theme(snaps: &[(&str, &str)]) -> Theme {
    let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    for (role, snap) in snaps {
        t["type"]["roles"][role]["snap"] = json!(snap);
    }
    Theme::from_json(&t.to_string()).unwrap()
}

fn placed(deck: &Deck, theme: &Theme, node: &str) -> PlacedText {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    Engine::new(fonts).text_layout(&req, node).unwrap()
}

/// Each line's baseline on the canvas.
fn baselines(p: &PlacedText) -> Vec<f32> {
    p.text.lines.iter().map(|l| p.origin[1] + l.baseline).collect()
}

/// How far `y` is from the nearest grid line, in grid lines.
fn off_grid(y: f32) -> f32 {
    let lines = (y - ORIGIN) / PITCH;
    (lines - lines.round()).abs()
}

const LONG: &str = "Body text set on the baseline grid, long enough to break into several lines in a narrow column";

#[test]
fn snapped_baselines_sit_on_grid_lines_whole_lines_apart() {
    let d =
        deck(json!({ "n": { "type": "text", "role": "body", "text": LONG, "at": { "col": [1, 4], "row": [2, 6] } } }));
    let free = placed(&d, &theme(&[]), "n");
    let snapped = placed(&d, &theme(&[("body", "baseline")]), "n");
    let (free, on) = (baselines(&free), baselines(&snapped));
    assert!(on.len() >= 3, "{on:?}");
    // Body is 40 at 1.35: lines 54 apart, 6.75 grid lines. Snapped, they are 7 apart.
    for pair in free.windows(2) {
        assert!((pair[1] - pair[0] - 54.0).abs() < 1e-3, "{free:?}");
    }
    for pair in on.windows(2) {
        assert!((pair[1] - pair[0] - 56.0).abs() < 1e-3, "{on:?}");
    }
    for y in &on {
        assert!(off_grid(*y) < 1e-3, "{y} is off the grid: {on:?}");
    }
    // Row 2 starts at 210, off the grid: the first baseline moves down to the next line.
    assert!(on[0] >= free[0] && on[0] - free[0] < PITCH, "{} → {}", free[0], on[0]);
}

#[test]
fn line_boxes_take_the_room_snapping_adds_above_each_line() {
    let d =
        deck(json!({ "n": { "type": "text", "role": "body", "text": LONG, "at": { "col": [1, 4], "row": [2, 6] } } }));
    let p = placed(&d, &theme(&[("body", "baseline")]), "n");
    let t = &p.text;
    let mut top = 0.0;
    for line in &t.lines {
        assert!((line.top - top).abs() < 1e-3, "each line box starts where the last ended");
        assert!(line.baseline > line.top && line.baseline < line.top + line.height);
        top += line.height;
    }
    assert!((t.height - top).abs() < 1e-3, "{} {top}", t.height);
    // Glyphs sit on their line's baseline.
    for run in &t.runs {
        let baseline = t.lines[run.line].baseline;
        assert!(run.glyphs.iter().all(|g| (g.y - baseline).abs() < 1e-3), "line {}", run.line);
    }
}

#[test]
fn end_aligned_text_moves_up_to_the_line_above_rather_than_out_of_its_box() {
    let d = deck(json!({ "n": { "type": "text", "role": "body", "text": LONG,
                                "align": { "y": "end" }, "at": { "col": [1, 4], "row": [2, 6] } } }));
    let p = placed(&d, &theme(&[("body", "baseline")]), "n");
    let on = baselines(&p);
    assert!(on.iter().all(|y| off_grid(*y) < 1e-3), "{on:?}");
    let free = placed(&d, &theme(&[]), "n");
    assert!(p.origin[1] < free.origin[1], "it moved up: {} → {}", free.origin[1], p.origin[1]);
    let bottom = p.origin[1] + p.text.height;
    let cell_bottom = p.cell[1] + p.cell[3];
    assert!(bottom <= cell_bottom + 1e-3, "{bottom} past {cell_bottom}");
    assert!(cell_bottom - bottom < PITCH, "it moved less than a grid line: {bottom} {cell_bottom}");
}

#[test]
fn cap_snapping_sets_the_first_cap_height_on_a_line_and_keeps_the_leading() {
    let d = deck(json!({ "n": { "type": "text", "role": "display", "text": "Display type in two lines",
                                "align": { "y": "center" }, "at": { "col": [1, 8], "row": [2, 7] } } }));
    let free = placed(&d, &theme(&[]), "n");
    let snapped = placed(&d, &theme(&[("display", "cap")]), "n");
    let first = &snapped.text.lines[0];
    let cap_top = snapped.origin[1] + first.baseline - first.cap_height.unwrap();
    assert!(off_grid(cap_top) < 1e-3, "cap top {cap_top}");
    // Its lines keep display's leading (144 × 0.95 = 136.8), and only move together.
    let on = baselines(&snapped);
    assert_eq!(on.len(), 2, "{on:?}");
    assert!((on[1] - on[0] - 136.8).abs() < 1e-3, "{on:?}");
    let dy = snapped.origin[1] - free.origin[1];
    assert!((0.0..PITCH).contains(&dy), "{dy}");
    assert_eq!(snapped.text, free.text, "only placement moves cap-snapped text");
}

#[test]
fn a_role_without_snap_stays_where_its_alignment_puts_it() {
    let d = deck(
        json!({ "n": { "type": "text", "role": "caption", "text": LONG, "at": { "col": [1, 4], "row": [2, 6] } } }),
    );
    let free = placed(&d, &theme(&[]), "n");
    let other = placed(&d, &theme(&[("body", "baseline"), ("display", "cap")]), "n");
    assert_eq!(free, other);
}

#[test]
fn texts_in_a_stack_each_sit_on_the_grid() {
    let d = deck(json!({
        "s": { "type": "stack", "gap": "space.4", "at": { "col": [1, 5], "row": [2, 8] } },
        "a": { "type": "text", "role": "caption", "text": "A caption above the body, on the grid.", "at": { "parent": "s" } },
        "b": { "type": "text", "role": "body", "text": LONG, "at": { "parent": "s" } }
    }));
    let theme = theme(&[("body", "baseline"), ("caption", "baseline")]);
    let (a, b) = (placed(&d, &theme, "a"), placed(&d, &theme, "b"));
    for y in baselines(&a).into_iter().chain(baselines(&b)) {
        assert!(off_grid(y) < 1e-3, "{y}");
    }
    // The caption's lines stay above the body's.
    let caption_bottom = a.origin[1] + a.text.height;
    assert!(caption_bottom <= b.origin[1] + 1e-3, "{caption_bottom} {}", b.origin[1]);
}

#[test]
fn texts_aligned_to_one_baseline_in_a_row_stay_on_it() {
    let d = deck(json!({
        "r": { "type": "stack", "axis": "x", "gap": "space.4", "at": { "col": [1, 12], "row": [3, 4] } },
        "a": { "type": "text", "role": "body", "text": "Two lines of body text in a narrow box", "size": { "w": 420 },
               "align": { "y": "baseline" }, "at": { "parent": "r" } },
        "b": { "type": "text", "role": "caption", "text": "A caption beside it", "align": { "y": "baseline" },
               "at": { "parent": "r" } }
    }));
    let theme = theme(&[("body", "baseline"), ("caption", "baseline")]);
    let (a, b) = (baselines(&placed(&d, &theme, "a")), baselines(&placed(&d, &theme, "b")));
    assert!(a.len() == 2 && b.len() == 1, "{a:?} {b:?}");
    let (last_a, last_b) = (a[1], b[0]);
    assert!((last_a - last_b).abs() < 1e-3 && off_grid(last_a) < 1e-3, "{a:?} {b:?}");
}

#[test]
fn shrink_fits_the_text_as_set_on_the_grid() {
    let d = deck(json!({ "n": { "type": "text", "role": "body", "fit": "shrink", "text": LONG,
                                "at": { "col": [1, 4], "row": 2 } } }));
    let p = placed(&d, &theme(&[("body", "baseline")]), "n");
    assert!(p.scale < 1.0 && !p.overflow, "{} {}", p.scale, p.overflow);
    assert!(p.text.height <= p.cell[3] + 1e-3, "{} {}", p.text.height, p.cell[3]);
    let on = baselines(&p);
    for pair in on.windows(2) {
        let lines = (pair[1] - pair[0]) / PITCH;
        assert!((lines - lines.round()).abs() < 1e-3 && lines >= 1.0, "{on:?}");
    }
}

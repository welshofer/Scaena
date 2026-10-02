//! Containers through the whole engine (SPEC §3.4, PLAN 1.7), on the torture deck's
//! `containers` case: typographic anchors inside a row stack, and a child that moves
//! from one container to another between states, which morphs like any box.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap()
}

/// The torture deck with only the `containers` state, then `edit` applied.
fn deck(edit: impl FnOnce(&mut Value)) -> Deck {
    let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    let states = d["states"].as_array_mut().unwrap();
    states.retain(|s| s["id"] == "containers");
    edit(&mut d);
    serde_json::from_value(d).unwrap()
}

fn engine(deck: &Deck) -> Engine {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut images = BundleImages::new();
    for path in deck.image_files() {
        images.register(&path, &read(&path)).unwrap();
    }
    Engine::new(fonts).with_images(images)
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap()
}

fn frame(deck: &Deck, state: &str, t_ms: f64) -> DisplayList {
    let theme = theme();
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state, t_ms };
    engine(deck).frame(&req).unwrap().display_list
}

/// Each layer's node and translation, in paint order.
fn layers(dl: &DisplayList) -> Vec<(String, [f32; 2])> {
    dl.ops
        .iter()
        .filter_map(|op| match op {
            Op::Layer { node: Some(n), transform, .. } => Some((n.clone(), [transform[4], transform[5]])),
            _ => None,
        })
        .collect()
}

fn at(dl: &DisplayList, node: &str) -> [f32; 2] {
    layers(dl).into_iter().find(|(n, _)| n == node).unwrap_or_else(|| panic!("no layer for `{node}`")).1
}

/// The baseline of a text node's last line, canvas units: its glyphs sit on it.
fn last_baseline(dl: &DisplayList, node: &str) -> f32 {
    let Some(Op::Layer { transform, ops, .. }) =
        dl.ops.iter().find(|op| matches!(op, Op::Layer { node: Some(n), .. } if n == node))
    else {
        panic!("no layer for `{node}`")
    };
    let y = ops
        .iter()
        .filter_map(|op| match op {
            Op::Glyphs { glyphs, .. } => glyphs.iter().map(|g| g.y).reduce(f32::max),
            _ => None,
        })
        .reduce(f32::max)
        .unwrap();
    transform[5] + y
}

#[test]
fn a_row_stack_puts_its_texts_on_one_baseline() {
    let dl = frame(&deck(|_| {}), "containers", f64::INFINITY);
    let (figure, label) = (last_baseline(&dl, "tally-figure"), last_baseline(&dl, "tally-label"));
    // Row 5 of the torture grid ends at 642: both baselines sit on it.
    assert!((figure - 642.0).abs() < 1e-3 && (label - 642.0).abs() < 1e-3, "{figure} {label}");
    // The cards share the row: the same width apart.
    let [a, b, c] = ["stat-a", "stat-b", "stat-c"].map(|n| at(&dl, n)[0]);
    assert!(((b - a) - (c - b)).abs() < 1e-3, "{a} {b} {c}");
}

#[test]
fn a_child_moved_to_another_container_morphs_between_them() {
    let d = deck(|d| {
        let mut moved = d["states"][0].clone();
        moved["id"] = json!("moved");
        moved["mode"] = json!("delta");
        moved["props"] = json!({ "stat-a-figure": { "at": { "parent": "stat-c", "index": 1 } } });
        moved["transition"] = json!({ "duration": 400, "ease": "linear" });
        d["states"].as_array_mut().unwrap().push(moved);
    });
    let (before, after) = (frame(&d, "containers", f64::INFINITY), frame(&d, "moved", f64::INFINITY));
    let (from, to) = (at(&before, "stat-a-figure"), at(&after, "stat-a-figure"));
    assert!(to[0] > from[0] + 1000.0, "into the third card: {from:?} → {to:?}");
    let mid = at(&frame(&d, "moved", 200.0), "stat-a-figure");
    for k in 0..2 {
        assert!((mid[k] - (from[k] + to[k]) / 2.0).abs() < 1e-2, "halfway: {from:?} {mid:?} {to:?}");
    }
    // Mid-transition, layers keep the at-rest paint order: each container under its children.
    let order: Vec<String> = layers(&frame(&d, "moved", 200.0)).into_iter().map(|(n, _)| n).collect();
    let rest: Vec<String> = layers(&after).into_iter().map(|(n, _)| n).collect();
    assert_eq!(order, rest);
}

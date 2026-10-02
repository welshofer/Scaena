//! The theme cascade end to end (SPEC §3.6, PLAN 1.6): a node's `style`, a state's props,
//! and the deck's `overrides` reach the frame in the order the cascade says.

use scaena_core::Deck;
use scaena_core::displaylist::{Color, Op, Paint};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const EXAMPLES: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{EXAMPLES}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn theme() -> Theme {
    Theme::from_json(&String::from_utf8(read("themes/dusk.theme.json")).unwrap()).unwrap()
}

/// The example deck, edited.
fn deck(edit: impl FnOnce(&mut Value)) -> Deck {
    let mut deck: Value = serde_json::from_slice(&read("revenue.deck.json")).unwrap();
    edit(&mut deck);
    serde_json::from_value(deck).unwrap()
}

/// Each glyph run `node` draws in the `intro` state: (size, color).
fn runs(deck: &Deck, node: &str) -> Vec<(f32, Color)> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let theme = theme();
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme: &theme, data: &data, state: "intro", t_ms: f64::INFINITY };
    let frame = Engine::new(fonts).frame(&req).unwrap();
    let layer = frame
        .display_list
        .ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), ops, .. } if n == node => Some(ops),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no layer for `{node}`"));
    layer
        .iter()
        .filter_map(|op| match op {
            Op::Glyphs { size, paint: Paint::Solid(color), .. } => Some((*size, *color)),
            _ => None,
        })
        .collect()
}

fn color(name: &str) -> Color {
    theme().color(name).unwrap()
}

#[test]
fn style_state_and_overrides_reach_the_frame_in_cascade_order() {
    let display = theme().text_role("display").unwrap().size;
    let plain = runs(&deck(|_| {}), "title");
    assert!(plain.iter().all(|r| *r == (display, color("onSurface"))), "{plain:?}");

    // The node's style over its role.
    let styled = deck(|d| d["nodes"]["title"]["style"] = json!({ "color": "accent" }));
    assert!(runs(&styled, "title").iter().all(|r| *r == (display, color("accent"))));

    // The state's props over the node's style.
    let state = deck(|d| {
        d["nodes"]["title"]["style"] = json!({ "color": "accent" });
        d["states"][0]["props"]["title"] = json!({ "style": { "color": "muted" } });
    });
    assert!(runs(&state, "title").iter().all(|r| *r == (display, color("muted"))));

    // Overrides over everything, in every state, literals included; what they do not
    // name still comes from the cascade below them.
    let over = deck(|d| {
        d["nodes"]["title"]["style"] = json!({ "color": "accent" });
        d["states"][0]["props"]["title"] = json!({ "style": { "color": "muted" } });
        d["overrides"] =
            json!({ "title": { "style": { "size": 96 } }, "subtitle": { "style": { "color": "#C2410C" } } });
    });
    assert!(runs(&over, "title").iter().all(|r| *r == (96.0, color("muted"))));
    let caption = theme().text_role("title").unwrap().size;
    let subtitle = runs(&over, "subtitle");
    assert!(subtitle.iter().all(|r| *r == (caption, Color::from_hex("#C2410C").unwrap())), "{subtitle:?}");
}

#[test]
fn a_run_takes_the_nodes_style_unless_it_is_set_in_a_role_of_its_own() {
    let deck = deck(|d| {
        d["nodes"]["subtitle"]["style"] = json!({ "color": "accent" });
        d["nodes"]["subtitle"]["text"] = Value::Null;
        d["nodes"]["subtitle"]["runs"] = json!([
            { "text": "Growth, " },
            { "text": "mix, ", "style": { "size": 40 } },
            { "text": "and 42", "role": "caption" }
        ]);
    });
    let runs = runs(&deck, "subtitle");
    let title = theme().text_role("title").unwrap().size;
    let caption = theme().text_role("caption").unwrap();
    let caption = (caption.size, color(caption.color.as_deref().unwrap_or("onSurface")));
    assert!(runs.contains(&(title, color("accent"))), "the node's role and style: {runs:?}");
    assert!(runs.contains(&(40.0, color("accent"))), "the node's look, refined by the run's style");
    assert!(runs.contains(&caption), "a run in its own role starts from that role, its color too");
}

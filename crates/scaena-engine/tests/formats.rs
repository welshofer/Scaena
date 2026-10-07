//! Formats (SPEC §3.4, PLAN 1.13): one deck laid out again on another canvas, with the
//! theme's grid and slots for that format, its nodes and states shared.

use scaena_core::Deck;
use scaena_core::displaylist::{DisplayList, Op};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, EngineError, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

struct Fx {
    deck: Deck,
    theme: Theme,
    data: DataFiles,
    engine: Engine,
}

impl Fx {
    /// The torture deck's fonts with a split slide, and a theme whose `split` stacks its
    /// halves in `9:16` on a grid of its own there.
    fn new(formats: Value) -> Fx {
        let mut d: Value = serde_json::from_slice(&read("deck.json")).unwrap();
        d["formats"] = formats;
        d["nodes"] = json!({
            "claim": { "type": "text", "role": "headline", "text": "Revenue grew this quarter", "at": { "in": "left" } },
            "figure": { "type": "shape", "kind": "rect", "fill": "accent", "at": { "in": "right" } },
            "mark": { "type": "shape", "kind": "ellipse", "fill": "ink", "at": { "col": [1, 2], "row": 1 } }
        });
        d["states"] = json!([{ "id": "s", "layout": "split", "props": { "claim": {}, "figure": {}, "mark": {} } }]);
        let mut t: Value = serde_json::from_slice(&read("theme.json")).unwrap();
        t["layouts"]["split"] = json!({
            "slots": { "left": { "col": [1, 6], "row": [1, 8] }, "right": { "col": [7, 12], "row": [1, 8] } },
            "formats": { "9:16": { "slots": { "left": { "col": [1, 12], "row": [1, 3] }, "right": { "col": [1, 12], "row": [4, 8] } } } }
        });
        t["formats"] = json!({ "9:16": { "grid": { "columns": 12, "rows": 8, "gutter": 24, "margin": [96, 48] } } });
        let deck: Deck = serde_json::from_value(d).unwrap();
        let theme = Theme::from_json(&t.to_string()).unwrap();
        let mut fonts = BundleFonts::new();
        for font in &deck.fonts {
            fonts.register(&font.file, read(&font.file)).unwrap();
        }
        Fx { deck, theme, data: DataFiles::new(), engine: Engine::new(fonts) }
    }

    fn frame(&mut self, format: Option<&str>) -> Result<DisplayList, EngineError> {
        let req = FrameRequest {
            deck: &self.deck,
            theme: &self.theme,
            data: &self.data,
            state: "s",
            t_ms: f64::INFINITY,
            format,
        };
        Ok(self.engine.frame(&req)?.display_list)
    }
}

/// `node`'s layer's translation: where its box starts.
fn at(dl: &DisplayList, node: &str) -> [f32; 2] {
    dl.ops
        .iter()
        .find_map(|op| match op {
            Op::Layer { node: Some(n), transform, .. } if n == node => Some([transform[4], transform[5]]),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no layer for `{node}`"))
}

#[test]
fn a_format_lays_the_deck_out_again_on_its_canvas_with_its_slots() {
    let mut fx = Fx::new(json!(["9:16"]));
    let wide = fx.frame(None).unwrap();
    let tall = fx.frame(Some("9:16")).unwrap();
    assert_eq!((wide.viewport, tall.viewport), ([1920.0, 1080.0], [1080.0, 1920.0]));
    // Side by side on the deck's canvas; stacked in 9:16, by the layout's slots there.
    let (claim, figure) = (at(&wide, "claim"), at(&wide, "figure"));
    assert!(figure[0] > claim[0] + 500.0 && (figure[1] - claim[1]).abs() < 100.0, "{claim:?} {figure:?}");
    let (claim, figure) = (at(&tall, "claim"), at(&tall, "figure"));
    assert!((figure[0] - claim[0]).abs() < 1.0 && figure[1] > claim[1] + 500.0, "{claim:?} {figure:?}");
    // The format's grid: its own margins.
    assert_eq!(at(&tall, "figure")[0], 48.0);
    // A node placed by column and row takes those cells of the format's grid.
    assert_eq!(at(&tall, "mark"), [48.0, 96.0]);
}

#[test]
fn the_decks_own_shape_is_its_canvas_and_other_formats_must_be_listed() {
    let mut fx = Fx::new(json!(["9:16"]));
    let own = fx.frame(None).unwrap();
    assert_eq!(fx.frame(Some("16:9")).unwrap(), own, "16:9 is a 1920 × 1080 deck's own canvas");
    let err = fx.frame(Some("1:1")).unwrap_err().to_string();
    assert!(err.contains("`1:1` is not one of the deck's formats (`9:16`)"), "{err}");
    let err = fx.frame(Some("21:9")).unwrap_err().to_string();
    assert!(err.contains("format `21:9`: expected one of `16:9`"), "{err}");
    let mut bare = Fx::new(json!([]));
    let err = bare.frame(Some("9:16")).unwrap_err().to_string();
    assert!(err.contains("deck's formats (none)"), "{err}");
}

#[test]
fn a_format_the_theme_says_nothing_about_keeps_the_grid_and_slots() {
    let mut fx = Fx::new(json!(["9:16", "1:1"]));
    let square = fx.frame(Some("1:1")).unwrap();
    assert_eq!(square.viewport, [1080.0, 1080.0]);
    // The base slots, side by side, on the base grid's margins.
    let (claim, figure) = (at(&square, "claim"), at(&square, "figure"));
    assert!(figure[0] > claim[0] && (figure[1] - claim[1]).abs() < 100.0, "{claim:?} {figure:?}");
    assert_eq!(claim[0], 96.0);
}

/// A node's layout in a format (ADR-0020) takes the place of its own there, and only there.
#[test]
fn a_node_lays_out_anew_in_a_format_and_as_it_was_in_the_rest() {
    let mut plain = Fx::new(json!(["9:16"]));
    let mut fx = Fx::new(json!(["9:16"]));
    let anew = json!({ "9:16": { "at": { "col": [11, 12], "row": 8 }, "transform": { "rotate": 45 } } });
    fx.deck.nodes.get_mut("mark").unwrap().props.insert("formats".into(), anew);
    // On the deck's own canvas, as it was.
    assert_eq!(fx.frame(None).unwrap(), plain.frame(None).unwrap());
    // In 9:16, in the last columns of the last row, and turned.
    let tall = fx.frame(Some("9:16")).unwrap();
    let mark = at(&tall, "mark");
    assert!(mark[0] > 800.0 && mark[1] > 1500.0, "{mark:?}");
    assert_ne!(tall, plain.frame(Some("9:16")).unwrap());
    // What else the slide holds stays where it was.
    assert_eq!(at(&tall, "claim"), at(&plain.frame(Some("9:16")).unwrap(), "claim"));
}

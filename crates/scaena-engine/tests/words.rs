//! Words in a shape (PLAN 2.108) stand in its middle on each axis their `align` does not name: a
//! choice of left, centered, or right across it keeps them in its middle down it.

use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::Content;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use serde_json::{Value, json};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";

/// The top of the words' first line in a 600 × 300 rect holding "Hi", the words aligned by
/// `align`, if they say how.
fn top(align: Option<Value>) -> f32 {
    let torture: Value = serde_json::from_slice(&std::fs::read(format!("{BUNDLE}/deck.json")).unwrap()).unwrap();
    let mut words = json!({ "type": "text", "role": "body", "text": "Hi", "at": { "parent": "box" } });
    if let Some(align) = align {
        words["align"] = align;
    }
    let deck = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "fonts": torture["fonts"],
        "nodes": {
            "box": { "type": "shape", "kind": "rect", "fill": "accent", "at": { "rect": [200, 200, 600, 300] } },
            "words": words
        },
        "states": [{ "id": "one", "props": { "box": {} } }]
    });
    let deck = Deck::from_json(&deck.to_string()).unwrap();
    let theme = Theme::from_json(&std::fs::read_to_string(format!("{BUNDLE}/theme.json")).unwrap()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, std::fs::read(format!("{BUNDLE}/{}", font.file)).unwrap()).unwrap();
    }
    let data = DataFiles::new();
    let mut engine = Engine::new(fonts);
    let req = FrameRequest { deck: &deck, theme: &theme, data: &data, state: "one", t_ms: f64::INFINITY, format: None };
    let scene = engine.at_rest(&req).unwrap();
    let node = scene.nodes.iter().find(|n| n.id == "words").expect("the words are drawn");
    let Content::Text(placed) = &node.content else { panic!("the words are a text") };
    placed.origin[1]
}

#[test]
fn words_aligned_across_their_shape_stay_in_its_middle_down_it() {
    let middle = top(None);
    assert!(middle > 300.0, "in the middle of the box, not at its top: {middle}");
    for x in ["start", "center", "end"] {
        assert_eq!(top(Some(json!({ "x": x }))), middle, "aligned {x} across");
    }
    // A keyword aligns both axes, and `y` its own.
    assert!(top(Some(json!("start"))) < middle);
    assert!(top(Some(json!({ "x": "start", "y": "end" }))) > middle);
}

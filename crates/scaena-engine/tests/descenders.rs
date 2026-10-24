//! A text's descenders and the text below it (dogfood): the line box is the font's metrics, so a
//! descender may hang past it, and lint E101 judges each text by its glyphs' ink, a descender's
//! tail included. The shipped themes, from `docs/examples`.

use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest, PlacedText};
use serde_json::{Value, json};

const BUNDLE: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{BUNDLE}/{path}")).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// A deck in the shipped theme `name`, its fonts the theme's, with `nodes` shown in one state,
/// `t`, in `layout`.
fn deck(name: &str, layout: &str, nodes: Value) -> (Deck, Theme) {
    let file = format!("themes/{name}.theme.json");
    let theme: Value = serde_json::from_slice(&read(&file)).unwrap();
    let mut fonts = Vec::new();
    for family in theme["type"]["families"].as_object().unwrap().values() {
        fonts.push(json!({ "family": family["family"], "file": family["file"] }));
        if let Some(italic) = family["italic"]["file"].as_str() {
            fonts.push(json!({ "family": family["family"], "file": italic, "style": "italic" }));
        }
    }
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), json!({}))).collect();
    let deck = json!({
        "scaena": scaena_core::FORMAT_VERSION,
        "canvas": { "width": 1920, "height": 1080 },
        "theme": file,
        "fonts": fonts,
        "nodes": nodes,
        "states": [{ "id": "t", "layout": layout, "props": props }],
    });
    (serde_json::from_value(deck).unwrap(), Theme::from_json(&theme.to_string()).unwrap())
}

/// Each of `nodes` as laid out in `t`.
fn placed(deck: &Deck, theme: &Theme, nodes: &[&str]) -> Vec<PlacedText> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let mut engine = Engine::new(fonts);
    let data = DataFiles::new();
    let req = FrameRequest { deck, theme, data: &data, state: "t", t_ms: f64::INFINITY, format: None };
    nodes.iter().map(|n| engine.text_layout(&req, n).unwrap()).collect()
}

/// Where a text's ink stands on the canvas, top and bottom.
fn ink(p: &PlacedText) -> (f32, f32) {
    (p.origin[1] + p.text.ink.0, p.origin[1] + p.text.ink.1)
}

/// The E101 findings lint makes of `deck`, as their messages.
fn e101(deck: &Deck, theme: &Theme) -> Vec<String> {
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    let found = scaena_engine::lint::lint(&mut Engine::new(fonts), deck, theme, &DataFiles::new(), None).unwrap();
    found.iter().filter(|f| f.code == "E101").map(|f| f.message.clone()).collect()
}

#[test]
fn e101_finds_a_descender_in_the_text_under_it() {
    // A display line's "g"s, "j"s, and "p"s hang past its line box, which its leading of 0.92
    // keeps tighter than its ink: a caption whose box starts 5 cu under that line box has its
    // capitals 8 cu under the descenders. Line boxes stand apart; the ink does not.
    let (deck, theme) = deck(
        "dusk",
        "four-numbers",
        json!({
            "f1": { "type": "text", "role": "display", "text": "gjpqy", "at": { "rect": [120, 100, 800, 200] } },
            "t1": { "type": "text", "role": "caption", "text": "CSAT across all 42 teams",
                    "at": { "rect": [120, 224, 800, 60] } },
        }),
    );
    let [f1, t1] = <[PlacedText; 2]>::try_from(placed(&deck, &theme, &["f1", "t1"])).unwrap();
    let lines = |p: &PlacedText| (p.origin[1], p.origin[1] + p.text.height);
    assert!(lines(&f1).1 + 2.0 < lines(&t1).0, "the line boxes stand apart: {:?} {:?}", lines(&f1), lines(&t1));
    assert!(ink(&f1).1 > ink(&t1).0 + 2.0, "the ink meets: {:?} {:?}", ink(&f1), ink(&t1));
    let found = e101(&deck, &theme);
    assert!(found.iter().any(|m| m.contains("`f1` and `t1`")), "{found:#?}");
}

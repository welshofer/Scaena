//! Decks at the edges of what validates, which a person typing in the editor reaches on the
//! way to what they mean: each laid out, sampled, and linted without a panic. A panic in the
//! browser's worker stops the editor mid-keystroke; a deck the engine cannot draw is an error
//! it says.

use scaena_core::Deck;
use scaena_core::displaylist::DisplayList;
use scaena_core::lint::{Backdrop, Pixels};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::render::FrameRequest;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, lint};
use serde_json::Value;

const B1: &str = "../../tests/bench/b1.scaena";

/// B1 as `edit` leaves it, with its theme and an engine built from its fonts.
fn b1(edit: impl FnOnce(&mut Value)) -> (Deck, Theme, Engine) {
    b1_themed(edit, |_| {})
}

/// B1 and its theme as `edit` and `restyle` leave them.
fn b1_themed(edit: impl FnOnce(&mut Value), restyle: impl FnOnce(&mut Value)) -> (Deck, Theme, Engine) {
    let read = |path: &str| std::fs::read(format!("{B1}/{path}")).unwrap();
    let mut doc: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    edit(&mut doc);
    let deck: Deck = serde_json::from_value(doc).unwrap();
    let mut theme: Value = serde_json::from_slice(&read("theme.json")).unwrap();
    restyle(&mut theme);
    let theme = Theme::from_json(&theme.to_string()).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    (deck, theme, Engine::new(fonts))
}

/// A backdrop that paints nothing: B1's text sits on the bare surface, which contrast
/// judges without painting.
struct Bare;

impl Backdrop for Bare {
    fn paint(&mut self, _: &DisplayList, _: f32) -> Result<Pixels, String> {
        Err("nothing painted here".into())
    }
}

/// A canvas a hair wide validates (it is wider than nothing), but contrast judges text at
/// presentation size, which makes it billions of pixels tall: an error, as a painter
/// refuses a raster it cannot make, not an overflow.
#[test]
fn a_canvas_too_thin_to_paint_lints_to_an_error() {
    let (deck, theme, mut engine) = b1(|d| d["canvas"]["width"] = 1e-9.into());
    let found = lint::lint(&mut engine, &deck, &theme, &DataFiles::new(), Some(&mut Bare));
    let error = found.expect_err("no raster that tall").to_string();
    assert!(error.contains("is not a raster this painter can make"), "{error}");
    // As it was, contrast judges the bare surface without a painter.
    let (deck, theme, mut engine) = b1(|_| {});
    lint::lint(&mut engine, &deck, &theme, &DataFiles::new(), Some(&mut Bare)).unwrap();
}

/// A size or leading past f32's range (a theme's schema bounds neither) would set glyphs
/// or lines an infinite length apart, and the line breaker would break them forever: the
/// frame is an error that says so. A size far past any canvas that f32 holds still lays out.
#[test]
fn text_too_large_to_lay_out_is_an_error() {
    let frame = |key: &str, value: f64| {
        let (deck, theme, mut engine) = b1_themed(
            |_| {},
            |t| {
                for role in t["type"]["roles"].as_object_mut().unwrap().values_mut() {
                    role[key] = value.into();
                }
            },
        );
        let data = DataFiles::new();
        let req =
            FrameRequest { deck: &deck, theme: &theme, data: &data, state: "cover", t_ms: f64::INFINITY, format: None };
        engine.frame(&req).map(|_| ()).map_err(|e| e.to_string())
    };
    for (key, value) in [("size", f64::MAX), ("size", 1e39), ("leading", 1e39), ("tracking", 1e37)] {
        let error = frame(key, value).expect_err("no layout that large");
        assert!(error.contains("too large to lay out"), "{key} {value}: {error}");
    }
    frame("size", 1e30).unwrap();
}

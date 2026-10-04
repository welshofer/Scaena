//! Decks at the edges of what validates, which a person typing in the editor reaches on the
//! way to what they mean: each laid out, sampled, and linted without a panic. A panic in the
//! browser's worker stops the editor mid-keystroke; a deck the engine cannot draw is an error
//! it says.

use scaena_core::Deck;
use scaena_core::displaylist::DisplayList;
use scaena_core::lint::{Backdrop, Pixels};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, lint};
use serde_json::Value;

const B1: &str = "../../tests/bench/b1.scaena";

/// B1 as `edit` leaves it, with its theme and an engine built from its fonts.
fn b1(edit: impl FnOnce(&mut Value)) -> (Deck, Theme, Engine) {
    let read = |path: &str| std::fs::read(format!("{B1}/{path}")).unwrap();
    let mut doc: Value = serde_json::from_slice(&read("deck.json")).unwrap();
    edit(&mut doc);
    let deck: Deck = serde_json::from_value(doc).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
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

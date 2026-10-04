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

/// A font file's table: where it starts and how long it is, by its record.
fn table(font: &[u8], tag: &[u8; 4]) -> (usize, usize, usize) {
    let tables = u16::from_be_bytes([font[4], font[5]]) as usize;
    let u32_at = |i: usize| u32::from_be_bytes(font[i..i + 4].try_into().unwrap()) as usize;
    (0..tables)
        .map(|i| 12 + 16 * i)
        .find(|&record| &font[record..record + 4] == tag)
        .map(|record| (record, u32_at(record + 8), u32_at(record + 12)))
        .unwrap_or_else(|| panic!("no {} table", String::from_utf8_lossy(tag)))
}

/// Where the glyph `font` draws `c` with starts, in its `glyf` table, by `loca`.
fn glyph(font: &[u8], c: char) -> usize {
    use skrifa::MetadataProvider;
    let gid = skrifa::FontRef::new(font).unwrap().charmap().map(c).unwrap().to_u32() as usize;
    let (_, head, _) = table(font, b"head");
    let long = i16::from_be_bytes([font[head + 50], font[head + 51]]) == 1;
    let (_, loca, _) = table(font, b"loca");
    let offset = if long {
        u32::from_be_bytes(font[loca + 4 * gid..loca + 4 * gid + 4].try_into().unwrap()) as usize
    } else {
        2 * u16::from_be_bytes([font[loca + 2 * gid], font[loca + 2 * gid + 1]]) as usize
    };
    table(font, b"glyf").1 + offset
}

/// B1 with `edit` made to the bytes of one of its fonts.
fn b1_with_font(file: &str, edit: impl FnOnce(&mut Vec<u8>)) -> (Deck, Theme, Engine) {
    let read = |path: &str| std::fs::read(format!("{B1}/{path}")).unwrap();
    let deck: Deck = serde_json::from_slice(&read("deck.json")).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let (mut fonts, mut edit) = (BundleFonts::new(), Some(edit));
    for font in &deck.fonts {
        let mut bytes = read(&font.file);
        if font.file == file
            && let Some(edit) = edit.take()
        {
            edit(&mut bytes);
        }
        fonts.register(&font.file, bytes).unwrap();
    }
    (deck, theme, Engine::new(fonts))
}

/// A font damaged inside a glyph that a deck sets would stop a painter: the CPU painter's
/// glyph cache unwraps what skrifa says of the glyph, and in the browser that stops the
/// editor's worker. The engine checks each glyph it places as the painters draw it, so the
/// frame is an error that names the font: one whose `head` table is cut short, one whose
/// outline of the title's first letter reads past its table, and one whose outline of it has
/// no points but data for some.
#[test]
fn a_damaged_font_is_an_error_not_a_panic() {
    const FRAUNCES: &str = "fonts/Fraunces-VF.ttf";
    let cover = |(deck, theme, mut engine): (Deck, Theme, Engine)| {
        let data = DataFiles::new();
        let req =
            FrameRequest { deck: &deck, theme: &theme, data: &data, state: "cover", t_ms: f64::INFINITY, format: None };
        engine.frame(&req).map(|_| ()).map_err(|e| e.to_string())
    };
    // Its `head` table one byte long.
    let error = cover(b1_with_font(FRAUNCES, |font| {
        let (record, _, _) = table(font, b"head");
        font[record + 12..record + 16].copy_from_slice(&1u32.to_be_bytes());
    }))
    .expect_err("a font with no `head` to read");
    assert!(error.contains(FRAUNCES) && error.contains("`head`") && error.contains("damaged"), "{error}");
    // The outline of "S" (Scaena) ending its contour at a point far past its own.
    let error = cover(b1_with_font(FRAUNCES, |font| {
        let glyph = glyph(font, 'S');
        // A glyph's header, then each contour's last point: "S" has one.
        assert_eq!(i16::from_be_bytes([font[glyph], font[glyph + 1]]), 1);
        font[glyph + 10..glyph + 12].copy_from_slice(&0xfff0u16.to_be_bytes());
    }))
    .expect_err("an outline that does not draw");
    assert!(error.contains(FRAUNCES) && error.contains("glyph") && error.contains("damaged"), "{error}");
    // The outline of "S" with no contours, and its points' data still after its header: the
    // read-fonts skrifa reads outlines with reads that past the end of no points, a panic.
    let error = cover(b1_with_font(FRAUNCES, |font| {
        let glyph = glyph(font, 'S');
        font[glyph..glyph + 2].copy_from_slice(&0u16.to_be_bytes());
    }))
    .expect_err("an outline with no points and data for some");
    assert!(error.contains(FRAUNCES) && error.contains("no points") && error.contains("damaged"), "{error}");
    // As it was, the cover draws.
    cover(b1_with_font(FRAUNCES, |_| {})).unwrap();
}

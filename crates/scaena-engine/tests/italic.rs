//! Italics (PLAN 2.40, SPEC §3.5): text that asks for italic is set in its family's italic
//! face, a file the theme names; a family without one sets it upright and says so (lint
//! W231), never a slanted roman. On the example decks' fonts, which carry each family's
//! italic. Bold is the family's own too: a family of one weight sets a heavier run in its
//! weight and says so (lint W232), never an emboldened outline.

use scaena_core::Deck;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::text::{TextEngine, TextLayout, TextSpec};
use scaena_engine::theme::Theme;

const EXAMPLES: &str = "../../docs/examples";

fn read(path: &str) -> Vec<u8> {
    std::fs::read(format!("{EXAMPLES}/{path}")).unwrap()
}

/// The fonts the revenue example lists, each family's and its italic's, registered.
fn fonts() -> BundleFonts {
    let deck: Deck = serde_json::from_slice(&read("revenue.deck.json")).unwrap();
    let mut fonts = BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, read(&font.file)).unwrap();
    }
    fonts
}

/// Dusk, as it ships, with `edit` made to its JSON first.
fn dusk(edit: impl FnOnce(&mut serde_json::Value)) -> Theme {
    let mut json: serde_json::Value = serde_json::from_slice(&read("themes/dusk.theme.json")).unwrap();
    edit(&mut json);
    Theme::from_json(&json.to_string()).unwrap()
}

/// `spans` set in Dusk's `body`, each `(text, italic)`, on one line.
fn set(fonts: &mut BundleFonts, theme: &Theme, spans: &[(&str, bool)]) -> TextLayout {
    let role = theme.text_role("body").unwrap();
    let mut spec = TextSpec::plain(role.clone(), "");
    spec.spans = spans
        .iter()
        .map(|&(text, italic)| {
            let mut span = TextSpec::plain(role.clone(), text).spans.remove(0);
            span.style.italic = italic;
            span
        })
        .collect();
    TextEngine::new().layout(fonts, theme, &spec, 1600.0).unwrap()
}

/// The font each glyph run is drawn in, in order.
fn faces(text: &TextLayout) -> Vec<&str> {
    text.runs.iter().map(|r| r.font.id.as_str()).collect()
}

#[test]
fn italic_is_the_familys_own_italic_face() {
    let mut fonts = fonts();
    let theme = dusk(|_| {});
    fonts.check_theme(&theme).unwrap();

    let upright = set(&mut fonts, &theme, &[("Revenue doubled", false)]);
    assert_eq!(faces(&upright), ["fonts/Inter-VF.ttf"]);
    let italic = set(&mut fonts, &theme, &[("Revenue doubled", true)]);
    assert_eq!(faces(&italic), ["fonts/Inter-Italic-VF.ttf"], "the italic file, not the roman slanted");
    assert!(italic.upright.is_empty() && italic.lighter.is_empty());
    assert_ne!(italic.width, upright.width, "an italic sets its own advances");

    // A run in italic, the rest upright: only its glyphs come from the italic.
    let mixed = set(&mut fonts, &theme, &[("Revenue ", false), ("doubled", true)]);
    assert_eq!(faces(&mixed), ["fonts/Inter-VF.ttf", "fonts/Inter-Italic-VF.ttf"]);

    // Bold italic is the italic at the weight asked: the same file, its `wght` axis moved.
    let role = theme.text_role("body").unwrap();
    let mut spec = TextSpec::plain(role, "doubled");
    spec.spans[0].style.italic = true;
    spec.spans[0].style.weight = 700.0;
    let bold = TextEngine::new().layout(&mut fonts, &theme, &spec, 1600.0).unwrap();
    assert_eq!(faces(&bold), ["fonts/Inter-Italic-VF.ttf"]);
    let plain = set(&mut fonts, &theme, &[("doubled", true)]);
    assert_ne!(bold.runs[0].coords, plain.runs[0].coords, "set at another weight");
}

#[test]
fn a_family_without_an_italic_sets_it_upright_and_says_so() {
    let mut fonts = fonts();
    let theme = dusk(|t| {
        t["type"]["families"]["body"].as_object_mut().unwrap().remove("italic");
    });
    let text = set(&mut fonts, &theme, &[("Revenue ", false), ("doubled", true)]);
    assert!(faces(&text).iter().all(|f| *f == "fonts/Inter-VF.ttf"), "upright, the roman as it is: {:?}", faces(&text));
    assert_eq!(text.upright, ["body"], "the family asked, by its key");
    assert!(text.lighter.is_empty());
    // Asked of none, nothing to say.
    assert!(set(&mut fonts, &theme, &[("Revenue doubled", false)]).upright.is_empty());
}

/// A family upright by design, whose script sets no italic (ADR-0031, call 6), names no italic
/// and sets italic text upright as one that lost it does, and W231 reads it all the same.
#[test]
fn a_family_upright_by_design_sets_italic_upright_and_says_so() {
    let mut fonts = fonts();
    let theme = dusk(|t| {
        let body = t["type"]["families"]["body"].as_object_mut().unwrap();
        body.remove("italic");
        body.insert("upright".into(), true.into());
    });
    assert_eq!(theme.families()["body"].upright, Some(true));
    let text = set(&mut fonts, &theme, &[("doubled", true)]);
    assert_eq!(faces(&text), ["fonts/Inter-VF.ttf"]);
    assert_eq!(text.upright, ["body"], "what W231 reads");
}

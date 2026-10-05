//! Italics (PLAN 2.40, SPEC §3.5): text that asks for italic is set in its family's italic
//! face, a file the theme names; a family without one sets it upright and says so (lint
//! W231), never a slanted roman. On the example decks' fonts, which carry each family's
//! italic.

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
    assert!(italic.upright.is_empty() && !italic.synthesized);
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
    assert!(!text.synthesized);
    // Asked of none, nothing to say.
    assert!(set(&mut fonts, &theme, &[("Revenue doubled", false)]).upright.is_empty());
}

#[test]
fn a_familys_italic_face_is_its_own_and_italic() {
    let fonts = fonts();
    // The roman named as the italic: upright.
    let roman = dusk(|t| t["type"]["families"]["body"]["italic"]["file"] = "fonts/Inter-VF.ttf".into());
    let e = fonts.check_theme(&roman).unwrap_err().to_string();
    assert!(e.contains("family `body`'s italic: fonts/Inter-VF.ttf is upright, not an italic"), "{e}");
    // Another family's italic.
    let other = dusk(|t| t["type"]["families"]["body"]["italic"]["file"] = "fonts/Fraunces-Italic-VF.ttf".into());
    let e = fonts.check_theme(&other).unwrap_err().to_string();
    assert!(
        e.contains("family `body`'s italic: fonts/Fraunces-Italic-VF.ttf provides [\"Fraunces\"], not `Inter`"),
        "{e}"
    );
    // One the bundle lacks.
    let missing = dusk(|t| t["type"]["families"]["body"]["italic"]["file"] = "fonts/Missing.ttf".into());
    let e = fonts.check_theme(&missing).unwrap_err().to_string();
    assert!(e.contains("family `body`'s italic: fonts/Missing.ttf is not in the bundle"), "{e}");
}

#[test]
fn command_i_toggles_italic_by_what_each_character_asks() {
    let mut fonts = fonts();
    let theme = dusk(|_| {});
    // "Revenue " upright, "doubled" italic by its own style.
    let carets = set(&mut fonts, &theme, &[("Revenue ", false), ("doubled", true)]).carets([0.0, 0.0]);
    let italic = |from: usize, to: usize| carets.italicizing(from, to)["style/italic"].clone();
    assert_eq!(italic(0, 7), serde_json::json!(true), "upright characters: italic");
    assert_eq!(italic(0, 15), serde_json::json!(true), "some upright: all italic");
    assert_eq!(italic(8, 15), serde_json::Value::Null, "all italic by their own style: taken away");

    // Characters italic by their role take `false` to stand upright.
    let role = theme.text_role("body").unwrap();
    let mut spec = TextSpec::plain(role, "quoted");
    spec.spans[0].style.italic = true;
    spec.spans[0].base_italic = true;
    let carets = TextEngine::new().layout(&mut fonts, &theme, &spec, 1600.0).unwrap().carets([0.0, 0.0]);
    assert_eq!(carets.italicizing(0, 6)["style/italic"], serde_json::json!(false));
    // Even where the family has no italic face: what is asked, not what is set.
    let plain = dusk(|t| {
        t["type"]["families"]["body"].as_object_mut().unwrap().remove("italic");
    });
    let carets = set(&mut fonts, &plain, &[("doubled", true)]).carets([0.0, 0.0]);
    assert_eq!(carets.italicizing(0, 7)["style/italic"], serde_json::Value::Null);
}

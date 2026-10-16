//! The layout library brought into an older copy of a theme that ships (PLAN 3.31): a deck whose
//! bundle carries Daybreak as it was before the library grew, with 11 layouts and none of them in
//! a section, opens with every layout the theme ships now, by section, and every slide it has
//! draws as it did.

use scaena_ops::theme::library;
use serde_json::{Value, json};

/// Daybreak as a bundle took it on 2026-10-06, in theme format 0.10.
const OLDER: &str = "../../tests/themes/older/daybreak-0.10.theme.json";
/// Daybreak as it ships.
const SHIPPED: &str = "../../docs/examples/authorability/themes/daybreak.theme.json";

fn read(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn keys(theme: &Value) -> Vec<String> {
    theme["layouts"].as_object().unwrap().keys().cloned().collect()
}

#[test]
fn an_older_copy_takes_the_library_and_draws_as_it_did() {
    let (older, shipped) = (read(OLDER), read(SHIPPED));
    let (theme, brought) = library(&older, &shipped, false, false).expect("an older copy takes the library");

    // Every layout the theme ships, in the library's order, each by name in its section.
    assert_eq!(keys(&theme), keys(&shipped));
    assert_eq!(brought.theme, "Daybreak");
    assert_eq!(brought.added.len(), keys(&shipped).len() - keys(&older).len());
    // Each layout it had, named, in the library's order.
    assert_eq!(brought.named, keys(&shipped).into_iter().filter(|k| keys(&older).contains(k)).collect::<Vec<_>>());
    for (key, layout) in theme["layouts"].as_object().unwrap() {
        assert!(layout["name"].is_string() && layout["group"].is_string(), "{key} has a name and a section");
    }
    // The role a number is set in, which the copy lacked; nothing else of its type changes.
    assert_eq!(brought.roles, ["hero"]);
    assert_eq!(theme["type"]["roles"]["hero"], shipped["type"]["roles"]["hero"]);
    let mut roles = theme["type"]["roles"].clone();
    roles.as_object_mut().unwrap().remove("hero");
    assert_eq!(roles, older["type"]["roles"]);
    assert_eq!(theme["type"]["families"], older["type"]["families"]);

    // Each layout it had keeps every slot where it was, in its role, and its own description;
    // it takes the words and pictures each slot waits with.
    for (key, own) in older["layouts"].as_object().unwrap() {
        let now = &theme["layouts"][key];
        assert_eq!(now.get("description"), own.get("description"), "{key}");
        assert_eq!(now.get("formats"), own.get("formats"), "{key}");
        for (slot, was) in own["slots"].as_object().unwrap() {
            let is = &now["slots"][slot];
            for field in ["col", "row", "role", "align", "fit", "inset", "bleed"] {
                assert_eq!(is.get(field), was.get(field), "{key}.{slot}.{field}");
            }
        }
    }
    assert_eq!(theme["layouts"]["title"]["slots"]["title"]["prompt"], "The talk's title");
    assert_eq!(theme["layouts"]["title"]["slots"]["art"]["example"], json!({ "picture": "backdrop" }));

    // The slide's number and footer, which the copy placed nowhere, placed as the theme ships them
    // (ADR-0028); the deck shows them only where it asks.
    assert!(brought.furniture);
    assert_eq!(theme["furniture"], shipped["furniture"]);

    // Nothing else of the theme changes but the format it is written in, which reads, the builds
    // it lacked (PLAN 2.112), and the shadows it lacked (PLAN 2.115).
    for part in ["name", "description", "grid", "shaders", "tables", "density"] {
        assert_eq!(theme[part], older[part], "{part}");
    }
    // Its charts, but a forecast's look, which it lacked (PLAN O.2f).
    let mut charts = theme["charts"].clone();
    assert_eq!(charts.as_object_mut().unwrap().remove("weather"), Some(shipped["charts"]["weather"].clone()));
    assert_eq!(charts, older["charts"], "charts");
    let mut tokens = theme["tokens"].clone();
    tokens.as_object_mut().unwrap().remove("shadow");
    assert_eq!(tokens, older["tokens"], "tokens, but the shadows");
    assert_eq!(brought.shadows, ["soft", "raised", "floating"]);
    assert_eq!(theme["tokens"]["shadow"], shipped["tokens"]["shadow"]);
    // Every build the theme ships, in its order, the copy's own as they were, with the spring
    // they spring on; its durations, easings, and limits as they were.
    let names =
        |t: &Value, key: &str| -> Vec<String> { t["motion"][key].as_object().unwrap().keys().cloned().collect() };
    let had = names(&older, "presets");
    assert_eq!(names(&theme, "presets"), names(&shipped, "presets"));
    assert_eq!(brought.builds, names(&shipped, "presets").into_iter().filter(|p| !had.contains(p)).collect::<Vec<_>>());
    assert!(brought.builds.iter().any(|b| b == "typewriter") && !brought.builds.iter().any(|b| b == "fade"));
    for p in &had {
        assert_eq!(theme["motion"]["presets"][p], older["motion"]["presets"][p], "{p}, as it was");
    }
    assert_eq!(brought.springs, ["bouncy"]);
    assert_eq!(theme["motion"]["springs"]["bouncy"], shipped["motion"]["springs"]["bouncy"]);
    for part in ["durations", "easings", "maxConcurrent", "maxBuild"] {
        assert_eq!(theme["motion"][part], older["motion"][part], "motion/{part}");
    }
    assert_eq!(theme["scaena-theme"], shipped["scaena-theme"]);
    scaena_engine::theme::Theme::from_json(&theme.to_string()).expect("the theme reads");

    // Taken once, the library brings nothing more.
    assert!(library(&theme, &shipped, false, false).is_none());
}

/// Only a copy of the theme takes its library: not another theme, nor one on another grid, nor
/// the theme as it ships.
#[test]
fn only_a_copy_of_the_theme_takes_it() {
    let (older, shipped) = (read(OLDER), read(SHIPPED));
    let mut other = older.clone();
    other["name"] = json!("Mine");
    assert!(library(&other, &shipped, false, false).is_none(), "another theme");
    let mut regridded = older.clone();
    regridded["grid"]["rows"] = json!(8);
    assert!(library(&regridded, &shipped, false, false).is_none(), "another grid");
    assert!(library(&shipped, &shipped, false, false).is_none(), "the theme as it ships");
    let dusk = read("../../docs/examples/themes/dusk.theme.json");
    assert!(library(&older, &dusk, false, false).is_none(), "another theme's library");
}

/// A layout of the copy's own stays, after the library's; a layout that sets words in a role the
/// copy cannot take is left out; and an example drawn by a preset the copy lacks is left off.
#[test]
fn the_copy_keeps_its_own_and_takes_only_what_it_can_draw() {
    let (mut older, mut shipped) = (read(OLDER), read(SHIPPED));
    older["layouts"]["mine"] = older["layouts"]["title"].clone();
    let (theme, _) = library(&older, &shipped, false, false).unwrap();
    assert_eq!(keys(&theme).last().map(String::as_str), Some("mine"), "the copy's own, after the library's");
    assert_eq!(theme["layouts"]["mine"], older["layouts"]["mine"], "and as it was");

    // `hero` in a family the copy does not have: no layout that sets a number in it.
    shipped["type"]["roles"]["hero"]["family"] = json!("nonesuch");
    let (theme, brought) = library(&older, &shipped, false, false).unwrap();
    assert!(brought.roles.is_empty() && theme["type"]["roles"].get("hero").is_none());
    let sets_hero = |layout: &Value| layout["slots"].as_object().unwrap().values().any(|s| s["role"] == "hero");
    assert!(brought.added.iter().all(|k| !sets_hero(&theme["layouts"][k])), "{:?}", brought.added);
    assert!(
        keys(&shipped)
            .iter()
            .filter(|k| sets_hero(&shipped["layouts"][*k]))
            .all(|k| theme["layouts"].get(k).is_none() || older["layouts"].get(k).is_some())
    );

    // No `backdrop` in the copy: no picture drawn by it, where the library would put one.
    let (mut older, shipped) = (read(OLDER), read(SHIPPED));
    older["shaders"]["presets"].as_object_mut().unwrap().remove("backdrop");
    let (theme, _) = library(&older, &shipped, false, false).unwrap();
    let drawn = theme["layouts"].as_object().unwrap().values().flat_map(|l| l["slots"].as_object().unwrap().values());
    assert!(drawn.filter_map(|s| s.get("example")).all(|e| e.get("picture").is_none()));
}

/// The digest of the display list `theme` draws a slide in that shows Rust as code, in the
/// authorability bundle's fonts, which set Daybreak.
fn code_digest(theme: &Value) -> String {
    let root = "../../docs/examples/authorability";
    let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(format!("{root}/deck.json")).unwrap()).unwrap();
    deck["nodes"] = json!({ "n": {
        "type": "text", "code": { "language": "rust" },
        "text": "fn main() {\n\tlet greeting = \"hello\"; // says hi\n\treturn 42;\n}",
        "at": { "col": [1, 12], "row": [2, 8] }
    } });
    deck["states"] = json!([{ "id": "a", "props": { "n": {} } }]);
    for key in ["data", "spine", "overrides"] {
        deck.as_object_mut().unwrap().remove(key);
    }
    let deck = scaena_core::Deck::from_json(&deck.to_string()).unwrap();
    let mut fonts = scaena_engine::fonts::BundleFonts::new();
    for font in &deck.fonts {
        fonts.register(&font.file, std::fs::read(format!("{root}/{}", font.file)).unwrap()).unwrap();
    }
    let theme = scaena_engine::theme::Theme::from_json(&theme.to_string()).unwrap();
    let data = scaena_engine::data::DataFiles::new();
    let req = scaena_engine::FrameRequest {
        deck: &deck,
        theme: &theme,
        data: &data,
        state: "a",
        t_ms: f64::INFINITY,
        format: None,
    };
    let frame = scaena_engine::Engine::new(fonts).frame(&req).unwrap();
    frame.display_list.digest().unwrap()
}

/// The look a theme gives code (PLAN 2.132) comes into an older copy only where the deck shows no
/// code text: code a slide shows keeps the colors it is drawn in, its display list the same, and a
/// copy with a look of its own keeps that.
#[test]
fn codes_look_comes_only_where_no_code_shows() {
    let (mut older, mut shipped) = (read(OLDER), read(SHIPPED));
    // The bundle's fonts, as it holds them.
    older["type"]["families"] = shipped["type"]["families"].clone();
    shipped["code"] =
        json!({ "syntax": { "keyword": { "color": "accent-2", "weight": 800 }, "comment": { "italic": false } } });

    let (theme, brought) = library(&older, &shipped, true, false).expect("the copy takes the library");
    assert!(!brought.code && theme.get("code").is_none(), "code shows: no look of the theme's");
    assert_eq!(code_digest(&theme), code_digest(&older), "the code draws as it did");

    let (theme, brought) = library(&older, &shipped, false, false).expect("the copy takes the library");
    assert!(brought.code);
    assert_eq!(theme["code"], shipped["code"]);
    assert_ne!(code_digest(&theme), code_digest(&older), "code typed in from now on takes the look");

    let mut own = older.clone();
    own["code"] = json!({ "tab": 2 });
    let (theme, brought) = library(&own, &shipped, false, false).expect("the copy takes the library");
    assert!(!brought.code);
    assert_eq!(theme["code"], json!({ "tab": 2 }), "a look of its own stays");
}

/// The look a theme gives a forecast (PLAN O.2f) comes into an older copy only where the deck shows
/// no weather chart, every role and color it names the copy's: a copy with a look of its own keeps
/// that.
#[test]
fn a_forecasts_look_comes_only_where_no_forecast_shows() {
    let (older, shipped) = (read(OLDER), read(SHIPPED));
    let (theme, brought) = library(&older, &shipped, false, false).expect("the copy takes the library");
    assert!(brought.weather);
    assert_eq!(theme["charts"]["weather"], shipped["charts"]["weather"]);

    let (theme, brought) = library(&older, &shipped, false, true).expect("the copy takes the library");
    assert!(!brought.weather && theme.pointer("/charts/weather").is_none(), "a forecast shows: no look of the theme's");

    let mut own = older.clone();
    own["charts"]["weather"] = json!({ "band": "surface" });
    let (theme, brought) = library(&own, &shipped, false, false).expect("the copy takes the library");
    assert!(!brought.weather);
    assert_eq!(theme["charts"]["weather"], json!({ "band": "surface" }), "a look of its own stays");

    let mut other = shipped.clone();
    other["charts"]["weather"]["high"]["role"] = json!("headline-xl");
    let (theme, brought) = library(&older, &other, false, false).expect("the copy takes the library");
    assert!(!brought.weather && theme.pointer("/charts/weather").is_none(), "a role the copy lacks");
}

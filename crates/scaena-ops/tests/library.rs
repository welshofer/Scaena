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
    let (theme, brought) = library(&older, &shipped).expect("an older copy takes the library");

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
    for part in ["name", "description", "grid", "shaders", "charts", "tables", "density"] {
        assert_eq!(theme[part], older[part], "{part}");
    }
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
    assert!(library(&theme, &shipped).is_none());
}

/// Only a copy of the theme takes its library: not another theme, nor one on another grid, nor
/// the theme as it ships.
#[test]
fn only_a_copy_of_the_theme_takes_it() {
    let (older, shipped) = (read(OLDER), read(SHIPPED));
    let mut other = older.clone();
    other["name"] = json!("Mine");
    assert!(library(&other, &shipped).is_none(), "another theme");
    let mut regridded = older.clone();
    regridded["grid"]["rows"] = json!(8);
    assert!(library(&regridded, &shipped).is_none(), "another grid");
    assert!(library(&shipped, &shipped).is_none(), "the theme as it ships");
    let dusk = read("../../docs/examples/themes/dusk.theme.json");
    assert!(library(&older, &dusk).is_none(), "another theme's library");
}

/// A layout of the copy's own stays, after the library's; a layout that sets words in a role the
/// copy cannot take is left out; and an example drawn by a preset the copy lacks is left off.
#[test]
fn the_copy_keeps_its_own_and_takes_only_what_it_can_draw() {
    let (mut older, mut shipped) = (read(OLDER), read(SHIPPED));
    older["layouts"]["mine"] = older["layouts"]["title"].clone();
    let (theme, _) = library(&older, &shipped).unwrap();
    assert_eq!(keys(&theme).last().map(String::as_str), Some("mine"), "the copy's own, after the library's");
    assert_eq!(theme["layouts"]["mine"], older["layouts"]["mine"], "and as it was");

    // `hero` in a family the copy does not have: no layout that sets a number in it.
    shipped["type"]["roles"]["hero"]["family"] = json!("nonesuch");
    let (theme, brought) = library(&older, &shipped).unwrap();
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
    let (theme, _) = library(&older, &shipped).unwrap();
    let drawn = theme["layouts"].as_object().unwrap().values().flat_map(|l| l["slots"].as_object().unwrap().values());
    assert!(drawn.filter_map(|s| s.get("example")).all(|e| e.get("picture").is_none()));
}

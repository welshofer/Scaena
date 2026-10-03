//! The theme cascade from the CLI (PLAN 1.6): `scaena theme --apply` re-themes a bundle
//! and reports the lint delta, and `scaena inspect --resolved` shows each text node's look
//! and what its overrides set.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// The example deck as a bundle directory of its own.
fn example(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("theme-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for sub in ["fonts", "themes", "data"] {
        copy_dir(&Path::new("../../docs/examples").join(sub), &dir.join(sub));
    }
    std::fs::copy("../../docs/examples/revenue.deck.json", dir.join("deck.json")).unwrap();
    dir
}

fn deck(dir: &Path) -> serde_json::Value {
    serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap()
}

const DAYBREAK: &str = "../../docs/examples/authorability/themes/daybreak.theme.json";

#[test]
fn a_theme_that_has_every_name_the_deck_uses_changes_no_finding() {
    let dir = example("daybreak");
    let before = deck(&dir);
    let out = scaena(&["theme", dir.to_str().unwrap(), "--apply", DAYBREAK]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("applied themes/daybreak.theme.json (was themes/dusk.theme.json)"), "{stdout}");
    assert!(stdout.contains("lint delta: none"), "{stdout}");

    // The theme is copied in, the deck names it, and nothing else in the deck changes.
    assert_eq!(std::fs::read(dir.join("themes/daybreak.theme.json")).unwrap(), std::fs::read(DAYBREAK).unwrap());
    let mut after = deck(&dir);
    assert_eq!(after["theme"], "themes/daybreak.theme.json");
    after["theme"] = before["theme"].clone();
    let typed = |v: serde_json::Value| serde_json::from_value::<scaena_core::Deck>(v).unwrap().to_json().unwrap();
    assert_eq!(typed(after), typed(before), "written canonically, and otherwise the same deck");

    // The same deck, in the new skin, still validates and renders.
    assert_eq!(scaena(&["validate", dir.to_str().unwrap()]).status.code(), Some(0));
    let png = dir.join("intro.png");
    let out = scaena(&[
        "render",
        dir.to_str().unwrap(),
        "--state",
        "intro",
        "--size",
        "480x270",
        "--out",
        png.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_theme_that_lacks_names_the_deck_uses_says_which_and_exits_1() {
    let dir = example("lacking");
    let before = std::fs::read(dir.join("deck.json")).unwrap();
    let theme = "../../tests/lint/theme.json";
    let out = scaena(&["--json", "theme", dir.to_str().unwrap(), "--apply", theme, "--dry-run"]);
    assert_eq!(out.status.code(), Some(1));
    let delta: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((delta["theme"].as_str(), delta["applied"].as_bool()), (Some("themes/theme.json"), Some(false)));
    let added: Vec<String> = delta["added"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| format!("{} {}", f["code"].as_str().unwrap(), f["path"].as_str().unwrap()))
        .collect();
    // It has no `title` role and no `figure` layout (whose slots are then not looked for).
    for expected in ["E102 /nodes/subtitle/role", "E102 /states/1/layout"] {
        assert!(added.contains(&expected.to_string()), "{expected} in {added:?}");
    }
    assert!(delta["removed"].as_array().unwrap().is_empty());
    // A dry run writes nothing.
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before);
    assert!(!dir.join("themes/theme.json").exists());
}

/// A theme that would leave the deck invalid is refused, as a patch that would is (PLAN
/// 1.35): the deck keeps its theme, and the new one is copied in for a patch that swaps it
/// with the fixes, all or none. `--force` applies it anyway.
#[test]
fn a_theme_that_would_leave_the_deck_invalid_is_refused_unless_forced() {
    let dir = example("refused");
    let before = std::fs::read(dir.join("deck.json")).unwrap();
    let theme = "../../tests/lint/theme.json";
    let out = scaena(&["--json", "theme", dir.to_str().unwrap(), "--apply", theme]);
    assert_eq!(out.status.code(), Some(1));
    let refused: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((&refused["refused"], &refused["applied"]), (&true.into(), &false.into()), "{refused:#}");
    assert!(refused["added"].as_array().unwrap().iter().any(|f| f["code"] == "E102"), "{refused:#}");
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before, "the deck keeps its theme");
    assert!(dir.join("themes/theme.json").exists(), "the theme is copied in");
    // The copy is for a patch with the `retheme` op and the fixes; alone, the op is refused too.
    let ops = dir.join("retheme.json");
    std::fs::write(&ops, r#"[{ "op": "retheme", "theme": "themes/theme.json" }]"#).unwrap();
    let out = scaena(&["patch", dir.to_str().unwrap(), "--ops", ops.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("refused:"));
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before);
    // Forced, it applies, and the deck is left with its errors.
    let out = scaena(&["theme", dir.to_str().unwrap(), "--apply", theme, "--force"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stdout).starts_with("applied themes/theme.json"));
    assert_eq!(deck(&dir)["theme"], "themes/theme.json");
}

#[test]
fn a_zip_bundle_is_rewritten_with_its_new_theme() {
    let dir = example("zip");
    let zip = dir.with_extension("scaena");
    let out = scaena(&["save", dir.to_str().unwrap(), "--to", zip.to_str().unwrap(), "--keep-fonts"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let out = scaena(&["theme", zip.to_str().unwrap(), "--apply", DAYBREAK]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}");
    // The saved zip names its fonts by content; Daybreak's families find them by name.
    assert!(stdout.contains("family `display`: fonts/Fraunces-VF.ttf → fonts/Fraunces-"), "{stdout}");
    assert!(stdout.contains("lint delta: none"), "{stdout}");
    let out = scaena(&["--json", "inspect", zip.to_str().unwrap(), "--state", "intro", "--resolved"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let states: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let theme: serde_json::Value = serde_json::from_slice(&std::fs::read(DAYBREAK).unwrap()).unwrap();
    let ink = theme["tokens"]["color"][theme["tokens"]["roles"]["onSurface"].as_str().unwrap()].as_str().unwrap();
    assert_eq!(states[0]["looks"]["title"]["hex"], format!("{}FF", ink.to_uppercase()), "the title is set in Daybreak");
}

#[test]
fn inspect_resolved_shows_each_look_and_what_overrides_set() {
    let dir = example("resolved");
    let mut d = deck(&dir);
    d["nodes"]["subtitle"]["style"] = serde_json::json!({ "color": "accent", "weight": 700 });
    d["overrides"] = serde_json::json!({ "title": { "style": { "size": 120, "color": "#C2410C" } } });
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(&d).unwrap()).unwrap();

    let out = scaena(&["--json", "inspect", dir.to_str().unwrap(), "--state", "intro", "--resolved"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let states: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let s = &states[0];
    assert_eq!(s["nodes"]["title"]["style"], serde_json::json!({ "size": 120, "color": "#C2410C" }));
    let title = &s["looks"]["title"];
    assert_eq!(
        (title["role"].as_str(), title["size"].as_f64(), title["hex"].as_str()),
        (Some("display"), Some(120.0), Some("#C2410CFF"))
    );
    let subtitle = &s["looks"]["subtitle"];
    assert_eq!((subtitle["color"].as_str(), subtitle["weight"].as_f64()), (Some("accent"), Some(700.0)));
    assert_eq!(s["overrides"]["title"], serde_json::json!(["/style/size", "/style/color"]));
    assert!(s["overrides"].get("subtitle").is_none(), "a node style is theme-safe; only overrides count");

    let out = scaena(&["inspect", dir.to_str().unwrap(), "--state", "intro", "--resolved"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("2 override(s), not theme-safe: style/size, style/color"), "{text}");
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}

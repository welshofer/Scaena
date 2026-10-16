//! `scaena new` (PLAN 2.13): a bundle from a theme and its fonts, as `deck_create` makes one. A
//! theme that ships comes by its name, with its fonts, from the binary, so a deck starts where
//! no file of the repository's is at hand; a theme file still comes with the fonts beside it.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// A place for a bundle that is not there yet.
fn place(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("new-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

#[test]
fn each_theme_that_ships_makes_a_deck_by_its_name() {
    for name in ["dusk", "Daybreak", "EMBER"] {
        let dir = place(&name.to_lowercase());
        let bundle = dir.to_str().unwrap();
        let out = scaena(&["--json", "new", bundle, "--theme", name, "--title", "Field notes"]);
        assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
        let made = json(&out);
        let file = format!("themes/{}.theme.json", name.to_lowercase());
        assert_eq!(made["created"], true, "{made:#}");
        assert_eq!(made["errors"], 0, "{made:#}");
        let fonts = ["fonts/Fraunces-VF.ttf", "fonts/Inter-VF.ttf", "fonts/JetBrainsMono-VF.ttf"];
        for f in fonts.iter().chain([&"deck.json", &file.as_str()]) {
            assert!(dir.join(f).is_file(), "{name}: {f}");
        }
        let deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
        assert_eq!(
            (deck["meta"]["title"].as_str(), deck["theme"].as_str()),
            (Some("Field notes"), Some(file.as_str()))
        );
        assert_eq!(scaena(&["validate", bundle]).status.code(), Some(0), "{name}: validates");
        let linted = json(&scaena(&["--json", "lint", bundle, "--severity", "error"]));
        assert_eq!(linted.as_array().map(Vec::len), Some(0), "{name}: {linted:#}");
    }
}

#[test]
fn a_theme_file_comes_with_the_fonts_beside_it() {
    let dir = place("file");
    let theme = "../../docs/examples/themes/dusk.theme.json";
    let out = scaena(&["new", dir.to_str().unwrap(), "--theme", theme]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.starts_with(&format!("made {}: deck.json, fonts/", dir.display())), "{said}");
    assert!(said.contains("next: scaena decompile"), "{said}");
    assert_eq!(std::fs::read(dir.join("themes/dusk.theme.json")).unwrap(), std::fs::read(theme).unwrap());
    let deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
    assert_eq!(deck["meta"]["title"], "Untitled");
}

#[test]
fn a_new_bundle_needs_a_place_of_its_own_and_a_theme_there_is() {
    let dir = place("refused");
    let bundle = dir.to_str().unwrap();
    assert_eq!(scaena(&["new", bundle]).status.code(), Some(0));
    let again = json(&scaena(&["--json", "new", bundle]));
    assert_eq!(again["error"]["exit"], 2);
    assert!(again["error"]["message"].as_str().unwrap().contains("is not an empty directory"), "{again:#}");

    let nowhere = place("nowhere");
    let lacking = json(&scaena(&["--json", "new", nowhere.to_str().unwrap(), "--theme", "midnight"]));
    assert_eq!(lacking["error"]["exit"], 2);
    let said = lacking["error"]["message"].as_str().unwrap();
    assert!(said.contains("nor is it a theme that ships: dusk, daybreak, ember, gallery"), "{said}");
    assert!(!nowhere.exists(), "nothing is made");
}

/// The themes in the registry, `(slug, the fonts it names)`, read from the theme each names first.
fn registry() -> Vec<(String, Vec<String>)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry: Value =
        serde_json::from_slice(&std::fs::read(root.join("docs/themes/registry.json")).unwrap()).unwrap();
    let themes = registry["themes"].as_array().unwrap().iter();
    themes
        .map(|t| {
            let file = root.join(t["files"][0].as_str().unwrap());
            let theme: Value = serde_json::from_slice(&std::fs::read(file).unwrap()).unwrap();
            let families = theme["type"]["families"].as_object().unwrap().values();
            let mut fonts: Vec<String> = families
                .flat_map(|f| [&f["file"], &f["italic"]["file"]])
                .filter_map(|f| f.as_str())
                .map(str::to_string)
                .collect();
            fonts.sort();
            fonts.dedup();
            (t["slug"].as_str().unwrap().to_string(), fonts)
        })
        .collect()
}

/// The fonts a bundle holds, by their paths.
fn fonts_in(dir: &Path) -> Vec<String> {
    let mut fonts: Vec<String> = std::fs::read_dir(dir.join("fonts"))
        .unwrap()
        .map(|e| format!("fonts/{}", e.unwrap().file_name().to_string_lossy()))
        .collect();
    fonts.sort();
    fonts
}

/// Every theme in the registry, made and saved by its name with no network and no font cache: the
/// binary carries every family (PLAN T.0c, ADR-0031 call 3), and the bundle takes its theme's
/// fonts alone, subset by the save.
#[test]
fn every_theme_in_the_registry_makes_and_saves_a_deck_offline() {
    let home = place("offline-home");
    std::fs::create_dir_all(&home).unwrap();
    let offline = |args: &[&str]| -> Output {
        Command::new(env!("CARGO_BIN_EXE_scaena"))
            .args(args)
            .env("HOME", &home)
            .env("SCAENA_FONT_CACHE", home.join("nothing"))
            .env("HTTPS_PROXY", "http://127.0.0.1:9")
            .env("HTTP_PROXY", "http://127.0.0.1:9")
            .output()
            .unwrap()
    };
    for (slug, fonts) in registry() {
        let dir = place(&format!("offline-{slug}"));
        let bundle = dir.to_str().unwrap();
        let made = offline(&["--json", "new", bundle, "--theme", &slug, "--title", "Field notes"]);
        assert_eq!(made.status.code(), Some(0), "{slug}: {}", String::from_utf8_lossy(&made.stdout));
        assert_eq!(fonts_in(&dir), fonts, "{slug}: its fonts and no other theme's");
        let saved = offline(&["--json", "save", bundle]);
        assert_eq!(saved.status.code(), Some(0), "{slug}: {}", String::from_utf8_lossy(&saved.stdout));
        // Each subset and named by its content (PLAN 1.4), which `validate` finds the theme naming.
        assert_eq!(fonts_in(&dir).len(), fonts.len(), "{slug}: saved with its fonts");
        assert_eq!(offline(&["validate", bundle]).status.code(), Some(0), "{slug}: validates");
    }
}

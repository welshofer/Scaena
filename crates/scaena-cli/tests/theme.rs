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

/// `theme --edit` (PLAN 2.61, ADR-0016): the theme the deck names, edited by RFC 6902 operations
/// and written in canonical form, the deck as it was; refused, exit 1, where the deck would not
/// validate in it; exit 2 at an op that does not apply, which it names.
#[test]
fn a_theme_is_edited_where_the_deck_names_it_and_refused_where_the_deck_needs_what_it_takes() {
    let dir = example("edit");
    let deck_before = std::fs::read(dir.join("deck.json")).unwrap();
    let ops = dir.join("ops.json");
    let edit = |ops_json: &str, json: bool| {
        std::fs::write(&ops, ops_json).unwrap();
        let mut args = vec!["theme", dir.to_str().unwrap(), "--edit", ops.to_str().unwrap()];
        if json {
            args.insert(0, "--json");
        }
        scaena(&args)
    };
    let out = edit(
        r##"[{"op": "replace", "path": "/tokens/color/accent", "value": "#2E86E4"},
             {"op": "replace", "path": "/grid/gutter", "value": 32}]"##,
        false,
    );
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
    assert!(stdout.contains("edited themes/dusk.theme.json at /tokens/color/accent, /grid/gutter"), "{stdout}");
    let text = std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap();
    let theme: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(theme["tokens"]["color"]["accent"], "#2E86E4");
    assert_eq!(theme["grid"]["gutter"], 32);
    assert_eq!(text, serde_json::to_string_pretty(&theme).unwrap() + "\n", "written in canonical form");
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), deck_before, "the deck is as it was");
    assert_eq!(scaena(&["validate", dir.to_str().unwrap()]).status.code(), Some(0));

    // A role the deck uses, taken out: refused, and nothing written.
    let out = edit(r#"[{"op": "remove", "path": "/type/roles/headline"}]"#, true);
    assert_eq!(out.status.code(), Some(1));
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((said["refused"].as_bool(), said["applied"].as_bool()), (Some(true), Some(false)), "{said:#}");
    assert!(said["added"].as_array().unwrap().iter().any(|f| f["code"] == "E102"), "{said:#}");
    assert_eq!(std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap(), text);

    // An op that does not apply.
    let out = edit(r#"[{"op": "replace", "path": "/no/such", "value": 1}]"#, true);
    assert_eq!(out.status.code(), Some(2));
    let failed: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(failed["error"]["op"], 0, "{failed:#}");
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
fn a_theme_whose_grid_is_too_small_is_refused_at_each_placement_past_it() {
    // Gate 1's case (PLAN 1.37): a deck placed on 12 rows, onto a theme of 6. Each cell past
    // the grid is an E102 naming its size, so the re-theme is refused, as for a missing name.
    let dir = example("small-grid");
    let mut ember: serde_json::Value =
        serde_json::from_slice(&std::fs::read("../../docs/examples/themes/ember.theme.json").unwrap()).unwrap();
    ember["grid"]["rows"] = 6.into();
    let small = dir.join("small.theme.json");
    std::fs::write(&small, serde_json::to_vec_pretty(&ember).unwrap()).unwrap();
    let deck = "../../docs/examples/higher-ed.deck.json";
    let out = scaena(&["--json", "theme", deck, "--apply", small.to_str().unwrap(), "--dry-run"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let t: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(t["refused"], true, "{t:#}");
    let past: Vec<&str> = t["added"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["code"] == "E102")
        .map(|f| f["message"].as_str().unwrap())
        .collect();
    assert!(
        !past.is_empty() && past.iter().all(|m| m.ends_with("past the theme's grid, which has 6 rows")),
        "{past:#?}"
    );
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

/// A theme in a typeface the bundle has never held brings its fonts from beside it, its own
/// bundle's (PLAN 2.102): Salon's Instrument Serif, into a bundle of Dusk's three families.
#[test]
fn a_theme_in_a_new_typeface_brings_its_fonts() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme-typeface");
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(Path::new("../../docs/examples/authorability"), &dir);
    let _ = std::fs::remove_dir_all(dir.join("history"));
    assert!(!dir.join("fonts/InstrumentSerif-Regular.ttf").exists());
    let out =
        scaena(&["--json", "theme", dir.to_str().unwrap(), "--apply", "../../docs/examples/themes/salon.theme.json"]);
    let t: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{t:#}");
    assert_eq!(t["applied"], true, "{t:#}");
    for face in ["fonts/InstrumentSerif-Regular.ttf", "fonts/InstrumentSerif-Italic.ttf"] {
        assert_eq!(
            std::fs::read(dir.join(face)).unwrap(),
            std::fs::read(Path::new("../../docs/examples").join(face)).unwrap(),
            "{face} is copied in"
        );
        assert!(deck(&dir)["fonts"].as_array().unwrap().iter().any(|f| f["file"] == face), "the deck lists {face}");
    }
    let out = scaena(&["validate", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
}

/// A deck with a bold word in a headline and a heavy big number moves into Salon, whose
/// Instrument Serif is one weight (PLAN 2.102): each is set in that weight, none synthesized,
/// and lint says so (W232), rather than the re-theme failing.
#[test]
fn bold_words_move_into_a_family_of_one_weight() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme-one-weight");
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(Path::new("../../docs/examples/authorability"), &dir);
    let _ = std::fs::remove_dir_all(dir.join("history"));
    let mut d = deck(&dir);
    d["nodes"]["headline"]["runs"] =
        serde_json::json!([{ "text": "Revenue grew " }, { "text": "every quarter", "style": { "weight": 700 } }]);
    d["nodes"]["headline"].as_object_mut().unwrap().remove("text");
    d["nodes"]["q3-total"]["style"] = serde_json::json!({ "weight": 600 });
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(&d).unwrap()).unwrap();
    let out =
        scaena(&["--json", "theme", dir.to_str().unwrap(), "--apply", "../../docs/examples/themes/salon.theme.json"]);
    let t: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{t:#}");
    assert_eq!(t["applied"], true, "{t:#}");
    let lighter: std::collections::BTreeSet<&str> = (t["added"].as_array().unwrap().iter())
        .filter(|f| f["code"] == "W232")
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(lighter, ["/nodes/headline", "/nodes/q3-total"].into(), "{t:#}");
    for state in ["topline", "quarters"] {
        let png = dir.join(format!("{state}.png"));
        let out = scaena(&["render", dir.to_str().unwrap(), "--state", state, "--out", png.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(0), "{state}: {}", String::from_utf8_lossy(&out.stderr));
    }
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

/// The shipped themes, as the registry lists them (PLAN T.0a): each by its slug, with the paths
/// of its file and of every copy of it.
fn shipped() -> Vec<(String, Vec<String>)> {
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read("../../docs/themes/registry.json").unwrap()).unwrap();
    let themes = registry["themes"].as_array().unwrap().iter();
    let path = |f: &serde_json::Value| format!("../../{}", f.as_str().unwrap());
    themes
        .map(|t| (t["slug"].as_str().unwrap().to_string(), t["files"].as_array().unwrap().iter().map(path).collect()))
        .collect()
}

/// What a theme names that a deck may use (SPEC §3.6): its layouts and their slots, roles,
/// families, colors, color roles, data palettes, shader presets and palettes, motion, and the
/// shape of its grid.
fn vocabulary(path: &str) -> serde_json::Value {
    let t: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let keys = |v: &serde_json::Value| {
        let mut k: Vec<String> = v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default();
        k.sort();
        k
    };
    let layouts: serde_json::Map<String, serde_json::Value> =
        t["layouts"].as_object().unwrap().iter().map(|(name, l)| (name.clone(), keys(&l["slots"]).into())).collect();
    serde_json::json!({
        "layouts": layouts,
        "roles": keys(&t["type"]["roles"]),
        "families": keys(&t["type"]["families"]),
        "colors": keys(&t["tokens"]["color"]),
        "colorRoles": keys(&t["tokens"]["roles"]),
        "data": keys(&t["tokens"]["data"]),
        "presets": keys(&t["shaders"]["presets"]),
        "palettes": keys(&t["shaders"]["palettes"]),
        "durations": keys(&t["motion"]["durations"]),
        "easings": keys(&t["motion"]["easings"]),
        "springs": keys(&t["motion"]["springs"]),
        "motion": keys(&t["motion"]["presets"]),
        "grid": [&t["grid"]["columns"], &t["grid"]["rows"]],
    })
}

/// One vocabulary for the shipped themes (PLAN 1.34): each defines the names the others do,
/// for the same jobs, on a grid of the same shape, so a deck moves between them by swapping
/// the file. Each copy of a theme, as Dusk's in the authorability bundle, is that theme.
#[test]
fn the_shipped_themes_name_the_same_things() {
    let shipped = shipped();
    let (first, files) = &shipped[0];
    let named = vocabulary(&files[0]);
    for (name, files) in &shipped {
        assert_eq!(vocabulary(&files[0]), named, "{name} names what {first} does");
        for copy in &files[1..] {
            assert_eq!(std::fs::read(copy).unwrap(), std::fs::read(&files[0]).unwrap(), "{copy} is {name}");
        }
        // Each name is one word, its slug ignoring case (PLAN T.0d, ADR-0031 decision 4).
        let theme: serde_json::Value = serde_json::from_slice(&std::fs::read(&files[0]).unwrap()).unwrap();
        let said = theme["name"].as_str().unwrap();
        assert!(said.chars().all(|c| c.is_ascii_alphabetic()) && said.eq_ignore_ascii_case(name), "{name}: {said}");
    }
    // An older name finds no other theme, and is no theme's own.
    let registry: serde_json::Value =
        serde_json::from_slice(&std::fs::read("../../docs/themes/registry.json").unwrap()).unwrap();
    let aliases: Vec<&str> = (registry["themes"].as_array().unwrap().iter())
        .flat_map(|t| t["aliases"].as_array().into_iter().flatten().filter_map(|a| a.as_str()))
        .collect();
    assert_eq!(aliases, ["gallery"]);
    assert!(aliases.iter().all(|a| shipped.iter().all(|(name, _)| name != a)));
}

/// `theme --apply` takes a theme that ships by its name, as `new --theme` does (PLAN T.0d), and
/// Salon by `gallery` too.
#[test]
fn a_theme_that_ships_applies_by_its_name() {
    for name in ["salon", "Gallery", "daybreak"] {
        let out = scaena(&["--json", "theme", "../../docs/examples/revenue.deck.json", "--apply", name, "--dry-run"]);
        let t: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(t["refused"], false, "{name}: {t:#}");
    }
    let out = scaena(&["--json", "theme", "../../docs/examples/revenue.deck.json", "--apply", "sepia", "--dry-run"]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("nor is it a theme that ships"));
}

/// Every example deck re-themes onto every shipped theme with no name it lacks (PLAN 1.34):
/// no E102, so no swap is refused. What the new type and colors break is lint's to say, in
/// the delta.
#[test]
fn every_example_deck_moves_between_the_shipped_themes() {
    let decks = [
        "revenue.deck.json",
        "charts.deck.json",
        "trails.deck.json",
        "higher-ed.deck.json",
        "ridgeline.deck.json",
        "code.deck.json",
        "haiku.deck.json",
        "authorability",
    ];
    let shipped = shipped();
    let runs: Vec<(String, String)> = std::thread::scope(|s| {
        let jobs: Vec<_> = decks
            .iter()
            .flat_map(|deck| shipped.iter().map(move |(name, files)| (*deck, name.as_str(), files[0].as_str())))
            .map(|(deck, name, theme)| {
                s.spawn(move || {
                    let path = format!("../../docs/examples/{deck}");
                    let out = scaena(&["--json", "theme", &path, "--apply", theme, "--dry-run"]);
                    (format!("{deck} onto {name}"), String::from_utf8(out.stdout).unwrap())
                })
            })
            .collect();
        jobs.into_iter().map(|j| j.join().unwrap()).collect()
    });
    for (run, stdout) in runs {
        let t: serde_json::Value = serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("{run}: {e}: {stdout}"));
        assert_eq!(t["refused"], false, "{run}: {t:#}");
        let missing: Vec<&serde_json::Value> =
            t["added"].as_array().unwrap().iter().filter(|f| f["code"] == "E102").collect();
        assert!(missing.is_empty(), "{run}: {missing:#?}");
        // A text the theme left too faint that a color of its own fixes was recolored (PLAN 2.82).
        let faint: Vec<&serde_json::Value> = (t["added"].as_array().unwrap().iter())
            .filter(|f| (f["code"] == "E110" || f["code"] == "E111") && !f["fix"].is_null())
            .collect();
        assert!(faint.is_empty(), "{run}: a contrast error with a fix stays: {faint:#?}");
    }
}

/// A text the theme leaves too faint to read where it stands is set in its color that reads there,
/// as its contrast error's fix offers (PLAN 2.82): the trails deck onto Daybreak, whose storm slide
/// sets its title and its note over a dark photo in the theme's dark ink, recolors both, writes
/// each where its color lives, and leaves lint no error to find.
#[test]
fn a_text_the_theme_leaves_too_faint_is_set_in_its_color_that_reads() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme-recolor");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for sub in ["fonts", "themes", "data", "assets"] {
        copy_dir(&Path::new("../../docs/examples").join(sub), &dir.join(sub));
    }
    std::fs::copy("../../docs/examples/trails.deck.json", dir.join("deck.json")).unwrap();
    let before = deck(&dir);

    // It says each text it recolored, and where.
    let said = scaena(&["theme", dir.to_str().unwrap(), "--apply", DAYBREAK, "--dry-run"]);
    let said = String::from_utf8(said.stdout).unwrap();
    assert!(said.contains("recolored `storm-title` in `storm`: it reads on what lies behind it (E111)"), "{said}");
    assert!(said.contains("recolored `storm-note` in `storm`"), "{said}");

    let out = scaena(&["--json", "theme", dir.to_str().unwrap(), "--apply", DAYBREAK]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
    let t: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let recolored: Vec<&str> = t["recolored"].as_array().unwrap().iter().filter_map(|f| f["node"].as_str()).collect();
    assert_eq!(recolored, ["storm-note", "storm-title"], "{t:#}");
    assert_eq!(t["errors"], 0, "{t:#}");
    let faint = |f: &&serde_json::Value| f["code"] == "E110" || f["code"] == "E111";
    assert!(t["added"].as_array().unwrap().iter().filter(faint).count() == 0, "{t:#}");

    // Each in a color of the theme, set on the node, where its color lives; the rest as it was.
    let after = deck(&dir);
    for node in ["storm-note", "storm-title"] {
        let color = after["nodes"][node]["style"]["color"].as_str();
        assert!(color.is_some_and(|c| !c.starts_with('#')), "{node} takes a color of the theme: {color:?}");
    }
    assert_eq!(after["theme"], "themes/daybreak.theme.json");
    let mut rest = after.clone();
    for node in ["storm-note", "storm-title"] {
        rest["nodes"][node] = before["nodes"][node].clone();
    }
    rest["theme"] = before["theme"].clone();
    rest["fonts"] = before["fonts"].clone();
    let typed = |v: serde_json::Value| serde_json::from_value::<scaena_core::Deck>(v).unwrap().to_json().unwrap();
    assert_eq!(typed(rest), typed(before), "nothing else changes");
}

/// `theme --from-photo` (PLAN 2.94): the theme's colors from an image the bundle holds, written as
/// `--edit` writes them. On the trails example, the ridge photo's violet becomes the accent, its
/// pink `accent-2`, the neutrals lean to its cast, and every color text is set in still reads.
#[test]
fn a_theme_takes_its_colors_from_a_photo_the_bundle_holds() {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("theme-photo");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for sub in ["fonts", "themes", "data", "assets"] {
        copy_dir(&Path::new("../../docs/examples").join(sub), &dir.join(sub));
    }
    std::fs::copy("../../docs/examples/trails.deck.json", dir.join("deck.json")).unwrap();
    let deck_before = std::fs::read(dir.join("deck.json")).unwrap();
    let theme_before = std::fs::read(dir.join("themes/dusk.theme.json")).unwrap();
    let bundle = dir.to_str().unwrap();

    // A dry run says what the photo's colors are and what each of the theme's takes.
    let out = scaena(&["theme", bundle, "--from-photo", "assets/trails-ridge.png", "--dry-run"]);
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert_eq!(out.status.code(), Some(0), "{stdout}{}", String::from_utf8_lossy(&out.stderr));
    assert!(
        stdout.starts_with("assets/trails-ridge.png: hues 303° (47% of it), 348° (24% of it), 24° (7% of it)"),
        "{stdout}"
    );
    assert!(stdout.contains("  accent     #FF6A3D → #B97CFF, 6.3:1 on the surfaces (needs 4.5)"), "{stdout}");
    assert!(stdout.contains("would edit themes/dusk.theme.json at /tokens/color/ink,"), "{stdout}");
    assert!(stdout.contains("lint delta: none"), "{stdout}");
    assert_eq!(std::fs::read(dir.join("themes/dusk.theme.json")).unwrap(), theme_before, "a dry run writes nothing");

    // Applied: the theme takes the colors, in canonical form, and the deck is as it was.
    let out = scaena(&["--json", "theme", bundle, "--from-photo", "assets/trails-ridge.png"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((said["applied"].as_bool(), said["refused"].as_bool()), (Some(true), Some(false)), "{said:#}");
    assert_eq!(said["photo"]["image"], "assets/trails-ridge.png");
    let reads: Vec<[f64; 2]> = (said["photo"]["set"].as_array().unwrap().iter())
        .filter_map(|s| serde_json::from_value(s["reads"].clone()).ok())
        .collect();
    assert_eq!(reads.len(), 3, "ink, accent, and muted set text: {said:#}");
    assert!(reads.iter().all(|[ratio, needs]| ratio >= needs), "{reads:?}");
    let text = std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap();
    let theme: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(theme["tokens"]["color"]["accent"], "#B97CFF");
    assert_eq!(theme["tokens"]["data"]["sequential"][4], "#B97CFF", "the sequential palette ends at the accent");
    assert_eq!(text, serde_json::to_string_pretty(&theme).unwrap() + "\n", "written in canonical form");
    let typed = |bytes: &[u8]| {
        scaena_core::Deck::from_value(&serde_json::from_slice(bytes).unwrap()).unwrap().to_json().unwrap()
    };
    assert_eq!(typed(&std::fs::read(dir.join("deck.json")).unwrap()), typed(&deck_before), "the deck is as it was");
    assert_eq!(scaena(&["validate", bundle]).status.code(), Some(0));

    // Taken again, the photo has nothing left to change; what is not a photo says so.
    let out = scaena(&["--json", "theme", bundle, "--from-photo", "assets/trails-ridge.png"]);
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(said["error"]["message"].as_str().unwrap().contains("nothing to edit"), "{said:#}");
    let out = scaena(&["--json", "theme", bundle, "--from-photo", "deck.json"]);
    let said: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(said["error"]["message"].as_str().unwrap().contains("not a photo to read"), "{said:#}");
}

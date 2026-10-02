//! Validation fixtures (PLAN 1.2): for each code, a deck that triggers it and one that must
//! not, in `tests/lint/<CODE>/`. Each is checked as a bundle that holds the files the clean
//! decks name, with `tests/lint/theme.json` as its theme.

use scaena_core::validate::{BundleFiles, validate_bundle};

/// The fixtures' bundle.
struct Fixtures;

impl BundleFiles for Fixtures {
    fn exists(&self, path: &str) -> bool {
        matches!(path, "theme.json" | "fonts/Display.ttf" | "fonts/Body.ttf" | "data/q3.csv" | "assets/photo.png")
    }

    fn read_text(&self, path: &str) -> Option<String> {
        (path == "theme.json").then(|| include_str!("../../../tests/lint/theme.json").to_string())
    }
}

macro_rules! fixture {
    ($code:literal, $which:literal) => {
        include_str!(concat!("../../../tests/lint/", $code, "/", $which, ".deck.json"))
    };
}

/// Each finding as `code path`, sorted.
fn findings(deck: &str) -> Vec<String> {
    let found = validate_bundle(deck, &Fixtures).expect("the fixture is JSON");
    let mut found: Vec<String> =
        found.into_iter().map(|f| format!("{} {}", f.code, f.path.unwrap_or_default())).collect();
    found.sort();
    found
}

fn triggers(code: &str, deck: &str, paths: &[&str]) {
    let mut expected: Vec<String> = paths.iter().map(|p| format!("{code} {p}")).collect();
    expected.sort();
    assert_eq!(findings(deck), expected, "tests/lint/{code}/trigger.deck.json");
}

#[test]
fn clean_fixtures_validate() {
    for (code, deck) in [
        ("E102", fixture!("E102", "clean")),
        ("E104", fixture!("E104", "clean")),
        ("E105", fixture!("E105", "clean")),
        ("E106", fixture!("E106", "clean")),
    ] {
        assert_eq!(findings(deck), Vec::<String>::new(), "tests/lint/{code}/clean.deck.json");
    }
}

#[test]
fn e102_unknown_references() {
    triggers(
        "E102",
        fixture!("E102", "trigger"),
        &[
            "/data/q3/source",
            "/nodes/bg/palette",
            "/nodes/chart/data",
            "/nodes/chart/labels/role",
            "/nodes/photo/src",
            "/nodes/title/enter",
            "/nodes/title/fill",
            "/nodes/title/role",
            "/nodes/title/style/color",
            "/nodes/title/style/family",
            "/overrides/chart/labels/role",
            "/overrides/ghost",
            "/states/0/layout",
            "/states/1/choreography/0/enter/preset",
            "/states/1/choreography/0/enter/spring",
            "/states/1/choreography/1",
            "/states/1/props/title/at/in",
            "/states/1/transition/duration",
            "/states/1/transition/ease",
            "/states/2/slide",
        ],
    );
}

#[test]
fn e104_a_state_cannot_change_a_type() {
    triggers("E104", fixture!("E104", "trigger"), &["/overrides/figure/type", "/states/1/props/figure/type"]);
}

#[test]
fn e105_ids() {
    triggers(
        "E105",
        fixture!("E105", "trigger"),
        &[
            "/nodes/Note",
            "/nodes/cover",
            "/nodes/group/children/1",
            "/spine/sections/0/beats/1/id",
            "/states/1/props/Note",
            "/states/2/id",
        ],
    );
}

#[test]
fn e106_schema_and_resolved_types() {
    triggers(
        "E106",
        fixture!("E106", "trigger"),
        &[
            "/nodes/headline/fit",
            "/nodes/logo/src",
            "/nodes/logo/style",
            "/nodes/photo/alt",
            "/nodes/photo/role",
            "/overrides/headline/style/weight",
            "/overrides/photo/fit",
            "/overrides/photo/kind",
            "/states/1/props/headline/fit",
            "/states/1/props/photo/src",
        ],
    );
}

/// The fixtures' bundle with its theme edited by `edit`.
struct EditedTheme(fn(String) -> String);

impl BundleFiles for EditedTheme {
    fn exists(&self, path: &str) -> bool {
        Fixtures.exists(path)
    }

    fn read_text(&self, path: &str) -> Option<String> {
        Fixtures.read_text(path).map(self.0)
    }
}

/// Each finding as (code, file, path).
fn placed(files: &dyn BundleFiles) -> Vec<(String, Option<String>, String)> {
    let found = validate_bundle(fixture!("E102", "clean"), files).unwrap();
    found.into_iter().map(|f| (f.code, f.file, f.path.unwrap_or_default())).collect()
}

#[test]
fn a_theme_file_is_checked_against_its_schema() {
    let files =
        EditedTheme(|t| t.replace(r#""row": 1 }, "subtitle""#, r#""row": 1, "align": "sideways" }, "subtitle""#));
    let at = "/layouts/title/slots/title/align".to_string();
    assert_eq!(placed(&files), [("E106".to_string(), Some("theme.json".to_string()), at)]);
}

#[test]
fn a_theme_name_the_theme_does_not_define_is_e102_in_the_theme_file() {
    let files = EditedTheme(|t| t.replace(r#""family": "display", "size": 128"#, r#""family": "serif", "size": 128"#));
    let at = "/type/roles/display/family".to_string();
    assert_eq!(placed(&files), [("E102".to_string(), Some("theme.json".to_string()), at)]);
}

#[test]
fn a_deck_that_is_not_json_is_an_error_not_a_finding() {
    assert!(validate_bundle("{ \"scaena\": ", &Fixtures).is_err());
}

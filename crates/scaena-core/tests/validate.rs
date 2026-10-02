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
        ("E103", fixture!("E103", "clean")),
        ("E104", fixture!("E104", "clean")),
        ("E105", fixture!("E105", "clean")),
        ("E106", fixture!("E106", "clean")),
    ] {
        assert_eq!(findings(deck), Vec::<String>::new(), "tests/lint/{code}/clean.deck.json");
    }
}

#[test]
fn e103_fields_charts_cannot_read() {
    triggers(
        "E103",
        fixture!("E103", "trigger"),
        &[
            "/data/bad/source",
            "/nodes/a/key",
            "/nodes/a/x/field",
            "/nodes/b/x/format",
            "/nodes/b/y/type",
            "/nodes/d/dataTransform/0/derive/m",
            "/nodes/e/y/field",
            "/nodes/f/columns/1/field",
            "/nodes/f/columns/2/format",
            "/nodes/f/key",
            "/nodes/g/annotations/0/at/x",
            "/nodes/g/annotations/1/at/series",
            "/nodes/h/annotations/0/at/x",
            "/states/1/props/c/y/field",
        ],
    );
    let found = validate_bundle(fixture!("E103", "trigger"), &Fixtures).unwrap();
    let say = |path: &str| found.iter().find(|f| f.path.as_deref() == Some(path)).unwrap().message.clone();
    assert!(say("/nodes/a/x/field").contains("has no column `qtr`; it has `quarter`, `rev`, `when`"));
    assert!(say("/nodes/b/y/type").contains("declare it `number`"), "{}", say("/nodes/b/y/type"));
    assert!(say("/data/bad/source").contains("`seven` is not a number"));
    // A transform's expression reads a column that is not there; a chart reads one its
    // transform does not leave.
    assert!(say("/nodes/d/dataTransform/0/derive/m").contains("no column `profit`"));
    assert!(say("/nodes/f/columns/2/format").contains("`columns[2].format` prints numbers and dates"));
    let after = say("/nodes/e/y/field");
    assert!(after.contains("after its `dataTransform` has no column `rev`; it has `quarter`, `total`"), "{after}");
    // Annotations name what the data has.
    let x = say("/nodes/g/annotations/0/at/x");
    assert!(x.contains("`Q3` is no category of `quarter`") && x.contains("`Q1`, `Q2`"), "{x}");
    assert!(say("/nodes/g/annotations/1/at/series").contains("`Q9` is no series of `quarter`"));
    assert!(say("/nodes/h/annotations/0/at/x").contains("must be a date in ISO 8601"));
}

#[test]
fn a_transform_that_does_not_parse_is_e106() {
    let deck = fixture!("E103", "clean").replace("\"-growth\"", "\"-growth\" }, { \"limit\": \"eight\"");
    let deck = deck.replace("rev > 0 &&", "rev > 0 &");
    let found = validate_bundle(&deck, &Fixtures).unwrap();
    let codes: Vec<(String, String)> =
        found.iter().map(|f| (f.code.clone(), f.path.clone().unwrap_or_default())).collect();
    assert_eq!(codes, [("E106".to_string(), "/nodes/t/dataTransform/0/filter".to_string())], "{found:#?}");
    assert!(found[0].message.contains("`&&` is and"), "{}", found[0].message);
}

#[test]
fn a_format_that_does_not_parse_is_e106() {
    let deck = fixture!("E103", "clean").replace("\"$,.0f\"", "\"$,.0q\"");
    let found = validate_bundle(&deck, &Fixtures).unwrap();
    let codes: Vec<(String, String)> =
        found.iter().map(|f| (f.code.clone(), f.path.clone().unwrap_or_default())).collect();
    let e106 = |path: &str| ("E106".to_string(), path.to_string());
    assert_eq!(codes, [e106("/nodes/a/y/format"), e106("/nodes/tb/columns/1/format")], "{found:#?}");
    assert!(found[0].message.contains("not a number type"));
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
            "/nodes/cell/at/area",
            "/nodes/chart/labels/role",
            "/nodes/figure/at/parent",
            "/nodes/note/at/parent",
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
            "/spine/sections/0/beats/1/id",
            "/states/0/choreography/0/target/1",
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
            "/nodes/badge/at/parent",
            "/nodes/headline/fit",
            "/nodes/left/at/parent",
            "/nodes/logo/src",
            "/nodes/logo/style",
            "/nodes/photo/alt",
            "/nodes/photo/role",
            "/nodes/plot/annotations/0",
            "/nodes/plot/annotations/1",
            "/nodes/right/at/parent",
            "/overrides/headline/style/weight",
            "/overrides/photo/fit",
            "/overrides/photo/kind",
            "/states/1/choreography/0",
            "/states/1/choreography/1/split",
            "/states/1/choreography/2/enter/split",
            "/states/1/props/headline/fit",
            "/states/1/props/photo/src",
            "/states/1/props/plot/annotations/0",
        ],
    );
    let found = validate_bundle(fixture!("E106", "trigger"), &Fixtures).unwrap();
    let say = |path: &str| found.iter().find(|f| f.path.as_deref() == Some(path)).unwrap().message.clone();
    assert!(say("/nodes/plot/annotations/0").contains("not both"));
    assert!(say("/nodes/plot/annotations/1").contains("a highlight says nothing"));
    assert!(say("/states/1/props/plot/annotations/0").contains("from one value to another"));
    assert!(say("/states/1/choreography/0").contains("enter and exit"));
    assert!(say("/states/1/choreography/1/split").contains("`photo`, an image node, into words"));
    assert!(say("/states/1/choreography/2/enter/split").contains("only a stack, grid, frame, or group"));
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

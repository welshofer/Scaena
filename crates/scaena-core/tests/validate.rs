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
            "/nodes/k/key",
            "/nodes/m/x/field",
            "/nodes/n/x/field",
            "/nodes/tk/columns/0",
            "/states/1/props/c/y/field",
            "/states/1/props/p/key",
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
    // Keys repeat as rendering makes them: the `key` field, else x, joined with the series
    // unless the series is the key; a donut's are its categories, and a table's its first
    // column. Where x and series would tell the data apart, dropping `key` is the fix.
    let k = say("/nodes/k/key");
    assert!(k.starts_with("keys `Core`, `Cloud` repeat in `@sales`: the chart keys each datum by `product`;"), "{k}");
    assert!(k.ends_with("drop `key` to key it by `quarter` and `product`, which tell its rows apart"), "{k}");
    let m = say("/nodes/m/x/field");
    assert!(
        m.contains("keys `Q1`, `Q2` repeat") && m.ends_with("give it a `key` field that tells its rows apart"),
        "{m}"
    );
    let n = say("/nodes/n/x/field");
    assert!(n.contains("keys `Core`, `Cloud` repeat in `@sales`: the chart keys each datum by `product`;"), "{n}");
    let tk = say("/nodes/tk/columns/0");
    assert!(tk.contains("row keys `Core`, `Cloud` repeat") && tk.contains("by `product`, its first column"), "{tk}");
    // Keyed by its series in state two only, the chart repeats there.
    let p = found.iter().find(|f| f.path.as_deref() == Some("/states/1/props/p/key")).unwrap();
    assert_eq!((p.state.as_deref(), p.node.as_deref()), (Some("two"), Some("p")));
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
            "/nodes/late/at/row",
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

/// A name the theme lacks comes with the names of that kind it has, so whoever uses
/// the deck, an agent re-theming it above all, need not guess them.
#[test]
fn e102_says_what_the_theme_has() {
    let found = validate_bundle(fixture!("E102", "trigger"), &Fixtures).unwrap();
    let says = |path: &str| {
        let finding = found.iter().find(|f| f.path.as_deref() == Some(path));
        finding.map_or_else(|| panic!("no finding at {path}: {found:#?}"), |f| f.message.clone())
    };
    assert_eq!(says("/states/0/layout"), "layout `titel` is not in the theme, which has title, full");
    assert_eq!(
        says("/nodes/title/role"),
        "text role `headlin` is not in the theme, which has display, headline, body, caption, label, numeral"
    );
    assert_eq!(says("/nodes/title/enter"), "motion preset `slide` is not in the theme, which has fade, rise, grow");
    assert_eq!(says("/nodes/bg/palette"), "shader palette `sunset` is not in the theme, which has ambient");
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
            "/nodes/late/at/row",
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
    assert_eq!(say("/nodes/late/at/row"), "`late` is placed in rows 4–3, which run backward: write [3, 4]");
}

#[test]
fn a_placement_past_the_grid_names_the_grid() {
    let found = validate_bundle(fixture!("E102", "trigger"), &Fixtures).unwrap();
    let late = found.iter().find(|f| f.path.as_deref() == Some("/nodes/late/at/row")).unwrap();
    assert_eq!(late.message, "`late` is placed in rows 6–7, past the theme's grid, which has 6 rows");
    assert_eq!((late.state.as_deref(), late.node.as_deref()), (Some("photo"), Some("late")));
}

#[test]
fn a_slot_past_the_grid_is_e102_in_the_theme_file() {
    let files = EditedTheme(|t| {
        t.replace(r#""subtitle": { "col": [1, 8], "row": 2 }"#, r#""subtitle": { "col": [1, 8], "row": [2, 7] }"#)
    });
    let found = validate_bundle(fixture!("E102", "clean"), &files).unwrap();
    // The clean deck shows nothing in `subtitle`: a slot no node is placed in places nothing.
    assert!(found.is_empty(), "{found:#?}");
    let deck = fixture!("E102", "clean")
        .replace(r#""at": { "in": "title" }, "fill""#, r#""at": { "in": "subtitle" }, "fill""#);
    let placed: Vec<_> =
        validate_bundle(&deck, &files).unwrap().into_iter().map(|f| (f.code, f.file, f.path, f.message)).collect();
    let message =
        "slot `subtitle` of layout `title` is in rows 2–7, past the theme's grid, which has 6 rows".to_string();
    assert_eq!(
        placed,
        [(
            "E102".to_string(),
            Some("theme.json".to_string()),
            Some("/layouts/title/slots/subtitle/row".to_string()),
            message
        )]
    );
}

#[test]
fn a_format_with_a_smaller_grid_is_judged_on_its_own() {
    // A tall format whose grid has 4 rows and 6 columns: `late` (columns 1–12, rows 5–6) is
    // past it, and so are the slots nodes stand in, `title` (columns 1–8) and `main` (1–12).
    let files = EditedTheme(|t| {
        t.replace(
            r#""grid": { "columns": 12, "gutter": 24, "margin": 96 },"#,
            r#""grid": { "columns": 12, "gutter": 24, "margin": 96 }, "formats": { "9:16": { "grid": { "columns": 6, "rows": 4, "gutter": 24, "margin": 64 } } },"#,
        )
    });
    let deck = fixture!("E102", "clean").replace(r#""canvas":"#, r#""formats": ["9:16"], "canvas":"#);
    let mut found: Vec<_> =
        validate_bundle(&deck, &files).unwrap().into_iter().map(|f| (f.path.unwrap_or_default(), f.message)).collect();
    found.sort();
    assert_eq!(
        found,
        [
            ("/layouts/full/slots/main/col".to_string(), "slot `main` of layout `full` is in columns 1–12, past the theme's grid in `9:16`, which has 6 columns".to_string()),
            ("/layouts/title/slots/title/col".to_string(), "slot `title` of layout `title` is in columns 1–8, past the theme's grid in `9:16`, which has 6 columns".to_string()),
            ("/nodes/late/at/col".to_string(), "`late` is placed in columns 1–12, past the theme's grid in `9:16`, which has 6 columns".to_string()),
            ("/nodes/late/at/row".to_string(), "`late` is placed in rows 5–6, past the theme's grid in `9:16`, which has 4 rows".to_string()),
        ]
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

#[test]
fn e103_two_rows_one_mark() {
    // A mark is known by its key, else its category, with its series beside it.
    let deck = serde_json::json!({
        "scaena": "0.9",
        "canvas": { "width": 1920, "height": 1080 },
        "theme": "theme.json",
        "fonts": [{ "family": "Display", "file": "fonts/Display.ttf" }, { "family": "Body", "file": "fonts/Body.ttf" }],
        "data": { "r": {
            "source": { "inline": [{ "q": "Q1", "p": "A", "v": 1 }, { "q": "Q1", "p": "B", "v": 2 }, { "q": "Q2", "p": "A", "v": 3 }] },
            "schema": { "v": "number" }
        } },
        "nodes": {
            "keyed": { "type": "chart", "kind": "bar", "data": "@r", "x": { "field": "q" }, "y": { "field": "v" },
                       "series": { "field": "p" }, "key": "p", "alt": "", "at": { "in": "main" } },
            "unseried": { "type": "chart", "kind": "bar", "data": "@r", "x": { "field": "q" }, "y": { "field": "v" },
                          "alt": "", "at": { "in": "main" } },
            "fine": { "type": "chart", "kind": "bar", "data": "@r", "x": { "field": "q" }, "y": { "field": "v" },
                      "series": { "field": "p" }, "alt": "", "at": { "in": "main" } }
        },
        "states": [{ "id": "a", "layout": "full", "props": { "keyed": {}, "unseried": {}, "fine": {} } }]
    });
    assert_eq!(findings(&deck.to_string()), ["E103 /nodes/keyed/key", "E103 /nodes/unseried/x/field"]);
}

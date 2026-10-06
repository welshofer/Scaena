//! `scaena lint` end to end (PLAN 1.15, SPEC §7.5): validation, the document rules, and
//! the layout rules, contrast painted by the CPU painter.
//!
//! Each rule has a deck that triggers it and one that must not, in `tests/lint/<CODE>/`,
//! checked in `tests/lint/bundle/` with two of the torture deck's fonts. The torture deck
//! lints to the findings in `tests/golden/lint/torture.txt`, its cases' own: a reviewed
//! diff when lint changes, `SCAENA_BLESS=1` rewrites it.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const ROOT: &str = "../..";

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("lint-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
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

/// `tests/lint/<code>/<which>.deck.json` as a bundle of `test`'s own: the lint theme, its
/// data and image, and the two torture fonts it names.
fn fixture(test: &str, code: &str, which: &str) -> PathBuf {
    let dir = scratch(&format!("{test}-{code}-{which}"));
    copy_dir(&Path::new(ROOT).join("tests/lint/bundle"), &dir);
    std::fs::create_dir_all(dir.join("fonts")).unwrap();
    for font in ["RobotoSerif-VF.ttf", "EBGaramond-VF.ttf"] {
        std::fs::copy(
            Path::new(ROOT).join("tests/fixtures/torture.scaena/fonts").join(font),
            dir.join("fonts").join(font),
        )
        .unwrap();
    }
    std::fs::copy(Path::new(ROOT).join(format!("tests/lint/{code}/{which}.deck.json")), dir.join("deck.json")).unwrap();
    dir
}

/// `scaena --json lint` on `bundle`: its exit code and its findings.
fn lint(bundle: &Path) -> (i32, Vec<Value>) {
    let out = scaena(&["--json", "lint", bundle.to_str().unwrap()]);
    let v: Value = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    });
    let findings = v.as_array().unwrap_or_else(|| panic!("{v:#}")).clone();
    (out.status.code().unwrap(), findings)
}

/// Each rule's trigger, and where in the deck it finds what it finds.
const RULES: [(&str, &[&str]); 37] = [
    ("E100", &["/nodes/t"]),
    ("E101", &["/nodes/b", "/nodes/note", "/nodes/src"]),
    ("E110", &["/nodes/t"]),
    ("E111", &["/nodes/t"]),
    ("E120", &["/nodes/t"]),
    ("W200", &["/nodes/t"]),
    ("W201", &["/nodes/t"]),
    ("W202", &["/nodes/t"]),
    ("W203", &["/nodes/t"]),
    ("W210", &["/states/0"]),
    ("W220", &["/states/0"]),
    ("W221", &["/theme/type/roles/body/leading", "/theme/type/roles/body/snap", "/theme/type/roles/caption/snap"]),
    ("W230", &["/fonts/2/file"]),
    ("W231", &["/nodes/t"]),
    ("W300", &["/nodes/t/style/color", "/nodes/t/style/size", "/states/0/props/box/radius"]),
    ("W301", &["/nodes/t/at/rect"]),
    ("W302", &["/nodes/t/at/col"]),
    ("W310", &["/nodes/c/labels", "/nodes/c/labels", "/nodes/c/labels"]),
    ("W311", &["/nodes/bg"]),
    ("W312", &["/nodes/c"]),
    ("W313", &["/nodes/c"]),
    ("W320", &["/states/0"]),
    ("W321", &["/states/0/choreography"]),
    ("W322", &["/states/1/choreography/0"]),
    ("W323", &["/states/1"]),
    ("W401", &["/states/1"]),
    ("W410", &["/nodes/p"]),
    ("W420", &["/spine/sections/0/beats/0"]),
    ("W421", &["/states/0"]),
    ("W422", &["/states/0"]),
    ("W423", &["/spine/sections/0/beats/0/evidence/0"]),
    ("W424", &["/states/0"]),
    ("W425", &["/spine/sections/0/beats/1/claim"]),
    ("W426", &["/spine/sections/0/beats/1"]),
    ("I400", &["/states/1"]),
    ("I401", &["/nodes/ghost"]),
    ("I402", &["/overrides/t"]),
];

#[test]
fn every_rule_triggers_on_its_fixture_and_not_on_its_clean_twin() {
    for (code, paths) in RULES {
        let (_, found) = lint(&fixture("rules", code, "trigger"));
        let mut at: Vec<&str> =
            found.iter().filter(|f| f["code"] == code).map(|f| f["path"].as_str().unwrap()).collect();
        at.sort();
        let mut want = paths.to_vec();
        want.sort();
        assert_eq!(at, want, "tests/lint/{code}/trigger.deck.json: {found:#?}");
        let (exit, found) = lint(&fixture("rules", code, "clean"));
        assert!(found.is_empty(), "tests/lint/{code}/clean.deck.json: {found:#?}");
        assert_eq!(exit, 0, "{code}");
    }
}

/// A bundle of `test`'s own: the lint theme as `theme` edits it, its fonts, and `deck`.
fn bundle(test: &str, deck: &Value, theme: impl FnOnce(&mut Value)) -> PathBuf {
    let dir = fixture(test, "E110", "trigger");
    let path = dir.join("theme.json");
    let mut t: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    theme(&mut t);
    std::fs::write(&path, serde_json::to_vec_pretty(&t).unwrap()).unwrap();
    std::fs::write(dir.join("deck.json"), serde_json::to_vec_pretty(deck).unwrap()).unwrap();
    dir
}

/// A deck in the lint bundle: `nodes`, shown in one state of the `full` layout.
fn deck(data: Value, nodes: Value) -> Value {
    let props: serde_json::Map<String, Value> =
        nodes.as_object().unwrap().keys().map(|k| (k.clone(), serde_json::json!({}))).collect();
    serde_json::json!({
        "scaena": "0.12",
        "canvas": { "width": 1920, "height": 1080 },
        "theme": "theme.json",
        "fonts": [
            { "family": "Roboto Serif", "file": "fonts/RobotoSerif-VF.ttf" },
            { "family": "EB Garamond", "file": "fonts/EBGaramond-VF.ttf" }
        ],
        "data": data,
        "nodes": nodes,
        "states": [{ "id": "a", "layout": "full", "props": props }]
    })
}

/// The contrast findings (E110, E111) in `found`.
fn contrast(found: &[Value]) -> Vec<&Value> {
    found.iter().filter(|f| f["code"] == "E110" || f["code"] == "E111").collect()
}

fn sales() -> Value {
    let rows = [("Q1", 10), ("Q2", 12), ("Q3", 15), ("Q4", 9)];
    let inline: Vec<Value> = rows.iter().map(|(q, v)| serde_json::json!({ "q": q, "v": v })).collect();
    serde_json::json!({ "sales": { "source": { "inline": inline }, "schema": { "v": "number" } } })
}

#[test]
fn chart_text_is_judged_in_its_color_over_what_the_chart_paints() {
    // An annotation in the theme's line color, on the dark surface: its text fails, and
    // the finding says which of the chart's texts it is.
    let chart = serde_json::json!({ "c": {
        "type": "chart", "kind": "bar", "data": "@sales", "x": { "field": "q" }, "y": { "field": "v" },
        "annotations": [{ "kind": "rule", "at": { "y": 11 }, "text": "Plan" }],
        "alt": "Sales by quarter against the plan.", "at": { "in": "main" }
    }});
    let dir = bundle("chart-note", &deck(sales(), chart.clone()), |t| {
        t["charts"]["annotation"] = serde_json::json!({ "color": "line" });
    });
    let (_, found) = lint(&dir);
    let bad = contrast(&found);
    assert_eq!(bad.len(), 1, "{found:#?}");
    assert_eq!((bad[0]["code"].as_str(), bad[0]["path"].as_str()), (Some("E111"), Some("/nodes/c")));
    assert_eq!(bad[0]["measure"]["part"], "annotation");
    assert_eq!(bad[0]["measure"]["label"], "Plan");
    // In the accent, the default, it reads; so do the chart's other texts over its bars.
    let dir = bundle("chart-note-clean", &deck(sales(), chart), |_| {});
    let (_, found) = lint(&dir);
    assert!(contrast(&found).is_empty(), "{found:#?}");
}

#[test]
fn chart_text_cut_off_at_the_charts_side_is_e100() {
    // A callout wider than its narrow chart's plot runs past the plot's side, where the
    // chart cuts it off.
    let chart = serde_json::json!({ "c": {
        "type": "chart", "kind": "bar", "data": "@sales", "x": { "field": "q" }, "y": { "field": "v" },
        "annotations": [{ "kind": "callout", "at": { "x": "Q3" }, "text": "The best quarter any product line has had" }],
        "alt": "Sales by quarter, the third its best.", "at": { "rect": [200, 200, 480, 600] }
    }});
    let (_, found) = lint(&bundle("chart-cut", &deck(sales(), chart), |_| {}));
    let cut: Vec<&Value> = found.iter().filter(|f| f["code"] == "E100").collect();
    assert_eq!(cut.len(), 1, "{found:#?}");
    assert_eq!((cut[0]["path"].as_str(), cut[0]["measure"]["part"].as_str()), (Some("/nodes/c"), Some("annotation")));
    assert!(cut[0]["measure"]["over"].as_f64().is_some_and(|over| over > 0.5), "{:#?}", cut[0]);
}

#[test]
fn a_lines_names_stand_inside_its_chart() {
    // Names at a line's end stand `lead` past its last point, as the room beside the plot
    // is measured: on a time axis the last point is the plot's side, so a name drawn past
    // its dot instead ran the dot's radius past the chart.
    let rows: Vec<Value> = ["2026-01-01", "2026-06-01", "2026-12-01"]
        .iter()
        .enumerate()
        .flat_map(|(i, m)| {
            let i = i as f64;
            [("Riverside", 10.0 + i), ("Old Town", 8.0 + 2.0 * i)]
                .map(|(s, v)| serde_json::json!({ "m": m, "s": s, "v": v }))
        })
        .collect();
    let data =
        serde_json::json!({ "trips": { "source": { "inline": rows }, "schema": { "m": "date", "v": "number" } } });
    let chart = serde_json::json!({ "c": {
        "type": "chart", "kind": "line", "data": "@trips", "x": { "field": "m", "type": "temporal", "format": "%b" },
        "y": { "field": "v" }, "series": { "field": "s" }, "labels": { "show": "ends" },
        "alt": "Trips by neighborhood.", "at": { "rect": [100, 100, 1200, 600] }
    }});
    let (_, found) = lint(&bundle("line-names", &deck(data, chart), |t| t["charts"]["pointRadius"] = 10.into()));
    assert!(found.iter().all(|f| f["code"] != "E100"), "{found:#?}");
}

#[test]
fn chart_text_is_judged_at_the_opacity_a_highlight_dims_it_to() {
    let chart = serde_json::json!({ "c": {
        "type": "chart", "kind": "bar", "data": "@sales", "x": { "field": "q" }, "y": { "field": "v" },
        "labels": { "show": "all" }, "annotations": [{ "kind": "highlight", "at": { "x": "Q3" } }],
        "alt": "Sales by quarter, the third picked out.", "at": { "in": "main" }
    }});
    // Value labels in the muted color, dimmed as far as a highlight dims words (half the
    // way to nothing, when marks dim all the way): the three it does not pick fail.
    let dimmed = |t: &mut Value, marks: f64| {
        t["type"]["roles"]["chart"]["color"] = "onSurfaceMuted".into();
        t["charts"]["annotation"] = serde_json::json!({ "dimmed": marks });
    };
    let (_, found) = lint(&bundle("chart-dim", &deck(sales(), chart.clone()), |t| dimmed(t, 0.0)));
    let bad = contrast(&found);
    assert_eq!(bad.len(), 1, "{found:#?}");
    assert_eq!((bad[0]["code"].as_str(), bad[0]["measure"]["part"].as_str()), (Some("E111"), Some("value label")));
    assert_ne!(bad[0]["measure"]["label"], "15", "the picked value is not dimmed");
    // At the default, words dim to three quarters, and they still read.
    let (_, found) = lint(&bundle("chart-dim-default", &deck(sales(), chart), |t| dimmed(t, 0.5)));
    assert!(contrast(&found).is_empty(), "{found:#?}");
}

#[test]
fn contrast_reads_the_pixels_under_the_glyphs_not_the_line_box() {
    // Where the text's baseline is: its glyphs' y in the display list, in its layer.
    let text =
        serde_json::json!({ "type": "text", "role": "body", "text": "nun", "at": { "rect": [200, 200, 800, 120] } });
    let dir = bundle("ink", &deck(serde_json::json!({}), serde_json::json!({ "t": text })), |_| {});
    let dl = dir.join("a.dl.json");
    let out = scaena(&[
        "render",
        dir.to_str().unwrap(),
        "--state",
        "a",
        "--out",
        dir.join("a.png").to_str().unwrap(),
        "--display-list",
        dl.to_str().unwrap(),
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let dl: Value = serde_json::from_slice(&std::fs::read(&dl).unwrap()).unwrap();
    let layer = dl["ops"].as_array().unwrap().iter().find(|op| op["layer"]["node"] == "t").expect("the text's layer");
    let (oy, glyph) =
        (layer["layer"]["transform"][5].as_f64().unwrap(), &layer["layer"]["ops"][0]["glyphs"]["glyphs"][0]);
    let baseline = oy + glyph[2].as_f64().unwrap();
    // A line in the text's own color: under the baseline, where "nun" has no ink, the
    // text reads; across its middle, it does not.
    let with_line = |y: f64| {
        let line = serde_json::json!({ "type": "shape", "kind": "rect", "fill": "onSurface", "at": { "rect": [200, y, 800, 3] } });
        deck(serde_json::json!({}), serde_json::json!({ "line": line, "t": text }))
    };
    let (_, found) = lint(&bundle("ink-under", &with_line(baseline + 4.0), |_| {}));
    assert!(contrast(&found).is_empty(), "a line under the baseline is not under the glyphs: {found:#?}");
    let (_, found) = lint(&bundle("ink-through", &with_line(baseline - 8.0), |_| {}));
    assert_eq!(contrast(&found).len(), 1, "a line through the glyphs is: {found:#?}");
}

#[test]
fn errors_exit_1_and_a_fix_is_kept_only_where_it_works() {
    // E100's title fits once it shrinks within its role's minSize: the fix is offered.
    let (exit, found) = lint(&fixture("fixes", "E100", "trigger"));
    assert_eq!(exit, 1);
    let e100 = found.iter().find(|f| f["code"] == "E100").unwrap();
    assert_eq!(e100["fix"], serde_json::json!([{ "op": "add", "path": "/nodes/t/fit", "value": "shrink" }]));
    // W203's headline is already shrinking: nothing safe would make it fit, so no fix.
    let (exit, found) = lint(&fixture("fixes", "W203", "trigger"));
    assert_eq!(exit, 0, "a warning exits 0");
    assert!(found.iter().all(|f| f.get("fix").is_none()), "{found:#?}");
}

#[test]
fn lint_fix_applies_what_lint_offers_and_lints_again() {
    let dir = fixture("fix", "E100", "trigger");
    let out = scaena(&["--json", "lint", dir.to_str().unwrap(), "--fix"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let v: Value = serde_json::from_slice(&out.stdout).unwrap();
    let fixed: Vec<&str> = v["fixed"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap()).collect();
    assert_eq!(fixed, ["E100"], "{v:#}");
    assert_eq!(v["findings"], serde_json::json!([]), "{v:#}");
    let deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
    assert_eq!(deck["nodes"]["t"]["fit"], "shrink");
    // Again: nothing left to fix, and nothing written.
    let before = std::fs::read(dir.join("deck.json")).unwrap();
    let out = scaena(&["lint", dir.to_str().unwrap(), "--fix"]);
    assert_eq!(out.status.code(), Some(0));
    assert!(String::from_utf8_lossy(&out.stdout).contains("nothing to fix"));
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), before);
}

#[test]
fn a_state_shown_for_no_time_is_judged_only_where_the_deck_runs_on_its_own() {
    // `b`'s seven words and photograph read in 4 s: four words a second, 2 s a figure.
    let (_, found) = lint(&fixture("w323", "W323", "trigger"));
    let w323 = found.iter().find(|f| f["code"] == "W323").unwrap();
    assert_eq!(w323["state"], "b");
    assert!(w323["message"].as_str().unwrap().ends_with("give it a `hold` of about 4000 ms"), "{w323:#}");
    assert_eq!(w323["measure"], serde_json::json!({ "words": 7, "figures": 1, "suggestedHold": 4000.0 }));
    // The trigger, edited by `edit`, lints clean.
    let clean = |test: &str, edit: &dyn Fn(&mut Value)| {
        let dir = fixture(test, "W323", "trigger");
        let mut deck: Value = serde_json::from_slice(&std::fs::read(dir.join("deck.json")).unwrap()).unwrap();
        edit(&mut deck);
        std::fs::write(dir.join("deck.json"), deck.to_string()).unwrap();
        let (exit, found) = lint(&dir);
        assert_eq!((exit, found.len()), (0, 0), "{test}: {found:#?}");
    };
    // Without a hold anywhere the deck is presented live, and `b` shows until the next click.
    clean("live", &|deck| {
        for state in deck["states"].as_array_mut().unwrap() {
            state.as_object_mut().unwrap().remove("hold");
        }
    });
    // With a transition, `b` plays its cue on its way to `c`.
    clean("passing", &|deck| deck["states"][1]["transition"] = "fast".into());
}

#[test]
fn a_placement_past_the_grid_is_a_finding_not_a_stop() {
    // The lint theme's grid has 6 rows. A node in row 7 cannot be laid out, and lint says
    // so, with the rest of what validation finds, instead of stopping.
    let late =
        serde_json::json!({ "type": "text", "role": "body", "text": "Late", "at": { "col": [1, 12], "row": 7 } });
    let dir = bundle("past-grid", &deck(serde_json::json!({}), serde_json::json!({ "t": late })), |_| {});
    let (exit, found) = lint(&dir);
    assert_eq!(exit, 1, "{found:#?}");
    let past: Vec<&str> =
        found.iter().filter(|f| f["code"] == "E102").map(|f| f["message"].as_str().unwrap()).collect();
    assert_eq!(past, ["`t` is placed in row 7, past the theme's grid, which has 6 rows"], "{found:#?}");
}

#[test]
fn the_example_decks_and_b1_lint_clean() {
    for bundle in [
        "docs/examples/revenue.deck.json",
        "docs/examples/charts.deck.json",
        "docs/examples/trails.deck.json",
        "docs/examples/higher-ed.deck.json",
        "docs/examples/ridgeline.deck.json",
        "tests/bench/b1.scaena",
    ] {
        let (exit, found) = lint(&Path::new(ROOT).join(bundle));
        assert_eq!((exit, found.len()), (0, 0), "{bundle}: {found:#?}");
    }
}

/// A finding as a line of the golden: what, in which format and state, where, and whether
/// it carries a fix.
fn line(f: &Value) -> String {
    let s = |k: &str| f[k].as_str().unwrap_or("-").to_string();
    let fix = if f.get("fix").is_some() { " fix" } else { "" };
    format!("{} {} {} {} {}{fix}", s("code"), s("format"), s("state"), s("node"), s("path"))
}

#[test]
fn the_torture_deck_lints_to_its_golden() {
    let (_, found) = lint(&Path::new(ROOT).join("tests/fixtures/torture.scaena"));
    let text: String = found.iter().map(|f| line(f) + "\n").collect();
    let golden = Path::new(ROOT).join("tests/golden/lint/torture.txt");
    if std::env::var_os("SCAENA_BLESS").is_some() {
        std::fs::create_dir_all(golden.parent().unwrap()).unwrap();
        std::fs::write(&golden, &text).unwrap();
    }
    let want = std::fs::read_to_string(&golden).unwrap_or_default();
    assert_eq!(text, want, "tests/golden/lint/torture.txt: review the change, then SCAENA_BLESS=1 rewrites it");
}

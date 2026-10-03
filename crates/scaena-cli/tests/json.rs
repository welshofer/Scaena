//! `--json` on every command (PLAN 1.14, SPEC §7.1): stdout is exactly one JSON value,
//! the command's result or, when it stops with exit 2 or 3, `{ "error": { … } }`. And
//! `inspect --timeline` and `--data` show what a build moves and what a chart reads.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";
const EXAMPLE: &str = "../../docs/examples/revenue.deck.json";
const PATCH: &str = "../../docs/examples/revenue.patch.json";

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("json-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Runs `scaena --json ARGS`, and returns its exit code and the one JSON value it printed.
fn json(args: &[&str]) -> (i32, Value) {
    let out = scaena(&[&["--json"], args].concat());
    let stdout = String::from_utf8(out.stdout).unwrap();
    let v = serde_json::from_str(&stdout)
        .unwrap_or_else(|e| panic!("{args:?}: stdout is not one JSON value ({e}):\n{stdout}"));
    (out.status.code().unwrap(), v)
}

/// What a command's JSON must say.
type Check = fn(&Value) -> bool;

#[test]
fn every_command_prints_one_json_value() {
    let dir = scratch("every");
    let png = dir.join("pretty.png");
    let saved = dir.join("saved.scaena");
    let pdf = dir.join("deck.pdf");
    let svgs = dir.join("svgs");
    let theme = "../../docs/examples/themes/dusk.theme.json".to_string();
    let scn = "../../docs/examples/revenue.deck.scn";
    let cases: Vec<(Vec<&str>, i32, Check)> = vec![
        (vec!["validate", TORTURE], 0, |v| v.as_array().is_some_and(Vec::is_empty)),
        (vec!["lint", EXAMPLE], 0, |v| v.as_array().is_some_and(Vec::is_empty)),
        (vec!["inspect", TORTURE, "--state", "chart"], 0, |v| v[0]["state_id"] == "chart"),
        (vec!["diff", TORTURE, "--from", "chart", "--to", "chart-next"], 0, Value::is_object),
        (vec!["compile", scn], 0, |v| {
            v["out"].is_null() && v["findings"] == serde_json::json!([]) && v["deck"].is_object()
        }),
        (vec!["decompile", EXAMPLE], 0, |v| {
            v["out"].is_null() && v["scn"].as_str().is_some_and(|s| s.starts_with("deck"))
        }),
        (vec!["render", TORTURE, "--state", "pretty", "--out", png.to_str().unwrap()], 0, |v| {
            v["size"] == serde_json::json!([1920, 1080])
        }),
        (vec!["save", TORTURE, "--to", saved.to_str().unwrap()], 0, |v| v["manifest"].is_object()),
        (vec!["export", EXAMPLE, "--format", "spine"], 0, |v| v["format"] == "spine" && v["spine"].is_object()),
        (vec!["export", EXAMPLE, "--format", "pdf", "--out", pdf.to_str().unwrap()], 0, |v| {
            v["format"] == "pdf"
                && v["pages"].as_array().is_some_and(|p| !p.is_empty())
                && v["bytes"].as_u64() > Some(0)
        }),
        (
            vec!["export", TORTURE, "--format", "svg", "--states", "liga,emoji", "--out", svgs.to_str().unwrap()],
            0,
            |v| {
                v["pages"] == serde_json::json!(["liga", "emoji"])
                    && v["files"].as_array().is_some_and(|f| f.len() == 2)
                    && v["size"] == serde_json::json!([1920, 1080])
            },
        ),
        (vec!["theme", EXAMPLE, "--apply", &theme, "--dry-run"], 0, |v| v["applied"] == false),
        (vec!["patch", EXAMPLE, "--ops", PATCH, "--dry-run"], 0, |v| {
            v["applied"] == false
                && v["patch"].as_array().is_some_and(|p| p.len() == 9)
                && v["added"] == serde_json::json!([])
        }),
    ];
    for (args, code, check) in &cases {
        let (got, v) = json(args);
        assert_eq!(got, *code, "{args:?}: {v:#}");
        assert!(check(&v), "{args:?}: {v:#}");
    }
}

#[test]
fn a_command_that_stops_prints_an_error_object() {
    let ops = scratch("errors").join("ops.json");
    std::fs::write(
        &ops,
        r#"[{ "op": "set_text", "node": "title", "text": "Q3" }, { "op": "remove_node", "id": "nobody" }]"#,
    )
    .unwrap();
    for (args, exit, plan) in [
        (vec!["inspect", TORTURE, "--state", "nope"], 2, None),
        (vec!["render", TORTURE, "--state", "pretty", "--size", "wide"], 2, None),
        (vec!["validate", "no/such/bundle"], 2, None),
        (vec!["export", EXAMPLE, "--format", "spine", "--states", "intro"], 2, None),
        (vec!["export", EXAMPLE, "--format", "gif"], 2, None),
        (vec!["inspect", TORTURE, "--no-such-flag"], 2, None),
        (vec!["patch", EXAMPLE, "--ops", ops.to_str().unwrap(), "--dry-run"], 2, None),
        (vec!["export", EXAMPLE, "--format", "pdf"], 2, None),
        // A video is a file: it needs `--out`.
        (vec!["export", EXAMPLE, "--format", "mp4", "--states", "intro,revenue"], 2, None),
        (vec!["export", EXAMPLE, "--format", "html"], 3, Some("2.5")),
        (vec!["serve", TORTURE], 3, Some("2.x")),
    ] {
        let (got, v) = json(&args);
        assert_eq!(got, exit, "{args:?}: {v:#}");
        let error = &v["error"];
        assert_eq!(error["exit"], exit, "{args:?}: {v:#}");
        assert!(error["message"].as_str().is_some_and(|m| !m.is_empty()), "{args:?}: {v:#}");
        assert_eq!(error["plan"].as_str(), plan, "{args:?}: {v:#}");
    }
    // Without `--json` the same stop is a line on stderr, and stdout stays empty.
    let out = scaena(&["inspect", TORTURE, "--state", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(out.stdout.is_empty());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown state `nope`"));
}

#[test]
fn inspect_timeline_places_each_motion_on_its_states_clock() {
    let (code, states) = json(&["inspect", TORTURE, "--timeline"]);
    assert_eq!(code, 0, "{states:#}");
    let states = states.as_array().unwrap();
    // The deck's states end to end: each starts where the one before it ends.
    let mut end = 0.0;
    for s in states {
        let t = &s["timeline"];
        let (start, span, hold) =
            (t["start"].as_f64().unwrap(), t["span"].as_f64().unwrap(), t["hold"].as_f64().unwrap());
        assert!((start - end).abs() < 1e-3, "{}: starts at {start}, not {end}", s["state_id"]);
        // A state's span is its transition or its last motion, whichever ends later.
        let last = t["motions"].as_array().unwrap().iter().map(|m| m["end"].as_f64().unwrap()).fold(0.0, f64::max);
        let transition = t["transition"]["duration"].as_f64().unwrap();
        assert!((span - last.max(transition)).abs() < 1e-3, "{}: span {span}", s["state_id"]);
        end = start + span + hold;
    }

    // Case 40: choreography in sequence and parallel, by words, children, and marks.
    let motion = states.iter().find(|s| s["state_id"] == "motion").unwrap();
    let t = &motion["timeline"];
    assert_eq!(t["transition"]["duration"], 0.0, "the state cuts in");
    let motions = t["motions"].as_array().unwrap();
    let on = |node: &str| motions.iter().find(|m| m["node"] == node).unwrap_or_else(|| panic!("{t:#}"));
    let title = on("mo-title");
    assert_eq!(
        (title["motion"].as_str(), title["split"].as_str(), title["units"].as_u64()),
        (Some("enter"), Some("words"), Some(6))
    );
    assert_eq!(
        (title["start"].as_f64(), title["stagger"].as_f64(), title["duration"].as_f64()),
        (Some(0.0), Some(60.0), Some(420.0))
    );
    assert_eq!(title["end"], 720.0, "the last of six words starts 5 × 60 ms in and takes 420");
    assert_eq!(title["from"]["opacity"], 0.0);
    assert_eq!(on("mo-cards")["split"], "children");
    let bars = on("mo-bars");
    assert_eq!(bars["split"], "marks");
    assert!(bars["curve"]["spring"]["stiffness"].is_number(), "{bars:#}");
    let dot = on("mo-dot");
    assert_eq!(dot["motion"], "emphasis");
    assert_eq!(dot["peak"]["scale"], serde_json::json!([1.25, 1.25]));

    // Case 42: a look that tints toward a theme color, and one that draws an outline on.
    let (_, morph) = json(&["inspect", TORTURE, "--state", "morph", "--timeline"]);
    let motions = morph[0]["timeline"]["motions"].as_array().unwrap();
    let arrow = motions.iter().find(|m| m["node"] == "mf-arrow").unwrap();
    assert_eq!(arrow["from"], serde_json::json!({ "progress": 0.0 }));
    let badge = motions.iter().find(|m| m["node"] == "mf-badge").unwrap();
    assert!(badge["peak"]["tint"]["color"].as_str().is_some_and(|c| c.starts_with('#')), "{badge:#}");

    // For a person: the slot, then a line per motion.
    let out = scaena(&["inspect", TORTURE, "--state", "motion", "--timeline"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("· cut"), "{text}");
    assert!(text.contains("▸ mo-title     enter by words ×6 0–720 ms, 420 ms each, 60 ms apart"), "{text}");
}

#[test]
fn inspect_data_shows_the_rows_each_chart_and_table_reads() {
    let (code, states) = json(&["inspect", TORTURE, "--state", "chart-kinds-2-next", "--data"]);
    assert_eq!(code, 0, "{states:#}");
    let table = &states[0]["data"]["k-table"];
    assert_eq!(table["source"], "regions");
    assert_eq!(table["columns"], serde_json::json!(["region", "rev", "growth"]));
    assert_eq!(table["types"], serde_json::json!(["string", "number", "number"]));
    // Through the state's `dataTransform`: sorted by growth, highest first.
    let regions: Vec<&str> = table["rows"].as_array().unwrap().iter().map(|r| r[0].as_str().unwrap()).collect();
    assert_eq!(regions, ["Asia Pacific", "North America", "Latin America", "Europe"]);
    // A chart's rows, from a CSV.
    let (_, states) = json(&["inspect", EXAMPLE, "--state", "revenue", "--data"]);
    let rev = &states[0]["data"]["rev"];
    assert_eq!(rev["rows"].as_array().unwrap().len(), 12, "{rev:#}");
    assert_eq!(rev["rows"][0], serde_json::json!(["2025-Q4", "Core", 18.2, 1210.0]));

    let out = scaena(&["inspect", EXAMPLE, "--state", "revenue", "--data"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("data @q3: 12 rows × 4 columns"), "{text}");
    assert!(text.contains("2026-Q3  Enterprise  14.3     88"), "{text}");
}

#[test]
fn inspect_keeps_its_snapshot_without_the_views() {
    // The views add keys; the snapshot is the same either way.
    let (_, plain) = json(&["inspect", TORTURE, "--state", "chart-next"]);
    let (_, all) = json(&["inspect", TORTURE, "--state", "chart-next", "--timeline", "--data"]);
    assert!(plain[0].get("timeline").is_none() && plain[0].get("data").is_none());
    for key in ["state_id", "slide_id", "layout", "nodes", "entered", "exited"] {
        assert_eq!(plain[0][key], all[0][key], "{key}");
    }
}

#[test]
fn diff_reads_as_changes_for_a_person_and_as_an_object_for_an_agent() {
    let (_, v) = json(&["diff", TORTURE, "--from", "chart", "--to", "chart-next"]);
    let changes = v.as_object().unwrap();
    assert!(!changes.is_empty(), "{v:#}");
    let out = scaena(&["diff", TORTURE, "--from", "chart", "--to", "chart-next"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    // One line per node: `+` enters, `-` exits, `~` changes these keys.
    assert_eq!(text.lines().count(), changes.len(), "{text}");
    for (id, change) in changes {
        let sign = match change.as_object().unwrap().keys().next().unwrap().as_str() {
            "enter" => "+",
            "exit" => "-",
            _ => "~",
        };
        assert!(text.lines().any(|l| l.starts_with(&format!("{sign} {id}"))), "{id}: {text}");
    }
    let out = scaena(&["diff", TORTURE, "--from", "chart", "--to", "chart"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap().trim(), "no changes from `chart` to `chart`");
}

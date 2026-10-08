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
    let kept = dir.join("kept.scaena");
    let pdf = dir.join("deck.pdf");
    let svgs = dir.join("svgs");
    let spine = dir.join("projection").join("spine.json");
    let theme = "../../docs/examples/themes/dusk.theme.json".to_string();
    let gutter = dir.join("gutter.json");
    std::fs::write(&gutter, r#"[{ "op": "replace", "path": "/grid/gutter", "value": 32 }]"#).unwrap();
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
        // `--history` starts the bundle's history, and the manifest lists it.
        (vec!["save", TORTURE, "--to", kept.to_str().unwrap(), "--history"], 0, |v| {
            v["manifest"]["files"].get("history/deck.loro").is_some()
        }),
        (vec!["history", kept.to_str().unwrap()], 0, |v| v["versions"].as_array().is_some_and(|v| v.len() == 1)),
        (vec!["export", EXAMPLE, "--format", "spine"], 0, |v| v["format"] == "spine" && v["spine"].is_object()),
        // Written, the spine is in its file, and the result names its renders.
        (vec!["export", EXAMPLE, "--format", "spine", "--out", spine.to_str().unwrap()], 0, |v| {
            v.get("spine").is_none()
                && v["files"].as_array().is_some_and(|f| f.len() == 6)
                && v["size"] == serde_json::json!([480, 270])
        }),
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
        (vec!["theme", EXAMPLE, "--edit", gutter.to_str().unwrap(), "--dry-run"], 0, |v| {
            v["applied"] == false && v["paths"] == serde_json::json!(["/grid/gutter"]) && v["refused"] == false
        }),
        (vec!["files", TORTURE], 0, |v| {
            v["files"].as_array().is_some_and(|f| f.iter().any(|x| x["path"] == "assets/test-card.png"))
        }),
        (vec!["data", EXAMPLE, "q3"], 0, |v| {
            v["edited"] == false && v["sheet"]["rows"].as_array().is_some_and(|r| r.len() == 12)
        }),
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
        (vec!["data", EXAMPLE, "q4"], 2, None),
        (vec!["render", TORTURE, "--state", "pretty", "--size", "wide"], 2, None),
        (vec!["validate", "no/such/bundle"], 2, None),
        (vec!["export", EXAMPLE, "--format", "spine", "--states", "intro"], 2, None),
        (vec!["export", EXAMPLE, "--format", "gif"], 2, None),
        (vec!["inspect", TORTURE, "--no-such-flag"], 2, None),
        (vec!["patch", EXAMPLE, "--ops", ops.to_str().unwrap(), "--dry-run"], 2, None),
        (vec!["export", EXAMPLE, "--format", "pdf"], 2, None),
        // A video is a file: it needs `--out`.
        (vec!["export", EXAMPLE, "--format", "mp4", "--states", "intro,revenue"], 2, None),
        // So is a single-file page; a scaena built without the web player stops with 3
        // (scaena-ops' tests/html.rs).
        (vec!["export", EXAMPLE, "--format", "html"], 2, None),
        // The GPU paints only a video's frames,
        (vec!["export", EXAMPLE, "--format", "png", "--painter", "gpu"], 2, None),
        // and only in a scaena built with it; one that has it stops at the missing `--out`.
        (
            vec!["export", EXAMPLE, "--format", "mp4", "--painter", "gpu"],
            if cfg!(feature = "gpu") { 2 } else { 3 },
            if cfg!(feature = "gpu") { None } else { Some("2.22") },
        ),
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
    // Where each is written, and its delay there: what `time_motion` sets (PLAN 2.44).
    assert_eq!((title["written"].as_str(), title["delay"].as_f64()), (Some("/states/41/choreography/0"), Some(0.0)));
    assert_eq!((dot["written"].as_str(), dot["delay"].as_f64()), (Some("/states/41/choreography/3"), Some(900.0)));
    assert_eq!(title["moving"], serde_json::json!([0.0, 720.0]));

    // Case 42: a look that tints toward a theme color, and one that draws an outline on.
    let (_, morph) = json(&["inspect", TORTURE, "--state", "morph", "--timeline"]);
    let motions = morph[0]["timeline"]["motions"].as_array().unwrap();
    let arrow = motions.iter().find(|m| m["node"] == "mf-arrow").unwrap();
    assert_eq!(arrow["from"], serde_json::json!({ "progress": 0.0 }));
    assert_eq!(arrow["written"], "/states/43/props/mf-arrow/enter", "the arrow's own entrance, where it lives");
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

/// What stands where (ADR-0013): each node's box at rest, and what draws at a point,
/// topmost first, in the deck's own canvas or one of its formats.
#[test]
fn inspect_says_what_stands_where() {
    let (code, states) = json(&["inspect", TORTURE, "--state", "containers", "--boxes"]);
    assert_eq!(code, 0, "{states:#}");
    let boxes = &states[0]["boxes"];
    let label = &boxes["card-tag-label"];
    assert_eq!(label["parent"], "card");
    assert_eq!(label["draws"], true);
    assert_eq!(boxes["marks"]["draws"], false);
    let rect: Vec<f64> = label["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let at = format!("{},{}", rect[0] + rect[2] / 2.0, rect[1] + rect[3] / 2.0);
    let (code, states) = json(&["inspect", TORTURE, "--state", "containers", "--at", &at]);
    assert_eq!(code, 0, "{states:#}");
    let hits = states[0]["hits"].as_array().unwrap();
    assert_eq!(hits[0]["node"], "card-tag-label");
    assert_eq!(hits[0]["containers"], serde_json::json!(["card"]));
    // A text says where a caret put at the point stands, in characters; a shape does not.
    assert!(hits[0]["offset"].as_u64().is_some(), "{hits:#?}");
    assert!(hits.iter().filter(|h| h["node"] != "card-tag-label").all(|h| h.get("offset").is_none()), "{hits:#?}");
    assert!(states[0].get("boxes").is_none());

    // In a format, the boxes are that format's layout's.
    let (code, tall) = json(&["inspect", TORTURE, "--state", "formats", "--boxes", "--format", "9:16"]);
    assert_eq!(code, 0, "{tall:#}");
    for (node, b) in tall[0]["boxes"].as_object().unwrap() {
        let r: Vec<f64> = b["rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!(r[0] + r[2] <= 1080.01, "{node}: {r:?} past the 9:16 canvas");
    }
    // A point that is no point, and a format the deck does not list, are errors that say so.
    assert_eq!(scaena(&["inspect", TORTURE, "--at", "middle"]).status.code(), Some(2));
    let (code, err) = json(&["inspect", TORTURE, "--state", "formats", "--boxes", "--format", "4:3"]);
    assert_ne!(code, 0);
    assert!(err["error"]["message"].as_str().is_some_and(|m| m.contains("4:3")), "{err:#}");

    // For a person: a line per box, and the hits, topmost first.
    let out = scaena(&["inspect", TORTURE, "--state", "containers", "--at", &at]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(", topmost first:") && text.contains("    card-tag-label (in card), a caret after "),
        "{text}"
    );
}

#[test]
fn inspect_says_what_an_inspector_offers() {
    let (code, states) = json(&["inspect", EXAMPLE, "--state", "revenue", "--choices", "title"]);
    assert_eq!(code, 0, "{states:#}");
    let c = &states[0]["choices"];
    assert_eq!(
        (c["node"].as_str(), c["type"].as_str(), c["state"].as_str()),
        (Some("title"), Some("text"), Some("revenue"))
    );
    let role = &c["fields"][0];
    assert_eq!(role["prop"], "role");
    assert_eq!(role["takes"]["kind"], "name");
    assert_eq!(role["takes"]["of"], "text-role");
    assert_eq!(
        (role["value"].as_str(), &role["lives"]),
        (Some("headline"), &serde_json::json!({ "state": "revenue" }))
    );
    let color = c["fields"].as_array().unwrap().iter().find(|f| f["prop"] == "style/color").unwrap();
    assert_eq!(color["takes"]["overrides"], true, "{color:#}");
    assert!(color.get("value").is_none() && color.get("lives").is_none(), "the role's color shows: {color:#}");

    // It names a node in a state.
    assert_eq!(scaena(&["inspect", EXAMPLE, "--choices", "title"]).status.code(), Some(2));
    let (code, err) = json(&["inspect", EXAMPLE, "--state", "intro", "--choices", "rev"]);
    assert_ne!(code, 0);
    assert!(err["error"]["message"].as_str().is_some_and(|m| m.contains("not on screen in `intro`")), "{err:#}");

    // For a person: a line per property, what it shows and where it lives, and what it takes.
    let out = scaena(&["inspect", EXAMPLE, "--state", "revenue", "--choices", "title"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("choices for title (text):"), "{text}");
    assert!(text.contains("role           headline, in revenue's delta · display, headline, title"), "{text}");
}

#[test]
fn inspect_picks_up_a_look_and_puts_it_on_other_nodes() {
    let (code, states) = json(&["inspect", EXAMPLE, "--state", "revenue", "--look", "title", "--onto", "note,rev"]);
    assert_eq!(code, 0, "{states:#}");
    let look = &states[0]["look"];
    assert_eq!((look["node"].as_str(), look["type"].as_str()), (Some("title"), Some("text")));
    assert_eq!(look["props"][0], serde_json::json!({ "prop": "role", "value": "headline" }));
    assert!(look["props"][1].get("value").is_none(), "the theme's family: {look:#}");
    let put = &states[0]["put"];
    assert_eq!(
        put["patch"],
        serde_json::json!([{ "op": "choose", "node": "note", "prop": "role", "value": "headline", "state": "revenue" }])
    );
    assert_eq!((put["took"].clone(), put["same"].clone()), (serde_json::json!(["note"]), serde_json::json!([])));
    assert_eq!(put["refused"], serde_json::json!([{ "node": "rev", "why": "a chart takes none of a text's look" }]));

    // It picks up a look in a state, and puts it down only once picked up.
    assert_eq!(scaena(&["inspect", EXAMPLE, "--look", "title"]).status.code(), Some(2));
    assert_eq!(scaena(&["inspect", EXAMPLE, "--state", "revenue", "--onto", "note"]).status.code(), Some(2));

    // For a person: each property and its value, then what it puts where, and what it does not.
    let out = scaena(&["inspect", EXAMPLE, "--state", "revenue", "--look", "title", "--onto", "note,rev"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("the look of title:\n    role: \"headline\"\n    style/family: the theme's"), "{text}");
    assert!(
        text.contains("put on note: [{\"op\":\"choose\"") && text.contains("rev: a chart takes none of a text's look"),
        "{text}"
    );
}

#[test]
fn inspect_says_what_a_state_offers() {
    let (code, states) = json(&["inspect", EXAMPLE, "--state", "mix", "--state-choices"]);
    assert_eq!(code, 0, "{states:#}");
    let c = &states[0]["state_choices"];
    assert_eq!(c["state"], "mix");
    let field = |prop: &str| c["fields"].as_array().unwrap().iter().find(|f| f["prop"] == prop).unwrap().clone();
    let layout = field("layout");
    assert_eq!(layout["takes"]["of"], "layout");
    assert_eq!(layout["takes"]["names"], serde_json::json!(["full", "figure", "narrow-figure"]), "{layout:#}");
    assert_eq!(
        (layout["value"].as_str(), &layout["lives"]),
        (Some("figure"), &serde_json::json!({ "state": "revenue" })),
        "`mix` takes its layout from `revenue`"
    );
    assert_eq!(field("transition/duration")["value"], "slow");
    assert_eq!(field("hold")["takes"]["kind"], "number");
    assert_eq!(field("notes")["takes"]["kind"], "text");

    // It says what a state offers: name the state.
    assert_eq!(scaena(&["inspect", EXAMPLE, "--state-choices"]).status.code(), Some(2));

    // For a person: a line per property, its value and where it is set, and what it takes.
    let out = scaena(&["inspect", EXAMPLE, "--state", "mix", "--state-choices"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("choices for state mix:"), "{text}");
    assert!(text.contains("layout               figure, set in revenue · full, figure, narrow-figure"), "{text}");
    assert!(text.contains("transition/ease      not set · standard, in, out, linear"), "{text}");
}

#[test]
fn inspect_says_what_may_be_inserted() {
    let (code, states) = json(&["inspect", TORTURE, "--state", "axes", "--inserts"]);
    assert_eq!(code, 0, "{states:#}");
    let offered = states[0]["inserts"].as_array().unwrap();
    let of = |kind: &str| offered.iter().filter(|i| i["node"]["type"] == kind).count();
    assert!(of("text") > 0 && of("image") > 0 && of("shader") > 0, "{offered:#?}");
    assert_eq!(of("shape"), 4, "a rectangle, an ellipse, a line, and an arrow");
    let rect = offered.iter().find(|i| i["label"] == "Shape · rect").unwrap();
    assert_eq!(rect["id"], "rect");
    assert!(rect["start"]["box"]["w"].as_f64().is_some_and(|w| w > 0.0), "{rect:#}");
    let shader = offered.iter().find(|i| i["node"]["type"] == "shader").unwrap();
    assert_eq!(
        (&shader["start"]["slot"], &shader["node"]["z"]),
        (&serde_json::json!("canvas"), &serde_json::json!(-1))
    );

    // It says what may be inserted in a state.
    assert_eq!(scaena(&["inspect", TORTURE, "--inserts"]).status.code(), Some(2));

    // For a person: a line per insert, the node it adds and the box it takes.
    let out = scaena(&["inspect", TORTURE, "--state", "axes", "--inserts"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("  inserts:\n"), "{text}");
    assert!(text.contains("    Shape · rect as rect…: "), "{text}");
}

#[test]
fn inspect_says_a_states_layers() {
    let (code, states) = json(&["inspect", EXAMPLE, "--state", "revenue", "--layers"]);
    assert_eq!(code, 0, "{states:#}");
    // Topmost first; the background and the subtitle leave in `revenue`, listed as `intro`
    // places them, hidden.
    let said: Vec<String> = (states[0]["layers"].as_array().unwrap().iter())
        .map(|l| format!("{} {} {}", l["node"].as_str().unwrap(), l["type"].as_str().unwrap(), l["shown"]))
        .collect();
    assert_eq!(said, ["note text true", "rev chart true", "subtitle text false", "title text true", "bg shader false"]);

    // It says a state's layers: name the state.
    assert_eq!(scaena(&["inspect", EXAMPLE, "--layers"]).status.code(), Some(2));

    // For a person: a line per node, under what holds it, those hidden marked.
    let out = scaena(&["inspect", EXAMPLE, "--state", "revenue", "--layers"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("  layers, topmost first:\n    note (text)\n    rev (chart)\n"), "{text}");
    assert!(text.contains("    bg (shader, hidden)\n"), "{text}");

    // A node locked (PLAN 2.95), by a patch of its own `locked`, says so.
    let dir = scratch("layers-locked");
    for sub in ["fonts", "themes", "data"] {
        copy_dir(&Path::new("../../docs/examples").join(sub), &dir.join(sub));
    }
    std::fs::copy(EXAMPLE, dir.join("deck.json")).unwrap();
    let ops = dir.join("lock.json");
    std::fs::write(&ops, r#"[{ "op": "add", "path": "/nodes/rev/locked", "value": true }]"#).unwrap();
    let bundle = dir.to_str().unwrap();
    assert_eq!(scaena(&["patch", bundle, "--ops", ops.to_str().unwrap()]).status.code(), Some(0));
    let (_, states) = json(&["inspect", bundle, "--state", "revenue", "--layers"]);
    let locked: Vec<&str> = (states[0]["layers"].as_array().unwrap().iter())
        .filter(|l| l["locked"] == true)
        .map(|l| l["node"].as_str().unwrap())
        .collect();
    assert_eq!(locked, ["rev"], "{states:#}");
    let out = scaena(&["inspect", bundle, "--state", "revenue", "--layers"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("    rev (chart, locked)\n"), "{text}");
}

/// `scaena files` (PLAN 2.59) says what uses each of a bundle's images, fonts, and data, and
/// takes out what nothing names, all or none: from a directory, and from a zip and its
/// manifest.
#[test]
fn files_says_what_uses_each_and_takes_out_what_nothing_names() {
    let dir = scratch("files").join("torture.scaena");
    copy_dir(Path::new(TORTURE), &dir);
    std::fs::write(dir.join("assets/stray.png"), b"not drawn").unwrap();
    std::fs::write(dir.join("data/old.csv"), b"a,b\n1,2\n").unwrap();
    let bundle = dir.to_str().unwrap();
    let (code, listed) = json(&["files", bundle]);
    assert_eq!(code, 0, "{listed:#}");
    let file = |path: &str| listed["files"].as_array().unwrap().iter().find(|f| f["path"] == path).cloned().unwrap();
    let card = file("assets/test-card.png");
    assert!(card["named"].as_array().unwrap().contains(&serde_json::json!({ "by": "node", "node": "image-cover" })));
    assert!(
        card["used"].as_array().unwrap().contains(&serde_json::json!({ "node": "image-cover", "states": ["images"] }))
    );
    assert_eq!(
        (file("assets/stray.png")["named"].clone(), file("data/old.csv")["type"].clone()),
        (serde_json::json!([]), "data".into())
    );
    assert!(
        listed["files"].as_array().unwrap().iter().all(|f| f["path"] != "deck.json" && f["path"] != "fonts/SOURCES.md")
    );

    // One something names stops them all; so does a file that is not one of them, or not there.
    let (code, r) = json(&["files", bundle, "--remove", "assets/stray.png,assets/test-card.png,deck.json,nope.png"]);
    assert_eq!((code, r["applied"].clone(), r["removed"].clone()), (1, false.into(), serde_json::json!([])), "{r:#}");
    let why: Vec<&str> = r["refused"].as_array().unwrap().iter().map(|f| f["why"].as_str().unwrap()).collect();
    assert!(why[0].starts_with("image ") && why[0].ends_with("names it: take that out of the deck first"), "{why:?}");
    assert_eq!(
        why[1..],
        ["deck.json is not one of the bundle's images, fonts, or data", "the bundle holds no nope.png"]
    );
    assert!(dir.join("assets/stray.png").is_file());

    // A dry run says so, and writes nothing; then they go.
    let (code, r) = json(&["files", bundle, "--remove", "assets/stray.png,data/old.csv", "--dry-run"]);
    assert_eq!((code, r["applied"].clone()), (0, false.into()), "{r:#}");
    assert!(dir.join("assets/stray.png").is_file());
    let out = scaena(&["files", bundle, "--remove", "assets/stray.png,data/old.csv"]);
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "took out assets/stray.png, data/old.csv\n");
    assert!(!dir.join("assets/stray.png").exists() && !dir.join("data/old.csv").exists());

    // From a saved zip, which lists it in its manifest: the manifest lists it no more.
    std::fs::write(dir.join("assets/stray.png"), b"not drawn").unwrap();
    let zip = dir.with_extension("saved.scaena");
    assert!(scaena(&["save", bundle, "--to", zip.to_str().unwrap()]).status.success());
    let zipped = zip.to_str().unwrap();
    let (code, r) = json(&["files", zipped, "--remove", "assets/stray.png"]);
    assert_eq!((code, r["applied"].clone()), (0, true.into()), "{r:#}");
    let (_, listed) = json(&["files", zipped]);
    assert!(listed["files"].as_array().unwrap().iter().all(|f| f["path"] != "assets/stray.png"));
    assert!(scaena(&["validate", zipped]).status.success());
    let bundle = scaena_store::Bundle::open(&zip).unwrap();
    let manifest: Value = serde_json::from_slice(&bundle.read("manifest.json").unwrap()).unwrap();
    assert!(
        manifest["files"].get("assets/stray.png").is_none()
            && manifest["files"].as_object().is_some_and(|f| !f.is_empty())
    );

    // For a person: each with what names it and where it draws, and what nothing names.
    let text = String::from_utf8(scaena(&["files", TORTURE]).stdout).unwrap();
    assert!(text.starts_with("images\n  assets/") && text.contains("\n  assets/test-card.png  "), "{text}");
    assert!(text.contains("    image-cover in images\n"), "{text}");
}

/// A bundle's versions (PLAN 2.60): listed by author and time; the deck as it was after one,
/// by number or by id; two compared, and one with the deck now; one restored, with its data
/// file, as one change; and one whose deck names a file gone since refused.
#[test]
fn history_lists_shows_compares_and_restores_versions() {
    use serde_json::json as j;
    let dir = scratch("history");
    let b = dir.join("talk");
    assert!(scaena(&["save", EXAMPLE, "--to", b.to_str().unwrap(), "--history"]).status.success());
    let bundle = b.to_str().unwrap();
    let ops = dir.join("ops.json");
    let patch = |ops_json: &str| {
        std::fs::write(&ops, ops_json).unwrap();
        assert!(scaena(&["patch", bundle, "--ops", ops.to_str().unwrap()]).status.success(), "{ops_json}");
    };
    patch(r#"[{"op": "set_text", "node": "title", "text": "Revenue tripled"}]"#);
    let edits = dir.join("edits.json");
    std::fs::write(&edits, r#"[{"op": "set", "row": 0, "column": "revenue", "value": "9"}]"#).unwrap();
    assert!(scaena(&["data", bundle, "q3", "--edits", edits.to_str().unwrap()]).status.success());

    let (code, listed) = json(&["history", bundle]);
    assert_eq!(code, 0, "{listed:#}");
    let versions = listed["versions"].as_array().unwrap().clone();
    let said: Vec<(i64, &str)> =
        versions.iter().map(|v| (v["n"].as_i64().unwrap(), v["message"].as_str().unwrap_or(""))).collect();
    assert_eq!(said, [(1, "history begins"), (2, "patch: set_text"), (3, "data_edit q3: revenue of row 0")]);
    assert!(versions.iter().all(|v| v["author"] == "user" && v["at"].as_str().is_some_and(|t| t.ends_with('Z'))));

    // The deck as it was, by number or by id.
    let (_, seen) = json(&["history", bundle, "--at", "1"]);
    assert_eq!(seen["seen"]["deck"]["nodes"]["title"]["text"], "Q3 Review", "{seen:#}");
    let (_, seen) = json(&["history", bundle, "--at", versions[1]["id"].as_str().unwrap(), "--scn"]);
    assert!(seen["seen"]["scn"].as_str().unwrap().contains("Revenue tripled"));

    // Compared with the deck now, and two with each other.
    let (_, c) = json(&["history", bundle, "--diff", "1"]);
    let c = &c["compared"];
    assert_eq!(c["states"]["intro"]["changed"]["nodes"]["title"]["change"]["text"], "Revenue tripled", "{c:#}");
    assert_eq!((c["files"].clone(), c["to"].clone()), (j!(["data/q3-revenue.csv"]), Value::Null));
    let (_, c) = json(&["history", bundle, "--diff", "2,3"]);
    assert_eq!(
        (c["compared"]["states"].clone(), c["compared"]["files"].clone()),
        (j!({}), j!(["data/q3-revenue.csv"]))
    );

    // Restored: the deck and its data file, as one change; a dry run writes nothing.
    let csv = std::fs::read(b.join("data/q3-revenue.csv")).unwrap();
    let (code, r) = json(&["history", bundle, "--restore", "1", "--dry-run"]);
    assert_eq!((code, r["restored"]["applied"].clone()), (0, false.into()), "{r:#}");
    assert_eq!(std::fs::read(b.join("data/q3-revenue.csv")).unwrap(), csv);
    let (code, r) = json(&["history", bundle, "--restore", "1"]);
    let restored = &r["restored"];
    assert_eq!(
        (code, restored["applied"].clone(), restored["files"].clone()),
        (0, true.into(), j!(["data/q3-revenue.csv"]))
    );
    let (_, c) = json(&["history", bundle, "--diff", "1"]);
    assert_eq!((c["compared"]["states"].clone(), c["compared"]["files"].clone()), (j!({}), j!([])), "{c:#}");
    let (_, listed) = json(&["history", bundle]);
    let last = listed["versions"].as_array().unwrap().last().unwrap().clone();
    assert_eq!(last["message"], format!("history --restore {}", versions[0]["id"].as_str().unwrap()));

    // A version whose deck names a file taken out since is refused, and says why.
    std::fs::create_dir_all(b.join("assets")).unwrap();
    std::fs::copy(Path::new(TORTURE).join("assets/test-card.png"), b.join("assets/card.png")).unwrap();
    patch(r#"[{"op": "add_node", "id": "photo", "node": {"type": "image", "src": "assets/card.png"}}]"#);
    patch(r#"[{"op": "remove_node", "id": "photo"}]"#);
    assert!(scaena(&["files", bundle, "--remove", "assets/card.png"]).status.success());
    let (_, listed) = json(&["history", bundle]);
    let shown = (listed["versions"].as_array().unwrap().len() - 1).to_string();
    let (code, r) = json(&["history", bundle, "--restore", &shown]);
    assert_eq!((code, r["restored"]["applied"].clone()), (1, false.into()), "{r:#}");
    assert!(r["restored"]["added"].as_array().unwrap().iter().any(|f| f["code"] == "E102"), "{r:#}");

    // Ask one thing at a time; and a bundle without a history says how to begin one.
    let (code, e) = json(&["history", bundle, "--diff", "1,2,3"]);
    assert_eq!(code, 2, "{e:#}");
    let (code, e) = json(&["history", EXAMPLE]);
    assert_eq!(code, 2);
    assert!(e["error"]["message"].as_str().unwrap().contains("scaena save --history"), "{e:#}");
    let text = String::from_utf8(scaena(&["history", bundle, "--diff", "1,2"]).stdout).unwrap();
    assert_eq!(text, "from version 1 to version 2\n  state intro:\n    title: text \"Revenue tripled\"\n");
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

/// `inspect --targets` says what holds a node and where it may go (ADR-0013); `--snap` lands
/// a dropped box there with the patch that puts it there, which `scaena patch` applies.
#[test]
fn inspect_says_where_a_node_may_go() {
    let (code, states) = json(&["inspect", TORTURE, "--state", "containers", "--targets", "board-dot"]);
    assert_eq!(code, 0, "{states:#}");
    let t = &states[0]["targets"];
    assert_eq!((t["by"].as_str(), t["parent"].as_str()), (Some("cells"), Some("board")));
    assert_eq!(t["columns"].as_array().map(Vec::len), Some(2));
    assert_eq!(t["slots"].as_object().unwrap().keys().collect::<Vec<_>>(), ["mark", "note", "photo"]);
    assert_eq!(t["snaps"], serde_json::json!(["move", "resize", "slot"]));
    let (_, states) = json(&["inspect", TORTURE, "--state", "containers", "--targets", "stat-b"]);
    assert_eq!(states[0]["targets"]["flow"], serde_json::json!(["stat-a", "stat-b", "stat-c"]));
    assert_eq!(states[0]["targets"]["snaps"], serde_json::json!(["order"]));

    // A box dropped on another slot of the template lands in it; its patch, applied, puts
    // the node there.
    let bundle = scratch("targets").join("torture.scaena");
    copy_dir(Path::new(TORTURE), &bundle);
    let b = bundle.to_str().unwrap();
    let (_, states) = json(&["inspect", b, "--state", "containers", "--targets", "case"]);
    let right: Vec<f64> =
        states[0]["targets"]["slots"]["right"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let to = format!("{},{},{},{}", right[0] + 20.0, right[1] + 20.0, right[2] / 2.0, right[3] / 2.0);
    let (code, states) =
        json(&["inspect", b, "--state", "containers", "--targets", "case", "--snap", "slot", "--to", &to]);
    assert_eq!(code, 0, "{states:#}");
    let snapped = &states[0]["snapped"];
    assert_eq!(
        snapped["patch"],
        serde_json::json!([{ "op": "place", "node": "case", "at": { "in": "right" }, "state": "containers" }])
    );
    // `case` shows in every state, placed by its own `at`: the move lives there, so lint
    // finds it in the other states it now stands over.
    let ops = bundle.parent().unwrap().join("ops.json");
    std::fs::write(&ops, snapped["patch"].to_string()).unwrap();
    let (code, patched) = json(&["patch", b, "--ops", ops.to_str().unwrap(), "--dry-run"]);
    assert_eq!(code, 1, "{patched:#}");
    assert!(patched["added"].as_array().unwrap().iter().any(|f| f["state"] != "containers"), "{patched:#}");
    // `tally` is in `containers` alone: moved a column right, it stands there.
    let (_, states) = json(&["inspect", b, "--state", "containers", "--targets", "tally"]);
    let cell: Vec<f64> = states[0]["targets"]["cell"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    let to = format!("{},{},{},{}", cell[0] + 150.0, cell[1], cell[2], cell[3]);
    let (_, states) =
        json(&["inspect", b, "--state", "containers", "--targets", "tally", "--snap", "move", "--to", &to]);
    let snapped = &states[0]["snapped"];
    std::fs::write(&ops, snapped["patch"].to_string()).unwrap();
    // The torture deck holds its failing cases' errors; the move adds none.
    let (_, patched) = json(&["patch", b, "--ops", ops.to_str().unwrap()]);
    assert_eq!(
        (&patched["applied"], &patched["added"]),
        (&serde_json::json!(true), &serde_json::json!([])),
        "{patched:#}"
    );
    let (_, states) = json(&["inspect", b, "--state", "containers", "--targets", "tally"]);
    assert_eq!(states[0]["targets"]["cell"], snapped["cell"]);
    assert_ne!(states[0]["targets"]["cell"], serde_json::json!(cell));

    // What it needs, and a way that does not place the node, are errors that say so.
    assert_eq!(scaena(&["inspect", TORTURE, "--targets", "case"]).status.code(), Some(2));
    assert_eq!(
        scaena(&["inspect", TORTURE, "--state", "containers", "--targets", "case", "--snap", "move"]).status.code(),
        Some(2)
    );
    assert_eq!(
        scaena(&[
            "inspect",
            TORTURE,
            "--state",
            "containers",
            "--targets",
            "case",
            "--snap",
            "sideways",
            "--to",
            "0,0,1,1"
        ])
        .status
        .code(),
        Some(2)
    );
    let (code, err) = json(&[
        "inspect",
        TORTURE,
        "--state",
        "containers",
        "--targets",
        "stat-a",
        "--snap",
        "free",
        "--to",
        "0,0,10,10",
    ]);
    assert_ne!(code, 0);
    assert!(err["error"]["message"].as_str().is_some_and(|m| m.contains("it snaps by order")), "{err:#}");
    let (code, err) = json(&["inspect", TORTURE, "--state", "containers", "--targets", "nobody"]);
    assert_ne!(code, 0);
    assert!(err["error"]["message"].as_str().is_some_and(|m| m.contains("`nobody`")), "{err:#}");

    // For a person: what holds it, its slots, and the patch.
    let out =
        scaena(&["inspect", TORTURE, "--state", "containers", "--targets", "case", "--snap", "slot", "--to", &to]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("targets: on the theme's grid") && text.contains("    right ") && text.contains("patch: [{"),
        "{text}"
    );
}

/// `inspect --arrange` puts several of one container's children in place at once (PLAN 2.42):
/// aligned, spread, moved together, or ordered, each one patch that `scaena patch` applies.
#[test]
fn inspect_arranges_several_nodes_at_once() {
    let arranged = |args: &[&str]| {
        let mut all = vec!["inspect", TORTURE, "--state", "containers", "--arrange"];
        all.extend_from_slice(args);
        let (code, states) = json(&all);
        assert_eq!(code, 0, "{args:?}: {states:#}");
        states[0]["arranged"].clone()
    };
    let place = |node: &str, at: serde_json::Value| serde_json::json!({ "op": "place", "node": node, "at": at, "state": "containers" });
    // The card takes the tally's left edge, a column of the grid; the tally, there already,
    // keeps its placement as it is.
    let left = arranged(&["tally,card", "--align", "left"]);
    assert_eq!(left["patch"], serde_json::json!([place("card", serde_json::json!({ "col": [1, 4], "row": [6, 8] }))]));
    assert_eq!(left["landed"].as_array().map(Vec::len), Some(2));
    // Up a row together.
    let up = arranged(&["tally,card", "--by", "0,-114"]);
    assert_eq!(
        up["patch"],
        serde_json::json!([
            place("tally", serde_json::json!({ "col": [1, 11], "row": 4 })),
            place("card", serde_json::json!({ "col": [9, 12], "row": [5, 7] })),
        ])
    );
    // The tally stands at the grid's left edge, so the two go no farther left at all.
    assert_eq!(arranged(&["card,tally", "--by=-150,0"])["patch"], serde_json::json!([]));
    // In front of what it overlaps in the card, and the card behind the rest: by `z`.
    let z = |node: &str, z: i64| serde_json::json!({ "op": "choose", "node": node, "prop": "z", "value": z, "state": "containers", "fork": false });
    assert_eq!(arranged(&["card-photo", "--order", "front"])["patch"], serde_json::json!([z("card-photo", 2)]));
    assert_eq!(arranged(&["card", "--order", "back"])["patch"], serde_json::json!([z("card", -1)]));
    // Listed before or after another of its container's children (PLAN 2.50): over or under
    // it by `z`, or, in a stack, before or after it in its order.
    assert_eq!(
        arranged(&["card-photo", "--before", "card-tag-label"])["patch"],
        serde_json::json!([z("card-photo", 2)])
    );
    assert_eq!(
        arranged(&["card-tag-label", "--after", "card-photo"])["patch"],
        serde_json::json!([z("card-tag-label", -1)])
    );
    let index = |node: &str, i: u32| place(node, serde_json::json!({ "index": i }));
    assert_eq!(
        arranged(&["stat-c", "--before", "stat-a"])["patch"],
        serde_json::json!([index("stat-a", 1), index("stat-b", 2)]),
        "the third first: the others renumbered after it"
    );
    // Before a child of another container, or into one: into it (`place` with `parent`),
    // placed as it places what it holds. Into another stack, at its place in the order.
    assert_eq!(
        arranged(&["tally-label", "--before", "stat-a-label"])["patch"],
        serde_json::json!([
            place("tally-label", serde_json::json!({ "parent": "stat-a", "index": 1 })),
            index("stat-a-label", 2)
        ])
    );
    // Onto the canvas, in the cells it stood in, and just over the card.
    let out = arranged(&["card-tag-label", "--before", "card"])["patch"].clone();
    assert_eq!(out[0], place("card-tag-label", serde_json::json!({ "parent": null, "col": 9, "row": 6 })), "{out:#}");
    assert_eq!(out[1], z("card-tag-label", 0), "{out:#}");
    // Into a frame, first among what it holds, by a `rect` inside its padding.
    let into = arranged(&["marks-dot", "--into", "card"])["patch"].clone();
    assert_eq!(into[0]["at"]["parent"], "card", "{into:#}");
    let rect: Vec<f64> = serde_json::from_value(into[0]["at"]["rect"].clone()).unwrap();
    assert!(rect[0] >= 0.0 && rect[1] >= 0.0, "inside the padding: {rect:?}");
    assert_eq!(into[1], z("marks-dot", 1), "{into:#}");

    // Applied, a patch moves what it says: the card now stands over the board, which lint
    // says, and nothing else.
    let bundle = scratch("arrange").join("torture.scaena");
    copy_dir(Path::new(TORTURE), &bundle);
    let b = bundle.to_str().unwrap();
    let ops = bundle.parent().unwrap().join("ops.json");
    std::fs::write(&ops, left["patch"].to_string()).unwrap();
    let (_, patched) = json(&["patch", b, "--ops", ops.to_str().unwrap()]);
    assert_eq!(patched["applied"], true, "{patched:#}");
    let added: Vec<&str> = patched["added"].as_array().unwrap().iter().filter_map(|f| f["code"].as_str()).collect();
    assert!(!added.is_empty() && added.iter().all(|c| *c == "E101"), "{patched:#}");
    let (_, boxes) = json(&["inspect", b, "--state", "containers", "--boxes"]);
    assert_eq!(boxes[0]["boxes"]["card"]["rect"][0], 96.0);
    // Moved into another stack, the label is listed there, and the deck stays valid.
    let moved = arranged(&["tally-label", "--before", "stat-a-label"]);
    std::fs::write(&ops, moved["patch"].to_string()).unwrap();
    let (_, patched) = json(&["patch", b, "--ops", ops.to_str().unwrap()]);
    assert_eq!(patched["applied"], true, "{patched:#}");
    let (_, listed) = json(&["inspect", b, "--state", "containers", "--layers"]);
    let stat_a =
        listed[0]["layers"].as_array().unwrap().iter().find(|l| l["node"] == "stats").unwrap()["children"][0].clone();
    let held: Vec<&str> = stat_a["children"].as_array().unwrap().iter().filter_map(|l| l["node"].as_str()).collect();
    assert_eq!(held, ["stat-a-figure", "tally-label", "stat-a-label"], "{stat_a:#}");

    // Children of two containers, a stack's children, and two ways at once are errors that
    // say why.
    let refused = |args: &[&str], says: &str| {
        let mut all = vec!["inspect", TORTURE, "--state", "containers", "--arrange"];
        all.extend_from_slice(args);
        let (code, err) = json(&all);
        assert_ne!(code, 0, "{args:?}");
        let message = err["error"]["message"].as_str().unwrap_or_default().to_string();
        assert!(message.contains(says), "{args:?}: {message}");
    };
    refused(&["tally,card-photo", "--align", "left"], "`card-photo` by `card`: arrange what one container holds");
    refused(&["stat-a,stat-b", "--align", "top"], "the stack `stats` places what it holds in its order");
    refused(&["tally,card", "--align", "left", "--order", "front"], "one way");
    refused(&["case,stats", "--spread", "down"], "three nodes or more");
    refused(&["tally,nobody", "--align", "left"], "`nobody`");
    refused(&["stat-a,stat-b", "--before", "stat-c"], "one node goes before or after another at a time");
    refused(&["card", "--before", "card-tag"], "a node goes into nothing it holds");
    refused(&["stats", "--into", "stat-a"], "a node goes into nothing it holds");
    refused(&["tally", "--into", "case"], "`case` holds nothing");
    refused(&["stat-a,stat-b", "--into", "card"], "one node goes into a container at a time");
    assert_eq!(scaena(&["inspect", TORTURE, "--arrange", "tally,card", "--align", "left"]).status.code(), Some(2));

    // For a person: where each lands, and the patch.
    let out = scaena(&["inspect", TORTURE, "--state", "containers", "--arrange", "tally,card", "--align", "left"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("arranged:") && text.contains("card lands at x 96") && text.contains("patch: [{"), "{text}");
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

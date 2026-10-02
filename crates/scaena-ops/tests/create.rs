//! Making a bundle from nothing but a theme, a data file, and ops (PLAN 1.17): what an
//! agent does through `deck_create`, `data_attach`, and `deck_patch`. Each step is checked
//! before it is written.

use scaena_ops::create::{Attach, Create, attach, create};
use scaena_ops::render::{Request, render};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const EXAMPLES: &str = "../../docs/examples";

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("create-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn dusk() -> PathBuf {
    Path::new(EXAMPLES).join("themes/dusk.theme.json")
}

fn q3() -> Attach {
    Attach { id: "q3".into(), file: Path::new(EXAMPLES).join("data/q3-revenue.csv"), schema: None, parse: None }
}

#[test]
fn a_bundle_from_a_theme_then_data_then_a_chart() {
    let dir = scratch("steps").join("deck");
    let made = create(&dir, &Create { theme: dusk(), title: Some("Q3".into()), ..Create::default() }).unwrap();
    assert!(made.created, "{made:#?}");
    assert_eq!(
        made.files,
        [
            "deck.json",
            "fonts/Fraunces-VF.ttf",
            "fonts/Inter-VF.ttf",
            "fonts/JetBrainsMono-VF.ttf",
            "themes/dusk.theme.json"
        ]
    );
    assert_eq!(made.errors, 0, "{:#?}", made.findings);
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(b.deck.meta.as_ref().and_then(|m| m.title.as_deref()), Some("Q3"));
    let families: Vec<&str> = b.deck.fonts.iter().map(|f| f.family.as_str()).collect();
    assert_eq!(families, ["Fraunces", "Inter", "JetBrains Mono"]);

    // The data file, typed by its values: the quarter reads `2025-Q4`, which is no date.
    let attached = attach(&b, &q3()).unwrap();
    assert!(attached.attached, "{attached:#?}");
    assert_eq!(attached.source, "data/q3-revenue.csv");
    let schema: Vec<(&str, &str)> = attached.schema.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(schema, [("quarter", "string"), ("product", "string"), ("revenue", "number"), ("customers", "number")]);
    assert_eq!(attached.rows, 12);
    let b = scaena_ops::open(&dir).unwrap();
    assert!(dir.join("data/q3-revenue.csv").is_file() && b.deck.data.contains_key("q3"));

    // A chart reading it, by a patch; then the state renders.
    let ops = json!([
        { "op": "add_node", "id": "rev", "state": "start",
          "node": { "type": "chart", "kind": "bar", "data": "@q3", "key": "quarter",
                    "x": { "field": "quarter", "type": "ordinal" },
                    "y": { "field": "revenue", "type": "quantitative" },
                    "dataTransform": [{ "aggregate": { "revenue": "sum(revenue)" }, "groupby": ["quarter"] }],
                    "alt": "Revenue by quarter.", "at": { "in": "canvas" } } },
    ]);
    let patched = scaena_ops::patch::patch(&b, &ops, false).unwrap();
    assert!(patched.applied, "{patched:#?}");
    let frame = render(&dir, &Request { state: "start".into(), ..Request::default() }).unwrap();
    assert_eq!(frame.size, [1920, 1080]);
    assert!(frame.png.starts_with(b"\x89PNG"));

    // Read back as JSON and as `.scn`; the spine replaced.
    let b = scaena_ops::open(&dir).unwrap();
    let read = scaena_ops::read::read(&b, true).unwrap();
    assert!(read.scn.unwrap().contains("rev"));
    let spine = json!({ "sections": [{ "id": "all", "beats": [
        { "id": "growth", "claim": "Revenue doubled.", "evidence": ["@q3"], "states": ["start"] }] }] });
    let updated = scaena_ops::read::spine_update(&b, spine, false).unwrap();
    assert!(updated.applied, "{updated:#?}");
    let b = scaena_ops::open(&dir).unwrap();
    assert_eq!(scaena_ops::read::spine(&b)["spine"]["sections"][0]["beats"][0]["id"], "growth");
}

#[test]
fn a_whole_deck_with_its_data_in_one_step() {
    let dir = scratch("whole").join("revenue");
    let deck: Value =
        serde_json::from_str(&std::fs::read_to_string(Path::new(EXAMPLES).join("revenue.deck.json")).unwrap()).unwrap();
    let made =
        create(&dir, &Create { theme: dusk(), deck: Some(deck), data: vec![q3()], ..Create::default() }).unwrap();
    assert!(made.created, "{made:#?}");
    // The example deck lints clean in its own bundle, and in this one: the same deck.
    assert_eq!(made.findings, vec![], "{:#?}", made.findings);
    for state in ["intro", "revenue", "mix", "close"] {
        render(&dir, &Request { state: state.into(), ..Request::default() }).unwrap();
    }
}

#[test]
fn nothing_is_written_that_does_not_validate() {
    let root = scratch("refused");
    // A deck naming data it does not have: E102, and no bundle.
    let dir = root.join("bad");
    let deck = json!({ "scaena": scaena_core::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 },
                       "nodes": { "c": { "type": "chart", "kind": "bar", "data": "@missing" } },
                       "states": [{ "id": "a", "props": { "c": {} } }] });
    let made = create(&dir, &Create { theme: dusk(), deck: Some(deck), ..Create::default() }).unwrap();
    assert!(!made.created && made.findings.iter().any(|f| f.code == "E102"), "{made:#?}");
    assert!(!dir.exists(), "nothing written");
    // A place that is not empty.
    let full = root.join("full");
    std::fs::create_dir_all(&full).unwrap();
    std::fs::write(full.join("notes.txt"), "mine").unwrap();
    let err = create(&full, &Create { theme: dusk(), ..Create::default() }).unwrap_err();
    assert!(err.message.contains("not an empty directory"), "{err}");
    // Data that does not fit the schema it is given: E103, and nothing attached.
    let dir = root.join("data");
    create(&dir, &Create { theme: dusk(), ..Create::default() }).unwrap();
    let b = scaena_ops::open(&dir).unwrap();
    let mut wrong = q3();
    wrong.schema = Some([("revenue".to_string(), "date".to_string())].into_iter().collect());
    let attached = attach(&b, &wrong).unwrap();
    assert!(!attached.attached && attached.added.iter().any(|f| f.code == "E103"), "{attached:#?}");
    assert!(!dir.join("data").exists());
    // A source by that id already.
    attach(&b, &q3()).unwrap();
    let b = scaena_ops::open(&dir).unwrap();
    let err = attach(&b, &q3()).unwrap_err();
    assert!(err.message.contains("has a data source `q3` already"), "{err}");
}

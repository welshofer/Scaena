//! A data source edited in place (PLAN 2.55, ADR-0014): read as a sheet, a cell set, rows
//! added and taken away, each change one write of the source's file and nothing else in it; a
//! value its column refuses, or edits that would leave the deck invalid, refused with why.

use scaena_ops::data::{DataEdit, DataEdited, data_edit};
use scaena_store::SaveOptions;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const REVENUE: &str = "../../docs/examples/revenue.deck.json";
const CSV: &str = "data/q3-revenue.csv";

/// The revenue example, saved where a test may write it.
fn revenue(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("data-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    let opts = SaveOptions { subset_fonts: false, now: "2026-10-05T00:00:00Z".into(), history: false };
    scaena_ops::open(Path::new(REVENUE)).unwrap().save(&dir, &opts).unwrap();
    dir
}

fn edit(dir: &Path, source: &str, edits: Value, dry_run: bool) -> Result<DataEdited, scaena_ops::OpsError> {
    let req: DataEdit = serde_json::from_value(json!({ "source": source, "edits": edits })).unwrap();
    data_edit(&scaena_ops::open(dir).unwrap(), &req, dry_run)
}

fn csv(dir: &Path) -> String {
    std::fs::read_to_string(dir.join(CSV)).unwrap()
}

#[test]
fn a_source_reads_as_a_sheet() {
    let dir = revenue("read");
    let read = edit(&dir, "q3", json!([]), false).unwrap();
    assert!(!read.edited && !read.refused);
    assert_eq!(read.file.as_deref(), Some(CSV));
    let columns: Vec<(&str, &str)> = read.sheet.columns.iter().map(|c| (c.name.as_str(), c.kind.name())).collect();
    assert_eq!(columns, [("quarter", "string"), ("product", "string"), ("revenue", "number"), ("customers", "number")]);
    assert_eq!(read.sheet.rows.len(), 12);
    assert_eq!(read.sheet.rows[0], ["2025-Q4", "Core", "18.2", "1210"]);
    assert!(read.sheet.problems.is_empty());
    let none = edit(&dir, "q4", json!([]), false).unwrap_err();
    assert_eq!(none.message, "the deck has no data source `q4`: it has q3");
}

#[test]
fn a_cell_set_writes_that_field_and_nothing_else() {
    let dir = revenue("set");
    let was = csv(&dir);
    let deck = std::fs::read(dir.join("deck.json")).unwrap();
    // A dry run says what it would do, and writes nothing.
    let dry = edit(&dir, "q3", json!([{ "op": "set", "row": 0, "column": "revenue", "value": 18.5 }]), true).unwrap();
    assert!(!dry.edited && !dry.refused);
    assert_eq!(dry.sheet.rows[0][2], "18.5");
    assert_eq!(csv(&dir), was);
    let set =
        edit(&dir, "q3", json!([{ "op": "set", "row": 0, "column": "revenue", "value": "18.5" }]), false).unwrap();
    assert!(set.edited, "{set:?}");
    assert_eq!(csv(&dir), was.replacen("2025-Q4,Core,18.2,1210", "2025-Q4,Core,18.5,1210", 1));
    assert_eq!(std::fs::read(dir.join("deck.json")).unwrap(), deck, "the deck is as it was");
    assert_eq!(set.errors, 0);
}

#[test]
fn rows_are_added_and_taken_away_in_one_write() {
    let dir = revenue("rows");
    let was = csv(&dir);
    let edits = json!([
        { "op": "add", "values": { "quarter": "2026-Q4", "product": "Core", "revenue": 25.1, "customers": 1600 } },
        { "op": "remove", "row": 0 },
    ]);
    let done = edit(&dir, "q3", edits, false).unwrap();
    assert!(done.edited, "{done:?}");
    assert_eq!(done.sheet.rows.len(), 12);
    let first = was.lines().nth(1).unwrap();
    assert_eq!(csv(&dir), format!("{}2026-Q4,Core,25.1,1600\n", was.replacen(&format!("{first}\n"), "", 1)));
}

#[test]
fn what_a_column_refuses_and_what_would_leave_the_deck_invalid_are_refused() {
    let dir = revenue("refused");
    let was = csv(&dir);
    // A value its column refuses stops every edit, by its index.
    let edits = json!([
        { "op": "set", "row": 0, "column": "revenue", "value": "19" },
        { "op": "set", "row": 1, "column": "revenue", "value": "lots" },
    ]);
    let refused = edit(&dir, "q3", edits, false).unwrap_err();
    assert_eq!(refused.op, Some(1));
    assert!(refused.message.contains("`lots` is not a number"), "{}", refused.message);
    assert_eq!(csv(&dir), was);
    // A second Core in 2025-Q4: the chart's marks would not be told apart.
    let twice =
        edit(&dir, "q3", json!([{ "op": "set", "row": 1, "column": "product", "value": "Core" }]), false).unwrap();
    assert!(twice.refused && !twice.edited, "{twice:?}");
    assert!(twice.added.iter().any(|f| f.code == "E103"), "{:?}", twice.added);
    assert_eq!(twice.sheet.rows[1][1], "Pro", "the sheet is the source as it is");
    assert_eq!(csv(&dir), was);
}

#[test]
fn rows_written_inline_are_the_decks() {
    let dir = revenue("inline");
    let path = dir.join("deck.json");
    let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    deck["data"]["goals"] =
        json!({ "source": { "inline": [{ "year": "2026", "target": 80 }] }, "schema": { "target": "number" } });
    std::fs::write(&path, serde_json::to_string_pretty(&deck).unwrap()).unwrap();
    let set = edit(&dir, "goals", json!([{ "op": "set", "row": 0, "column": "target", "value": 95 }]), false).unwrap();
    assert!(set.edited && set.file.is_none(), "{set:?}");
    let after: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(after["data"]["goals"]["source"]["inline"], json!([{ "year": "2026", "target": 95 }]));
    assert_eq!(set.sheet.rows, [["2026", "95"]]);
}

#[test]
fn a_cell_set_to_what_it_holds_writes_nothing() {
    let dir = revenue("same");
    let same =
        edit(&dir, "q3", json!([{ "op": "set", "row": 0, "column": "revenue", "value": "18.2" }]), false).unwrap();
    assert!(!same.edited && !same.refused, "{same:?}");
    assert_eq!(same.sheet.rows[0][2], "18.2");
}

#[test]
fn a_figure_that_quotes_the_data_is_set_again_with_its_claim() {
    // ADR-0019, PLAN 2.72: the title, in `revenue`, quotes Pro's Q3 revenue, and the beat that
    // shows it claims the figure.
    let dir = revenue("quote");
    let path = dir.join("deck.json");
    let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let quote = json!({ "data": "@q3", "row": { "quarter": "2026-Q3", "product": "Pro" }, "column": "revenue", "format": "$.1f" });
    deck["states"][1]["props"]["title"]["runs"] = json!([{ "text": "Pro hit " }, { "text": "$19.4", "quote": quote }]);
    deck["states"][1]["props"]["title"].as_object_mut().unwrap().shift_remove("text");
    deck["spine"]["sections"][1]["beats"][0]["claim"] = json!("Pro hit $19.4M in Q3.");
    std::fs::write(&path, serde_json::to_string_pretty(&deck).unwrap()).unwrap();
    let set = edit(&dir, "q3", json!([{ "op": "set", "row": 10, "column": "revenue", "value": 20 }]), false).unwrap();
    assert!(set.edited, "{set:?}");
    assert_eq!(set.quoted, ["title"]);
    let deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(deck["states"][1]["props"]["title"]["runs"][1]["text"], json!("$20.0"));
    assert_eq!(deck["spine"]["sections"][1]["beats"][0]["claim"], json!("Pro hit $20.0M in Q3."));
    assert!(set.added.iter().all(|f| f.code != "W427"), "{:?}", set.added);
}

#[test]
fn notes_and_descriptions_that_say_a_figure_follow_it() {
    // PLAN 2.90: where the quoted figure shows (`revenue`), its notes, the beat's notes, and the
    // chart's description say $19.4M too, and follow it; `intro`, where the title shows no figure,
    // keeps its notes.
    let dir = revenue("quote-words");
    let path = dir.join("deck.json");
    let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let quote = json!({ "data": "@q3", "row": { "quarter": "2026-Q3", "product": "Pro" }, "column": "revenue", "format": "$.1f" });
    deck["states"][1]["props"]["title"]["runs"] = json!([{ "text": "Pro hit " }, { "text": "$19.4", "quote": quote }]);
    deck["states"][1]["props"]["title"].as_object_mut().unwrap().shift_remove("text");
    deck["states"][1]["notes"] = json!("Pause on $19.4M: Pro's best quarter.");
    deck["states"][0]["notes"] = json!("Last year Pro was nowhere near $19.4M.");
    deck["spine"]["sections"][1]["beats"][0]["notes"] = json!("Say $19.4M slowly.");
    deck["nodes"]["rev"]["alt"] = json!("Quarterly revenue by product; Pro leads at $19.4M in Q3.");
    std::fs::write(&path, serde_json::to_string_pretty(&deck).unwrap()).unwrap();
    let set = edit(&dir, "q3", json!([{ "op": "set", "row": 10, "column": "revenue", "value": 20 }]), false).unwrap();
    assert!(set.edited, "{set:?}");
    let deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(deck["states"][1]["notes"], json!("Pause on $20.0M: Pro's best quarter."));
    assert_eq!(deck["spine"]["sections"][1]["beats"][0]["notes"], json!("Say $20.0M slowly."));
    assert_eq!(deck["nodes"]["rev"]["alt"], json!("Quarterly revenue by product; Pro leads at $20.0M in Q3."));
    assert_eq!(deck["states"][0]["notes"], json!("Last year Pro was nowhere near $19.4M."), "intro shows no figure");
}

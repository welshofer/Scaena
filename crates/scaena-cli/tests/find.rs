//! `scaena find` end to end (PLAN 2.47, 2.83): each of the deck's words that hold what is sought
//! (its texts, a node's description, a beat's claim, a chart's words, a data file's cells), once for each place they are written, and
//! every match replaced in one patch, written where each lives. On a copy of the example bundle,
//! never the example itself.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const EXAMPLES: &str = "../../docs/examples";

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// A copy of the example bundle, `test`'s own: the path of its deck.
fn example(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("find-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    for sub in ["themes", "data", "fonts"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
        for entry in std::fs::read_dir(Path::new(EXAMPLES).join(sub)).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), dir.join(sub).join(entry.file_name())).unwrap();
        }
    }
    std::fs::copy(Path::new(EXAMPLES).join("revenue.deck.json"), dir.join("revenue.deck.json")).unwrap();
    dir.join("revenue.deck.json")
}

/// `scaena --json find deck …`: its exit code and its one JSON value.
fn find(deck: &Path, args: &[&str]) -> (i32, Value) {
    let out = scaena(&[&["--json", "find", deck.to_str().unwrap()], args].concat());
    let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    });
    (out.status.code().unwrap(), v)
}

fn deck(path: &Path) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[test]
fn each_text_is_found_where_it_is_written_and_replaced_there_in_one_patch() {
    let path = example("replace");
    let original = std::fs::read_to_string(&path).unwrap();

    // The beat's claim, `revenue`'s title in its own delta, the chart's description and its axis's
    // title, and the note in its own props: each once, where it is written.
    let (code, found) = find(&path, &["revenue"]);
    assert_eq!(code, 0, "{found}");
    assert_eq!(found["matches"], 5, "{found}");
    let places: Vec<(&str, &str, &str)> = found["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["kind"].as_str().unwrap(), f["lives"].as_str().unwrap(), f["state"].as_str().unwrap()))
        .collect();
    assert_eq!(
        places,
        [
            ("claim", "/spine/sections/1/beats/0/claim", "revenue"),
            ("text", "/states/1/props/title/text", "revenue"),
            ("alt", "/nodes/rev/alt", "revenue"),
            ("chart", "/nodes/rev/y/title", "revenue"),
            ("text", "/nodes/note/text", "revenue"),
        ],
        "{found}"
    );
    assert_eq!(found["found"][0]["beat"], "doubled", "{found}");
    assert_eq!(found["found"][2]["node"], "rev", "{found}");
    assert_eq!(found["found"][3]["node"], "rev", "{found}");
    assert_eq!(found["found"][4]["text"], "Revenue in $M. Enterprise recognized on delivery.");
    assert_eq!(found["found"][4]["matches"][0], serde_json::json!([0, 7]));
    assert!(
        found["found"][4]["states"].as_array().unwrap().len() > 1,
        "the note shows in more than one state: {found}"
    );
    assert!(found.get("replaced").is_none(), "nothing replaced without --replace");

    // Case apart, and whole words only, as asked.
    assert_eq!(find(&path, &["revenue", "--case"]).1["matches"], 1, "the chart's description");
    assert_eq!(find(&path, &["Rev", "--words"]).1["matches"], 0);
    assert_eq!(
        find(&path, &["Rev"]).1["matches"],
        6,
        "Review, and Revenue in the claim, the title, the description, the axis, and the note"
    );

    // A dry run says what replacing would do, and writes nothing.
    let (code, dry) = find(&path, &["revenue", "--replace", "Income", "--dry-run"]);
    assert_eq!(code, 0, "{dry}");
    assert_eq!(dry["replaced"]["applied"], false, "{dry}");
    assert_eq!(dry["replaced"]["patch"].as_array().unwrap().len(), 5, "{dry}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    // Replaced, each where it is written: the title in `revenue`'s delta, the note in its own, the
    // claim in its beat, and the description in the chart's own props.
    let (code, replaced) = find(&path, &["revenue", "--replace", "Income"]);
    assert_eq!(code, 0, "{replaced}");
    assert_eq!(replaced["replaced"]["applied"], true, "{replaced}");
    assert_eq!(
        replaced["replaced"]["states"].as_array().unwrap().len(),
        found["found"][4]["states"].as_array().unwrap().len()
    );
    let after = deck(&path);
    assert_eq!(after["states"][1]["props"]["title"]["text"], "Income doubled");
    assert_eq!(after["nodes"]["note"]["text"], "Income in $M. Enterprise recognized on delivery.");
    assert_eq!(after["nodes"]["title"]["text"], "Q3 Review", "a text with no match is as it was");
    assert_eq!(after["spine"]["sections"][1]["beats"][0]["claim"], "Income doubled year over year, and Pro drove it.");
    assert_eq!(after["nodes"]["rev"]["alt"], "Quarterly Income by product, Q4 2025 through Q3 2026.");
    assert_eq!(after["nodes"]["rev"]["y"]["title"], "Income ($M)", "the axis's title, its field as it was");
    assert_eq!(after["nodes"]["rev"]["y"]["field"], "revenue");
    assert_eq!(find(&path, &["revenue"]).1["matches"], 0, "nothing is left to find");
    assert_eq!(scaena(&["validate", path.to_str().unwrap()]).status.code(), Some(0));

    // Said in words without --json.
    let out = scaena(&["find", path.to_str().unwrap(), "income"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.starts_with("5 matches in 5 places"), "{said}");
    assert!(said.contains("note in revenue") && said.contains("(/nodes/note/text)"), "{said}");
    assert!(
        said.contains("beat doubled's claim in revenue")
            && said.contains("rev's description in revenue")
            && said.contains("rev's words in revenue (/nodes/rev/y/title)"),
        "{said}"
    );
}

#[test]
fn a_data_files_cells_are_found_and_kept_as_they_are() {
    let path = example("cells");
    let csv = path.with_file_name("data").join("q3-revenue.csv");
    let original = std::fs::read(&csv).unwrap();

    // The note's text, and each of the file's cells that holds the product's name, by its row and
    // column, in the states that draw a chart of it.
    let (code, found) = find(&path, &["Enterprise", "--case"]);
    assert_eq!(code, 0, "{found}");
    let cells: Vec<&Value> = found["found"].as_array().unwrap().iter().filter(|f| f["kind"] == "data").collect();
    assert!(!cells.is_empty(), "{found}");
    for cell in &cells {
        assert_eq!(cell["file"], "data/q3-revenue.csv", "{cell}");
        assert_eq!(cell["source"], "q3", "{cell}");
        assert!(cell["lives"].as_str().unwrap().starts_with("data/q3-revenue.csv#"), "{cell}");
        assert!(cell["lives"].as_str().unwrap().ends_with("/product"), "{cell}");
        assert!(cell["states"].as_array().unwrap().contains(&Value::from("revenue")), "{cell}");
    }
    assert!(found.get("kept").is_none(), "nothing is kept without a replacement: {found}");

    // A replacement writes the deck alone and names the file it left as it is.
    let (code, replaced) = find(&path, &["Enterprise", "--case", "--replace", "Business"]);
    assert_eq!(code, 0, "{replaced}");
    assert_eq!(replaced["kept"], serde_json::json!(["data/q3-revenue.csv"]), "{replaced}");
    assert_eq!(deck(&path)["nodes"]["note"]["text"], "Revenue in $M. Business recognized on delivery.");
    assert_eq!(std::fs::read(&csv).unwrap(), original, "the file is as it was");
    let out = scaena(&["find", path.to_str().unwrap(), "Enterprise", "--replace", "Business"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.contains("data q3 in data/q3-revenue.csv"), "{said}");
    assert!(said.contains("kept: data/q3-revenue.csv holds matches"), "{said}");
}

#[test]
fn nothing_found_is_nothing_replaced() {
    let path = example("none");
    let original = std::fs::read_to_string(&path).unwrap();
    let (code, found) = find(&path, &["no such words", "--replace", "anything"]);
    assert_eq!(code, 0, "{found}");
    assert_eq!(found["matches"], 0);
    assert!(found["found"].as_array().unwrap().is_empty());
    assert!(found.get("replaced").is_none(), "{found}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    // `--dry-run` asks for a replacement to try.
    assert_eq!(scaena(&["find", path.to_str().unwrap(), "x", "--dry-run"]).status.code(), Some(2));
}

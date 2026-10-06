//! `scaena find` end to end (PLAN 2.47): each text the deck shows that holds what is sought,
//! once for each place it is written, and every match replaced in one patch, written where
//! each text lives. On a copy of the example bundle, never the example itself.

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

    // `revenue`'s title, in its own delta, and the note, in its own props: each once.
    let (code, found) = find(&path, &["revenue"]);
    assert_eq!(code, 0, "{found}");
    assert_eq!(found["matches"], 2, "{found}");
    let places: Vec<(&str, &str, &str)> = found["found"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["node"].as_str().unwrap(), f["lives"].as_str().unwrap(), f["state"].as_str().unwrap()))
        .collect();
    assert_eq!(
        places,
        [("title", "/states/1/props/title/text", "revenue"), ("note", "/nodes/note/text", "revenue")],
        "{found}"
    );
    assert_eq!(found["found"][1]["text"], "Revenue in $M. Enterprise recognized on delivery.");
    assert_eq!(found["found"][1]["matches"][0], serde_json::json!([0, 7]));
    assert!(
        found["found"][1]["states"].as_array().unwrap().len() > 1,
        "the note shows in more than one state: {found}"
    );
    assert!(found.get("replaced").is_none(), "nothing replaced without --replace");

    // Case apart, and whole words only, as asked.
    assert_eq!(find(&path, &["revenue", "--case"]).1["matches"], 0);
    assert_eq!(find(&path, &["Rev", "--words"]).1["matches"], 0);
    assert_eq!(find(&path, &["Rev"]).1["matches"], 3, "Revenue, Revenue, and Review");

    // A dry run says what replacing would do, and writes nothing.
    let (code, dry) = find(&path, &["revenue", "--replace", "Income", "--dry-run"]);
    assert_eq!(code, 0, "{dry}");
    assert_eq!(dry["replaced"]["applied"], false, "{dry}");
    assert_eq!(dry["replaced"]["patch"].as_array().unwrap().len(), 2, "{dry}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);

    // Replaced, each where it is written: the title in `revenue`'s delta, the note in its own.
    let (code, replaced) = find(&path, &["revenue", "--replace", "Income"]);
    assert_eq!(code, 0, "{replaced}");
    assert_eq!(replaced["replaced"]["applied"], true, "{replaced}");
    assert_eq!(
        replaced["replaced"]["states"].as_array().unwrap().len(),
        found["found"][1]["states"].as_array().unwrap().len()
    );
    let after = deck(&path);
    assert_eq!(after["states"][1]["props"]["title"]["text"], "Income doubled");
    assert_eq!(after["nodes"]["note"]["text"], "Income in $M. Enterprise recognized on delivery.");
    assert_eq!(after["nodes"]["title"]["text"], "Q3 Review", "a text with no match is as it was");
    assert_eq!(find(&path, &["revenue"]).1["matches"], 0, "nothing is left to find");
    assert_eq!(scaena(&["validate", path.to_str().unwrap()]).status.code(), Some(0));

    // Said in words without --json.
    let out = scaena(&["find", path.to_str().unwrap(), "income"]);
    let said = String::from_utf8_lossy(&out.stdout);
    assert!(said.starts_with("2 matches in 2 texts"), "{said}");
    assert!(said.contains("note in revenue") && said.contains("(/nodes/note/text)"), "{said}");
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

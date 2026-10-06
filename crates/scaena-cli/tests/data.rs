//! `scaena data` end to end (PLAN 2.55, ADR-0014): a source shown as a table, and edited in
//! place, one write of its file that keeps every other byte. On a copy of the example bundle,
//! never the example itself.

use serde_json::Value;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const EXAMPLES: &str = "../../docs/examples";

/// `scaena args`, with `stdin` on its standard input.
fn scaena(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

/// A copy of the example bundle, `test`'s own: the path of its deck.
fn example(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("data-{test}"));
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

/// `scaena --json data deck q3 --edits -`, the edits on stdin: its exit code and its one value.
fn edit(deck: &Path, edits: &str, more: &[&str]) -> (i32, Value) {
    let args = [&["--json", "data", deck.to_str().unwrap(), "q3", "--edits", "-"], more].concat();
    let out = scaena(&args, edits);
    let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    });
    (out.status.code().unwrap(), v)
}

fn csv(deck: &Path) -> String {
    std::fs::read_to_string(deck.with_file_name("data/q3-revenue.csv")).unwrap()
}

#[test]
fn a_source_shows_as_a_table() {
    let deck = example("table");
    let out = scaena(&["data", deck.to_str().unwrap(), "q3"], "");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[0], "@q3  data/q3-revenue.csv  12 rows");
    assert_eq!(lines[1], "row  quarter  product     revenue  customers");
    assert_eq!(lines[2], "     string   string      number   number");
    assert_eq!(lines[3], "0    2025-Q4  Core        18.2     1210");
    assert_eq!(lines.len(), 15);
}

#[test]
fn edits_write_the_file_and_nothing_else() {
    let deck = example("edits");
    let was = csv(&deck);
    let edits = r#"[{ "op": "set", "row": 2, "column": "revenue", "value": 4.6 }]"#;
    let (code, dry) = edit(&deck, edits, &["--dry-run"]);
    assert_eq!((code, dry["edited"].clone(), dry["sheet"]["rows"][2][2].clone()), (0, false.into(), "4.6".into()));
    assert_eq!(csv(&deck), was);
    let (code, done) = edit(&deck, edits, &[]);
    assert_eq!((code, done["edited"].clone()), (0, true.into()), "{done:#}");
    assert_eq!(csv(&deck), was.replacen("2025-Q4,Enterprise,4.4,38", "2025-Q4,Enterprise,4.6,38", 1));
}

#[test]
fn a_value_refused_exits_2_with_its_index_and_an_invalid_deck_exits_1() {
    let deck = example("refused");
    let was = csv(&deck);
    let (code, refused) = edit(
        &deck,
        r#"[{ "op": "remove", "row": 0 }, { "op": "set", "row": 0, "column": "customers", "value": "many" }]"#,
        &[],
    );
    assert_eq!((code, refused["error"]["op"].clone()), (2, 1.into()), "{refused:#}");
    assert!(refused["error"]["message"].as_str().unwrap().contains("`many` is not a number"), "{refused:#}");
    let (code, invalid) = edit(&deck, r#"[{ "op": "set", "row": 1, "column": "product", "value": "Core" }]"#, &[]);
    assert_eq!((code, invalid["refused"].clone()), (1, true.into()), "{invalid:#}");
    assert_eq!(csv(&deck), was);
}

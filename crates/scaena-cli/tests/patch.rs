//! `scaena patch` end to end (PLAN 1.16, SPEC §7.3): ops compiled to JSON Patch, the deck
//! they make checked before it is written, and what changes in what `lint` finds. On a copy
//! of the example bundle, never the example itself.

use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const EXAMPLES: &str = "../../docs/examples";

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// A copy of the example bundle, `test`'s own: the path of its deck.
fn example(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("patch-{test}"));
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

/// `scaena --json patch deck --ops <ops> [extra]`: its exit code and its one JSON value.
fn patch(deck: &Path, ops: &Value, extra: &[&str]) -> (i32, Value) {
    let file = deck.with_file_name("ops.json");
    std::fs::write(&file, ops.to_string()).unwrap();
    let args = [&["--json", "patch", deck.to_str().unwrap(), "--ops", file.to_str().unwrap()], extra].concat();
    parse(scaena(&args))
}

fn parse(out: Output) -> (i32, Value) {
    let v = serde_json::from_slice(&out.stdout).unwrap_or_else(|e| {
        panic!("{e}: {}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    });
    (out.status.code().unwrap(), v)
}

/// Each finding as `code [format] path`.
fn codes(findings: &Value) -> Vec<String> {
    let line = |f: &Value| {
        let format = f["format"].as_str().map(|f| format!(" [{f}]")).unwrap_or_default();
        format!("{}{format} {}", f["code"].as_str().unwrap(), f["path"].as_str().unwrap())
    };
    findings.as_array().unwrap().iter().map(line).collect()
}

#[test]
fn the_example_patch_applies_and_says_what_it_did() {
    let deck = example("apply");
    let ops: Value =
        serde_json::from_str(&std::fs::read_to_string(Path::new(EXAMPLES).join("revenue.patch.json")).unwrap())
            .unwrap();
    let before = std::fs::read(&deck).unwrap();
    // A dry run: the patch as JSON Patch, what lint would find differently (nothing), and
    // nothing written.
    let (code, v) = patch(&deck, &ops, &["--dry-run"]);
    assert_eq!(code, 0, "{v:#}");
    assert_eq!(
        (&v["applied"], &v["added"], &v["removed"], &v["errors"]),
        (&json!(false), &json!([]), &json!([]), &json!(0))
    );
    assert_eq!(
        v["patch"].as_array().unwrap()[0..3],
        [
            json!({ "op": "move", "from": "/nodes/rev", "path": "/nodes/revenue-chart" }),
            json!({ "op": "move", "from": "/states/1/props/rev", "path": "/states/1/props/revenue-chart" }),
            json!({ "op": "replace", "path": "/states/1/choreography/0/target", "value": "revenue-chart" }),
        ]
    );
    assert_eq!(std::fs::read(&deck).unwrap(), before);
    // For real: written canonically, and it lints clean.
    let (code, v) = patch(&deck, &ops, &[]);
    assert_eq!((code, &v["applied"]), (0, &json!(true)), "{v:#}");
    let text = std::fs::read_to_string(&deck).unwrap();
    let written = scaena_core::Deck::from_json(&text).unwrap();
    assert_eq!(text, written.to_json().unwrap() + "\n", "written canonically");
    let ids: Vec<&str> = written.states.iter().map(|s| s.id.as_str()).collect();
    assert_eq!(ids, ["intro", "revenue", "mix", "pro", "close"]);
    assert!(written.nodes.contains_key("revenue-chart") && !written.nodes.contains_key("rev"));
    let (code, found) = parse(scaena(&["--json", "lint", deck.to_str().unwrap()]));
    assert_eq!((code, found), (0, json!([])));
    // Again, its first op names a node that is gone: nothing applies.
    let before = std::fs::read(&deck).unwrap();
    let (code, v) = patch(&deck, &ops, &[]);
    assert_eq!(code, 2, "{v:#}");
    assert_eq!(v["error"]["op"], 0);
    assert!(v["error"]["message"].as_str().unwrap().contains("no node `rev`"), "{v:#}");
    assert_eq!(std::fs::read(&deck).unwrap(), before);
}

#[test]
fn a_patch_that_would_make_the_deck_invalid_is_refused() {
    let deck = example("refused");
    let before = std::fs::read(&deck).unwrap();
    for (ops, says) in [
        // A slot the state's layout does not have.
        (
            json!([{ "op": "set_prop", "node": "note", "state": "revenue", "prop": "at/in", "value": "sidebar" }]),
            "E102",
        ),
        // JSON Patch says nothing of references: a node removed that states still name.
        (json!([{ "op": "remove", "path": "/nodes/note" }]), "E102"),
        // Nor of types.
        (json!([{ "op": "add", "path": "/nodes/note/kind", "value": "bar" }]), "E106"),
    ] {
        let (code, v) = patch(&deck, &ops, &[]);
        assert_eq!((code, &v["applied"]), (1, &json!(false)), "{ops}: {v:#}");
        let added = v["added"].as_array().unwrap();
        assert!(!added.is_empty() && added.iter().all(|f| f["code"] == says), "{ops}: {v:#}");
        assert!(v["errors"].as_u64().unwrap() >= 1);
        assert_eq!(std::fs::read(&deck).unwrap(), before, "{ops}: nothing written");
    }
}

#[test]
fn the_delta_shows_what_a_patch_breaks_and_what_fixes_it() {
    let deck = example("delta");
    // A headline too long for its slot, in both formats; in 9:16 it runs into the chart.
    let long = json!({ "op": "set_text", "node": "title", "state": "mix",
                       "text": "…and the mix shifted toward Pro, quarter after quarter" });
    let (code, v) = patch(&deck, &json!([long]), &["--dry-run"]);
    assert_eq!(code, 1, "an error in the deck it makes exits 1: {v:#}");
    assert_eq!(
        codes(&v["added"]),
        ["E100 /nodes/title", "E100 [9:16] /nodes/title", "E101 [9:16] /nodes/title", "W202 [9:16] /nodes/title"],
        "{v:#}"
    );
    let fix = v["added"][0]["fix"].as_array().unwrap().clone();
    assert_eq!(fix, [json!({ "op": "add", "path": "/nodes/title/fit", "value": "shrink" })]);
    // With the fix lint offers in the same patch, nothing is added: the title shrinks to
    // fit, in both formats, and clears the chart.
    let mut ops = vec![long];
    ops.extend(fix);
    let (code, v) = patch(&deck, &Value::Array(ops), &["--dry-run"]);
    assert_eq!((code, &v["added"]), (0, &json!([])), "{v:#}");
    // A rename keeps a finding the same finding: what lint finds of `note` before is what
    // it finds of `source-note` after.
    let (code, v) =
        patch(&deck, &json!([{ "op": "set_prop", "node": "note", "prop": "style/size", "value": 22 }]), &[]);
    assert_eq!((code, codes(&v["added"])), (0, vec!["W300 /nodes/note/style/size".to_string()]), "{v:#}");
    let (code, v) = patch(&deck, &json!([{ "op": "rename_node", "id": "note", "to": "source-note" }]), &["--dry-run"]);
    assert_eq!((code, &v["added"], &v["removed"]), (0, &json!([]), &json!([])), "{v:#}");
}

#[test]
fn ops_come_from_stdin_and_a_patch_is_an_array() {
    let deck = example("stdin");
    let before = std::fs::read(&deck).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(["--json", "patch", deck.to_str().unwrap(), "--ops", "-", "--dry-run"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let ops = json!([{ "op": "set_text", "node": "subtitle", "text": "A quarter that changed the business." }]);
    child.stdin.take().unwrap().write_all(ops.to_string().as_bytes()).unwrap();
    let (code, v) = parse(child.wait_with_output().unwrap());
    assert_eq!(code, 0, "{v:#}");
    assert_eq!(
        v["patch"],
        json!([{ "op": "add", "path": "/nodes/subtitle/text", "value": "A quarter that changed the business." }])
    );
    // One op on its own is not a patch.
    let (code, v) = patch(&deck, &ops[0], &[]);
    assert_eq!(code, 2, "{v:#}");
    assert!(v["error"]["message"].as_str().unwrap().contains("a JSON array of ops"), "{v:#}");
    assert_eq!(std::fs::read(&deck).unwrap(), before);
}

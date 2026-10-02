//! `scaena compile` and `scaena decompile` end to end (PLAN 1.5): the example's source is
//! its deck both ways; a deck that does not validate exits 1 with each finding shown at
//! its source; source that does not parse exits 2; and a failed compile writes nothing.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).env("NO_COLOR", "1").output().unwrap()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("dsl-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn canonical(json: &str) -> String {
    scaena_core::Deck::from_json(json).unwrap().to_json().unwrap() + "\n"
}

#[test]
fn the_example_source_and_its_deck_are_each_other() {
    let out = scaena(&["decompile", "../../docs/examples/revenue.deck.json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let scn = std::fs::read_to_string("../../docs/examples/revenue.deck.scn").unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), scn);

    // Checked where the source is, beside the theme, fonts, and data it names.
    let out = scaena(&["compile", "../../docs/examples/revenue.deck.scn"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let json = std::fs::read_to_string("../../docs/examples/revenue.deck.json").unwrap();
    assert_eq!(String::from_utf8(out.stdout).unwrap(), canonical(&json));
}

#[test]
fn decompile_reads_any_bundle_and_compile_writes_where_told() {
    let dir = scratch("bundle");
    let scn = dir.join("deck.scn");
    let out = scaena(&["decompile", "../../docs/examples/authorability", "-o", scn.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(out.stdout.is_empty());

    // Written into the authorability bundle's copy, so its files are there to check.
    let bundle = dir.join("authorability");
    copy_dir(Path::new("../../docs/examples/authorability"), &bundle);
    let deck = bundle.join("deck.json");
    let original = std::fs::read_to_string(&deck).unwrap();
    std::fs::write(&deck, "{}").unwrap();
    let out = scaena(&["compile", scn.to_str().unwrap(), "-o", deck.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(std::fs::read_to_string(&deck).unwrap(), canonical(&original));
}

#[test]
fn findings_exit_1_at_the_source_that_wrote_them() {
    let dir = scratch("findings");
    let source = "deck \"T\" canvas:1920x1080\n\ndata q3 \"data/missing.csv\"\n\nstate a\n  t text \"Hi\" sise:12\n";
    let scn = dir.join("deck.scn");
    std::fs::write(&scn, source).unwrap();
    let out_json = dir.join("deck.json");

    let out = scaena(&["--json", "compile", scn.to_str().unwrap(), "-o", out_json.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!out_json.exists(), "a deck that does not validate is not written");
    let findings: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let at = |path: &str| {
        let f =
            findings.as_array().unwrap().iter().find(|f| f["path"] == path).unwrap_or_else(|| panic!("{findings:#}"));
        (f["code"].as_str().unwrap().to_string(), f["line"].as_u64().unwrap(), f["col"].as_u64().unwrap())
    };
    assert_eq!(at("/data/q3/source"), ("E102".into(), 3, 9));
    assert_eq!(at("/nodes/t/sise"), ("E106".into(), 6, 15));

    let out = scaena(&["compile", scn.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(out.stdout.is_empty());
    let stderr = String::from_utf8(out.stderr).unwrap();
    // The finding, its code, the line it is about, and where in the deck it lands.
    assert!(stderr.contains("E106"), "{stderr}");
    assert!(stderr.contains("t text \"Hi\" sise:12"), "{stderr}");
    assert!(stderr.contains("/nodes/t/sise"), "{stderr}");
    assert!(stderr.contains("deck.scn:6:15"), "{stderr}");
}

#[test]
fn source_that_does_not_parse_exits_2() {
    let dir = scratch("syntax");
    let scn = dir.join("deck.scn");
    std::fs::write(&scn, "deck \"T\" canvas:1920x1080\nnod t text\n").unwrap();
    let out = scaena(&["compile", scn.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("`nod` does not start a declaration"), "{stderr}");
    assert!(stderr.contains("deck.scn:2:1"), "{stderr}");

    let out = scaena(&["--json", "compile", scn.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    let errors: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!((errors[0]["line"].as_u64(), errors[0]["col"].as_u64()), (Some(2), Some(1)));

    let out = scaena(&["compile", dir.join("missing.scn").to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "a file that is not there is invalid input");
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

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
const RULES: [(&str, &[&str]); 29] = [
    ("E100", &["/nodes/t"]),
    ("E101", &["/nodes/b", "/nodes/note"]),
    ("E110", &["/nodes/t"]),
    ("E111", &["/nodes/t"]),
    ("E120", &["/nodes/t"]),
    ("W200", &["/nodes/t"]),
    ("W201", &["/nodes/t"]),
    ("W202", &["/nodes/t"]),
    ("W203", &["/nodes/t"]),
    ("W210", &["/states/0"]),
    ("W220", &["/states/0"]),
    ("W300", &["/nodes/t/style/color", "/nodes/t/style/size", "/states/0/props/box/radius"]),
    ("W301", &["/nodes/t/at/rect"]),
    ("W302", &["/nodes/t/at/col"]),
    ("W310", &["/nodes/c/labels", "/nodes/c/labels", "/nodes/c/labels"]),
    ("W320", &["/states/0"]),
    ("W321", &["/states/0/choreography"]),
    ("W322", &["/states/1/choreography/0"]),
    ("W401", &["/states/1"]),
    ("W410", &["/nodes/p"]),
    ("W420", &["/spine/sections/0/beats/0"]),
    ("W421", &["/states/0"]),
    ("W422", &["/states/0"]),
    ("W423", &["/spine/sections/0/beats/0/evidence/0"]),
    ("W424", &["/states/0"]),
    ("W425", &["/spine/sections/0/beats/1/claim"]),
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
fn the_example_decks_and_b1_lint_clean() {
    for bundle in [
        "docs/examples/revenue.deck.json",
        "docs/examples/charts.deck.json",
        "docs/examples/trails.deck.json",
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

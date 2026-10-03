//! `scaena validate` end to end (PLAN 1.2): every bundle in the repository validates, a
//! broken one exits 1 with its findings, and input that is not a deck exits 2.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("validate-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn every_bundle_in_the_repository_validates() {
    for bundle in [
        "../../tests/fixtures/torture.scaena",
        "../../tests/bench/b1.scaena",
        "../../docs/examples/revenue.deck.json",
        "../../docs/examples/trails.deck.json",
        "../../docs/examples/higher-ed.deck.json",
        "../../docs/examples/authorability",
    ] {
        let out = scaena(&["validate", bundle]);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(out.status.code(), Some(0), "{bundle}: {stdout}{}", String::from_utf8_lossy(&out.stderr));
        assert_eq!(stdout.trim(), "ok: no findings", "{bundle}");
    }
}

#[test]
fn findings_exit_1_and_name_the_theme_file() {
    let dir = scratch("findings");
    let deck = std::fs::read_to_string("../../tests/lint/E102/trigger.deck.json").unwrap();
    std::fs::write(dir.join("deck.json"), deck).unwrap();
    let theme = std::fs::read_to_string("../../tests/lint/theme.json").unwrap();
    std::fs::write(
        dir.join("theme.json"),
        theme.replace(r#""family": "display", "size": 128"#, r#""family": "serif", "size": 128"#),
    )
    .unwrap();
    let out = scaena(&["--json", "validate", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    let findings: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let findings = findings.as_array().unwrap();
    assert!(findings.iter().all(|f| f["code"] == "E102"), "{findings:#?}");
    // The theme's own broken name, and its font files, which this bundle lacks like the
    // deck's (`/fonts/0/file`).
    let mut in_theme: Vec<&str> =
        findings.iter().filter(|f| f["file"] == "theme.json").map(|f| f["path"].as_str().unwrap()).collect();
    in_theme.sort();
    assert_eq!(in_theme, ["/type/families/body/file", "/type/families/display/file", "/type/roles/display/family"]);
    assert!(findings.iter().any(|f| f["path"] == "/fonts/0/file" && f["file"].is_null()), "{findings:#?}");
}

#[test]
fn input_that_is_not_a_deck_exits_2() {
    let dir = scratch("not-json");
    std::fs::write(dir.join("deck.json"), "{ \"scaena\": ").unwrap();
    assert_eq!(scaena(&["validate", dir.to_str().unwrap()]).status.code(), Some(2));
    assert_eq!(scaena(&["validate", "../../tests/lint/nothing-here"]).status.code(), Some(2));
}

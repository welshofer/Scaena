//! `scaena generate` (PLAN G.2, ADR-0026): a deck made by Narrato. With `--slides` the slides
//! are written elsewhere and no model is called, so the test runs anywhere; without a key the
//! command says which to set, before any call is made; and a source waits on PLAN G.3.

use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const KEYS: [&str; 4] = ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "GEMINI_API_KEY", "GOOGLE_API_KEY"];

/// `scaena` run with no key in its environment.
fn scaena(args: &[&str]) -> Output {
    let mut c = Command::new(env!("CARGO_BIN_EXE_scaena"));
    for k in KEYS.into_iter().chain(["SCAENA_MODEL"]) {
        c.env_remove(k);
    }
    c.args(args).output().unwrap()
}

/// A place for a bundle that is not there yet.
fn place(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("generate-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout).unwrap_or_else(|e| panic!("{e}: {}", String::from_utf8_lossy(&out.stdout)))
}

fn catalog() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../scaena-narrato/tests/answers/catalog.json")
        .to_string_lossy()
        .into_owned()
}

#[test]
fn slides_written_elsewhere_make_a_bundle_with_no_call() {
    let dir = place("slides");
    let bundle = dir.to_str().unwrap();
    let out =
        scaena(&["--json", "generate", bundle, "Tidewater's Series A", "--slides", &catalog(), "--theme", "ember"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let made = json(&out);
    assert_eq!(made["created"]["created"], true);
    assert_eq!(made["created"]["errors"], 0);
    assert_eq!(made["layouts"].as_array().unwrap().len(), 56);
    // The arc the heuristic chose for the brief: the slides came with none.
    assert!(made["arc"].as_str().is_some_and(|a| !a.is_empty()));
    let deck: Value = serde_json::from_str(&std::fs::read_to_string(dir.join("deck.json")).unwrap()).unwrap();
    assert_eq!(deck["theme"], "themes/ember.theme.json");
    assert_eq!(deck["states"].as_array().unwrap().len(), 56);
    assert_eq!(deck["meta"]["narrative"]["generator"], "narrato");
    // The bundle validates as it is on disk.
    assert_eq!(scaena(&["validate", bundle]).status.code(), Some(0));
    // A place taken is refused before anything is made.
    let again = json(&scaena(&["--json", "generate", bundle, "again", "--slides", &catalog()]));
    assert!(again["error"]["message"].as_str().unwrap().contains("not an empty directory"));
}

#[test]
fn with_no_key_it_says_which_to_set_before_any_call() {
    let dir = place("no-key");
    let out = json(&scaena(&["--json", "generate", dir.to_str().unwrap(), "A pitch", "--provider", "gemini"]));
    assert_eq!(out["error"]["exit"], 2);
    let message = out["error"]["message"].as_str().unwrap();
    assert!(message.contains("GEMINI_API_KEY"), "{message}");
    assert!(!dir.exists());
    let out = json(&scaena(&["--json", "generate", "--models", "--provider", "openai"]));
    assert!(out["error"]["message"].as_str().unwrap().contains("OPENAI_API_KEY"));
    let out = json(&scaena(&["--json", "generate", dir.to_str().unwrap(), "A pitch", "--arc", "Fairy Tale"]));
    assert!(out["error"]["message"].as_str().unwrap().contains("is no arc of Narrato's"));
}

#[test]
fn a_source_waits_on_grounding() {
    let dir = place("source");
    let out = json(&scaena(&["--json", "generate", dir.to_str().unwrap(), "A pitch", "--source", "notes.md"]));
    assert_eq!(out["error"]["exit"], 3);
    assert_eq!(out["error"]["plan"], "G.3");
}

/// A live run, where the environment holds a key and `$SCAENA_MODEL` names a model: a deck of a
/// few slides made by the provider, which validates. Skipped elsewhere, as no key is set there.
#[test]
fn a_live_generation_makes_a_deck_where_a_key_is_set() {
    let keyed = KEYS.iter().any(|k| std::env::var(k).is_ok_and(|v| !v.trim().is_empty()));
    if !keyed || std::env::var("SCAENA_MODEL").is_err() {
        eprintln!("skipped: no key and $SCAENA_MODEL in the environment");
        return;
    }
    let dir = place("live");
    let bundle = dir.to_str().unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_scaena"))
        .args(["--json", "generate", bundle, "Why trail maintenance needs volunteers", "--audience", "A town council"])
        .output()
        .unwrap();
    // Lint may find what a model wrote too long even rewritten (exit 1); the deck is made all
    // the same.
    let said = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(matches!(out.status.code(), Some(0 | 1)), "{said}");
    let made = json(&out);
    assert_eq!(made["created"]["created"], true, "{said}");
    assert!(made["layouts"].as_array().unwrap().len() >= 5);
    assert_eq!(scaena(&["validate", bundle]).status.code(), Some(0));
}

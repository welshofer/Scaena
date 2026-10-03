//! A bundle's history (PLAN 1.23, SPEC §8): kept from `save --history` on, every write
//! records its change by its author; a `deck.json` edited outside Scaena goes in as a change
//! by `fs`; and a node renamed is one operation, its references untouched.

use scaena_store::SaveOptions;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const REVENUE: &str = "../../docs/examples/revenue.deck.json";

fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("history-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::remove_file(dir.with_extension("scaena")).ok();
    dir
}

fn keeping_history(to: &Path) {
    let opts = SaveOptions { subset_fonts: false, now: "2026-10-03T00:00:00Z".into(), history: true };
    scaena_ops::open(Path::new(REVENUE)).unwrap().save(to, &opts).unwrap();
}

/// The bundle's changes: (author, message, operations).
fn changes(bundle: &Path) -> Vec<(String, String, usize)> {
    let doc = scaena_ops::open(bundle).unwrap().history().unwrap().expect("the bundle keeps history");
    doc.changes().into_iter().map(|c| (c.author.unwrap_or_default(), c.message.unwrap_or_default(), c.ops)).collect()
}

#[test]
fn every_change_is_recorded_with_who_made_it() {
    let dir = scratch("dir");
    keeping_history(&dir);
    assert!(dir.join("history/deck.loro").is_file());
    assert_eq!(
        changes(&dir).iter().map(|c| (c.0.as_str(), c.1.as_str())).collect::<Vec<_>>(),
        [("user", "history begins")]
    );
    // Someone edits deck.json by hand.
    let path = dir.join("deck.json");
    let mut deck: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    deck["nodes"]["title"]["text"] = json!("Revenue doubled, again");
    std::fs::write(&path, serde_json::to_string_pretty(&deck).unwrap()).unwrap();
    // Then an agent renames a node.
    let mut b = scaena_ops::open(&dir).unwrap();
    b.author = "agent:test".into();
    let patched =
        scaena_ops::patch::patch(&b, &json!([{ "op": "rename_node", "id": "title", "to": "headline" }]), false);
    assert!(patched.unwrap().applied);
    let log = changes(&dir);
    let who: Vec<(&str, &str)> = log.iter().map(|c| (c.0.as_str(), c.1.as_str())).collect();
    assert_eq!(
        who,
        [("user", "history begins"), ("fs", "deck.json changed outside Scaena"), ("agent:test", "patch: rename_node")]
    );
    // A rename sets the node's id, and nothing that names it changes: one operation, though
    // four states, choreography, and the spine's notes all point at `title`.
    assert_eq!(log[2].2, 1, "{log:?}");
    // The history holds what deck.json does, and opening it again takes nothing in.
    let b = scaena_ops::open(&dir).unwrap();
    let doc = b.history().unwrap().unwrap();
    assert_eq!(doc.deck().unwrap().to_json().unwrap(), b.deck.to_json().unwrap());
    assert_eq!(doc.changes().len(), 3);
    assert_eq!(b.deck.nodes["headline"].props["text"], "Revenue doubled, again");
}

#[test]
fn a_zip_keeps_its_history_too_and_a_bundle_without_one_keeps_none() {
    let zip = scratch("zip").with_extension("scaena");
    keeping_history(&zip);
    let b = scaena_ops::open(&zip).unwrap();
    let fixed =
        scaena_ops::patch::patch(&b, &json!([{ "op": "set_text", "node": "title", "text": "Q3, in full" }]), false);
    assert!(fixed.unwrap().applied);
    let log = changes(&zip);
    assert_eq!(log.last().map(|c| (c.0.as_str(), c.1.as_str())), Some(("user", "patch: set_text")));
    // Without `--history`, nothing starts one.
    let flat = scratch("flat");
    let opts = SaveOptions { subset_fonts: false, now: "2026-10-03T00:00:00Z".into(), history: false };
    scaena_ops::open(Path::new(REVENUE)).unwrap().save(&flat, &opts).unwrap();
    let b = scaena_ops::open(&flat).unwrap();
    scaena_ops::patch::patch(&b, &json!([{ "op": "set_text", "node": "title", "text": "Q3" }]), false).unwrap();
    assert!(!flat.join("history").exists());
    assert!(scaena_ops::open(&flat).unwrap().history().unwrap().is_none());
}

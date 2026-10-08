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

/// A data file's edits are the history's too (PLAN 2.55, ADR-0014): the history begins with
/// the data the deck is drawn from, a `data_edit` is one change by its author, holding the
/// file as it wrote it, and a file edited outside Scaena goes in as a change by `fs`.
#[test]
fn a_data_files_edits_are_recorded_with_who_made_them() {
    let dir = scratch("data");
    keeping_history(&dir);
    let csv = dir.join("data/q3-revenue.csv");
    let was = std::fs::read(&csv).unwrap();
    let held = |dir: &Path| scaena_ops::open(dir).unwrap().history().unwrap().unwrap().files();
    assert_eq!(held(&dir)["data/q3-revenue.csv"], was, "the history begins with the data");
    let mut b = scaena_ops::open(&dir).unwrap();
    b.author = "agent:test".into();
    let req = serde_json::from_value(json!({
        "source": "q3", "edits": [{ "op": "set", "row": 0, "column": "revenue", "value": "18.5" }]
    }))
    .unwrap();
    assert!(scaena_ops::data::data_edit(&b, &req, false).unwrap().edited);
    let log = changes(&dir);
    assert_eq!(
        log.last().map(|c| (c.0.as_str(), c.1.as_str())),
        Some(("agent:test", "data_edit q3: revenue of row 0"))
    );
    let edited = std::fs::read(&csv).unwrap();
    assert_eq!(held(&dir)["data/q3-revenue.csv"], edited);
    // Then the file is replaced on disk, as a spreadsheet would write it.
    std::fs::write(&csv, String::from_utf8(edited).unwrap().replace('\n', "\r\n")).unwrap();
    let b = scaena_ops::open(&dir).unwrap();
    scaena_ops::patch::patch(&b, &json!([{ "op": "set_text", "node": "title", "text": "Q3" }]), false).unwrap();
    let who: Vec<(String, String)> = changes(&dir).into_iter().map(|c| (c.0, c.1)).collect();
    let tail: Vec<(&str, &str)> = who[who.len() - 2..].iter().map(|(a, m)| (a.as_str(), m.as_str())).collect();
    assert_eq!(tail, [("fs", "data/q3-revenue.csv changed outside Scaena"), ("user", "patch: set_text")]);
    assert_eq!(held(&dir)["data/q3-revenue.csv"], std::fs::read(&csv).unwrap());
}

/// A theme edit (ADR-0016) is a change by its author with the theme's bytes: a version compares
/// it as a file that changed, and restoring the version before it writes the theme back. A theme
/// replaced outside Scaena goes in as a change by `fs`.
#[test]
fn a_theme_edit_is_a_version_compared_and_restored() {
    let dir = scratch("theme");
    keeping_history(&dir);
    let theme = || std::fs::read(dir.join("themes/dusk.theme.json")).unwrap();
    let was = theme();
    let mut b = scaena_ops::open(&dir).unwrap();
    b.author = "agent:test".into();
    let ops = vec![json!({ "op": "replace", "path": "/type/roles/body/size", "value": 30 })];
    let edited = scaena_ops::theme::theme_edit(&b, &scaena_ops::theme::ThemeEdit { ops, photo: None }, false).unwrap();
    assert!(edited.applied && !edited.refused, "{edited:?}");
    let log = changes(&dir);
    assert_eq!(
        log.iter().map(|c| (c.0.as_str(), c.1.as_str())).collect::<Vec<_>>(),
        [("user", "history begins"), ("agent:test", "theme_edit: type/roles/body/size")]
    );

    // Compared with the version before it: the theme's bytes, and nothing on a slide's own.
    let b = scaena_ops::open(&dir).unwrap();
    let compared = scaena_ops::history::diff(&b, "1", None).unwrap();
    assert_eq!(compared.files, ["themes/dusk.theme.json"], "{compared:?}");
    assert!(compared.states.is_empty() && compared.deck.is_empty(), "{compared:?}");

    // Restored: the theme as it was, and the deck drawn in it again.
    let restored = scaena_ops::history::restore(&b, "1", false).unwrap();
    assert!(restored.applied && restored.files == ["themes/dusk.theme.json"], "{restored:?}");
    assert_eq!(theme(), was);

    // A theme replaced on disk is taken in as `fs`'s.
    let mut text: Value = serde_json::from_slice(&theme()).unwrap();
    text["grid"]["gutter"] = json!(32);
    std::fs::write(dir.join("themes/dusk.theme.json"), serde_json::to_string_pretty(&text).unwrap()).unwrap();
    let log = changes(&dir);
    let last = log.last().map(|c| (c.0.as_str(), c.1.as_str()));
    assert_eq!(last, Some(("fs", "themes/dusk.theme.json changed outside Scaena")), "{log:?}");
}

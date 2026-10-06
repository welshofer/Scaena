//! # scaena-history
//!
//! A bundle's history in the browser (PLAN 2.9): the CRDT document that `scaena save` and
//! every command that writes a deck record their changes in ([`scaena_store::crdt`], SPEC
//! §8), as a WASM module of its own. A page loads it when it saves a bundle that keeps a
//! history, and the engine's save hands it the history and the changes to record in it
//! (`Player.save`): the page's edits, as the user's, and its assistant's, as its agent's. The
//! engine's module leaves the CRDT out: it would add a third again to it (SPEC §15).

use scaena_store::crdt::{DeckDoc, Recorded};
use wasm_bindgen::prelude::*;

/// `history`, what a bundle's `history/deck.loro` holds, with `changes` recorded in it: the
/// bytes to write in its place. `changes` is JSON, a list of changes in the order they were
/// made, each the deck it leaves (as deck.json's text) and who made it, why, and when
/// (`{ deck, author, message?, timestamp?, renamedNodes?, renamedStates? }`). One that
/// leaves the deck as it was is no change.
#[wasm_bindgen]
pub fn record(history: &[u8], changes: &str) -> Result<Vec<u8>, JsError> {
    recorded(history, changes).map_err(|e| JsError::new(&e))
}

/// [`record`], for a caller in Rust.
pub fn recorded(history: &[u8], changes: &str) -> Result<Vec<u8>, String> {
    let changes: Vec<Recorded> = serde_json::from_str(changes).map_err(|e| format!("the changes to record: {e}"))?;
    let doc = DeckDoc::load(history).map_err(|e| e.to_string())?;
    doc.record(&changes).map_err(|e| e.to_string())?;
    doc.save().map_err(|e| e.to_string())
}

/// Every change `history` holds, oldest first, as JSON: `[{ id, author, message, timestamp,
/// peer, ops }]`, its id what names its version (PLAN 2.60), its timestamp in seconds since
/// 1970, and the editor that made it as a decimal string (it may not fit in a JavaScript
/// number).
#[wasm_bindgen]
pub fn changes(history: &[u8]) -> Result<String, JsError> {
    listed(history).map_err(|e| JsError::new(&e))
}

/// [`changes`], for a caller in Rust.
pub fn listed(history: &[u8]) -> Result<String, String> {
    let doc = DeckDoc::load(history).map_err(|e| e.to_string())?;
    let changes = doc.changes().into_iter().map(|c| {
        serde_json::json!({
            "id": c.id,
            "author": c.author,
            "message": c.message,
            "timestamp": c.timestamp,
            "peer": c.peer.to_string(),
            "ops": c.ops,
        })
    });
    Ok(serde_json::Value::Array(changes.collect()).to_string())
}

/// The version of the deck `history` names `id` (a change's id, as [`changes`] lists it), as
/// JSON: `{ deck, files }`, the deck as deck.json's text as it was just after that change, and
/// the files it was drawn from, its data files and its theme, by their paths, as their text then
/// (PLAN 2.60, ADR-0016). An id the history does not hold says so.
#[wasm_bindgen]
pub fn at(history: &[u8], id: &str) -> Result<String, JsError> {
    version(history, id).map_err(|e| JsError::new(&e))
}

/// [`at`], for a caller in Rust.
pub fn version(history: &[u8], id: &str) -> Result<String, String> {
    let doc = DeckDoc::load(history).map_err(|e| e.to_string())?;
    let then = doc.at(id).map_err(|e| e.to_string())?.ok_or_else(|| format!("the history holds no version {id}"))?;
    let deck = then.deck().map_err(|e| e.to_string())?;
    let named = scaena_store::kept_paths(&deck);
    let files: serde_json::Map<String, serde_json::Value> = (then.files().into_iter())
        .filter(|(path, _)| named.contains(&path.as_str()))
        .map(|(path, bytes)| (path, String::from_utf8_lossy(&bytes).into_owned().into()))
        .collect();
    let deck = deck.to_json().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "deck": deck, "files": files }).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use scaena_store::crdt::{Edit, FS, OUTSIDE};
    use scaena_store::{Bundle, HISTORY, SaveOptions};
    use std::path::Path;

    /// The revenue example saved with a history begun, as `scaena save --history` saves it.
    fn begun() -> std::collections::BTreeMap<String, Vec<u8>> {
        let bundle = Bundle::open(Path::new("../../docs/examples/revenue.deck.json")).unwrap();
        let opts = SaveOptions { subset_fonts: false, now: "2026-10-03T00:00:00Z".into(), history: true };
        bundle.saving(&opts).unwrap().files
    }

    #[test]
    fn it_records_as_a_command_that_writes_the_deck_does() {
        let files = begun();
        let held = String::from_utf8(files["deck.json"].clone()).unwrap();
        let edited = held.replace("Revenue doubled\"", "Revenue more than doubled\"");
        // A minute after the history began: a change is never stamped before what it follows.
        let at = DeckDoc::load(&files[HISTORY]).unwrap().changes()[0].timestamp + 60;
        let change = |deck: &str, author: &str, message: &str| Recorded {
            deck: deck.into(),
            files: Default::default(),
            author: author.into(),
            message: Some(message.into()),
            timestamp: Some(at),
            renamed_nodes: Vec::new(),
            renamed_states: Vec::new(),
        };
        let changes = [change(&held, FS, OUTSIDE), change(&edited, "user", "save")];
        let ours = recorded(&files[HISTORY], &serde_json::to_string(&changes).unwrap()).unwrap();

        // What the store records for the same edit, by the same author.
        let mut bundle = Bundle::in_memory(files.clone()).unwrap();
        bundle.deck = scaena_core::Deck::from_json(&edited).unwrap();
        let edit = Edit { message: Some("save"), timestamp: Some(at), ..Edit::by("user") };
        let written = Default::default();
        let theirs = DeckDoc::load(&bundle.record(&bundle.deck, &written, &edit).unwrap().unwrap()).unwrap();
        let ours = DeckDoc::load(&ours).unwrap();
        assert_eq!(ours.deck().unwrap().to_json().unwrap(), theirs.deck().unwrap().to_json().unwrap());
        let said = |doc: &DeckDoc| -> Vec<_> {
            doc.changes().into_iter().map(|c| (c.author, c.message, c.timestamp, c.ops)).collect()
        };
        assert_eq!(said(&ours), said(&theirs));
        assert_eq!(said(&ours).len(), 2, "the deck as held is the history's: no change by fs");

        let listed: serde_json::Value = serde_json::from_str(&listed(&ours.save().unwrap()).unwrap()).unwrap();
        assert_eq!(listed[1]["author"], "user");
        assert_eq!(listed[1]["message"], "save");
        assert_eq!(listed[1]["timestamp"], at);
        assert!(listed[1]["peer"].as_str().unwrap().parse::<u64>().is_ok());
    }

    /// A version (PLAN 2.60): the deck as it was just after a change, with its data files.
    #[test]
    fn a_version_is_the_deck_and_its_data_as_they_were() {
        let files = begun();
        let held = String::from_utf8(files["deck.json"].clone()).unwrap();
        let edited = held.replace("Revenue doubled\"", "Revenue more than doubled\"");
        let change = Recorded {
            deck: edited,
            files: [("data/q3-revenue.csv".to_string(), "quarter,revenue\n".to_string())].into(),
            author: "user".into(),
            message: Some("save".into()),
            timestamp: None,
            renamed_nodes: Vec::new(),
            renamed_states: Vec::new(),
        };
        let history = recorded(&files[HISTORY], &serde_json::to_string(&[change]).unwrap()).unwrap();
        let listed: serde_json::Value = serde_json::from_str(&listed(&history).unwrap()).unwrap();
        let first = listed[0]["id"].as_str().unwrap();
        let then: serde_json::Value = serde_json::from_str(&version(&history, first).unwrap()).unwrap();
        assert_eq!(then["deck"].as_str().unwrap(), held.trim_end());
        let csv = String::from_utf8(files["data/q3-revenue.csv"].clone()).unwrap();
        assert_eq!(then["files"]["data/q3-revenue.csv"], csv);
        let now: serde_json::Value =
            serde_json::from_str(&version(&history, listed[1]["id"].as_str().unwrap()).unwrap()).unwrap();
        assert_eq!(now["files"]["data/q3-revenue.csv"], "quarter,revenue\n");
        assert!(version(&history, "9@9").unwrap_err().contains("no version"));
    }

    #[test]
    fn what_it_cannot_record_it_says() {
        let files = begun();
        assert!(recorded(&files[HISTORY], "[{}]").unwrap_err().starts_with("the changes to record:"));
        assert!(recorded(b"not a history", "[]").is_err());
    }
}

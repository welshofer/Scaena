//! # scaena-store
//!
//! Bundle I/O (SPEC §3.1) now; the CRDT document (SPEC §8, ADR-0002) in PLAN 1.23.
//!
//! A bundle is a directory `name.scaena/` (zip support in PLAN 1.4). `deck.json`
//! is canonical. The theme is referenced from the deck (`theme` field) relative
//! to the bundle root.

use scaena_core::Deck;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("deck.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("not a bundle or deck file: {0}")]
    NotABundle(PathBuf),
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
}

/// An opened bundle: the deck plus where it came from.
#[derive(Debug, Clone)]
pub struct Bundle {
    pub root: PathBuf,
    pub deck: Deck,
    pub theme_json: Option<String>,
}

impl Bundle {
    /// Open a bundle directory, or a bare `deck.json` (its parent becomes the root).
    pub fn open(path: &Path) -> Result<Bundle, StoreError> {
        let (root, deck_path) = if path.is_dir() {
            (path.to_path_buf(), path.join("deck.json"))
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            (path.parent().unwrap_or(Path::new(".")).to_path_buf(), path.to_path_buf())
        } else {
            return Err(StoreError::NotABundle(path.to_path_buf()));
        };
        let deck = Deck::from_json(&std::fs::read_to_string(&deck_path)?)?;
        let theme_json = match &deck.theme {
            Some(serde_json::Value::String(rel)) => Some(std::fs::read_to_string(root.join(rel))?),
            Some(inline @ serde_json::Value::Object(_)) => Some(inline.to_string()),
            _ => None,
        };
        Ok(Bundle { root, deck, theme_json })
    }

    /// Write `deck.json` back (canonical pretty JSON). CRDT history: PLAN 1.23.
    pub fn save(&self) -> Result<(), StoreError> {
        std::fs::write(self.root.join("deck.json"), self.deck.to_json()?)?;
        Ok(())
    }
}

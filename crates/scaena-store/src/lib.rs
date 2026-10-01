//! # scaena-store
//!
//! Bundle I/O (SPEC §3.1) now; the CRDT document (SPEC §8, ADR-0002) in PLAN 1.23.
//!
//! A bundle is a directory `name.scaena/` (zip support in PLAN 1.4). `deck.json`
//! is canonical. The theme is referenced from the deck (`theme` field) relative
//! to the bundle root.

use scaena_core::Deck;
use std::path::{Component, Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("reading {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("`{0}` is not a path inside the bundle (relative, no `..`)")]
    OutsideBundle(String),
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
            Some(serde_json::Value::String(rel)) => Some(std::fs::read_to_string(inside(&root, rel)?)?),
            Some(inline @ serde_json::Value::Object(_)) => Some(inline.to_string()),
            _ => None,
        };
        Ok(Bundle { root, deck, theme_json })
    }

    /// A path the deck names (theme, fonts, data), resolved inside the bundle root.
    /// Bundles are self-contained (SPEC §3.1): absolute paths and `..` are refused.
    pub fn path(&self, rel: &str) -> Result<PathBuf, StoreError> {
        inside(&self.root, rel)
    }

    /// The deck's font files (`fonts[].file`) in deck order, as (bundle id, bytes). The
    /// id is the file's path inside the bundle, which is how display lists name fonts.
    pub fn read_fonts(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let read = |rel: &str| -> Result<Vec<u8>, StoreError> {
            let path = self.path(rel)?;
            std::fs::read(&path).map_err(|source| StoreError::Read { path, source })
        };
        self.deck.fonts.iter().map(|f| Ok((f.file.clone(), read(&f.file)?))).collect()
    }

    /// Write `deck.json` back (canonical pretty JSON). CRDT history: PLAN 1.23.
    pub fn save(&self) -> Result<(), StoreError> {
        std::fs::write(self.root.join("deck.json"), self.deck.to_json()?)?;
        Ok(())
    }
}

fn inside(root: &Path, rel: &str) -> Result<PathBuf, StoreError> {
    let normal = Path::new(rel).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if rel.is_empty() || !normal {
        return Err(StoreError::OutsideBundle(rel.to_string()));
    }
    Ok(root.join(rel))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TORTURE: &str = "../../tests/fixtures/torture.scaena";

    #[test]
    fn fonts_are_read_in_deck_order_under_their_bundle_ids() {
        let b = Bundle::open(Path::new(TORTURE)).unwrap();
        let fonts = b.read_fonts().unwrap();
        let ids: Vec<&str> = fonts.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "fonts/RobotoSerif-VF.ttf",
                "fonts/EBGaramond-VF.ttf",
                "fonts/NotoSansHebrew-VF.ttf",
                "fonts/NotoSansArabic-VF.ttf",
                "fonts/NotoColorEmoji-COLRv1.ttf",
            ]
        );
        // Every file is an OpenType font (TrueType outlines: version 0x00010000).
        assert!(fonts.iter().all(|(_, bytes)| bytes.starts_with(&[0, 1, 0, 0])));
    }

    #[test]
    fn paths_outside_the_bundle_are_refused() {
        let b = Bundle::open(Path::new(TORTURE)).unwrap();
        for rel in ["../torture.scaena/theme.json", "fonts/../../x.ttf", "/etc/hosts", ""] {
            assert!(matches!(b.path(rel), Err(StoreError::OutsideBundle(_))), "{rel:?}");
        }
        assert_eq!(b.path("./fonts/a.ttf").unwrap(), Path::new(TORTURE).join("./fonts/a.ttf"));
    }

    #[test]
    fn a_missing_font_names_its_path() {
        let mut b = Bundle::open(Path::new(TORTURE)).unwrap();
        b.deck.fonts[0].file = "fonts/missing.ttf".into();
        let err = b.read_fonts().unwrap_err().to_string();
        assert!(err.contains("fonts/missing.ttf"), "{err}");
    }
}

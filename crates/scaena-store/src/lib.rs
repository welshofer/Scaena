//! # scaena-store
//!
//! Bundle I/O (SPEC §3.1, PLAN 1.4), and the CRDT document that keeps a bundle's history
//! ([`crdt`], SPEC §8, ADR-0002, PLAN 1.23).
//!
//! A bundle is a directory `name.scaena/` or a zip of the same layout, `name.scaena`. It
//! opens either way, or from a bare deck file (its directory is the bundle). `deck.json`
//! is canonical. The theme is a file the deck names (`theme`), relative to the bundle root,
//! or inline. [`Bundle::save`] writes a bundle back as SPEC §3.1 lays it out: fonts
//! subset to what the deck can draw ([`subset`]), assets and fonts content-addressed, and a
//! manifest.
//!
//! A bundle that keeps `history/deck.loro` keeps its history: whatever writes its deck
//! records the change there too ([`Bundle::record`]), by the bundle's `author`, after taking
//! in, as a change by `fs`, any edit made to `deck.json` outside Scaena since. It keeps the
//! files the deck is drawn from beside it ([`kept_paths`]): its data files (ADR-0014) and its
//! theme (ADR-0016).
//!
//! A bundle can also live in memory, as a page holds one (PLAN 2.4): opened from its files
//! ([`Bundle::in_memory`]) or a `.scaena` zip's bytes ([`Bundle::from_zip`]), and saved to
//! a set of files ([`Bundle::saving`]) the caller writes where it keeps them, or zips
//! ([`zip`]).

pub mod crdt;
mod save;
pub mod subset;

pub use save::{SaveOptions, Saved, Saving, place, zip};

use crdt::{CrdtError, DeckDoc, Edit};
use scaena_core::Deck;
use scaena_core::validate::BundleFiles;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("reading {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    #[error("`{0}` is not in the bundle")]
    Missing(String),
    #[error("`{0}` is not a path inside the bundle (relative, no `..`)")]
    OutsideBundle(String),
    #[error("deck.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{}: not a zip bundle: {source}", path.display())]
    Zip { path: PathBuf, source: zip::result::ZipError },
    #[error("not a bundle or deck file: {0}")]
    NotABundle(PathBuf),
    #[error("{0}: {1}")]
    Subset(String, subset::SubsetError),
    #[error("{}: will not save over a directory that is not this bundle's", .0.display())]
    Occupied(PathBuf),
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("{HISTORY}: {0}")]
    Crdt(#[from] CrdtError),
    /// What keeps the history elsewhere, as a page's module does (PLAN 2.9), could not
    /// record in it.
    #[error("{HISTORY}: {0}")]
    History(String),
}

/// Where a bundle keeps its CRDT document and its history (SPEC §3.1, §8).
pub const HISTORY: &str = "history/deck.loro";

/// Where a bundle's files are: a directory on disk, or in memory, by their paths inside
/// the bundle: the entries of a zip, read once, or the files a page holds.
#[derive(Debug, Clone)]
pub enum Files {
    Dir(PathBuf),
    Zip(Arc<BTreeMap<String, Vec<u8>>>),
}

impl Files {
    /// The file at `rel`, a path inside the bundle.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>, StoreError> {
        match self {
            Files::Dir(root) => {
                let path = inside(root, rel)?;
                std::fs::read(&path).map_err(|source| StoreError::Read { path, source })
            }
            Files::Zip(entries) => {
                let key = normal(rel)?;
                entries.get(&key).cloned().ok_or(StoreError::Missing(key))
            }
        }
    }

    /// The size of the file at `rel`, in bytes.
    pub fn size(&self, rel: &str) -> Result<u64, StoreError> {
        match self {
            Files::Dir(root) => {
                let path = inside(root, rel)?;
                std::fs::metadata(&path).map(|m| m.len()).map_err(|source| StoreError::Read { path, source })
            }
            Files::Zip(entries) => {
                let key = normal(rel)?;
                entries.get(&key).map(|bytes| bytes.len() as u64).ok_or(StoreError::Missing(key))
            }
        }
    }

    /// Every file in the bundle, by its path inside it, sorted.
    pub fn list(&self) -> Result<Vec<String>, StoreError> {
        match self {
            Files::Dir(root) => {
                let mut out = Vec::new();
                walk(root, root, &mut out)?;
                scaena_core::sort::sort(&mut out);
                Ok(out)
            }
            Files::Zip(entries) => Ok(entries.keys().cloned().collect()),
        }
    }
}

impl BundleFiles for Files {
    fn exists(&self, path: &str) -> bool {
        match self {
            Files::Dir(root) => inside(root, path).is_ok_and(|p| p.is_file()),
            Files::Zip(entries) => normal(path).is_ok_and(|key| entries.contains_key(&key)),
        }
    }

    fn read_text(&self, path: &str) -> Option<String> {
        String::from_utf8(self.read(path).ok()?).ok()
    }
}

/// The JSON of the theme `deck` names: a file in the bundle, or inline.
fn theme_of(deck: &Deck, files: &Files) -> Result<Option<String>, StoreError> {
    Ok(match &deck.theme {
        Some(serde_json::Value::String(rel)) => Some(String::from_utf8_lossy(&files.read(rel)?).into_owned()),
        Some(inline @ serde_json::Value::Object(_)) => Some(inline.to_string()),
        _ => None,
    })
}

/// An opened bundle: the deck, its theme's JSON, and its files.
#[derive(Debug, Clone)]
pub struct Bundle {
    /// The directory, or the zip file, it was opened from.
    pub root: PathBuf,
    /// The deck file's path inside the bundle (`deck.json`, unless opened from another
    /// deck file).
    pub deck_file: String,
    pub deck: Deck,
    pub theme_json: Option<String>,
    pub files: Files,
    /// Who edits it: `user` unless the client says otherwise, as `agent:<name>` (SPEC §8.2).
    /// Changes written to its history are theirs.
    pub author: String,
}

impl Bundle {
    /// Open a bundle directory, a `.scaena` zip, or a bare deck file (its directory is the
    /// bundle).
    pub fn open(path: &Path) -> Result<Bundle, StoreError> {
        let (root, deck_file, files) = locate(path)?;
        let text = String::from_utf8_lossy(&files.read(&deck_file)?).into_owned();
        let deck = Deck::from_json(&text)?;
        let theme_json = theme_of(&deck, &files)?;
        Ok(Bundle { root, deck_file, deck, theme_json, files, author: "user".into() })
    }

    /// A bundle held in memory: `files`, by their paths inside it, with its deck at
    /// `deck.json` (PLAN 2.4). A path outside the bundle is refused.
    pub fn in_memory(files: BTreeMap<String, Vec<u8>>) -> Result<Bundle, StoreError> {
        let files = scaena_core::sort::try_map(
            files.into_iter().map(|(rel, bytes)| Ok::<_, StoreError>((normal(&rel)?, bytes))),
        )?;
        let files = Files::Zip(Arc::new(files));
        let deck_file = "deck.json".to_string();
        let text = String::from_utf8_lossy(&files.read(&deck_file)?).into_owned();
        let deck = Deck::from_json(&text)?;
        let theme_json = theme_of(&deck, &files)?;
        Ok(Bundle { root: PathBuf::from("deck.scaena"), deck_file, deck, theme_json, files, author: "user".into() })
    }

    /// A `.scaena` zip's bytes, opened in memory.
    pub fn from_zip(bytes: &[u8]) -> Result<Bundle, StoreError> {
        Bundle::in_memory(unzip_from(std::io::Cursor::new(bytes), Path::new("deck.scaena"))?)
    }

    /// A path the deck names (theme, fonts, data), resolved inside a directory bundle.
    /// Bundles are self-contained (SPEC §3.1): absolute paths and `..` are refused.
    pub fn path(&self, rel: &str) -> Result<PathBuf, StoreError> {
        inside(&self.root, rel)
    }

    /// The file at `rel` inside the bundle.
    pub fn read(&self, rel: &str) -> Result<Vec<u8>, StoreError> {
        self.files.read(rel)
    }

    /// The bundle's CRDT document, if it keeps one ([`HISTORY`]), with `deck.json` and the
    /// files it is drawn from as they are now taken in: a deck edited outside Scaena since it
    /// was last written goes in as a change by `fs` (SPEC §8.1), and then so does a data file
    /// or a theme replaced or edited outside it, or one the history does not hold yet
    /// (ADR-0014, ADR-0016).
    pub fn history(&self) -> Result<Option<DeckDoc>, StoreError> {
        if !self.files.exists(HISTORY) {
            return Ok(None);
        }
        let doc = DeckDoc::load(&self.read(HISTORY)?)?;
        let disk = Deck::from_json(&String::from_utf8_lossy(&self.read(&self.deck_file)?))?;
        doc.outside(&disk, &self.kept_files(&disk, &BTreeMap::new()), None)?;
        Ok(Some(doc))
    }

    /// The history to write beside `deck`, if the bundle keeps one: with the change from
    /// the deck it holds to `deck` recorded as `edit` says, and the files the history keeps
    /// beside `deck` ([`kept_paths`]) that `written` holds, as they are written there, in the
    /// same change (ADR-0014, ADR-0016).
    pub fn record(
        &self,
        deck: &Deck,
        written: &BTreeMap<String, Vec<u8>>,
        edit: &Edit,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        let Some(doc) = self.history()? else { return Ok(None) };
        let files: Vec<(String, Vec<u8>)> = kept_paths(deck)
            .into_iter()
            .filter_map(|path| written.get(path).map(|bytes| (path.to_string(), bytes.clone())))
            .collect();
        doc.apply_with(deck, &files, edit)?;
        Ok(Some(doc.save()?))
    }

    /// The files the history keeps beside `deck` ([`kept_paths`]), as (bundle path, bytes), in
    /// deck order: each as `written` has it, else as the bundle holds it; one neither holds is
    /// left out.
    pub fn kept_files(&self, deck: &Deck, written: &BTreeMap<String, Vec<u8>>) -> Vec<(String, Vec<u8>)> {
        let bytes = |path: &str| written.get(path).cloned().or_else(|| self.read(path).ok());
        kept_paths(deck).into_iter().filter_map(|path| Some((path.to_string(), bytes(path)?))).collect()
    }

    /// The deck's font files (`fonts[].file`) in deck order, as (bundle id, bytes). The
    /// id is the file's path inside the bundle, which is how display lists name fonts.
    pub fn read_fonts(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        self.deck.fonts.iter().map(|f| Ok((f.file.clone(), self.read(&f.file)?))).collect()
    }

    /// The image files the deck's image nodes name, as (bundle path, bytes).
    pub fn read_images(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        self.deck.image_files().into_iter().map(|path| Ok((path.clone(), self.read(&path)?))).collect()
    }

    /// The files the deck's data sources name (`data.*.source` strings), as (bundle
    /// path, bytes), in deck order; inline sources need no file.
    pub fn read_data(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let mut out = Vec::new();
        for source in self.deck.data.values() {
            if let serde_json::Value::String(rel) = &source.source {
                out.push((rel.clone(), self.read(rel)?));
            }
        }
        Ok(out)
    }
}

/// The files a bundle's history keeps beside `deck`, by their paths, each once: those its data
/// sources name (`data.*.source` strings), in deck order (ADR-0014), then the theme file it
/// names (ADR-0016). An inline theme or inline rows are the deck's own.
pub fn kept_paths(deck: &Deck) -> Vec<&str> {
    let mut out: Vec<&str> = Vec::new();
    let data = deck.data.values().filter_map(|source| source.source.as_str());
    for path in data.chain(deck.theme.as_ref().and_then(|theme| theme.as_str())) {
        if !out.contains(&path) {
            out.push(path);
        }
    }
    out
}

/// A bundle's deck file as text, unparsed, and the bundle's files: what `scaena validate`
/// checks (`scaena_core::validate::validate_bundle`).
pub fn open_unparsed(path: &Path) -> Result<(String, Files), StoreError> {
    let (_, deck_file, files) = locate(path)?;
    let deck = String::from_utf8_lossy(&files.read(&deck_file)?).into_owned();
    Ok((deck, files))
}

/// The bundle at `path`: its root, its deck file's path inside it, and its files.
fn locate(path: &Path) -> Result<(PathBuf, String, Files), StoreError> {
    let ext = path.extension().and_then(|e| e.to_str());
    if path.is_dir() {
        Ok((path.to_path_buf(), "deck.json".into(), Files::Dir(path.to_path_buf())))
    } else if ext == Some("scaena") {
        Ok((path.to_path_buf(), "deck.json".into(), Files::Zip(Arc::new(unzip(path)?))))
    } else if ext == Some("json") {
        let root = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let name = path.file_name().and_then(|n| n.to_str()).ok_or_else(|| StoreError::NotABundle(path.into()))?;
        Ok((root.clone(), name.to_string(), Files::Dir(root)))
    } else {
        Err(StoreError::NotABundle(path.to_path_buf()))
    }
}

/// Every file in a zip bundle, by its path inside it.
fn unzip(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
    let file = std::fs::File::open(path).map_err(|source| StoreError::Read { path: path.into(), source })?;
    unzip_from(file, path)
}

/// Every file in the zip `reader` holds, by its path inside the bundle; `path` names it in
/// an error.
fn unzip_from(reader: impl Read + std::io::Seek, path: &Path) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
    let zip_error = |source| StoreError::Zip { path: path.to_path_buf(), source };
    let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
    let mut out = BTreeMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }
        let name = normal(entry.name())?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;
        out.insert(name, bytes);
    }
    Ok(out)
}

/// The files under `dir`, as paths relative to `root` with `/` between parts.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), StoreError> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path.is_dir() {
            walk(root, &path, out)?;
        } else if let Ok(rel) = path.strip_prefix(root) {
            let parts: Vec<String> = rel.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect();
            out.push(parts.join("/"));
        }
    }
    Ok(())
}

/// `rel` as a bundle path: relative, `/`-separated, no `.` or `..` parts.
fn normal(rel: &str) -> Result<String, StoreError> {
    let parts: Vec<&str> = rel.split('/').filter(|p| !p.is_empty() && *p != ".").collect();
    if parts.is_empty() || rel.starts_with('/') || parts.contains(&"..") || rel.contains('\\') {
        return Err(StoreError::OutsideBundle(rel.to_string()));
    }
    Ok(parts.join("/"))
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
            assert!(matches!(normal(rel), Err(StoreError::OutsideBundle(_))), "{rel:?}");
        }
        assert_eq!(b.path("./fonts/a.ttf").unwrap(), Path::new(TORTURE).join("./fonts/a.ttf"));
        assert_eq!(normal("./fonts//a.ttf").unwrap(), "fonts/a.ttf");
    }

    #[test]
    fn a_missing_font_names_its_path() {
        let mut b = Bundle::open(Path::new(TORTURE)).unwrap();
        b.deck.fonts[0].file = "fonts/missing.ttf".into();
        let err = b.read_fonts().unwrap_err().to_string();
        assert!(err.contains("fonts/missing.ttf"), "{err}");
    }

    #[test]
    fn a_directory_lists_its_files_with_slashes() {
        let files = Files::Dir(PathBuf::from(TORTURE)).list().unwrap();
        assert!(files.contains(&"deck.json".to_string()));
        assert!(files.contains(&"fonts/RobotoSerif-VF.ttf".to_string()));
        assert!(files.windows(2).all(|w| w[0] < w[1]), "sorted");
    }
}

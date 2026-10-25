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

pub mod common;
pub mod crdt;
mod save;
pub mod subset;

pub use save::{SaveOptions, Saved, Saving, place, sha256, zip};

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
    /// A file or folder of the bundle that could not be read, and why, its `source`: not found,
    /// permission denied, a folder where a file goes (lift-up 10-24b USE-3). The message names
    /// the path and the source says why, once: anyhow's chain says it in the CLI, and
    /// `scaena_ops`' errors join it.
    #[error("reading {}", path.display())]
    Read { path: PathBuf, source: std::io::Error },
    /// A file of the bundle that could not be written or removed, and why, its `source`.
    #[error("writing {}", path.display())]
    Write { path: PathBuf, source: std::io::Error },
    #[error("`{0}` is not in the bundle")]
    Missing(String),
    #[error("`{0}` is not a path inside the bundle (relative, no `..`)")]
    OutsideBundle(String),
    /// A path inside a folder bundle whose link leads out of it: a bundle is self-contained
    /// (SPEC §3.1), and what is outside it is neither read into a save nor written over.
    #[error("`{0}` is a link that leads out of the bundle; copy the file into the bundle in its place")]
    LinkedOut(String),
    #[error("deck.json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{}: not a zip bundle: {source}", path.display())]
    Zip { path: PathBuf, source: zip::result::ZipError },
    /// A zip bundle that would unpack past what a bundle may hold ([`Unzipped`]): a zip bomb,
    /// or a header that claims a size it does not hold, refused before it takes the memory.
    #[error("{}: {why}", path.display())]
    TooLarge { path: PathBuf, why: String },
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

/// What a zip bundle may unpack to, at most (lift-up SEC-1): a `.scaena` someone sends is opened
/// by the browser, the Mac and the iPad, the CLI, and the MCP server, and one that would unpack
/// past these is refused, saying why, before it takes the memory. Far above any deck: B4, the
/// largest bench deck, unpacks to some 30 MB.
#[derive(Clone, Copy, Debug)]
pub struct Unzipped {
    /// Files, at most.
    pub entries: usize,
    /// Bytes in one file, at most.
    pub entry: u64,
    /// Bytes in all of them, at most.
    pub total: u64,
}

/// The bounds every zip bundle is opened within.
pub const UNZIPPED: Unzipped = Unzipped { entries: 10_000, entry: 256 << 20, total: 1 << 30 };

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
        self.reading(rel, &mut Vec::new())
    }

    /// [`Self::read`], for one of several files read together: `known` holds the folders already
    /// found to be folders and no links, which are not looked at again.
    fn reading(&self, rel: &str, known: &mut Vec<PathBuf>) -> Result<Vec<u8>, StoreError> {
        match self {
            Files::Dir(root) => opened(root, rel, known),
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

    fn linked_out(&self, path: &str) -> bool {
        matches!(self, Files::Dir(root) if matches!(inside(root, path), Err(StoreError::LinkedOut(_))))
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
        let known = &mut Vec::new();
        self.deck.fonts.iter().map(|f| Ok((f.file.clone(), self.files.reading(&f.file, known)?))).collect()
    }

    /// The image files the deck's image nodes name, as (bundle path, bytes).
    pub fn read_images(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let known = &mut Vec::new();
        self.deck.image_files().into_iter().map(|path| Ok((path.clone(), self.files.reading(&path, known)?))).collect()
    }

    /// The files the deck's data sources name (`data.*.source` strings), as (bundle
    /// path, bytes), in deck order; inline sources need no file.
    pub fn read_data(&self) -> Result<Vec<(String, Vec<u8>)>, StoreError> {
        let (mut out, known) = (Vec::new(), &mut Vec::new());
        for source in self.deck.data.values() {
            if let serde_json::Value::String(rel) = &source.source {
                out.push((rel.clone(), self.files.reading(rel, known)?));
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
    unzip_within(reader, path, UNZIPPED)
}

/// [`unzip_from`], within `most`: each file read no further than one byte past what one may
/// hold, and its room kept by what it may hold, not by what its header claims.
fn unzip_within(
    reader: impl Read + std::io::Seek,
    path: &Path,
    most: Unzipped,
) -> Result<BTreeMap<String, Vec<u8>>, StoreError> {
    let zip_error = |source| StoreError::Zip { path: path.to_path_buf(), source };
    let too_large = |why: String| StoreError::TooLarge { path: path.to_path_buf(), why };
    let mut archive = zip::ZipArchive::new(reader).map_err(zip_error)?;
    if archive.len() > most.entries {
        return Err(too_large(format!("holds {} files, more than a bundle may ({})", archive.len(), most.entries)));
    }
    let mut out = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(zip_error)?;
        if entry.is_dir() {
            continue;
        }
        let name = normal(entry.name())?;
        let room = entry.size().min(most.entry).min(most.total - total);
        let mut bytes = Vec::with_capacity(usize::try_from(room).unwrap_or(0));
        (&mut entry).take(most.entry + 1).read_to_end(&mut bytes)?;
        let read = bytes.len() as u64;
        if read > most.entry {
            return Err(too_large(format!(
                "`{name}` unpacks past {} bytes, more than a file in a bundle may",
                most.entry
            )));
        }
        total += read;
        if total > most.total {
            return Err(too_large(format!("unpacks past {} bytes, more than a bundle may", most.total)));
        }
        out.insert(name, bytes);
    }
    Ok(out)
}

/// The files under `dir`, as paths relative to `root` with `/` between parts; none whose name
/// starts with a dot, as a partial a save left behind does ([`write_whole`]), and as the Mac's
/// open and `scaena serve`'s scan pass them over. A link is listed only where it is a file that
/// stays inside the bundle: a linked folder is not walked, so a link to `/` or to a folder
/// above lists nothing and never walks the disk.
fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) -> Result<(), StoreError> {
    let unread = |source| StoreError::Read { path: dir.to_path_buf(), source };
    for entry in std::fs::read_dir(dir).map_err(unread)? {
        let entry = entry.map_err(unread)?;
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = entry.path();
        let kind = entry.file_type().map_err(|source| StoreError::Read { path: path.clone(), source })?;
        if kind.is_symlink() && (path.is_dir() || !links_inside(root, &path)) {
            continue;
        }
        if kind.is_dir() {
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

/// Write `bytes` at `path` whole, or not at all (lift-up REL-3): to a file beside it named
/// `.<name>.partial`, made new, so nothing left in its place, a link included, can take the
/// write elsewhere; synced; then renamed over `path`, which the file system does in one step.
/// A crash, a full disk, or a process killed partway leaves what was at `path` as it was, and
/// at most the partial, which a bundle's listing passes over and the next write replaces.
/// `path`'s directory must exist.
pub fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let partial = staged(path, bytes)?;
    std::fs::rename(&partial, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&partial);
    })
}

/// Write each of `files` at its path whole or not at all, and all of them as nearly together as a
/// file system allows (lift-up REL-3): every file's bytes go to a partial beside it first
/// ([`write_whole`]'s), synced, and only once each is written whole is each renamed into place,
/// in `files`' order. A disk that fills or a path that cannot be written stops the write before
/// any file has changed; a rename that fails leaves those before it written and those after it,
/// `deck.json` among them where it is last, as they were. Each file's directory is made. A
/// failure names the file it stopped at, and why ([`StoreError::Write`]).
pub fn write_all(files: &[(PathBuf, &[u8])]) -> Result<(), StoreError> {
    let mut partials: Vec<PathBuf> = Vec::with_capacity(files.len());
    let unstaged = |partials: &[PathBuf]| {
        for partial in partials {
            let _ = std::fs::remove_file(partial);
        }
    };
    for (path, bytes) in files {
        let made = path.parent().filter(|p| !p.as_os_str().is_empty()).map_or(Ok(()), std::fs::create_dir_all);
        match made.and_then(|()| staged(path, bytes)) {
            Ok(partial) => partials.push(partial),
            Err(source) => {
                unstaged(&partials);
                return Err(StoreError::Write { path: path.clone(), source });
            }
        }
    }
    for (i, (path, _)) in files.iter().enumerate() {
        if let Err(source) = std::fs::rename(&partials[i], path) {
            unstaged(&partials[i..]);
            return Err(StoreError::Write { path: path.clone(), source });
        }
    }
    Ok(())
}

/// `bytes` written whole and synced to the partial beside `path`, `.<name>.partial`, made new;
/// the partial's path. One left there by an earlier write is replaced.
fn staged(path: &Path, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let partial = path.with_file_name(format!(".{name}.partial"));
    let _ = std::fs::remove_file(&partial);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .and_then(|mut file| {
            std::io::Write::write_all(&mut file, bytes)?;
            file.sync_all()
        })
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&partial);
        })?;
    Ok(partial)
}

/// `rel`, a path inside the bundle at `root`, as a path on disk: relative, with no `..`, and,
/// read through every link, still inside the bundle. A bundle taken from git or an archive
/// keeps its links, and one that leads out would read a file the user did not mean into what
/// a save or an export carries, or write a data edit over one outside.
fn inside(root: &Path, rel: &str) -> Result<PathBuf, StoreError> {
    held(root, rel, &mut Vec::new(), true)
}

/// The file at `rel` in the bundle at `root`, read as [`inside`] holds it. On unix it is opened
/// with O_NOFOLLOW, which refuses a link: the folders on its way are looked at, each once for a
/// batch of reads (`known`), and the file itself is read through its link only where it is one
/// that stays inside. So a read costs what `std::fs::read` does, and a look at each new folder.
fn opened(root: &Path, rel: &str, known: &mut Vec<PathBuf>) -> Result<Vec<u8>, StoreError> {
    #[cfg(unix)]
    {
        use std::io::Read;
        use std::os::unix::fs::OpenOptionsExt;
        let path = held(root, rel, known, false)?;
        let read = |path: PathBuf| std::fs::read(&path).map_err(|source| StoreError::Read { path, source });
        match std::fs::OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW).open(&path) {
            Ok(mut file) => {
                let mut bytes = Vec::new();
                file.read_to_end(&mut bytes).map_err(|source| StoreError::Read { path, source })?;
                Ok(bytes)
            }
            Err(e) if e.raw_os_error() == Some(libc::ELOOP) => read(resolved(root, path, rel)?),
            Err(source) => Err(StoreError::Read { path, source }),
        }
    }
    #[cfg(not(unix))]
    {
        let path = held(root, rel, known, true)?;
        std::fs::read(&path).map_err(|source| StoreError::Read { path, source })
    }
}

/// [`inside`], its last part looked at only with `last`. A folder in `known` is taken as looked
/// at; each folder found to be no link joins it.
fn held(root: &Path, rel: &str, known: &mut Vec<PathBuf>, last: bool) -> Result<PathBuf, StoreError> {
    let normal = Path::new(rel).components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir));
    if rel.is_empty() || !normal {
        return Err(StoreError::OutsideBundle(rel.to_string()));
    }
    let path = root.join(rel);
    // Only a link inside the bundle can lead out of it: each part of `rel` is looked at as it is
    // on disk, and read through its links only where one is a link. A part not there yet, and
    // all after it, is made new. So a read costs a look at each part of its path in the bundle,
    // not a walk of the whole path from the disk's root.
    let parts: Vec<Component> = Path::new(rel).components().filter(|c| matches!(c, Component::Normal(_))).collect();
    let looked = if last { parts.len() } else { parts.len().saturating_sub(1) };
    let mut at = root.to_path_buf();
    for part in &parts[..looked] {
        at.push(part);
        if known.contains(&at) {
            continue;
        }
        match std::fs::symlink_metadata(&at) {
            Err(_) => return Ok(path),
            Ok(meta) if meta.file_type().is_symlink() => return resolved(root, path, rel),
            Ok(meta) if meta.is_dir() => known.push(at.clone()),
            Ok(_) => {}
        }
    }
    Ok(path)
}

/// `path`, at `rel` in the bundle at `root` and passing through a link, where read through its
/// links it is still inside the bundle.
fn resolved(root: &Path, path: PathBuf, rel: &str) -> Result<PathBuf, StoreError> {
    let Ok(real_root) = std::fs::canonicalize(root) else { return Ok(path) };
    // The nearest part of it that is there, read through its links; the rest is made new.
    let mut there = path.as_path();
    while std::fs::symlink_metadata(there).is_err() {
        match there.parent() {
            Some(parent) => there = parent,
            None => return Ok(path),
        }
    }
    match std::fs::canonicalize(there) {
        Ok(real) if real.starts_with(&real_root) => Ok(path),
        _ => Err(StoreError::LinkedOut(rel.to_string())),
    }
}

/// Whether `path`, a link inside the bundle at `root`, leads somewhere still inside it.
fn links_inside(root: &Path, path: &Path) -> bool {
    match (std::fs::canonicalize(root), std::fs::canonicalize(path)) {
        (Ok(root), Ok(real)) => real.starts_with(root),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TORTURE: &str = "../../tests/fixtures/torture.scaena";

    /// A zip of `files`, as a save writes one.
    fn zipped(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
        save::zip(&files.iter().map(|(name, bytes)| (name.to_string(), bytes.clone())).collect()).unwrap()
    }

    /// Unpacks `zip` within small bounds, saying why it is refused where it is.
    fn within(zip: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let most = Unzipped { entries: 3, entry: 1000, total: 1500 };
        unzip_within(std::io::Cursor::new(zip), Path::new("sent.scaena"), most).map_err(|e| e.to_string())
    }

    /// A zip bundle unpacks only within what a bundle may hold (lift-up SEC-1): too many files, a
    /// file that inflates past its bound, or all of them past theirs, is refused naming the
    /// bundle; and the room a file is read into is what it may hold, not what its header claims.
    #[test]
    fn a_zip_that_unpacks_past_a_bundle_is_refused_before_it_takes_the_memory() {
        let fits = within(&zipped(&[("deck.json", b"{}".to_vec()), ("data/a.csv", vec![b'a'; 1000])])).unwrap();
        assert_eq!(fits["data/a.csv"].len(), 1000);
        let many: Vec<(&str, Vec<u8>)> = ["a", "b", "c", "d"].iter().map(|n| (*n, vec![])).collect();
        let refused = within(&zipped(&many)).unwrap_err();
        assert!(refused.starts_with("sent.scaena: holds 4 files"), "{refused}");
        // A bomb: a thousand and one zeros deflate to a few bytes.
        let refused = within(&zipped(&[("fonts/bomb.ttf", vec![0; 1001])])).unwrap_err();
        assert!(refused.contains("`fonts/bomb.ttf` unpacks past 1000 bytes"), "{refused}");
        let refused = within(&zipped(&[("a", vec![1; 800]), ("b", vec![2; 800])])).unwrap_err();
        assert!(refused.contains("unpacks past 1500 bytes, more than a bundle may"), "{refused}");
        // A header that claims some 4 GB for ten bytes: read whole, in the room ten bytes need at
        // most, never the room it claims.
        let mut forged = zipped(&[("a", vec![7; 10])]);
        let central = forged.windows(4).position(|w| w == [0x50, 0x4b, 0x01, 0x02]).unwrap();
        forged[central + 24..central + 28].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        let read = within(&forged).unwrap();
        assert_eq!(read["a"], vec![7; 10]);
        assert!(read["a"].capacity() <= 1000, "room kept by the header's claim: {}", read["a"].capacity());
    }

    /// The bounds every zip bundle opens within reach the browser's, the Mac's, and the CLI's
    /// open: a bundle of more files than one may hold is refused, not unpacked.
    #[test]
    fn a_bundle_sent_as_a_zip_of_too_many_files_does_not_open() {
        let mut files: Vec<(String, Vec<u8>)> =
            (0..=UNZIPPED.entries).map(|i| (format!("data/{i}.csv"), vec![])).collect();
        files.push(("deck.json".into(), std::fs::read(Path::new(TORTURE).join("deck.json")).unwrap()));
        let zip = save::zip(&files.into_iter().collect()).unwrap();
        let Err(refused) = Bundle::from_zip(&zip) else { panic!("a zip of too many files opened") };
        let refused = refused.to_string();
        assert!(refused.contains("files, more than a bundle may (10000)"), "{refused}");
    }

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
                "fonts/JetBrainsMono-VF.ttf",
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

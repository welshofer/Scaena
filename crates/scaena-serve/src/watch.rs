//! What the server knows of the bundle's folder, and how it keeps up with it (PLAN 2.11): each
//! file's size and time as last seen or written, and `deck.scn` as last compiled. A thread scans
//! the folder. What changed there, once it settles, is announced, and a changed `deck.scn` is
//! compiled into `deck.json` first, as `scaena compile` does. A page's own writes update what is
//! known as they happen, so each save is announced once, as that page's.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Weak;
use std::time::{Duration, Instant, SystemTime};

use scaena_core::document::Deck;
use scaena_ops::compile::{Compiled, line_col};
use scaena_store::Files;
use serde::Serialize;
use serde_json::json;

use crate::{Note, Shared};

/// The deck's source, which the server compiles when it changes on disk.
pub const SOURCE: &str = "deck.scn";
/// The deck (SPEC §3.1).
pub const DECK: &str = "deck.json";

/// How often the folder is scanned.
const POLL: Duration = Duration::from_millis(150);
/// How long a page's writes must stop before they are announced.
const SETTLE: Duration = Duration::from_millis(250);

/// A file as the folder last showed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: Option<SystemTime>,
}

/// The folder's files by their paths inside it.
type Scan = BTreeMap<String, Stamp>;

/// What the server knows of the folder.
#[derive(Default)]
pub struct State {
    known: Scan,
    /// `deck.scn` as last compiled or written; `None` without one.
    source: Option<Vec<u8>>,
    /// Counts the changes announced.
    version: u64,
    /// Why `deck.scn` does not compile, while it does not.
    failed: Option<Failure>,
    /// A page's writes, announced once they stop.
    pending: Option<Pending>,
    /// A scan that differs from what is known, while the folder settles.
    unsettled: Option<Scan>,
}

struct Pending {
    by: Option<String>,
    paths: BTreeSet<String>,
    at: Instant,
}

/// Why `deck.scn` does not compile: the source as it was, and each problem in it.
#[derive(Clone, Debug, Serialize)]
pub struct Failure {
    #[serde(skip)]
    pub source: String,
    pub problems: Vec<Problem>,
}

/// One thing the compiler found (SPEC §4, §7.4), at the source it is about.
#[derive(Clone, Debug, Serialize)]
pub struct Problem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
    /// 1-based line and column of the source it is about, when it is about the source.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub col: Option<usize>,
    /// Byte offset and length into the source.
    #[serde(skip)]
    pub span: Option<(usize, usize)>,
    /// Where it lands in the deck, a JSON pointer.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// The file it is about, when that is not the deck (the theme).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl Problem {
    fn said(message: impl Into<String>) -> Problem {
        Problem {
            code: None,
            message: message.into(),
            line: None,
            col: None,
            span: None,
            path: None,
            file: None,
            hint: None,
        }
    }
}

/// Scan the folder every [`POLL`] until the server is gone.
pub fn run(shared: Weak<Shared>) {
    loop {
        std::thread::sleep(POLL);
        match shared.upgrade() {
            Some(shared) => shared.tick(),
            None => return,
        }
    }
}

impl Shared {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// What the server knows, first: the folder as it is, and `deck.scn` compiled if it is newer
    /// than `deck.json`, as `make` would.
    pub fn start(&self) {
        let mut state = self.state();
        state.known = scan(&self.root);
        state.source = std::fs::read(self.root.join(SOURCE)).ok();
        let newer = match (state.known.get(SOURCE), state.known.get(DECK)) {
            (Some(source), Some(deck)) => source.modified > deck.modified,
            (Some(_), None) => true,
            _ => false,
        };
        if let (true, Some(source)) = (newer, state.source.clone()) {
            self.compile(&mut state, &source);
        }
    }

    /// Where the bundle stands, as JSON: how many changes it has had, its folder's name, whether
    /// it keeps its deck's source as `deck.scn`, and why that does not compile, while it does not.
    pub fn status(&self) -> serde_json::Value {
        let state = self.state();
        let name = self.root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        json!({ "version": state.version, "name": name, "source": state.source.is_some(), "failed": state.failed })
    }

    /// A page's write of `bytes` at `rel`, by `by` (its `X-Scaena-Client`): written whole or not
    /// at all, and known at once, so it is announced as that page's.
    pub fn write(&self, rel: &str, bytes: &[u8], by: Option<String>) -> Result<(), String> {
        let mut state = self.state();
        let path = self.writable(rel)?;
        write_whole(&path, bytes).map_err(|e| format!("{rel}: {e}"))?;
        let stamp = stamp(&path).ok_or_else(|| format!("{rel}: written, but cannot be read back"))?;
        state.known.insert(rel.to_string(), stamp);
        if rel == SOURCE {
            state.source = Some(bytes.to_vec());
        }
        self.touched(&mut state, rel, by);
        Ok(())
    }

    /// A page's removal of the file at `rel`, by `by`: the name in the folder, so a link goes and
    /// what it leads to stays.
    pub fn remove(&self, rel: &str, by: Option<String>) -> Result<(), String> {
        let mut state = self.state();
        let path = self.root.join(rel);
        let inside = path.parent().is_some_and(|dir| self.inside(dir));
        if !inside || !path.symlink_metadata().is_ok_and(|m| !m.is_dir()) {
            return Err(format!("{rel}: no such file in the bundle"));
        }
        std::fs::remove_file(&path).map_err(|e| format!("{rel}: {e}"))?;
        state.known.remove(rel);
        if rel == SOURCE {
            state.source = None;
        }
        self.touched(&mut state, rel, by);
        Ok(())
    }

    /// The file at `rel` inside the folder, which must be there and be the folder's own: no
    /// link out of it.
    pub fn existing(&self, rel: &str) -> Result<PathBuf, String> {
        let path = self.root.join(rel).canonicalize().map_err(|_| format!("{rel}: no such file in the bundle"))?;
        if !path.starts_with(&self.root) || !path.is_file() {
            return Err(format!("{rel}: no such file in the bundle"));
        }
        Ok(path)
    }

    /// Where a write of `rel` goes: inside the folder, its directories made. None is made
    /// through a link out of the folder: the nearest that is there must be the folder's own.
    fn writable(&self, rel: &str) -> Result<PathBuf, String> {
        let refused = || format!("{rel}: not a file in the bundle");
        let path = self.root.join(rel);
        let dir = path.parent().ok_or_else(refused)?;
        let there = dir.ancestors().find(|d| d.symlink_metadata().is_ok()).ok_or_else(refused)?;
        if !self.inside(there) {
            return Err(refused());
        }
        std::fs::create_dir_all(dir).map_err(|e| format!("{rel}: {e}"))?;
        if !self.inside(dir) || path.is_dir() {
            return Err(refused());
        }
        Ok(path)
    }

    /// Whether `dir`, wherever its links lead, is the folder or inside it.
    fn inside(&self, dir: &Path) -> bool {
        dir.canonicalize().is_ok_and(|dir| dir.starts_with(&self.root))
    }

    fn touched(&self, state: &mut State, rel: &str, by: Option<String>) {
        let pending =
            state.pending.get_or_insert_with(|| Pending { by: None, paths: BTreeSet::new(), at: Instant::now() });
        pending.by = by;
        pending.paths.insert(rel.to_string());
        pending.at = Instant::now();
    }

    /// One scan: a page's writes that have stopped, announced; then what changed on disk, once
    /// the folder settles, with `deck.scn` compiled first.
    fn tick(&self) {
        let mut state = self.state();
        if state.pending.as_ref().is_some_and(|p| p.at.elapsed() >= SETTLE) {
            let pending = state.pending.take().expect("pending");
            self.announce(&mut state, pending.by, pending.paths);
        }
        let now = scan(&self.root);
        if now == state.known {
            state.unsettled = None;
            return;
        }
        // A text editor may write a file in steps: wait for two scans that agree.
        if state.unsettled.as_ref() != Some(&now) {
            state.unsettled = Some(now);
            return;
        }
        state.unsettled = None;
        let mut paths: BTreeSet<String> = (state.known.keys().chain(now.keys()))
            .filter(|path| state.known.get(*path) != now.get(*path))
            .cloned()
            .collect();
        state.known = now;
        if paths.contains(SOURCE) {
            let source = std::fs::read(self.root.join(SOURCE)).ok();
            if source == state.source {
                paths.remove(SOURCE);
            } else {
                state.source = source.clone();
                match source {
                    Some(source) => match self.compile(&mut state, &source) {
                        Some(true) => {
                            paths.insert(DECK.to_string());
                        }
                        Some(false) => {}
                        // What failed is announced as such; the deck is as it was.
                        None => {
                            paths.remove(SOURCE);
                        }
                    },
                    None => state.failed = None,
                }
            }
        }
        if !paths.is_empty() {
            self.announce(&mut state, None, paths);
        }
    }

    /// Compile `source` into `deck.json` (SPEC §4), as `scaena compile` does: written only when
    /// it compiles to a valid deck that differs from the one there. `Some(written)`, or `None`
    /// when it does not compile, which is announced.
    fn compile(&self, state: &mut State, source: &[u8]) -> Option<bool> {
        let started = Instant::now();
        let text = String::from_utf8_lossy(source).into_owned();
        let deck = match compiled(&self.root, &text) {
            Ok(deck) => deck,
            Err(problems) => return self.failed(state, Failure { source: text, problems }),
        };
        let path = self.root.join(DECK);
        let written = std::fs::read(&path).ok().as_deref() != Some(deck.as_bytes());
        if written {
            if let Err(e) = write_whole(&path, deck.as_bytes()) {
                let problems = vec![Problem::said(format!("{DECK}: {e}"))];
                return self.failed(state, Failure { source: text, problems });
            }
            if let Some(stamp) = stamp(&path) {
                state.known.insert(DECK.to_string(), stamp);
            }
        }
        state.failed = None;
        let ms = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        let _ = self.notes.send(Note::Compiled { ms, written });
        Some(written)
    }

    /// `deck.scn` did not become the deck, for `failure`: told to every page and to whoever
    /// runs the server, and kept until it does.
    fn failed(&self, state: &mut State, failure: Failure) -> Option<bool> {
        state.failed = Some(failure.clone());
        let data = json!({ "version": state.version, "failed": failure });
        let _ = self.events.send(crate::event("failed", &data));
        let _ = self.notes.send(Note::Failed(failure));
        None
    }

    /// Tell every page, and whoever runs the server, that `paths` changed, by `by`.
    fn announce(&self, state: &mut State, by: Option<String>, paths: BTreeSet<String>) {
        state.version += 1;
        let data = json!({ "version": state.version, "by": by, "paths": paths, "failed": state.failed });
        let _ = self.events.send(crate::event("changed", &data));
        let _ = self.notes.send(Note::Changed { paths: paths.into_iter().collect(), by });
    }
}

/// `source` compiled in the bundle at `root` (SPEC §4): the deck as canonical JSON, or what
/// the compiler found, each at the source it is about.
fn compiled(root: &Path, source: &str) -> Result<String, Vec<Problem>> {
    let compiled = scaena_ops::compile::compile(source, &Files::Dir(root.to_path_buf())).map_err(|e| {
        vec![Problem {
            line: Some(e.line),
            col: Some(e.col),
            span: Some((e.offset, e.len)),
            path: e.pointer.clone(),
            ..Problem::said(e.message)
        }]
    })?;
    if !compiled.findings.is_empty() {
        return Err(compiled.findings.iter().map(|f| found(source, &compiled, f)).collect());
    }
    let deck = Deck::from_json(&compiled.json.to_string()).map_err(|e| vec![Problem::said(e.to_string())])?;
    Ok(deck.to_json().map_err(|e| vec![Problem::said(e.to_string())])? + "\n")
}

/// A finding `validate` made (SPEC §7.4), at the source that wrote that part of the deck.
fn found(source: &str, compiled: &Compiled, f: &scaena_core::Finding) -> Problem {
    let span = compiled.span(f);
    let (line, col) = span.map(|(offset, _)| line_col(source, offset)).unzip();
    Problem {
        code: Some(f.code.clone()),
        message: f.message.clone(),
        line,
        col,
        span,
        path: f.path.clone(),
        file: f.file.clone(),
        hint: f.hint.clone(),
    }
}

/// Every file under `root` but those whose names start with a dot, by its path inside it.
fn scan(root: &Path) -> Scan {
    let mut out = Scan::new();
    walk(root, "", &mut out);
    out
}

fn walk(dir: &Path, prefix: &str, out: &mut Scan) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        // Not through a link: the folder's own files only.
        let Ok(meta) = entry.metadata() else { continue };
        let rel = format!("{prefix}{name}");
        if meta.is_dir() {
            walk(&entry.path(), &format!("{rel}/"), out);
        } else if meta.is_file() {
            out.insert(rel, Stamp { len: meta.len(), modified: meta.modified().ok() });
        }
    }
}

fn stamp(path: &Path) -> Option<Stamp> {
    let meta = std::fs::metadata(path).ok()?;
    Some(Stamp { len: meta.len(), modified: meta.modified().ok() })
}

/// Write `bytes` at `path` whole: to a file beside it whose name starts with a dot, which a
/// scan passes over, then renamed into place. That file is made new, so nothing left in its
/// place, a link included, can take the write elsewhere.
fn write_whole(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let partial = path.with_file_name(format!(".{name}.scaena-serve"));
    let _ = std::fs::remove_file(&partial);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&partial)
        .and_then(|mut file| std::io::Write::write_all(&mut file, bytes))
        .and_then(|()| std::fs::rename(&partial, path))
        .inspect_err(|_| {
            let _ = std::fs::remove_file(&partial);
        })
}

//! The bundle a page edits, in memory (PLAN 2.4): every file it was opened with, by its
//! path inside the bundle. It saves as `scaena save` does (SPEC §3.1): fonts subset to what
//! the deck draws, or kept whole, files named by their content, a manifest. The page writes
//! what a save makes where it keeps the bundle (a folder it was given, or the browser's own
//! storage) and goes on from the saved bundle, or zips it.
//!
//! A bundle that keeps a history (SPEC §8) has the save recorded in it (PLAN 2.9), by the
//! history's own module (`scaena-history`), which the page hands the save: the engine's
//! module keeps no CRDT (SPEC §15). The session keeps what that module records: each edit an
//! operation made, by whoever called it ([`Caller`]), with what it says it did.

use crate::assistant::Caller;
use crate::{Error, Session};
use scaena_core::{Deck, Severity};
use scaena_ops::OpsError;
use scaena_ops::create::{Create, creating};
use scaena_ops::export::Progress;
use scaena_ops::lint::Why;
use scaena_store::crdt::{FS, OUTSIDE, Recorded};
use scaena_store::subset::SubsetError;
use scaena_store::{Bundle, Files, HISTORY, SaveOptions, Saving, StoreError};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

/// What records a save in a bundle's history: given the history the bundle holds and the
/// changes to record (JSON, as `scaena-history` takes them), the history to save in its place.
pub type Recorder<'a> = dyn Fn(&[u8], &str) -> Result<Vec<u8>, String> + 'a;

/// Who makes the page's own edits: what the user types, a finding's fix they click, and the
/// save (SPEC §8.2).
const USER: &str = "user";
/// What a run of typing on the canvas is called in the bundle's history (PLAN 2.32).
pub(crate) const TYPED: &str = "type";

/// Seconds since 1970 at `rfc3339`, as a page says the time (`Date.toISOString`, in UTC);
/// `None` when it says it otherwise.
pub fn seconds(rfc3339: &str) -> Option<i64> {
    let utc = rfc3339.strip_suffix('Z')?;
    let whole = utc.split_once('.').map_or(utc, |(whole, _)| whole);
    scaena_core::format::read_iso(whole).ok().map(|t| t.0)
}

impl Session {
    /// A bundle's files, by their paths inside it, opened: its deck from `deck.json`, its
    /// theme from the file the deck names, and every other file handed over.
    pub fn open(mut files: BTreeMap<String, Vec<u8>>) -> Result<Session, Error> {
        let mut text = |path: &str| {
            let bytes = files.remove(path).ok_or_else(|| Error::Missing(path.to_string()))?;
            String::from_utf8(bytes).map_err(|e| Error::Deck(format!("{path}: {e}")))
        };
        let deck = text("deck.json")?;
        let theme = match Deck::from_json(&deck).map_err(|e| Error::Deck(e.to_string()))?.theme {
            Some(serde_json::Value::String(path)) => text(&path)?,
            Some(inline) => inline.to_string(),
            None => return Err(Error::Deck("the deck names no theme".into())),
        };
        let mut session = Session::new(&deck, &theme)?;
        for (path, bytes) in files {
            session.add_file(&path, bytes);
        }
        Ok(session)
    }

    /// A new bundle (PLAN 2.12), as `deck_create` makes one (SPEC §7.2), opened: the theme file
    /// named `file` (`dusk.theme.json`), whose text is `theme`; each font its families name,
    /// from `fonts` by the path it gives it; and one state with nothing on it, titled `title`, on
    /// a 1920 × 1080 canvas. A theme that makes no valid deck says why.
    pub fn create(file: &str, theme: &str, fonts: &BTreeMap<String, Vec<u8>>, title: &str) -> Result<Session, Error> {
        let req = Create { theme: file.into(), title: Some(title.into()), ..Create::default() };
        let font = |path: &str| -> Result<Vec<u8>, OpsError> {
            fonts
                .get(path)
                .cloned()
                .ok_or_else(|| OpsError::new(format!("the theme's font `{path}` was not handed over")))
        };
        let data = |path: &Path| -> Result<Vec<u8>, OpsError> {
            Err(OpsError::new(format!("{}: a new deck here attaches no data", path.display())))
        };
        let (created, made) = creating(&req, theme.into(), &font, &data).map_err(|e| Error::Ops(e.to_string()))?;
        let Some(made) = made else {
            let errors = created.findings.iter().filter(|f| f.severity == Severity::Error);
            let said: Vec<String> = errors.map(|f| format!("{} {}", f.code, f.message)).collect();
            return Err(Error::Deck(format!("{file} makes no valid deck: {}", said.join("; "))));
        };
        let mut files = made.files;
        let deck = made.deck.to_json().map_err(|e| Error::Deck(e.to_string()))?;
        files.insert("deck.json".into(), (deck + "\n").into_bytes());
        Session::open(files)
    }

    /// A `.scaena` zip's bytes, opened.
    pub fn from_zip(bytes: &[u8]) -> Result<Session, Error> {
        let bundle = Bundle::from_zip(bytes).map_err(|e| Error::Deck(e.to_string()))?;
        let Files::Zip(files) = bundle.files else { unreachable!("a zip opens in memory") };
        Session::open(Arc::unwrap_or_clone(files))
    }

    /// The bundle as it stands: the deck shown, in the files handed over.
    pub(crate) fn bundle(&self) -> Bundle {
        Bundle {
            root: "deck.scaena".into(),
            deck_file: "deck.json".into(),
            deck: self.deck.clone(),
            theme_json: Some(self.theme_json.clone()),
            files: Files::Zip(Arc::clone(&self.files)),
            author: "user".into(),
        }
    }

    /// What a save that subsets needs subset: the characters the deck can draw, as a
    /// string, and each font file to keep them of, by its path.
    pub fn subsetting(&self) -> Result<(String, Vec<String>), Error> {
        let (chars, fonts) = self.bundle().subsetting().map_err(|e| Error::Ops(e.to_string()))?;
        Ok((chars.into_iter().collect(), fonts))
    }

    /// `font` subset to `chars`, as [`Session::subsetting`] gave them, by the page's
    /// subsetter (`scaena-subset`): what the next save that subsets writes for it.
    pub fn add_subset(&mut self, font: &str, chars: &str, bytes: Vec<u8>) {
        self.subsets.insert(font.to_string(), (chars.to_string(), bytes));
    }

    /// The bundle with the deck shown, saved as SPEC §3.1 lays one out, at `now` (RFC 3339;
    /// the engine reads no clock), with fonts subset to what the deck can draw if `subset`.
    /// The session keeps the bundle as it was until it [adopts](Session::adopt) the save.
    ///
    /// The engine's module carries no subsetter, and no CRDT: each would add a quarter of
    /// the module or more (SPEC §15). A save that subsets writes the subsets handed over
    /// ([`Session::add_subset`]), each for the characters the deck can draw now. A bundle that
    /// keeps a history (SPEC §8) has the save recorded in it by `history`, the history's own
    /// module, as [`Session::changes`] says. Without it, the history is carried as it is: the
    /// next save that records, `scaena save` or any command that writes the deck, takes in
    /// the page's edits as a change by `fs`.
    pub fn save(&self, now: &str, subset: bool, history: Option<&Recorder>) -> Result<Saving, Error> {
        let opts = SaveOptions { subset_fonts: subset, now: now.into(), history: false };
        let record = |saved: &Deck| match (history, self.files.get(HISTORY)) {
            (Some(record), Some(held)) => {
                let changes = self.changes(saved, seconds(now)).map_err(|e| StoreError::History(e.to_string()))?;
                record(held, &changes).map(Some).map_err(StoreError::History)
            }
            _ => Ok(None),
        };
        let subsets = |font: &str, _: &[u8], chars: &BTreeSet<char>| self.subset(font, chars);
        let mut saving = self.bundle().saving_with(&opts, record, subsets).map_err(|e| Error::Ops(e.to_string()))?;
        // What the Files panel took out goes where the bundle is kept too (PLAN 2.59).
        saving.replaced.extend(self.removed.iter().cloned());
        Ok(saving)
    }

    /// `font`'s subset for `chars`, as the page handed it over ([`Session::add_subset`]).
    fn subset(&self, font: &str, chars: &BTreeSet<char>) -> Result<Vec<u8>, StoreError> {
        match self.subsets.get(font) {
            Some((kept, bytes)) if kept.chars().eq(chars.iter().copied()) => Ok(bytes.clone()),
            kept => {
                let why = if kept.is_some() {
                    "its subset keeps other characters"
                } else {
                    "no subset of it was handed over"
                };
                Err(StoreError::Subset(font.to_string(), SubsetError::Subset(format!("{why}: subset it again"))))
            }
        }
    }

    /// The deck's pages laid out for its PDF (PLAN 2.54): each at rest on the deck's canvas, as
    /// `scaena export --format pdf` orders them, with the fonts and images the frames name, as
    /// bytes the PDF's own module (`scaena-pdf`) draws. This module carries no PDF writer: only
    /// an export draws one (SPEC §15).
    pub fn pdf_laid_out(&self) -> Result<Vec<u8>, Error> {
        let (laid, _) = scaena_ops::export::pdf_laid_out(&self.bundle(), None, &Progress::default())
            .map_err(|e| Error::Ops(e.to_string()))?;
        laid.to_bytes().map_err(|e| Error::Ops(e.to_string()))
    }

    /// The deck as one HTML file that plays offline (PLAN 2.54): `page`, the single-file player's
    /// page, filled in with the bundle as `scaena export --format html` fills it, named `name`:
    /// the bundle as a save that subsets writes it, its fonts the subsets handed over
    /// ([`Session::subsetting`]).
    pub fn standalone(&self, page: &str, name: &str) -> Result<String, Error> {
        let saved = || self.save("", true, None).map(|saving| saving.files).map_err(|e| OpsError::new(e.to_string()));
        let (html, _) =
            scaena_ops::export::standalone_with(&self.bundle(), None, page, name, &Progress::default(), saved)
                .map_err(|e| Error::Ops(e.to_string()))?;
        Ok(html)
    }

    /// What a save of `saved`, the deck as saved, records in the bundle's history, at `at`
    /// (PLAN 2.9): as JSON, the changes `scaena-history` records, in order.
    /// - The bundle as it was opened, by `fs`: `deck.json` and the data files it names, each
    ///   a change only if it says otherwise than the history, edited outside Scaena since it
    ///   was recorded (SPEC §8.1, ADR-0014).
    /// - Each edit an operation made since the bundle was opened or saved, after the deck as
    ///   it stood before it, which holds the user's edits until then
    ///   ([`Session::keep`]), with the data files it wrote.
    /// - `saved`, by the user: the rest of their edits, the files the save renamed, and the
    ///   data files it names as they are, a file dropped on the page among them.
    ///
    /// Each is stamped when it was made, the first as the earliest: the history never
    /// stamps a change before the one it follows.
    pub fn changes(&self, saved: &Deck, at: Option<i64>) -> Result<String, Error> {
        let held = self.files.get("deck.json").ok_or_else(|| Error::Missing("deck.json".into()))?;
        let held = String::from_utf8(held.clone()).map_err(|e| Error::Deck(e.to_string()))?;
        let opened = Deck::from_json(&held).map_err(|e| Error::Deck(e.to_string()))?;
        let first = self.recorded.first().map_or(at, |c| c.timestamp);
        // Each data file as it was before an edit here wrote it.
        let files = data_texts(&opened, |path| self.held.get(path).or_else(|| self.files.get(path)));
        let mut changes = vec![Recorded { message: Some(OUTSIDE.into()), files, ..change(held, FS, first) }];
        changes.extend(self.recorded.iter().cloned());
        let files = data_texts(saved, |path| self.files.get(path));
        let saved = saved.to_json().map_err(|e| Error::Deck(e.to_string()))?;
        changes.push(Recorded { message: Some("save".into()), files, ..change(saved, USER, at) });
        serde_json::to_string(&changes).map_err(|e| Error::Deck(e.to_string()))
    }

    /// Keep `deck`, which an operation `by` called wrote for `why`, and `files`, the data files it
    /// wrote, as their text, for the next save to record, after the deck shown before it: the
    /// user's edits until then. Only a bundle that keeps a history records anything.
    pub(crate) fn keep(
        &mut self,
        deck: &Deck,
        files: &BTreeMap<String, String>,
        why: &Why,
        by: Caller,
    ) -> Result<(), Error> {
        if !self.files.contains_key(HISTORY) {
            return Ok(());
        }
        let json = |deck: &Deck| deck.to_json().map_err(|e| Error::Deck(e.to_string()));
        let before = json(&self.deck)?;
        // A run of typing on the canvas is one change: each keystroke takes the place of the
        // one before it, which nothing has changed since.
        let typing = why.message == TYPED
            && (self.recorded.last()).is_some_and(|last| {
                last.deck == before && last.author == by.author && last.message.as_deref() == Some(TYPED)
            });
        if typing {
            self.recorded.pop();
        } else if self.recorded.last().is_none_or(|last| last.deck != before) {
            self.recorded.push(Recorded { message: Some("edit".into()), ..change(before, USER, by.at) });
        }
        self.recorded.push(Recorded {
            files: files.clone(),
            message: Some(why.message.clone()),
            renamed_nodes: why.renamed_nodes.clone(),
            renamed_states: why.renamed_states.clone(),
            ..change(json(deck)?, by.author, by.at)
        });
        Ok(())
    }

    /// Go on from `saved`: its files are the bundle's from now on, and its deck, which names
    /// files by their content, is shown. What a page does once it has written a save where
    /// it keeps the bundle. The editor's source is the saved deck's from now on: a page
    /// takes [`Session::source`] again.
    pub fn adopt(&mut self, saved: &Saving) -> Result<(), Error> {
        // The theme names the fonts by their new names too.
        let mut next = Session::open(saved.files.clone())?;
        next.set_format(self.format.as_deref())?;
        if self.edit.is_some() {
            let source = next.source();
            next.compile(&source);
        }
        // A save writes each data file as it is, under its own name: the Data panel's undo
        // goes on undoing what it did.
        next.done = std::mem::take(&mut self.done);
        next.undone = std::mem::take(&mut self.undone);
        *self = next;
        Ok(())
    }
}

/// The data files `deck`'s sources name, by their paths, as text: each as `bytes` gives it, where
/// it does (ADR-0014).
fn data_texts<'a>(deck: &Deck, bytes: impl Fn(&str) -> Option<&'a Vec<u8>>) -> BTreeMap<String, String> {
    let paths = deck.data.values().filter_map(|source| source.source.as_str());
    paths.filter_map(|path| Some((path.to_string(), String::from_utf8_lossy(bytes(path)?).into_owned()))).collect()
}

/// `deck`'s change by `author`, at `at`, saying nothing yet.
fn change(deck: String, author: &str, at: Option<i64>) -> Recorded {
    Recorded {
        deck,
        files: Default::default(),
        author: author.into(),
        message: None,
        timestamp: at,
        renamed_nodes: Vec::new(),
        renamed_states: Vec::new(),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use scaena_store::crdt::DeckDoc;
    use serde_json::json;
    use std::path::Path;

    /// A new deck from each theme that ships (PLAN 2.12), as the editor's New makes one: it
    /// opens with its title, its theme, and the fonts the theme names; its source compiles, and
    /// it lints with no error, laid out. A font not handed over is named.
    #[test]
    fn a_new_deck_from_each_theme_that_ships_lints_with_no_error() {
        let examples = Path::new("../../docs/examples");
        let fonts: BTreeMap<String, Vec<u8>> = [
            "Fraunces-VF.ttf",
            "Inter-VF.ttf",
            "JetBrainsMono-VF.ttf",
            "Fraunces-Italic-VF.ttf",
            "Inter-Italic-VF.ttf",
            "JetBrainsMono-Italic-VF.ttf",
        ]
        .iter()
        .map(|f| (format!("fonts/{f}"), std::fs::read(examples.join("fonts").join(f)).unwrap()))
        .collect();
        for theme in ["themes/dusk.theme.json", "authorability/themes/daybreak.theme.json", "themes/ember.theme.json"] {
            let text = std::fs::read_to_string(examples.join(theme)).unwrap();
            let file = Path::new(theme).file_name().unwrap().to_str().unwrap();
            let mut made = Session::create(file, &text, &fonts, "Field notes").unwrap();
            assert!(fonts.keys().all(|f| made.file(f).is_some()), "{file}: its fonts");
            let source = made.source();
            assert!(source.starts_with(&format!("deck \"Field notes\" theme:\"themes/{file}\"")), "{source}");
            assert!(made.compile(&source).valid, "{file}: {source}");
            let linted = made.lint(None).unwrap();
            let errors: Vec<&str> = (linted.findings.iter())
                .filter(|f| f.finding.severity == Severity::Error)
                .map(|f| f.finding.code.as_str())
                .collect();
            assert!(errors.is_empty() && linted.laid, "{file}: {errors:?}");
        }
        let dusk = std::fs::read_to_string(examples.join("themes/dusk.theme.json")).unwrap();
        let lacking = Session::create("dusk.theme.json", &dusk, &BTreeMap::new(), "Field notes").err().unwrap();
        assert!(lacking.to_string().contains("`fonts/Fraunces-VF.ttf` was not handed over"), "{lacking}");
    }

    /// A bundle on disk as a page holds it: every file of it, by its path inside it.
    fn files(path: &str) -> BTreeMap<String, Vec<u8>> {
        let bundle = Bundle::open(Path::new(path)).unwrap();
        bundle.files.list().unwrap().into_iter().map(|p| (p.clone(), bundle.read(&p).unwrap())).collect()
    }

    /// The revenue example's files: its deck, theme, fonts, and data, as a bundle of its own.
    pub(crate) fn revenue() -> BTreeMap<String, Vec<u8>> {
        let bundle = Bundle::open(Path::new("../../docs/examples/revenue.deck.json")).unwrap();
        let mut files: BTreeMap<String, Vec<u8>> =
            bundle.read_fonts().unwrap().into_iter().chain(bundle.read_data().unwrap()).collect();
        files.insert("deck.json".into(), bundle.read("revenue.deck.json").unwrap());
        let theme = bundle.deck.theme.as_ref().and_then(|t| t.as_str()).unwrap();
        files.insert(theme.into(), bundle.read(theme).unwrap());
        files
    }

    const NOW: &str = "2026-10-03T00:00:00Z";

    /// Every state's display list at rest, as JSON, with font ids put back to the names
    /// they had before a save `renamed` them.
    fn drawn(s: &mut Session, renamed: &[(String, String)]) -> Vec<String> {
        let mut out = Vec::new();
        for state in s.states() {
            let mut json = serde_json::to_string(&s.frame(&state, f64::INFINITY).unwrap()).unwrap();
            for (old, new) in renamed {
                json = json.replace(new.as_str(), old.as_str());
            }
            out.push(json);
        }
        out
    }

    /// Each font a save subsets, subset as the page's subsetter (`scaena-subset`) does, and
    /// handed over.
    fn subset_all(s: &mut Session) {
        let (chars, fonts) = s.subsetting().unwrap();
        for font in fonts {
            let bytes = scaena_store::subset::subset(s.file(&font).unwrap(), &chars.chars().collect()).unwrap();
            s.add_subset(&font, &chars, bytes);
        }
    }

    #[test]
    fn a_page_saves_the_bundle_as_scaena_save_does() {
        let files = files("../../tests/bench/b1.scaena");
        let mut s = Session::open(files.clone()).unwrap();
        assert_eq!(s.files(), files.keys().cloned().collect::<Vec<_>>(), "it holds every file it was handed");
        let unsubset = s.save(NOW, true, None).unwrap_err().to_string();
        assert!(unsubset.contains("no subset of it was handed over"), "{unsubset}");
        subset_all(&mut s);
        let page = s.save(NOW, true, None).unwrap();
        let opts = SaveOptions { subset_fonts: true, now: NOW.into(), history: false };
        assert_eq!(page.files, Bundle::in_memory(files).unwrap().saving(&opts).unwrap().files);
        // Fonts subset (B1's are subset to its text already) and named by their content,
        // licenses carried, and the save opens.
        assert_eq!(page.saved.subset.len(), 4, "{:?}", page.saved.subset);
        assert!(page.files.keys().any(|p| p.starts_with("fonts/Inter-") && p.ends_with(".ttf")));
        assert!(page.files.contains_key("fonts/OFL-Inter.txt"));
        let mut reopened = Session::from_zip(&scaena_store::zip(&page.files).unwrap()).unwrap();
        assert_eq!(reopened.files(), page.files.keys().cloned().collect::<Vec<_>>());
        for state in reopened.states() {
            reopened.frame(&state, f64::INFINITY).unwrap();
        }
        // A subset made for other characters is not a subset of this deck.
        let mut deck = s.deck.clone();
        deck.meta.get_or_insert_with(Default::default).title = Some("Ωmega".into());
        s.set_deck(deck);
        let stale = s.save(NOW, true, None).unwrap_err().to_string();
        assert!(stale.contains("its subset keeps other characters"), "{stale}");
    }

    /// What the editor exports (PLAN 2.54) is what `scaena export` writes for the same bundle:
    /// a state as a PNG at the size asked, the pages its PDF draws, and the single file, its
    /// fonts subset by the page's subsetter.
    #[test]
    fn the_editor_exports_what_scaena_export_writes() {
        let path = "../../tests/bench/b1.scaena";
        let disk = Bundle::open(Path::new(path)).unwrap();
        let mut s = Session::open(files(path)).unwrap();
        let state = s.states()[1].clone();
        let dir = std::env::temp_dir().join(format!("scaena-wasm-png-{}", std::process::id()));
        let req = scaena_ops::export::Request {
            format: "png".into(),
            states: Some(vec![state.clone()]),
            out: Some(dir.clone()),
            size: Some("960x540".into()),
            ..Default::default()
        };
        scaena_ops::export::export(&disk, &req).unwrap();
        let cli = std::fs::read(dir.join(format!("{state}.png"))).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(s.png(&state, 960).unwrap() == cli, "{state}: the PNG differs from the CLI's");
        assert!(s.png("nowhere", 960).is_err());

        let (laid, _) = scaena_ops::export::pdf_laid_out(&disk, None, &Progress::default()).unwrap();
        assert!(s.pdf_laid_out().unwrap() == laid.to_bytes().unwrap(), "the PDF's pages differ from the CLI's");

        let page = r#"<html lang="__SCAENA_LANG__"><title>__SCAENA_TITLE__</title><body><!--__SCAENA_DECK__--></body>"#;
        let unsubset = s.standalone(page, "b1").unwrap_err().to_string();
        assert!(unsubset.contains("no subset of it was handed over"), "{unsubset}");
        subset_all(&mut s);
        let saved = || {
            let opts = SaveOptions { subset_fonts: true, now: String::new(), history: false };
            let subset = |font: &str, bytes: &[u8], chars: &BTreeSet<char>| {
                scaena_store::subset::subset(bytes, chars).map_err(|e| StoreError::Subset(font.to_string(), e))
            };
            Ok(disk.saving_with(&opts, |_| Ok(None), subset)?.files)
        };
        let (cli, _) =
            scaena_ops::export::standalone_with(&disk, None, page, "b1", &Progress::default(), saved).unwrap();
        assert!(s.standalone(page, "b1").unwrap() == cli, "the single file differs from the CLI's");
    }

    #[test]
    fn a_save_writes_the_deck_as_edited_and_the_page_goes_on_from_it() {
        let mut s = Session::open(revenue()).unwrap();
        let source = s.source().replace("Revenue doubled\"", "Revenue more than doubled\"");
        assert!(s.compile(&source).valid);
        let before = drawn(&mut s, &[]);
        let saved = s.save(NOW, false, None).unwrap();
        let reopened = Bundle::from_zip(&scaena_store::zip(&saved.files).unwrap()).unwrap();
        assert!(reopened.deck.to_json().unwrap().contains("Revenue more than doubled"));
        // Kept whole, each font is the bytes it was, under its content's name.
        let fonts = |files: &BTreeMap<String, Vec<u8>>| {
            let mut sizes: Vec<usize> =
                files.iter().filter(|(p, _)| p.ends_with(".ttf")).map(|(_, bytes)| bytes.len()).collect();
            sizes.sort();
            sizes
        };
        assert_eq!(fonts(&saved.files), fonts(&s.files));
        assert!(saved.replaced.contains("fonts/Inter-VF.ttf") && !saved.files.contains_key("fonts/Inter-VF.ttf"));

        s.adopt(&saved).unwrap();
        assert_eq!(s.files(), saved.files.keys().cloned().collect::<Vec<_>>());
        assert!(s.source().contains("Revenue more than doubled") && !s.source().contains("Inter-VF.ttf"));
        assert_eq!(drawn(&mut s, &saved.saved.renamed), before, "a save changes no frame");
        assert!(s.lint(None).is_ok());
    }

    #[test]
    fn a_page_carries_the_history_and_the_next_save_takes_its_edits_in_as_fs() {
        let begun = SaveOptions { subset_fonts: false, now: NOW.into(), history: true };
        let files = Bundle::in_memory(revenue()).unwrap().saving(&begun).unwrap().files;
        let history = files[scaena_store::HISTORY].clone();
        let mut s = Session::open(files).unwrap();
        let source = s.source().replace("Revenue doubled\"", "Revenue more than doubled\"");
        assert!(s.compile(&source).valid);
        let saved = s.save(NOW, false, None).unwrap();
        assert_eq!(saved.files[scaena_store::HISTORY], history, "carried as it was");

        let opts = SaveOptions { subset_fonts: false, now: NOW.into(), history: false };
        let recorded = Bundle::in_memory(saved.files).unwrap().saving(&opts).unwrap();
        let doc = scaena_store::crdt::DeckDoc::load(&recorded.files[scaena_store::HISTORY]).unwrap();
        let authors: Vec<_> = doc.changes().into_iter().filter_map(|c| c.author).collect();
        assert!(authors.iter().any(|a| a == scaena_store::crdt::FS), "{authors:?}");
        assert!(doc.deck().unwrap().to_json().unwrap().contains("Revenue more than doubled"));
    }

    /// What records a save in a bundle's history, as the page's module does (PLAN 2.9).
    fn recorder(held: &[u8], changes: &str) -> Result<Vec<u8>, String> {
        scaena_history::recorded(held, changes)
    }

    /// The revenue example saved with its history begun, and when that was.
    fn begun() -> (BTreeMap<String, Vec<u8>>, i64) {
        let begun = SaveOptions { subset_fonts: false, now: NOW.into(), history: true };
        let files = Bundle::in_memory(revenue()).unwrap().saving(&begun).unwrap().files;
        let at = DeckDoc::load(&files[HISTORY]).unwrap().changes()[0].timestamp;
        (files, at)
    }

    /// `t`, seconds since 1970, as a page says it.
    fn rfc3339(t: i64) -> String {
        let c = scaena_core::format::DateTime(t).civil();
        format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.000Z", c.year, c.month, c.day, c.hour, c.minute, c.second)
    }

    /// Who made each change of `history` after its first `after`, what they said, and when.
    fn said(history: &[u8], after: usize) -> Vec<(String, String, i64)> {
        let changes = DeckDoc::load(history).unwrap().changes().into_iter().skip(after);
        changes.map(|c| (c.author.unwrap_or_default(), c.message.unwrap_or_default(), c.timestamp)).collect()
    }

    fn by(author: &str, message: &str, at: i64) -> (String, String, i64) {
        (author.into(), message.into(), at)
    }

    #[test]
    fn a_page_says_the_time_as_a_history_keeps_it() {
        let t = 1_791_064_457;
        assert_eq!(seconds(&rfc3339(t)), Some(t));
        assert_eq!(seconds("2026-10-03T21:54:17Z"), Some(t));
        assert_eq!(seconds("2026-10-03T21:54:17+02:00"), None);
    }

    #[test]
    fn a_save_records_the_users_edits_and_the_assistants_each_by_its_author() {
        let (files, t0) = begun();
        let mut s = Session::open(files).unwrap();
        // The user types; the assistant renames the title; the page takes its source, and the
        // user types again.
        let typed = s.source().replace("Revenue doubled\"", "Revenue more than doubled\"");
        assert!(s.compile(&typed).valid);
        let agent = Caller { author: "agent:scripted", at: Some(t0 + 60) };
        let patch = json!({ "ops": [{ "op": "rename_node", "id": "title", "to": "heading" }] });
        assert!(s.tool("deck_patch", patch, agent).unwrap().edited);
        let source = s.source();
        assert!(s.compile(&source).valid);
        let typed = s.source().replace("Q3 Review", "Third-quarter review");
        assert!(s.compile(&typed).valid);

        let saved = s.save(&rfc3339(t0 + 120), false, Some(&recorder)).unwrap();
        let history = &saved.files[HISTORY];
        assert_eq!(
            said(history, 1),
            [
                by("user", "edit", t0 + 60),
                by("agent:scripted", "patch: rename_node", t0 + 60),
                by("user", "save", t0 + 120)
            ]
        );
        // The rename kept the node: it changed its id, one operation.
        let doc = DeckDoc::load(history).unwrap();
        assert_eq!(doc.changes()[2].ops, 1);
        // The history holds the deck as saved, so the next command that records takes in no
        // change by `fs`.
        assert_eq!(doc.deck().unwrap().to_json().unwrap() + "\n", String::from_utf8_lossy(&saved.files["deck.json"]));
        let next = Bundle::in_memory(saved.files.clone()).unwrap().history().unwrap().unwrap();
        assert_eq!(next.changes().len(), 4);

        // The page goes on from the save, which recorded what the session kept.
        s.adopt(&saved).unwrap();
        let again = s.save(&rfc3339(t0 + 180), false, Some(&recorder)).unwrap();
        assert_eq!(said(&again.files[HISTORY], 4), []);
    }

    #[test]
    fn a_deck_edited_outside_goes_in_first_by_fs() {
        let (mut files, t0) = begun();
        let held = String::from_utf8(files["deck.json"].clone()).unwrap().replace("Q3 Review", "Q3, reviewed");
        files.insert("deck.json".into(), held.into_bytes());
        let mut s = Session::open(files).unwrap();
        let agent = Caller { author: "agent:scripted", at: Some(t0 + 60) };
        let patch = json!({ "ops": [{ "op": "set_text", "node": "title", "text": "Q3, in review" }] });
        assert!(s.tool("deck_patch", patch, agent).unwrap().edited);
        let saved = s.save(&rfc3339(t0 + 120), true, Some(&recorder));
        assert!(saved.unwrap_err().to_string().contains("no subset of it was handed over"));
        subset_all(&mut s);
        // A download records too: the bytes `scaena save` writes.
        let saved = s.save(&rfc3339(t0 + 120), true, Some(&recorder)).unwrap();
        // Stamped no later than what follows it; the save itself changed only the fonts'
        // names, which the user's save records.
        assert_eq!(
            said(&saved.files[HISTORY], 1),
            [by(FS, OUTSIDE, t0 + 60), by("agent:scripted", "patch: set_text", t0 + 60), by("user", "save", t0 + 120)]
        );
    }

    /// Typing on the canvas (PLAN 2.32): a run of keystrokes is one change, `type`, by the
    /// user, stamped at its last; anything made between two runs ends the first.
    #[test]
    fn a_run_of_typing_on_the_canvas_is_one_change() {
        let (files, t0) = begun();
        let mut s = Session::open(files).unwrap();
        let typed = |s: &mut Session, from: u32, text: &str, at: i64| {
            let ops = json!([{ "op": "replace_text", "node": "title", "state": "revenue", "from": from, "to": from, "text": text }]);
            assert!(s.typed(&ops, Some(at)).unwrap());
        };
        typed(&mut s, 15, "!", t0 + 10);
        typed(&mut s, 16, "!", t0 + 11);
        typed(&mut s, 17, "?", t0 + 12);
        let user = Caller { author: "user", at: Some(t0 + 20) };
        let patch = json!({ "ops": [{ "op": "set_text", "node": "note", "text": "In $M." }] });
        assert!(s.tool("deck_patch", patch, user).unwrap().edited);
        typed(&mut s, 0, "Net ", t0 + 30);
        let saved = s.save(&rfc3339(t0 + 60), false, Some(&recorder)).unwrap();
        assert_eq!(
            said(&saved.files[HISTORY], 1),
            [by("user", TYPED, t0 + 12), by("user", "patch: set_text", t0 + 20), by("user", TYPED, t0 + 30)],
            "the save changes nothing after them"
        );
        let deck = DeckDoc::load(&saved.files[HISTORY]).unwrap().deck().unwrap().to_json().unwrap();
        assert!(deck.contains("Net Revenue doubled!!?"), "{deck}");
    }

    const CSV: &str = "data/q3-revenue.csv";

    /// `edits` of the revenue example's source, `q3`, as a page sends them.
    fn q3(edits: serde_json::Value) -> scaena_ops::data::DataEdit {
        serde_json::from_value(json!({ "source": "q3", "edits": edits })).unwrap()
    }

    /// A cell set in the Data panel (PLAN 2.55): the file written with that one field changed, and
    /// the chart that reads it drawn again; an edit refused writes nothing; the file put back by
    /// the panel's undo and written again by its redo, before a save and after it.
    #[test]
    fn a_cell_set_from_the_page_is_drawn_and_undone() {
        let mut s = Session::open(revenue()).unwrap();
        let original = s.file(CSV).unwrap().to_vec();
        let before = drawn(&mut s, &[]);
        let user = Caller { author: USER, at: None };
        let set = q3(json!([{ "op": "set", "row": 2, "column": "revenue", "value": "4.6" }]));
        let (edited, wrote) = s.data_edit(&set, false, user).unwrap();
        assert!(wrote && edited.edited && edited.file.as_deref() == Some(CSV), "{edited:?}");
        let expected = String::from_utf8_lossy(&original).replace("Enterprise,4.4,38", "Enterprise,4.6,38");
        assert_eq!(String::from_utf8_lossy(s.file(CSV).unwrap()), expected);
        assert_eq!(s.data_sheet("q3").unwrap().0.rows[2][2], "4.6");
        let after = drawn(&mut s, &[]);
        assert_ne!(after, before, "the chart that reads it is drawn again");

        // A second Core in 2025-Q4 makes two marks of one key (E103): refused, nothing written.
        let twice = q3(json!([{ "op": "set", "row": 1, "column": "product", "value": "Core" }]));
        let (refused, wrote) = s.data_edit(&twice, false, user).unwrap();
        assert!(refused.refused && !wrote && refused.added.iter().any(|f| f.code == "E103"), "{refused:?}");
        assert_eq!(String::from_utf8_lossy(s.file(CSV).unwrap()), expected);

        assert_eq!(s.data_undo(false, user).unwrap().as_deref(), Some("q3"));
        assert_eq!(s.file(CSV).unwrap(), original.as_slice());
        assert_eq!(drawn(&mut s, &[]), before, "undone, the chart is drawn as it was");
        assert_eq!(s.data_undo(false, user).unwrap(), None, "nothing more to undo");
        assert_eq!(s.data_undo(true, user).unwrap().as_deref(), Some("q3"));
        assert_eq!(drawn(&mut s, &[]), after);
        assert_eq!(s.data_undo(true, user).unwrap(), None, "nothing more to redo");

        // A save writes the file as it is, and the panel goes on undoing what it did.
        let saved = s.save(NOW, false, None).unwrap();
        assert_eq!(String::from_utf8_lossy(&saved.files[CSV]), expected);
        s.adopt(&saved).unwrap();
        assert_eq!(s.data_undo(false, user).unwrap().as_deref(), Some("q3"));
        assert_eq!(s.file(CSV).unwrap(), original.as_slice());
    }

    /// An editor undoes its own changes, never a file's (SPEC §8.2): the Data panel undoes the
    /// assistant's edit of a file, as the source's undo does its edits of the deck, then the
    /// user's; a file changed by other means, dropped on the page, has nothing left to undo.
    #[test]
    fn the_data_panel_undoes_the_editors_own_edits_and_never_a_files() {
        let mut s = Session::open(revenue()).unwrap();
        let original = s.file(CSV).unwrap().to_vec();
        let user = Caller { author: USER, at: None };
        let agent = Caller { author: "agent:scripted", at: None };
        s.data_edit(&q3(json!([{ "op": "set", "row": 2, "column": "revenue", "value": "4.6" }])), false, user).unwrap();
        let users = s.file(CSV).unwrap().to_vec();
        let edits = json!([{ "op": "set", "row": 0, "column": "customers", "value": 1211 }]);
        let called = s.tool("data_edit", json!({ "source": "q3", "edits": edits }), agent).unwrap();
        assert!(called.edited, "{}", called.result);
        assert_eq!(s.data_undo(false, user).unwrap().as_deref(), Some("q3"));
        assert_eq!(s.file(CSV).unwrap(), users.as_slice(), "the assistant's edit undone");
        assert_eq!(s.data_undo(false, user).unwrap().as_deref(), Some("q3"));
        assert_eq!(s.file(CSV).unwrap(), original.as_slice(), "then the user's");

        s.data_edit(&q3(json!([{ "op": "remove", "row": 11 }])), false, user).unwrap();
        s.add_file(CSV, users.clone());
        let changed = s.data_undo(false, user).unwrap_err().to_string();
        assert!(changed.contains("has changed since"), "{changed}");
        assert_eq!(s.file(CSV).unwrap(), users.as_slice());
        assert_eq!((s.data_undo(false, user).unwrap(), s.data_undo(true, user).unwrap()), (None, None));
    }

    /// A save records each version of a data file as each edit wrote it, by its author (ADR-0014):
    /// the file as it was opened, changed outside Scaena, first, by `fs`; then the user's cell, the
    /// assistant's row, the user's next cell, and its undo. The history ends holding the file as
    /// saved, so the next command that records takes in nothing.
    #[test]
    fn a_save_records_each_version_of_a_data_file_by_its_author() {
        let (mut files, t0) = begun();
        let outside = String::from_utf8(files[CSV].clone()).unwrap().replace("2026-Q3,Core,23.9", "2026-Q3,Core,24.1");
        files.insert(CSV.into(), outside.into_bytes());
        let mut s = Session::open(files).unwrap();
        let user = |at: i64| Caller { author: USER, at: Some(t0 + at) };
        s.data_edit(&q3(json!([{ "op": "set", "row": 2, "column": "revenue", "value": "4.6" }])), false, user(10))
            .unwrap();
        let row = json!({ "op": "add", "values": { "quarter": "2026-Q4", "product": "Core", "revenue": 25.2, "customers": 1600 } });
        let agent = Caller { author: "agent:scripted", at: Some(t0 + 20) };
        let called = s.tool("data_edit", json!({ "source": "q3", "edits": [row] }), agent).unwrap();
        assert!(called.edited, "{}", called.result);
        s.data_edit(&q3(json!([{ "op": "set", "row": 0, "column": "revenue", "value": "18.3" }])), false, user(30))
            .unwrap();
        assert_eq!(s.data_undo(false, user(40)).unwrap().as_deref(), Some("q3"));
        // A dry run writes nothing, and records nothing.
        let dry = json!({ "source": "q3", "edits": [{ "op": "remove", "row": 0 }], "dry_run": true });
        assert!(!s.tool("data_edit", dry, agent).unwrap().edited);

        let saved = s.save(&rfc3339(t0 + 60), false, Some(&recorder)).unwrap();
        let history = &saved.files[HISTORY];
        assert_eq!(
            said(history, 1),
            [
                by(FS, &format!("{CSV} changed outside Scaena"), t0 + 10),
                by("user", "data_edit q3: revenue of row 2", t0 + 10),
                by("agent:scripted", "data_edit q3: a row added", t0 + 20),
                by("user", "data_edit q3: revenue of row 0", t0 + 30),
                by("user", "undo data_edit q3: revenue of row 0", t0 + 40),
            ]
        );
        let doc = DeckDoc::load(history).unwrap();
        assert_eq!(doc.files()[CSV], saved.files[CSV]);
        assert!(String::from_utf8_lossy(&saved.files[CSV]).ends_with("2026-Q4,Core,25.2,1600\n"));
        let next = Bundle::in_memory(saved.files.clone()).unwrap().history().unwrap().unwrap();
        assert_eq!(next.changes().len(), 6, "the file as saved is the history's");
    }

    #[test]
    fn a_bundle_without_a_history_records_nothing() {
        let mut s = Session::open(revenue()).unwrap();
        let agent = Caller { author: "agent:scripted", at: None };
        let patch = json!({ "ops": [{ "op": "set_text", "node": "title", "text": "Q3, in review" }] });
        assert!(s.tool("deck_patch", patch, agent).unwrap().edited);
        assert!(s.recorded.is_empty(), "nothing kept for a history it does not keep");
        let never = |_: &[u8], _: &str| -> Result<Vec<u8>, String> { panic!("nothing to record") };
        let saved = s.save(NOW, false, Some(&never)).unwrap();
        assert!(!saved.files.contains_key(HISTORY));
    }

    #[test]
    fn a_file_dropped_on_the_page_is_drawn_once_the_deck_names_it() {
        let mut s = Session::open(revenue()).unwrap();
        let source = s.source();
        assert!(s.compile(&source).valid);
        s.frame("intro", f64::INFINITY).unwrap();
        let png = std::fs::read("../../tests/fixtures/torture.scaena/assets/test-card.png").unwrap();
        let path = scaena_store::place("Test Card.PNG", &png);
        assert!(path.starts_with("assets/") && path.ends_with(".png") && path.len() == 7 + 64 + 4, "{path}");

        // Named before it is in the bundle, it is a missing file.
        let line = format!("state intro layout:title hold:4s\n  card image \"{path}\" at:in(canvas) alt:\"\" z:-50\n");
        let shown = source.replacen("state intro layout:title hold:4s\n", &line, 1);
        let compiled = s.compile(&shown);
        assert!(!compiled.valid && compiled.findings.iter().any(|f| f.finding.code == "E102"), "{compiled:?}");

        s.add_file(&path, png);
        let compiled = s.compile(&shown);
        assert!(compiled.valid, "{compiled:?}");
        let drawn = s.frame("intro", f64::INFINITY).unwrap();
        let images =
            |dl: &scaena_core::displaylist::DisplayList| serde_json::to_string(dl).unwrap().matches("sha256:").count();
        assert_eq!(images(&drawn), 1);
    }

    /// The Files panel (PLAN 2.59): the bundle's images, fonts, and data and what uses each; a
    /// file nothing names taken out, one step the panel undoes and redoes; one something names
    /// refused, with why; and a save takes out what was taken out where the bundle is kept.
    #[test]
    fn a_file_nothing_names_is_taken_out_and_put_back_and_a_save_takes_it_out() {
        let mut files = revenue();
        files.insert("assets/stray.png".into(), b"not drawn".to_vec());
        let mut s = Session::open(files).unwrap();
        let listed = s.bundle_files().unwrap();
        let stray = listed.iter().find(|f| f.path == "assets/stray.png").unwrap();
        assert!(stray.named.is_empty() && stray.bytes == 9);
        let q3 = listed.iter().find(|f| f.path == "data/q3-revenue.csv").unwrap();
        assert_eq!(q3.used.first().map(|u| u.node.as_str()), Some("rev"));

        let refused = s.remove_file("data/q3-revenue.csv").unwrap_err().to_string();
        assert!(refused.contains("data source q3 names it"), "{refused}");
        assert!(s.remove_file("deck.json").is_err() && s.file("data/q3-revenue.csv").is_some());

        s.remove_file("assets/stray.png").unwrap();
        assert!(s.file("assets/stray.png").is_none());
        assert!(s.bundle_files().unwrap().iter().all(|f| f.path != "assets/stray.png"));
        let user = Caller { author: "user", at: None };
        assert_eq!(s.data_undo(false, user).unwrap().as_deref(), Some("assets/stray.png"));
        assert_eq!(s.file("assets/stray.png"), Some(&b"not drawn"[..]));
        assert_eq!(s.data_undo(true, user).unwrap().as_deref(), Some("assets/stray.png"));
        assert!(s.file("assets/stray.png").is_none());

        let saved = s.save(NOW, false, None).unwrap();
        assert!(!saved.files.contains_key("assets/stray.png") && saved.replaced.contains("assets/stray.png"));
    }
}

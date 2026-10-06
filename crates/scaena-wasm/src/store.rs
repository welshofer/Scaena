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
        let subsets = |font: &str, _: &[u8], chars: &BTreeSet<char>| match self.subsets.get(font) {
            Some((kept, bytes)) if kept.chars().eq(chars.iter().copied()) => Ok(bytes.clone()),
            kept => {
                let why = if kept.is_some() {
                    "its subset keeps other characters"
                } else {
                    "no subset of it was handed over"
                };
                Err(StoreError::Subset(font.to_string(), SubsetError::Subset(format!("{why}: subset it again"))))
            }
        };
        self.bundle().saving_with(&opts, record, subsets).map_err(|e| Error::Ops(e.to_string()))
    }

    /// What a save of `saved`, the deck as saved, records in the bundle's history, at `at`
    /// (PLAN 2.9): as JSON, the changes `scaena-history` records, in order.
    /// - `deck.json` as the bundle holds it, by `fs`: a change only if it says otherwise than
    ///   the history, edited outside Scaena since it was recorded (SPEC §8.1).
    /// - Each edit an operation made since the bundle was opened or saved, after the deck as
    ///   it stood before it, which holds the user's edits until then
    ///   ([`Session::keep`]).
    /// - `saved`, by the user: the rest of their edits, and the files the save renamed.
    ///
    /// Each is stamped when it was made, the first as the earliest: the history never
    /// stamps a change before the one it follows.
    pub fn changes(&self, saved: &Deck, at: Option<i64>) -> Result<String, Error> {
        let held = self.files.get("deck.json").ok_or_else(|| Error::Missing("deck.json".into()))?;
        let held = String::from_utf8(held.clone()).map_err(|e| Error::Deck(e.to_string()))?;
        let first = self.recorded.first().map_or(at, |c| c.timestamp);
        let mut changes = vec![Recorded { message: Some(OUTSIDE.into()), ..change(held, FS, first) }];
        changes.extend(self.recorded.iter().cloned());
        let saved = saved.to_json().map_err(|e| Error::Deck(e.to_string()))?;
        changes.push(Recorded { message: Some("save".into()), ..change(saved, USER, at) });
        serde_json::to_string(&changes).map_err(|e| Error::Deck(e.to_string()))
    }

    /// Keep `deck`, which an operation `by` called wrote for `why`, for the next save to
    /// record, after the deck shown before it: the user's edits until then. Only a bundle
    /// that keeps a history records anything.
    pub(crate) fn keep(&mut self, deck: &Deck, why: &Why, by: Caller) -> Result<(), Error> {
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
        *self = next;
        Ok(())
    }
}

/// `deck`'s change by `author`, at `at`, saying nothing yet.
fn change(deck: String, author: &str, at: Option<i64>) -> Recorded {
    Recorded {
        deck,
        author: author.into(),
        message: None,
        timestamp: at,
        renamed_nodes: Vec::new(),
        renamed_states: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
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
    fn revenue() -> BTreeMap<String, Vec<u8>> {
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
}

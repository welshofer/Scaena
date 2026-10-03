//! The bundle a page edits, in memory (PLAN 2.4): every file it was opened with, by its
//! path inside the bundle. It saves as `scaena save` does (SPEC §3.1): fonts subset to what
//! the deck draws, or kept whole, files named by their content, a manifest. The page writes
//! what a save makes where it keeps the bundle (a folder it was given, or the browser's own
//! storage) and goes on from the saved bundle, or zips it.

use crate::{Error, Session};
use scaena_core::Deck;
use scaena_store::subset::SubsetError;
use scaena_store::{Bundle, Files, SaveOptions, Saving, StoreError};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

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

    /// A `.scaena` zip's bytes, opened.
    pub fn from_zip(bytes: &[u8]) -> Result<Session, Error> {
        let bundle = Bundle::from_zip(bytes).map_err(|e| Error::Deck(e.to_string()))?;
        let Files::Zip(files) = bundle.files else { unreachable!("a zip opens in memory") };
        Session::open(Arc::unwrap_or_clone(files))
    }

    /// The bundle as it stands: the deck shown, in the files handed over.
    fn bundle(&self) -> Bundle {
        Bundle {
            root: "deck.scaena".into(),
            deck_file: "deck.json".into(),
            deck: self.deck.clone(),
            theme_json: Some(self.theme_json.clone()),
            files: Files::Zip(Arc::new(self.files.clone())),
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
    /// ([`Session::add_subset`]), each for the characters the deck can draw now. A bundle's
    /// history (SPEC §8) is carried as it is: the next save that records, `scaena save` or
    /// any command that writes the deck, takes in the page's edits as a change by `fs`.
    pub fn save(&self, now: &str, subset: bool) -> Result<Saving, Error> {
        let opts = SaveOptions { subset_fonts: subset, now: now.into(), history: false };
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
        self.bundle().saving_with(&opts, |_| Ok(None), subsets).map_err(|e| Error::Ops(e.to_string()))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

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
        let unsubset = s.save(NOW, true).unwrap_err().to_string();
        assert!(unsubset.contains("no subset of it was handed over"), "{unsubset}");
        subset_all(&mut s);
        let page = s.save(NOW, true).unwrap();
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
        let stale = s.save(NOW, true).unwrap_err().to_string();
        assert!(stale.contains("its subset keeps other characters"), "{stale}");
    }

    #[test]
    fn a_save_writes_the_deck_as_edited_and_the_page_goes_on_from_it() {
        let mut s = Session::open(revenue()).unwrap();
        let source = s.source().replace("Revenue doubled\"", "Revenue more than doubled\"");
        assert!(s.compile(&source).valid);
        let before = drawn(&mut s, &[]);
        let saved = s.save(NOW, false).unwrap();
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
        let saved = s.save(NOW, false).unwrap();
        assert_eq!(saved.files[scaena_store::HISTORY], history, "carried as it was");

        let opts = SaveOptions { subset_fonts: false, now: NOW.into(), history: false };
        let recorded = Bundle::in_memory(saved.files).unwrap().saving(&opts).unwrap();
        let doc = scaena_store::crdt::DeckDoc::load(&recorded.files[scaena_store::HISTORY]).unwrap();
        let authors: Vec<_> = doc.changes().into_iter().filter_map(|c| c.author).collect();
        assert!(authors.iter().any(|a| a == scaena_store::crdt::FS), "{authors:?}");
        assert!(doc.deck().unwrap().to_json().unwrap().contains("Revenue more than doubled"));
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

//! The bundle's versions in the editor (PLAN 2.60, SPEC §8). The history's own module
//! (`scaena-history`) reads a version, the deck and its data files as they were; here it is
//! drawn read-only by a session of its own, compared as `scaena history --diff` compares, and
//! restored as one change, its data files with it, which the editor's undo takes back whole.

use crate::assistant::Caller;
use crate::{Error, Session};
use scaena_core::Deck;
use scaena_ops::history::{Restored, Version, compare, differing, restored};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A version as the page hands it over: the deck as deck.json's text, and the files it is drawn
/// from, its data files and its theme, by their paths, as their text.
#[derive(Debug, Clone, Deserialize)]
pub struct Held {
    pub deck: String,
    #[serde(default)]
    pub files: BTreeMap<String, String>,
}

/// A file an edit wrote beside the deck, a data file or the theme: its text before and after,
/// none where the bundle did not hold it. The editor's undo writes it back (PLAN 2.60, 2.61).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Rewritten {
    pub path: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// A data file written back as an undo or a redo has it: none to take it out.
#[derive(Debug, Clone, Deserialize)]
pub struct Written {
    pub path: String,
    pub text: Option<String>,
}

fn deck(held: &Held) -> Result<Deck, Error> {
    Deck::from_json(&held.deck).map_err(|e| Error::Deck(e.to_string()))
}

fn bytes(files: &BTreeMap<String, String>) -> BTreeMap<String, Vec<u8>> {
    files.iter().map(|(path, text)| (path.clone(), text.clone().into_bytes())).collect()
}

impl Session {
    /// Show version `held` read-only, in a session of its own: the bundle with its deck and its
    /// data files as they were, the rest as it is now. Its states' ids, in order.
    pub fn view_version(&mut self, held: &Held) -> Result<Vec<String>, Error> {
        let mut files: BTreeMap<String, Vec<u8>> = (*self.files).clone();
        files.insert("deck.json".into(), held.deck.clone().into_bytes());
        files.extend(bytes(&held.files));
        let viewed = Session::open(files)?;
        let states = viewed.deck.states.iter().map(|s| s.id.clone()).collect();
        self.viewing = Some(Box::new(viewed));
        Ok(states)
    }

    /// The version shown's `state` at rest, as a PNG `width` pixels wide. A version that names a
    /// file the bundle no longer holds says so, as one a restore would refuse (E102).
    pub fn version_png(&mut self, state: &str, width: u32) -> Result<Vec<u8>, Error> {
        let viewed = self.viewing.as_mut().ok_or_else(|| Error::Ops("no version is shown".into()))?;
        viewed.png(state, width).map_err(|e| match e {
            Error::Missing(path) => Error::Ops(format!("it names {path}, which the bundle no longer holds")),
            e => e,
        })
    }

    /// What changed from `from` to `to`, or without it to the deck and its data files as they are
    /// now: `{ states, deck, files }`, as `scaena history --diff` says it.
    pub fn compare_versions(&self, from: &Held, to: Option<&Held>) -> Result<serde_json::Value, Error> {
        let earlier = deck(from)?;
        let (later, files) = match to {
            Some(to) => (deck(to)?, bytes(&to.files)),
            None => {
                let named = scaena_store::kept_paths(&self.deck);
                let now = named.iter().filter_map(|path| Some((path.to_string(), self.files.get(*path)?.clone())));
                (self.deck.clone(), now.collect())
            }
        };
        let (states, fields) = compare(&earlier, &later).map_err(|e| Error::Ops(e.to_string()))?;
        let files = differing((&earlier, &bytes(&from.files)), (&later, &files));
        Ok(serde_json::json!({ "states": states, "deck": fields, "files": files }))
    }

    /// Make version `held`, `version` as listed, the deck again, with its data files, as `scaena
    /// history --restore` does, by `by`: refused as a patch is where the deck would not validate
    /// in the bundle as it is. Each data file it writes, before and after, for the editor's undo;
    /// the change is kept for the next save to record. The editor shows the deck's new source.
    pub fn restore_version(
        &mut self,
        held: &Held,
        version: Version,
        by: Caller,
    ) -> Result<(Restored, Vec<Rewritten>), Error> {
        let (said, write) = restored(&self.bundle(), version, deck(held)?, bytes(&held.files))
            .map_err(|e| Error::Ops(e.to_string()))?;
        let Some(w) = write else { return Ok((said, Vec::new())) };
        let rewritten = self.rewritten(&w.files);
        // A file taken out since that the version reads comes back, to stay.
        for path in w.files.keys() {
            self.removed.remove(path);
        }
        // Written as any operation's edit is: kept for the save, each file's bytes before it
        // kept as the bundle's own, and a step the Data panel undoes.
        self.write(Some(w), by)?;
        Ok((said, rewritten))
    }

    /// Each of `files` as it is now and as it would be written, for the editor's undo.
    pub(crate) fn rewritten(&self, files: &BTreeMap<String, Vec<u8>>) -> Vec<Rewritten> {
        let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
        (files.iter())
            .map(|(path, after)| Rewritten {
                path: path.clone(),
                before: self.files.get(path).map(|b| text(b)),
                after: Some(text(after)),
            })
            .collect()
    }

    /// Files written back as an undo or a redo of a restore or a theme edit has them: each set to
    /// its text, or taken out where it has none. The deck is the source's, which the editor
    /// compiles after.
    pub fn write_files(&mut self, files: Vec<Written>) {
        for Written { path, text } in files {
            match text {
                Some(text) => {
                    self.removed.remove(&path);
                    self.add_file(&path, text.into_bytes());
                }
                None => {
                    std::sync::Arc::make_mut(&mut self.files).remove(&path);
                    self.removed.insert(path);
                }
            }
        }
        self.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::revenue;

    const CSV: &str = "data/q3-revenue.csv";

    fn held(s: &Session, title: &str, csv: &str) -> Held {
        let mut deck: serde_json::Value = serde_json::from_str(&s.deck.to_json().unwrap()).unwrap();
        deck["nodes"]["title"]["text"] = title.into();
        Held { deck: deck.to_string(), files: [(CSV.to_string(), csv.to_string())].into() }
    }

    fn version(n: usize) -> Version {
        Version { n, id: format!("0@{n}"), author: None, message: None, at: None, ops: 1 }
    }

    /// A version (PLAN 2.60): shown read-only by a session of its own, compared with the deck
    /// now, restored with its data file as one change, and written back as an undo has it.
    #[test]
    fn a_version_is_shown_compared_restored_and_undone() {
        let mut s = Session::open(revenue()).unwrap();
        let now = String::from_utf8(s.file(CSV).unwrap().to_vec()).unwrap();
        let then = held(&s, "Revenue, as it was", &now.replacen("18.2", "11.1", 1));
        let states = s.view_version(&then).unwrap();
        assert_eq!(states, ["intro", "revenue", "mix", "close"]);
        assert!(s.version_png("revenue", 480).unwrap().starts_with(b"\x89PNG"));
        assert_eq!(s.deck.nodes["title"].props["text"], "Q3 Review", "showing a version changes nothing");

        let changed = s.compare_versions(&then, None).unwrap();
        assert_eq!(changed["states"]["intro"]["changed"]["nodes"]["title"]["change"]["text"], "Q3 Review");
        assert_eq!(changed["files"], serde_json::json!([CSV]));

        let user = Caller { author: "user", at: None };
        let (restored, files) = s.restore_version(&then, version(1), user).unwrap();
        assert!(restored.applied && !restored.refused, "{restored:?}");
        assert_eq!(
            files,
            [Rewritten { path: CSV.into(), before: Some(now.clone()), after: then.files.get(CSV).cloned() }]
        );
        assert_eq!(s.deck.nodes["title"].props["text"], "Revenue, as it was");
        assert_eq!(s.compare_versions(&then, None).unwrap()["files"], serde_json::json!([]));
        // The next save takes the file as the bundle held it before for the bundle's own: the
        // restore wrote it, not something outside Scaena.
        let changes: serde_json::Value =
            serde_json::from_str(&s.changes(&s.deck.clone(), &BTreeMap::new(), None).unwrap()).unwrap();
        assert_eq!(changes[0]["files"][CSV], now.as_str(), "{changes:#}");

        // An undo writes the file back; the deck is the source's, which the editor compiles.
        s.write_files(vec![Written { path: CSV.into(), text: Some(now.clone()) }]);
        assert_eq!(s.file(CSV), Some(now.as_bytes()));

        // A version whose deck names a file the bundle no longer holds is refused.
        let mut gone = held(&s, "Gone", &now);
        let mut deck: serde_json::Value = serde_json::from_str(&gone.deck).unwrap();
        deck["nodes"]["photo"] = serde_json::json!({ "type": "image", "src": "assets/gone.png" });
        gone.deck = deck.to_string();
        let (refused, files) = s.restore_version(&gone, version(2), user).unwrap();
        assert!(refused.refused && !refused.applied && files.is_empty(), "{refused:?}");
        assert!(refused.added.iter().any(|f| f.code == "E102"));
        assert_eq!(s.deck.nodes["title"].props["text"], "Revenue, as it was");
    }
}

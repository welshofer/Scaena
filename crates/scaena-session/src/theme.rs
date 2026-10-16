//! The theme, edited from the editor (PLAN 2.61, ADR-0016): RFC 6902 operations on the theme the
//! deck names, as `scaena theme --edit` makes them, written into the session. The source does not
//! change, so the editor keeps each edit in its source's history as the theme's text before and
//! after, which its undo writes back (`Session::write_files`).
//!
//! And the layout library of a theme that ships, brought into an older copy of it as a deck opens
//! (PLAN 3.31).

use crate::assistant::Caller;
use crate::versions::Rewritten;
use crate::{Error, Session};
use scaena_ops::lint::{Why, Write};
use scaena_ops::theme::{Library, ThemeEdit, ThemeEdited, theme_editing};
use serde_json::Value;

impl Session {
    /// Edit the theme the deck names by `edit`, as `by`: refused, as `scaena theme --edit` refuses
    /// one, where the deck would not validate in the theme it leaves. Unless refused or `dry_run`,
    /// the theme is written into the session and frames are drawn in it from now on; the change
    /// is kept for the next save to record. What it did, and the theme file it wrote, before and
    /// after, for the editor's undo: none for an inline theme, which the deck's source carries.
    pub fn theme_edit(
        &mut self,
        edit: &ThemeEdit,
        dry_run: bool,
        by: Caller,
    ) -> Result<(ThemeEdited, Vec<Rewritten>), Error> {
        let (mut edited, write) = theme_editing(&self.bundle(), edit).map_err(|e| Error::Ops(e.to_string()))?;
        edited.applied &= !dry_run;
        let Some(w) = write.filter(|_| !dry_run) else { return Ok((edited, Vec::new())) };
        let rewritten = self.rewritten(&w.files);
        self.write(Some(w), by)?;
        Ok((edited, rewritten))
    }

    /// The layout library of `shipped`, the JSON of a theme that ships, brought into the theme the
    /// deck names where that is an older copy of it (PLAN 3.31, SPEC §3.6): what an editor does as
    /// it opens a deck, so Add Slide offers every layout the theme ships, by section, each drawn as
    /// it is used. Written into the session as a theme edit by `by` is, for the next save to keep
    /// and record, and frames are drawn in it from now on, every slide as it was
    /// (`scaena_ops::theme::library`). What it brought; none where the deck sets its theme inline,
    /// or names another theme, or one with the library already.
    pub fn library(&mut self, shipped: &str, by: Caller) -> Result<Option<Library>, Error> {
        let Some(path) = self.deck().theme.as_ref().and_then(Value::as_str).map(str::to_string) else {
            return Ok(None);
        };
        let Some(bytes) = self.file(&path) else { return Ok(None) };
        let theme: Value = serde_json::from_slice(bytes).map_err(|e| Error::Ops(format!("{path}: {e}")))?;
        let shipped: Value =
            serde_json::from_str(shipped).map_err(|e| Error::Ops(format!("the theme that ships: {e}")))?;
        let Some((theme, brought)) = scaena_ops::theme::library(&theme, &shipped, self.shows_code()) else {
            return Ok(None);
        };
        let text = serde_json::to_string_pretty(&theme).map_err(|e| Error::Ops(e.to_string()))? + "\n";
        // Frames are drawn in it from now on: it reads as a theme, or nothing is written.
        scaena_engine::theme::Theme::from_json(&text).map_err(|e| Error::Ops(format!("{path}: {e}")))?;
        let mut w = Write::new(self.deck().clone(), Why::new(format!("library: the layouts {} ships", brought.theme)));
        w.files.insert(path, text.into_bytes());
        self.write(Some(w), by)?;
        Ok(Some(brought))
    }
}

impl Session {
    /// Whether a state of the deck shows a code text (ADR-0029), the deck's overrides over it: one
    /// the look a theme brings for code would draw otherwise. A deck whose states do not resolve
    /// is taken to show one.
    fn shows_code(&self) -> bool {
        let deck = self.deck();
        let code =
            |id: &str, props: &scaena_core::document::Props| match deck.overrides.get(id).and_then(|o| o.get("code")) {
                Some(code) => code.is_object(),
                None => props.get("code").is_some_and(Value::is_object),
            };
        let Ok(snapshots) = scaena_core::resolve_states(deck) else { return true };
        snapshots.iter().any(|s| {
            s.nodes.iter().any(|(id, props)| {
                deck.nodes.get(id).is_some_and(|n| n.node_type == scaena_core::document::NodeType::Text)
                    && code(id, props)
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::tests::revenue;
    use serde_json::{Value, json};

    const THEME: &str = "themes/dusk.theme.json";

    fn ops(ops: Value) -> ThemeEdit {
        ThemeEdit { ops: serde_json::from_value(ops).unwrap(), photo: None }
    }

    fn user() -> Caller<'static> {
        Caller { author: "user", at: Some(1_790_000_000) }
    }

    fn theme(s: &Session) -> Value {
        serde_json::from_slice(s.file(THEME).unwrap()).unwrap()
    }

    /// An edit of the paper is drawn, kept for the save with the theme's text, and undone by
    /// writing the theme back; a name the deck uses taken out is refused and changes nothing.
    #[test]
    fn a_theme_edit_is_drawn_kept_and_undone() {
        let mut s = Session::open(revenue()).unwrap();
        let before = s.png("revenue", 480).unwrap();
        let was = String::from_utf8(s.file(THEME).unwrap().to_vec()).unwrap();
        let paper = ops(json!([{ "op": "replace", "path": "/tokens/color/paper", "value": "#0B0B10" }]));

        // A dry run says what it would do, and writes nothing.
        let (said, files) = s.theme_edit(&paper, true, user()).unwrap();
        assert!(!said.applied && !said.refused && files.is_empty(), "{said:?}");
        assert_eq!(s.file(THEME).unwrap(), was.as_bytes());

        let (said, files) = s.theme_edit(&paper, false, user()).unwrap();
        assert!(said.applied && said.paths == ["/tokens/color/paper"], "{said:?}");
        assert_eq!(theme(&s)["tokens"]["color"]["paper"], "#0B0B10");
        assert_eq!(files.len(), 1, "{files:?}");
        assert_eq!((files[0].path.as_str(), files[0].before.as_deref()), (THEME, Some(was.as_str())));
        assert_ne!(s.png("revenue", 480).unwrap(), before, "frames are drawn in the theme as edited");

        // The editor's undo writes the theme back, and frames are drawn as they were.
        s.write_files(vec![crate::versions::Written { path: THEME.into(), text: files[0].before.clone() }]);
        assert_eq!(s.png("revenue", 480).unwrap(), before, "undone, the deck is drawn as it was");
        s.write_files(vec![crate::versions::Written { path: THEME.into(), text: files[0].after.clone() }]);
        assert_eq!(theme(&s)["tokens"]["color"]["paper"], "#0B0B10", "and redone");

        // Taking out a role the deck uses is refused, with E102, and nothing changes.
        let now = s.file(THEME).unwrap().to_vec();
        let gone = ops(json!([{ "op": "remove", "path": "/type/roles/headline" }]));
        let (said, files) = s.theme_edit(&gone, false, user()).unwrap();
        assert!(said.refused && !said.applied && files.is_empty(), "{said:?}");
        assert!(said.added.iter().any(|f| f.code == "E102"), "{:?}", said.added);
        assert_eq!(s.file(THEME).unwrap(), now.as_slice());

        // An op that does not apply says which.
        let wrong = ops(json!([{ "op": "replace", "path": "/no/such", "value": 1 }]));
        let e = s.theme_edit(&wrong, false, user()).unwrap_err().to_string();
        assert!(e.contains("op 0"), "{e}");
    }

    /// A photo the bundle holds gives the theme its colors (PLAN 2.94): one edit, drawn, and undone
    /// by writing the theme back, as any theme edit is.
    #[test]
    fn a_photo_gives_the_theme_its_colors() {
        let mut files = revenue();
        let ridge = "assets/trails-ridge.png";
        files.insert(ridge.into(), std::fs::read(format!("../../docs/examples/{ridge}")).unwrap());
        let mut s = Session::open(files).unwrap();
        let before = s.png("revenue", 480).unwrap();
        let photo = ThemeEdit { ops: Vec::new(), photo: Some(ridge.into()) };
        let (said, files) = s.theme_edit(&photo, false, user()).unwrap();
        assert!(said.applied && said.paths.contains(&"/tokens/color/accent".to_string()), "{said:?}");
        assert_eq!(said.photo.as_ref().map(|p| p.image.as_str()), Some(ridge));
        assert_eq!(theme(&s)["tokens"]["color"]["accent"], "#B97CFF");
        assert_ne!(s.png("revenue", 480).unwrap(), before, "frames are drawn in the photo's colors");
        s.write_files(vec![crate::versions::Written { path: THEME.into(), text: files[0].before.clone() }]);
        assert_eq!(s.png("revenue", 480).unwrap(), before, "undone, the deck is drawn as it was");
        // A photo the bundle does not hold says so.
        let none = ThemeEdit { ops: Vec::new(), photo: Some("assets/none.png".into()) };
        let e = s.theme_edit(&none, false, user()).unwrap_err().to_string();
        assert!(e.contains("assets/none.png"), "{e}");
    }

    /// Dusk as a bundle took it on 2026-10-06, before the library grew, and Dusk as it ships.
    const OLDER: &str = include_str!("../../../tests/themes/older/dusk-0.10.theme.json");
    const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");

    /// A deck whose bundle took Dusk before its library grew (PLAN 3.31): it takes every layout
    /// Dusk ships, each in its section, and draws every slide as it did; taken once, the library
    /// brings nothing more; and the save keeps the theme with the library, recorded by its author.
    #[test]
    fn an_older_dusk_takes_the_library_and_draws_as_it_did() {
        let (mut files, _) = crate::store::tests::begun();
        // Saved, as the bundle was, with the fonts it holds, named by their content.
        let held: Value = serde_json::from_slice(&files[THEME]).unwrap();
        let mut older: Value = serde_json::from_str(OLDER).unwrap();
        older["type"]["families"] = held["type"]["families"].clone();
        files.insert(THEME.into(), serde_json::to_vec_pretty(&older).unwrap());
        let mut s = Session::open(files).unwrap();
        let sections = |s: &Session| {
            let starters = s.starters();
            let grouped = starters.iter().filter_map(|l| l["group"].as_str().map(String::from));
            scaena_core::sort::set(grouped)
        };
        assert_eq!(s.starters().len(), 11 + 1, "the older Dusk's layouts, and a blank slide");
        assert!(sections(&s).is_empty(), "none of them in a section");
        let states: Vec<String> = s.deck().states.iter().map(|state| state.id.clone()).collect();
        let before: Vec<Vec<u8>> = states.iter().map(|state| s.png(state, 320).unwrap()).collect();

        let brought = s.library(DUSK, user()).unwrap().expect("it takes the library");
        assert_eq!(
            (brought.theme.as_str(), brought.roles.clone()),
            ("Dusk", vec!["hero".to_string(), "verse".to_string()])
        );
        let shipped: Value = serde_json::from_str(DUSK).unwrap();
        let layouts = shipped["layouts"].as_object().unwrap().len();
        assert_eq!(s.starters().len(), layouts + 1, "every layout Dusk ships, and a blank slide");
        assert_eq!(sections(&s).len(), 16, "{:?}", sections(&s));
        let after: Vec<Vec<u8>> = states.iter().map(|state| s.png(state, 320).unwrap()).collect();
        assert!(before == after, "every slide draws as it did");
        assert!(s.library(DUSK, user()).unwrap().is_none(), "taken once, it brings nothing more");

        let saved = s.save("2026-10-12T12:00:00Z", false, Some(&crate::store::tests::recorder)).unwrap();
        let kept: Value = serde_json::from_slice(&saved.files[THEME]).unwrap();
        assert_eq!(kept["layouts"].as_object().unwrap().len(), layouts, "the save keeps the theme with the library");
        assert_eq!(kept["scaena-theme"], shipped["scaena-theme"]);
        let doc = scaena_store::crdt::DeckDoc::load(&saved.files[scaena_store::HISTORY]).unwrap();
        let said: Vec<(String, String)> = (doc.changes().into_iter())
            .map(|c| (c.author.unwrap_or_default(), c.message.unwrap_or_default()))
            .collect();
        assert!(said.contains(&("user".into(), "library: the layouts Dusk ships".into())), "{said:?}");
    }

    /// The save records the edit by its author, with the theme's text as the save writes it, and
    /// nothing as changed outside Scaena: the session kept the theme's bytes from before it.
    #[test]
    fn a_save_records_the_theme_edit_by_its_author() {
        let (files, _) = crate::store::tests::begun();
        let mut s = Session::open(files).unwrap();
        let size = ops(json!([{ "op": "replace", "path": "/type/roles/body/size", "value": 30 }]));
        s.theme_edit(&size, false, user()).unwrap();
        let saved = s.save("2026-10-06T12:00:00Z", false, Some(&crate::store::tests::recorder)).unwrap();
        let doc = scaena_store::crdt::DeckDoc::load(&saved.files[scaena_store::HISTORY]).unwrap();
        let said: Vec<(String, String)> = (doc.changes().into_iter())
            .map(|c| (c.author.unwrap_or_default(), c.message.unwrap_or_default()))
            .collect();
        assert!(said.contains(&("user".into(), "theme_edit: type/roles/body/size".into())), "{said:?}");
        assert!(!said.iter().any(|(author, _)| author == "fs"), "{said:?}");
        let held = doc.files();
        let kept: Value = serde_json::from_slice(&held[THEME]).unwrap();
        assert_eq!(kept["type"]["roles"]["body"]["size"], 30);
        assert_eq!(held[THEME], saved.files[THEME], "the history holds the theme as the save wrote it");
    }
}

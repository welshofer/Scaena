//! The theme, edited from the editor (PLAN 2.61, ADR-0016): RFC 6902 operations on the theme the
//! deck names, as `scaena theme --edit` makes them, written into the session. The source does not
//! change, so the editor keeps each edit in its source's history as the theme's text before and
//! after, which its undo writes back (`Session::write_files`).

use crate::assistant::Caller;
use crate::versions::Rewritten;
use crate::{Error, Session};
use scaena_ops::theme::{ThemeEdit, ThemeEdited, theme_editing};

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

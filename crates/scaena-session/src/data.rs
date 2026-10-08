//! A data source edited from the page (PLAN 2.55, ADR-0014, SPEC §3.10): read as a sheet, and
//! edited as `data_edit` edits it. Each data file an edit writes is kept as it was before and
//! after, for the Data panel's undo and redo; the bundle's history records each version when the
//! page saves (`store`). A file the Files panel takes out (PLAN 2.59) is kept so too, and the
//! same undo puts it back.

use crate::assistant::Caller;
use crate::{Error, Session};
use indexmap::IndexMap;
use scaena_core::data::cells::Cells;
use scaena_core::files::BundleFile;
use scaena_core::format::Locale;
use scaena_ops::create::{Attach, Attached};
use scaena_ops::data::{DataEdit, DataEdited, Sheet};
use scaena_ops::lint::Why;
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A data file dropped on the canvas (PLAN 2.76): where it is in the bundle, the source a chart
/// of it reads (its `data` less the `@`), and, where that source is new, what `data_attach` says
/// of it and the patch that declares it, which the page applies as it applies any edit on the
/// canvas, one change and one undo. A source the deck declares already for the file has neither;
/// a declaration refused has an empty patch.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Attaching {
    pub path: String,
    pub data: String,
    pub attached: Option<Attached>,
    pub patch: Vec<Value>,
}

/// A file an edit wrote, or took out: its path, its bytes before and after (none where the
/// bundle did not hold it), and what the edit says it did.
#[derive(Debug, Clone)]
pub(crate) struct Written {
    pub path: String,
    pub before: Option<Vec<u8>>,
    pub after: Option<Vec<u8>>,
    pub why: String,
}

impl Session {
    /// The bundle's images, fonts, and data (PLAN 2.59), as `scaena files` lists them: each with
    /// what in the deck names it, and the nodes drawn from it in the states that show them so.
    pub fn bundle_files(&self) -> Result<Vec<BundleFile>, Error> {
        let held: Vec<(String, u64)> =
            self.files.iter().map(|(path, bytes)| (path.clone(), bytes.len() as u64)).collect();
        scaena_core::files::files(&self.deck, &self.theme, &held).map_err(Error::Ops)
    }

    /// `path` taken out of the bundle (PLAN 2.59): one of its images, fonts, or data that nothing
    /// names, so the deck draws and reads the same. It is kept for the panel's undo as an edit of
    /// a data file is, and the next save takes it out where the bundle is kept. Refused, with
    /// why, where something names it, or it is none of them.
    pub fn remove_file(&mut self, path: &str) -> Result<(), Error> {
        let listed = self.bundle_files()?;
        let held: Vec<String> = self.files.keys().cloned().collect();
        if let Some(refused) = scaena_ops::files::refusals(&listed, &held, &[path.to_string()]).pop() {
            return Err(Error::Ops(refused.why));
        }
        let before = std::sync::Arc::make_mut(&mut self.files).remove(path);
        self.removed.insert(path.to_string());
        self.undone.clear();
        self.done.push(Written { path: path.to_string(), before, after: None, why: format!("take out {path}") });
        Ok(())
    }

    /// Where a data file dropped on the canvas goes (PLAN 2.76): where `place` puts it, under
    /// `data/` by its own name, unless the bundle holds other bytes there; then `-2`, `-3`, …
    /// before its extension, so no source the deck declares reads other rows than it did.
    pub fn placing(&self, name: &str, bytes: &[u8]) -> String {
        let first = scaena_store::place(name, bytes);
        let (stem, ext) = match first.rsplit_once('.') {
            Some((stem, ext)) if !stem.ends_with('/') => (stem.to_string(), format!(".{ext}")),
            _ => (first.clone(), String::new()),
        };
        (1..)
            .map(|n| if n == 1 { first.clone() } else { format!("{stem}-{n}{ext}") })
            .find(|path| self.files.get(path).is_none_or(|held| held[..] == *bytes))
            .expect("some number is free")
    }

    /// `path`, a data file the bundle holds (a CSV or JSON file dropped on the canvas), as the
    /// source a chart of it reads: the one the deck declares for it, or one declared as
    /// `data_attach` declares it, under an id made from its name and new to the deck's sources,
    /// each column typed as narrowly as its values allow. Nothing is written here: the page
    /// applies the patch.
    /// `schema`, where given, types the columns, as a pasted sheet's cells say they read
    /// (PLAN 2.96); else each is typed as narrowly as its values allow.
    pub fn attaching(&self, path: &str, schema: Option<IndexMap<String, String>>) -> Result<Attaching, Error> {
        let bytes =
            (self.files.get(path).cloned()).ok_or_else(|| Error::Ops(format!("the bundle holds no `{path}`")))?;
        if let Some(id) = self.deck.data.iter().find(|(_, d)| d.source.as_str() == Some(path)).map(|(id, _)| id) {
            return Ok(Attaching { path: path.into(), data: id.clone(), attached: None, patch: vec![] });
        }
        let stem = std::path::Path::new(path).file_stem().and_then(|s| s.to_str()).unwrap_or("data");
        let base = scaena_core::inserts::slug(stem, "data");
        let id = (1..)
            .map(|n| if n == 1 { base.clone() } else { format!("{base}-{n}") })
            .find(|id| !self.deck.data.contains_key(id))
            .expect("some number is free");
        let req = Attach { id: id.clone(), file: path.into(), schema, parse: None };
        let (attached, write) =
            scaena_ops::create::attaching(&self.bundle(), &req, bytes).map_err(|e| Error::Ops(e.to_string()))?;
        let declared = write.as_ref().and_then(|w| w.deck.data.get(&id));
        let patch = match declared.map(serde_json::to_value).transpose().map_err(|e| Error::Ops(e.to_string()))? {
            None => vec![],
            // A deck with no data source has no `data` to add one to.
            Some(source) if self.deck.data.is_empty() => {
                vec![json!({ "op": "add", "path": "/data", "value": { id.clone(): source } })]
            }
            Some(source) => vec![json!({ "op": "add", "path": format!("/data/{id}"), "value": source })],
        };
        Ok(Attaching { path: path.into(), data: id, attached: Some(attached), patch })
    }

    /// `text` pasted on the canvas as a sheet's cells, read in the deck's language (PLAN 2.96):
    /// the source they would be, or none where `text` is not cells, so it pastes as text.
    pub fn cells(&self, text: &str) -> Option<Cells> {
        let lang = self.deck.meta.as_ref().and_then(|m| m.lang.as_deref());
        scaena_core::data::cells::read(text, Locale::of(lang))
    }

    /// The deck's data sources, in its order, each with the file it is: none for rows written
    /// inline.
    pub fn data_sources(&self) -> Vec<(String, Option<String>)> {
        (self.deck.data.iter()).map(|(name, d)| (name.clone(), d.source.as_str().map(String::from))).collect()
    }

    /// Source `name` as a sheet (`scaena data`, SPEC §3.10), and the file it is.
    pub fn data_sheet(&self, name: &str) -> Result<(Sheet, Option<String>), Error> {
        let source =
            self.deck.data.get(name).ok_or_else(|| Error::Ops(format!("the deck has no data source `{name}`")))?;
        let sheet = scaena_core::data::edit::sheet(&self.deck, &*self.files, name).map_err(Error::Ops)?;
        Ok((sheet, source.source.as_str().map(String::from)))
    }

    /// `req` made by `by`, as `data_edit` makes it, linted before and after if `linted`, else only
    /// validated, as for a value typed in a cell: what it did, and whether it wrote.
    pub fn data_edit(&mut self, req: &DataEdit, linted: bool, by: Caller) -> Result<(DataEdited, bool), Error> {
        let (edited, made) = scaena_ops::data::editing(&self.bundle(), req, linted)?;
        let wrote = self.write(made, by)?;
        Ok((edited, wrote))
    }

    /// The file the user's last edit wrote, or took out, put back as it was, by `by`, and an
    /// edit of a data file recorded so; with `redo`, the last put back written, or taken out,
    /// again. The source it is, by name (its path, where the deck names it no more, or names no
    /// source with it); none where there was nothing to undo or redo. A file changed since by
    /// other means, a file dropped on the page, has nothing left to undo, and says so.
    pub fn data_undo(&mut self, redo: bool, by: Caller) -> Result<Option<String>, Error> {
        let Some(step) = (if redo { self.undone.pop() } else { self.done.pop() }) else { return Ok(None) };
        let (now, then) = if redo { (&step.before, &step.after) } else { (&step.after, &step.before) };
        if self.files.get(&step.path) != now.as_ref() {
            self.done.retain(|s| s.path != step.path);
            self.undone.retain(|s| s.path != step.path);
            return Err(Error::Ops(format!("{} has changed since: nothing of it is left to undo", step.path)));
        }
        let deck = self.deck.clone();
        match then.clone() {
            Some(bytes) => {
                // An edit of a data file is recorded in the bundle's history; a file put back is
                // only the bundle's again.
                if step.before.is_some() && step.after.is_some() {
                    let text = String::from_utf8_lossy(&bytes).into_owned();
                    let why = Why::new(format!("{} {}", if redo { "redo" } else { "undo" }, step.why));
                    self.keep(&deck, &BTreeMap::from([(step.path.clone(), text)]), &why, by)?;
                }
                self.removed.remove(&step.path);
                self.add_file(&step.path, bytes);
            }
            None => {
                std::sync::Arc::make_mut(&mut self.files).remove(&step.path);
                self.removed.insert(step.path.clone());
            }
        }
        let name =
            (deck.data.iter()).find(|(_, d)| d.source.as_str() == Some(step.path.as_str())).map(|(n, _)| n.clone());
        let said = name.unwrap_or_else(|| step.path.clone());
        if redo {
            self.done.push(step)
        } else {
            self.undone.push(step)
        }
        Ok(Some(said))
    }
}

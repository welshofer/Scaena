//! A data source's rows, read and edited in place (PLAN 2.55, ADR-0014, SPEC §3.10): a cell
//! set, a row added, a row taken away, each value checked by its column's type as a chart reads
//! it. A source in a file is edited byte by byte and written as that file, one write for all the
//! edits; rows written inline are the deck's, patched. Refused, as a patch is, where the deck it
//! leaves validates worse.

use crate::lint::{View, Why, Write, errors, lint, lint_in, write};
use crate::{Bundle, OpsError};
use scaena_core::data::Texts;
pub use scaena_core::data::edit::{CellProblem, RowEdit, Sheet, SheetColumn};
use scaena_core::data::edit::{Edited, sheet};
use scaena_core::lint::{Delta, delta};
use scaena_core::validate::validate_bundle;
use scaena_core::{Deck, Finding};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Edits of a data source's rows.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataEdit {
    /// The source's id: what a chart or a table names as `@id`.
    pub source: String,
    /// The edits, made in order, all or none: each names a row as the edits before it leave the
    /// rows. With none, the source is read, and nothing is written.
    #[serde(default)]
    pub edits: Vec<RowEdit>,
}

/// A data source edited, or read.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct DataEdited {
    /// Whether it was written: not under a dry run, with no edits, nor when refused.
    pub edited: bool,
    pub source: String,
    /// The file it is, which an edit writes; none for rows written inline, which are the deck's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// The source as it reads after the edits, or as it is where there are none or they are
    /// refused: its columns, their types, its rows as written, and the cells a column does not
    /// read.
    pub sheet: Sheet,
    /// What `validate` and `lint` find after the edits that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not after.
    pub removed: Vec<Finding>,
    /// The findings that are errors, after.
    pub errors: usize,
    /// Refused: the edits would have added a validation finding (in `added`).
    pub refused: bool,
    /// The texts whose quoted figures the edits set again, by node id, each once (ADR-0019):
    /// written in the deck in the same change, with each claim that held the old figure.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub quoted: Vec<String>,
}

/// Make `req.edits` in source `req.source`, all or none, and write the file it is, or the deck
/// where its rows are written inline: recorded in the bundle's history, if it keeps one, as one
/// change. An edit that does not apply stops them all with `op`, its index; edits that would
/// make the deck invalid are refused. Neither, nor a dry run, nor a read, writes anything.
pub fn data_edit(b: &Bundle, req: &DataEdit, dry_run: bool) -> Result<DataEdited, OpsError> {
    let (mut edited, made) = editing(b, req, true)?;
    match made {
        Some(made) if !dry_run => write(b, made)?,
        _ => edited.edited = false,
    }
    Ok(edited)
}

/// [`data_edit`] with nothing written: what the edits do, and what to write, unless they are
/// refused. A client that keeps its bundle in memory writes it there (the web page, PLAN 2.55).
/// Without `linted`, the deck is validated but not linted, so `added` and `removed` say only
/// what validation finds, as for a value typed in a cell: the editor lints the state it shows
/// once it is written, as after a keystroke (`patch::typing`).
pub fn editing(b: &Bundle, req: &DataEdit, linted: bool) -> Result<(DataEdited, Option<Write>), OpsError> {
    let name = req.source.as_str();
    let Some(declared) = b.deck.data.get(name) else {
        let names: Vec<&str> = b.deck.data.keys().map(String::as_str).collect();
        let has = match names.as_slice() {
            [] => "it has none".to_string(),
            names => format!("it has {}", names.join(", ")),
        };
        return Err(OpsError::new(format!("the deck has no data source `{name}`: {has}")));
    };
    let file = declared.source.as_str().map(String::from);
    let mut deck = b.deck.clone();
    let mut view = View::of(b);
    for (i, edit) in req.edits.iter().enumerate() {
        let at = |message: String| OpsError { message, plan: None, op: Some(i) };
        match scaena_core::data::edit::edit(&deck, &Texts(&view), name, edit).map_err(at)? {
            Edited::File { path, bytes } => view = view.with(path, bytes),
            Edited::Inline(ops) => {
                let mut doc = deck.to_value()?;
                scaena_core::patch::apply(&mut doc, &ops).map_err(|e| at(e.to_string()))?;
                deck = Deck::from_value(&doc).map_err(at)?;
            }
        }
    }
    // Each figure a run quotes, set again from the data as the edits leave it (ADR-0019).
    let doc = deck.to_value()?;
    let stale = scaena_core::quotes::stale(&deck, &doc, &Texts(&view));
    let mut quoted: Vec<String> = Vec::new();
    if !stale.is_empty() {
        let ops: Vec<serde_json::Value> =
            scaena_core::quotes::requote(&stale).iter().map(serde_json::to_value).collect::<Result<_, _>>()?;
        let mut doc = doc;
        scaena_core::patch::apply(&mut doc, &ops).map_err(|e| OpsError::new(e.to_string()))?;
        deck = Deck::from_value(&doc).map_err(OpsError::new)?;
        for s in &stale {
            if !quoted.contains(&s.node) {
                quoted.push(s.node.clone());
            }
        }
    }
    let read = |deck: &Deck, view: &View| sheet(deck, &Texts(view), name).map_err(OpsError::new);
    let invalid = validate_bundle(&b.deck.to_json()?, &b.files)?;
    // Edits that leave the file and the deck as they were, a cell set to what it held, write
    // nothing.
    let same =
        |path: &String| view.pending.get(path).is_none_or(|bytes| b.files.read(path).is_ok_and(|was| was == *bytes));
    if req.edits.is_empty() || (file.as_ref().is_none_or(same) && deck.to_json()? == b.deck.to_json()?) {
        let errors = errors(&invalid);
        let sheet = read(&b.deck, &View::of(b))?;
        let edited = DataEdited {
            edited: false,
            source: name.into(),
            file,
            sheet,
            added: vec![],
            removed: vec![],
            errors,
            refused: false,
            quoted: vec![],
        };
        return Ok((edited, None));
    }
    // The deck and its files as the edits leave them, checked as `validate` checks a bundle: a
    // validation finding they add refuses them all.
    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let invalid_after = validate_bundle(&deck.to_json()?, &view)?;
    let refused = !delta(&invalid, &states, &invalid_after, &states, &[]).added.is_empty();
    let (before, after) = match linted && !refused {
        true => (lint(b)?.findings, lint_in(&deck, &view)?.findings),
        false => (invalid, invalid_after),
    };
    let Delta { added, removed } = delta(&before, &states, &after, &states, &[]);
    let sheet = if refused { read(&b.deck, &View::of(b))? } else { read(&deck, &view)? };
    let edited = DataEdited {
        edited: !refused,
        source: name.into(),
        file: file.clone(),
        sheet,
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        errors: errors(&after),
        refused,
        quoted: if refused { vec![] } else { quoted },
    };
    if refused {
        return Ok((edited, None));
    }
    let mut made = Write::new(deck, Why::new(format!("data_edit {name}: {}", said(&req.edits))));
    if let Some(path) = file {
        let bytes = view.pending.remove(&path).expect("an edit of a file writes it");
        made.files.insert(path, bytes);
    }
    Ok((edited, Some(made)))
}

/// What `edits` do, as the history says it: `rev of row 3`, `a row added`, `row 2 taken away`,
/// or how many.
fn said(edits: &[RowEdit]) -> String {
    match edits {
        [RowEdit::Set { row, column, .. }] => format!("{column} of row {row}"),
        [RowEdit::Add { row: Some(row), .. }] => format!("a row added at {row}"),
        [RowEdit::Add { row: None, .. }] => "a row added".into(),
        [RowEdit::Remove { row }] => format!("row {row} taken away"),
        edits => format!("{} edits", edits.len()),
    }
}

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
    /// Words that still write a cell's old value, as the source wrote it before an edit set it:
    /// each place, by the slides it is on, as the editors' status line says it ("Slide 9's notes
    /// still say 4,605"). Nothing is changed in them: a figure written in words, not quoted, may
    /// be meant (R1's dogfood 10).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stale: Vec<StillSaid>,
}

/// Words that still write an edited cell's old value.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct StillSaid {
    /// The value as the source wrote it before the edit.
    pub was: String,
    /// The value the edit set.
    pub now: String,
    /// What says it: `words` (a text a slide shows), `description`, `notes`, or `claim`.
    pub what: String,
    /// The slides it is on, by number.
    pub slides: Vec<usize>,
    /// In a person's words: "Slide 9's notes still say 4,605".
    pub said: String,
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
    // Each cell a `set` edits, as the source wrote it before, and the value set.
    let mut figures: Vec<(String, String)> = Vec::new();
    for (i, edit) in req.edits.iter().enumerate() {
        let at = |message: String| OpsError { message, plan: None, op: Some(i) };
        if let RowEdit::Set { row, column, value } = edit
            && let Ok(was) = sheet(&deck, &Texts(&view), name)
            && let Some(c) = was.columns.iter().position(|col| &col.name == column)
            && let Some(old) = was.rows.get(*row).and_then(|r| r.get(c))
        {
            figures.push((old.clone(), value.clone()));
        }
        match scaena_core::data::edit::edit(&deck, &Texts(&view), name, edit).map_err(at)? {
            Edited::File { path, bytes } => view = view.with(path, bytes),
            Edited::Inline(ops) => {
                let mut doc = deck.to_value()?;
                scaena_core::patch::apply(&mut doc, &ops).map_err(|e| at(e.to_string()))?;
                deck = Deck::from_value(&doc).map_err(at)?;
            }
        }
        // A column renamed is renamed through the deck in the same edit: the source's schema,
        // its rows written inline, and every chart, table, transform, and quote that reads it.
        if let RowEdit::Rename { column, to } = edit {
            let mut doc = deck.to_value()?;
            let ops = scaena_core::data::rename::renaming(&doc, name, column, to).map_err(at)?;
            scaena_core::patch::apply(&mut doc, &ops).map_err(|e| at(e.to_string()))?;
            deck = Deck::from_value(&doc).map_err(at)?;
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
            stale: vec![],
        };
        return Ok((edited, None));
    }
    // Rows fetched from a service and edited since say so in their credit, as CC BY asks a change
    // to be indicated (ADR-0030): in the same change.
    if let Some(serde_json::Value::Object(fetched)) = deck.data.get_mut(name).and_then(|d| d.fetched.as_mut()) {
        fetched.insert("edited".into(), true.into());
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
        stale: if refused { vec![] } else { still_said(&deck, &figures) },
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

/// Where `deck` still writes each old value of `figures` (as the source wrote it, and the value
/// set) as a word, by `quotes::places`, the matcher quotes set figures again by: in the texts
/// each slide shows, their descriptions, its states' notes, and its beats' claims and notes. A
/// value with no digit, or a single character, is no figure to look for, and one set to itself
/// is not stale. A number is looked for as written and with its thousands grouped by commas
/// (`4605` as `4,605`), never at another scale.
fn still_said(deck: &Deck, figures: &[(String, String)]) -> Vec<StillSaid> {
    let looked: Vec<(&str, &str, Vec<String>)> = figures
        .iter()
        .filter(|(was, now)| was != now && was.chars().count() > 1 && was.chars().any(|c| c.is_ascii_digit()))
        .map(|(was, now)| (was.as_str(), now.as_str(), written(was)))
        .collect();
    if looked.is_empty() {
        return Vec::new();
    }
    let Ok(snaps) = scaena_core::resolve_states(deck) else { return Vec::new() };
    let slides = deck.slides();
    let slide_of = |state: usize| slides.iter().position(|s| s.contains(&state)).map(|k| k + 1);
    // Each place words are written: what they are, the slide they are on, and the words.
    let mut places: Vec<(&'static str, usize, String)> = Vec::new();
    for (i, state) in deck.states.iter().enumerate() {
        let Some(k) = slide_of(i) else { continue };
        if let Some(snap) = snaps.get(i) {
            for props in snap.nodes.values() {
                places.push(("words", k, scaena_core::reading::words(props)));
                if let Some(alt) = props.get("alt").and_then(serde_json::Value::as_str) {
                    places.push(("description", k, alt.to_string()));
                }
            }
        }
        if let Some(notes) = &state.notes {
            places.push(("notes", k, notes.clone()));
        }
    }
    for beat in deck.spine.iter().flat_map(|s| s.sections.iter()).flat_map(|s| s.beats.iter()) {
        let first = beat.states.iter().filter_map(|id| deck.state_index(id)).filter_map(slide_of).min();
        let Some(k) = first else { continue };
        places.push(("claim", k, beat.claim.clone()));
        if let Some(notes) = &beat.notes {
            places.push(("notes", k, notes.clone()));
        }
    }
    let mut out: Vec<StillSaid> = Vec::new();
    for (was, now, forms) in &looked {
        for what in ["words", "description", "notes", "claim"] {
            let mut on: Vec<usize> = places
                .iter()
                .filter(|(w, _, text)| {
                    *w == what && forms.iter().any(|f| !scaena_core::quotes::places(text, f).is_empty())
                })
                .map(|(_, k, _)| *k)
                .collect();
            scaena_core::sort::sort(&mut on);
            on.dedup();
            if on.is_empty() {
                continue;
            }
            let figure = forms
                .iter()
                .find(|f| {
                    places.iter().any(|(w, k, text)| {
                        *w == what && on.contains(k) && !scaena_core::quotes::places(text, f).is_empty()
                    })
                })
                .map_or(*was, String::as_str);
            let says = if what == "claim" && on.len() == 1 { "says" } else { "say" };
            let said = format!("{}'s {what} still {says} {figure}", slides_named(&on));
            out.push(StillSaid { was: was.to_string(), now: now.to_string(), what: what.into(), slides: on, said });
        }
    }
    out
}

/// A cell's value as words may write it: as written, and, for a whole number of five digits or
/// more, or four, with its thousands grouped by commas.
fn written(was: &str) -> Vec<String> {
    let mut forms = vec![was.to_string()];
    let (sign, digits) = was.strip_prefix('-').map_or(("", was), |d| ("-", d));
    let (whole, rest) = digits.split_once('.').map_or((digits, None), |(w, f)| (w, Some(f)));
    if whole.len() >= 4
        && whole.bytes().all(|b| b.is_ascii_digit())
        && rest.is_none_or(|f| f.bytes().all(|b| b.is_ascii_digit()))
    {
        let mut grouped = String::new();
        for (i, c) in whole.chars().enumerate() {
            if i > 0 && (whole.len() - i) % 3 == 0 {
                grouped.push(',');
            }
            grouped.push(c);
        }
        forms.push(match rest {
            Some(f) => format!("{sign}{grouped}.{f}"),
            None => format!("{sign}{grouped}"),
        });
    }
    forms
}

/// Slides by number, as a person names them: "Slide 9", "Slides 2 and 3", "Slides 2, 3, and 7".
fn slides_named(on: &[usize]) -> String {
    match on {
        [one] => format!("Slide {one}"),
        [a, b] => format!("Slides {a} and {b}"),
        [rest @ .., last] => {
            let rest: Vec<String> = rest.iter().map(usize::to_string).collect();
            format!("Slides {}, and {last}", rest.join(", "))
        }
        [] => String::new(),
    }
}

/// What `edits` do, as the history says it: `rev of row 3`, `a row added`, `row 2 taken away`,
/// or how many.
fn said(edits: &[RowEdit]) -> String {
    match edits {
        [RowEdit::Set { row, column, .. }] => format!("{column} of row {row}"),
        [RowEdit::Add { row: Some(row), .. }] => format!("a row added at {row}"),
        [RowEdit::Add { row: None, .. }] => "a row added".into(),
        [RowEdit::Remove { row }] => format!("row {row} taken away"),
        [RowEdit::Rename { column, to }] => format!("{column} renamed {to}"),
        edits => format!("{} edits", edits.len()),
    }
}

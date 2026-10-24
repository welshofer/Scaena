//! Find and replace across the deck's words, in every state (PLAN 2.47, 2.83, ADR-0013): its
//! texts, each node's description, each state's notes, each beat's claim and notes, the words its
//! charts and tables draw, and its data sources' cells, once for each place they are written, with
//! where a query matches them; and, with what to put there, the one patch that replaces every
//! match where it is written (`scaena_core::patch::find`, `replacing`). A cell of a source read
//! from a file is found and left as it is, the file named: the replacement writes the deck alone.

use crate::lint::{Write, write};
use crate::patch::{Patched, patching};
use crate::{Bundle, OpsError};
use scaena_core::patch::{Found, Query};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

/// What a search found and, asked to replace it, what the replacement did.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Searched {
    /// The words the query matches, once for each place they are written, in the order the
    /// deck first shows them.
    pub found: Vec<Found>,
    /// The matches in all of them.
    pub matches: usize,
    /// With a replacement and a match: the one patch that replaces every match, a
    /// `replace_text` for each in a text and a `replace` of other words, as `patch` reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced: Option<Patched>,
    /// With a replacement: the data files holding matches it left as they are, the deck's
    /// replacement writing the deck alone. Each wants its own edit (`scaena data --edits`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept: Vec<String>,
}

/// The deck's words that `query` matches, and the cells of its data files that it matches.
pub fn find(b: &Bundle, query: &Query) -> Result<Vec<Found>, OpsError> {
    let mut found = scaena_core::patch::find(&b.deck.to_value()?, query).map_err(OpsError::new)?;
    found.extend(in_files(b, query)?);
    Ok(found)
}

/// The cells of each data source read from a file that `query` matches, each where it is: the
/// file, its row (from 0, after a CSV's header), and its column (`data/q3.csv#4/region`).
fn in_files(b: &Bundle, query: &Query) -> Result<Vec<Found>, OpsError> {
    // A file the bundle lacks is validation's to report; the deck's own words are found all the same.
    let Ok(files) = crate::lint::data_files(b) else { return Ok(Vec::new()) };
    let snaps = scaena_core::resolve_states(&b.deck).map_err(|e| OpsError::new(e.to_string()))?;
    let mut out = Vec::new();
    for (name, source) in &b.deck.data {
        let Value::String(path) = &source.source else { continue };
        let Ok(sheet) = scaena_core::data::edit::sheet(&b.deck, &files, name) else { continue };
        let reading = format!("@{name}");
        let states: Vec<String> = (snaps.iter())
            .filter(|s| s.nodes.values().any(|p| p.get("data").and_then(Value::as_str) == Some(reading.as_str())))
            .map(|s| s.state_id.clone())
            .collect();
        let first = states.first().cloned().or_else(|| snaps.first().map(|s| s.state_id.clone())).unwrap_or_default();
        for (r, row) in sheet.rows.iter().enumerate() {
            for (column, text) in sheet.columns.iter().zip(row) {
                if column.kind != scaena_core::data::ColumnType::String {
                    continue;
                }
                let matches = scaena_core::patch::matches(text, query);
                if matches.is_empty() {
                    continue;
                }
                out.push(Found {
                    kind: scaena_core::patch::Kind::Data,
                    node: None,
                    source: Some(name.clone()),
                    file: Some(path.clone()),
                    beat: None,
                    state: first.clone(),
                    states: states.clone(),
                    lives: format!("{path}#{r}/{}", column.name),
                    text: text.clone(),
                    matches,
                });
            }
        }
    }
    Ok(out)
}

/// The words `query` matches and, with `replace`, every match replaced by it in one patch,
/// written unless `dry_run`, as `patch` writes.
pub fn search(b: &Bundle, query: &Query, replace: Option<&str>, dry_run: bool) -> Result<Searched, OpsError> {
    let (mut searched, deck) = searching(b, query, replace)?;
    match deck {
        Some(deck) if !dry_run => write(b, deck)?,
        _ => {
            if let Some(replaced) = &mut searched.replaced {
                replaced.applied &= !dry_run;
            }
        }
    }
    Ok(searched)
}

/// [`search`], writing nothing: what it found, and the deck the replacement makes, for a
/// client that keeps the bundle itself (the editor's worker, ADR-0011).
pub fn searching(b: &Bundle, query: &Query, replace: Option<&str>) -> Result<(Searched, Option<Write>), OpsError> {
    let found = find(b, query)?;
    let matches = found.iter().map(|f| f.matches.len()).sum();
    let mut kept: Vec<String> = Vec::new();
    if replace.is_some() {
        for file in found.iter().filter_map(|f| f.file.clone()) {
            if !kept.contains(&file) {
                kept.push(file);
            }
        }
    }
    let replaceable = found.iter().any(|f| f.file.is_none() && !f.matches.is_empty());
    let (replaced, deck) = match replace {
        Some(with) if replaceable => {
            let ops = Value::Array(scaena_core::patch::replacing(&found, with));
            let (patched, deck) = patching(b, &ops, Some(&format!("replace {:?} with {with:?}", query.find)))?;
            (Some(patched), deck)
        }
        _ => (None, None),
    };
    Ok((Searched { found, matches, replaced, kept }, deck))
}

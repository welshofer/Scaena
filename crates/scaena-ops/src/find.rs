//! Find and replace across the deck's texts, in every state (PLAN 2.47, ADR-0013): each text
//! the deck shows, once for each place it is written, with where a query matches it; and, with
//! what to put there, the one patch that replaces every match where each text lives
//! (`scaena_core::patch::find`, `replacing`).

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
    /// Each text the query matches, once for each place it is written, in the order the
    /// deck first shows them.
    pub found: Vec<Found>,
    /// The matches in all of them.
    pub matches: usize,
    /// With a replacement and a match: the one patch that replaces every match, a
    /// `replace_text` for each, as `patch` reports it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replaced: Option<Patched>,
}

/// Each text of the deck that `query` matches.
pub fn find(b: &Bundle, query: &Query) -> Result<Vec<Found>, OpsError> {
    scaena_core::patch::find(&b.deck.to_value()?, query).map_err(OpsError::new)
}

/// The texts `query` matches and, with `replace`, every match replaced by it in one patch,
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
    let (replaced, deck) = match replace {
        Some(with) if matches > 0 => {
            let ops = Value::Array(scaena_core::patch::replacing(&found, with));
            let (patched, deck) = patching(b, &ops, Some(&format!("replace {:?} with {with:?}", query.find)))?;
            (Some(patched), deck)
        }
        _ => (None, None),
    };
    Ok((Searched { found, matches, replaced }, deck))
}

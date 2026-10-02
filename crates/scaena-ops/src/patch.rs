//! Patch a deck (PLAN 1.16, SPEC §7.3): JSON Patch and semantic ops, compiled against the
//! deck, checked as `validate` checks a bundle, and written canonically unless that adds a
//! validation finding.

use crate::lint::{View, errors, lint, lint_in, write_deck};
use crate::{Bundle, Context, OpsError};
use scaena_core::lint::{Delta, delta};
use scaena_core::patch::JsonOp;
use scaena_core::validate::validate_bundle;
use scaena_core::{Deck, Finding};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// What a patch did, or would do.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Patched {
    /// Whether the deck was written: not under a dry run, nor when the patch was refused.
    pub applied: bool,
    /// The patch as RFC 6902, in order.
    pub patch: Vec<JsonOp>,
    /// What `validate` and `lint` find after the patch that they did not before.
    pub added: Vec<Finding>,
    /// What they found before that they do not after.
    pub removed: Vec<Finding>,
    /// The findings that are errors, after.
    pub errors: usize,
    /// Refused: the patch would have added a validation finding (in `added`).
    #[serde(skip)]
    pub refused: bool,
}

/// Apply `ops` (a patch: a JSON array of ops) to the bundle's deck, all or none. An op that
/// does not apply stops it with `op`, its index; a patch that would make the deck invalid is
/// refused. Neither, nor a dry run, writes anything.
pub fn patch(b: &Bundle, ops: &Value, dry_run: bool) -> Result<Patched, OpsError> {
    let Some(list) = ops.as_array() else {
        return Err(OpsError::new("a patch is a JSON array of ops (SPEC §7.3, docs/schema/patch.schema.json)"));
    };
    let doc = serde_json::to_value(&b.deck)?;
    let compiled = scaena_core::patch::compile(&doc, list, &b.files).map_err(|e| OpsError {
        message: e.to_string(),
        plan: None,
        op: Some(e.index),
    })?;
    let was: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let is: Vec<&str> =
        compiled.doc["states"].as_array().into_iter().flatten().map(|s| s["id"].as_str().unwrap_or_default()).collect();
    // The deck it makes, checked as `validate` checks a bundle: a validation error it adds
    // refuses the whole patch.
    let text = serde_json::to_string_pretty(&compiled.doc)?;
    let invalid = validate_bundle(&b.deck.to_json()?, &b.files)?;
    let invalid_after = validate_bundle(&text, &b.files)?;
    let refused = !delta(&invalid, &was, &invalid_after, &is, &compiled.renamed).added.is_empty();
    let (before, after) = if refused {
        (invalid, invalid_after)
    } else {
        let next = Deck::from_json(&text).context("the patched deck")?;
        let before = lint(b)?.findings;
        let after = lint_in(&next, &View::of(b))?.findings;
        if !dry_run && compiled.doc != doc {
            write_deck(b, &next, BTreeMap::new())?;
        }
        (before, after)
    };
    let Delta { added, removed } = delta(&before, &was, &after, &is, &compiled.renamed);
    Ok(Patched {
        applied: !refused && !dry_run,
        patch: compiled.patch,
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        errors: errors(&after),
        refused,
    })
}

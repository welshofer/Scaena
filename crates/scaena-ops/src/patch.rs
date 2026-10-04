//! Patch a deck (PLAN 1.16, SPEC §7.3): JSON Patch and semantic ops, compiled against the
//! deck, checked as `validate` checks a bundle, and written canonically unless that adds a
//! validation finding.

use crate::lint::{View, Why, Write, errors, lint, lint_in, write};
use crate::{Bundle, Context, OpsError};
use scaena_core::lint::{Delta, delta};
use scaena_core::patch::{JsonOp, Renamed};
use scaena_core::validate::validate_bundle;
use scaena_core::{Deck, Finding};
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;

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
    patch_as(b, ops, dry_run, None)
}

/// [`patch`], recorded in the bundle's history as `what`; without it, by its ops' names.
pub(crate) fn patch_as(b: &Bundle, ops: &Value, dry_run: bool, what: Option<&str>) -> Result<Patched, OpsError> {
    let (mut patched, deck) = patching(b, ops, what)?;
    match deck {
        Some(deck) if !dry_run => write(b, deck)?,
        _ => patched.applied &= !dry_run,
    }
    Ok(patched)
}

/// [`patch`] with nothing written: what the patch does, and the deck to write, if it
/// changes it and is not refused. A client that keeps its bundle in memory writes it there
/// (the web page's assistant, PLAN 2.6). `what` names the change in the bundle's history;
/// without it, its ops' names do.
pub fn patching(b: &Bundle, ops: &Value, what: Option<&str>) -> Result<(Patched, Option<Write>), OpsError> {
    let Some(list) = ops.as_array() else {
        return Err(OpsError::new("a patch is a JSON array of ops (SPEC §7.3, docs/schema/patch.schema.json)"));
    };
    let doc = b.deck.to_value()?;
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
    let mut write = None;
    let (before, after) = if refused {
        (invalid, invalid_after)
    } else {
        let next = Deck::from_json(&text).context("the patched deck")?;
        let before = lint(b)?.findings;
        let after = lint_in(&next, &View::of(b))?.findings;
        if compiled.doc != doc {
            let mut names: Vec<&str> = list.iter().filter_map(|op| op.get("op").and_then(Value::as_str)).collect();
            names.dedup();
            let mut why = Why::new(what.map_or_else(|| format!("patch: {}", names.join(", ")), String::from));
            for renamed in &compiled.renamed {
                match renamed {
                    Renamed::Node { from, to } => why.renamed_nodes.push((from.clone(), to.clone())),
                    Renamed::State { from, to } => why.renamed_states.push((from.clone(), to.clone())),
                }
            }
            write = Some(Write::new(next, why));
        }
        (before, after)
    };
    let Delta { added, removed } = delta(&before, &was, &after, &is, &compiled.renamed);
    let patched = Patched {
        applied: !refused,
        patch: compiled.patch,
        added: added.into_iter().cloned().collect(),
        removed: removed.into_iter().cloned().collect(),
        errors: errors(&after),
        refused,
    };
    Ok((patched, write))
}

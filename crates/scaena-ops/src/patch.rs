//! Patch a deck (PLAN 1.16, SPEC §7.3): JSON Patch and semantic ops, compiled against the
//! deck, checked as `validate` checks a bundle, and written canonically unless that adds a
//! validation finding.

use crate::lint::{View, Why, Write, errors, lint, lint_in, lint_validated, write};
use crate::{Bundle, Context, OpsError};
use scaena_core::lint::{Delta, delta};
use scaena_core::model::theme::Furniture;
use scaena_core::patch::{JsonOp, Renamed};
use scaena_core::validate::{BundleFiles, validate_bundle};
use scaena_core::{Deck, Finding};
use scaena_engine::cascade::with_overrides;
use scaena_engine::theme::Theme;
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
    /// The states it changes, by id: each whose nodes, resolved with the deck's overrides,
    /// or whose layout, transition, choreography, hold, notes, or slide number and footer are
    /// not as they were, and each it adds. None when it is refused.
    pub states: Vec<String>,
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
    made(b, ops, what, Judge::Whole)
}

/// [`patching`] in an editor that keeps its engine between edits (ADR-0021): checked, and
/// refused, as a patch is, with validation and the document rules on the whole deck, and the
/// layout rules on the states the patch reaches alone, laid out by `layout` (a deck, its theme,
/// and the states' ids): each state it changes, and each whose cue starts from one it changes
/// or from another state than it did, before it and after. So `added`, `removed`, and `errors`
/// say what lint finds in those states, which every other state shows as it did, and an edit
/// costs what the states it changes cost, however long the deck. A patch that changes what the
/// deck holds besides its states and nodes (its theme, canvas, formats, fonts, data, or
/// furniture) reaches every state.
pub fn editing(
    b: &Bundle,
    ops: &Value,
    what: Option<&str>,
    mut layout: impl FnMut(&Deck, &Theme, &[&str]) -> Result<Vec<Finding>, OpsError>,
) -> Result<(Patched, Option<Write>), OpsError> {
    made(b, ops, what, Judge::Reached(&mut layout))
}

/// [`patching`] for an edit made as it is typed, a keystroke at a time (ADR-0013, PLAN
/// 2.32): validated, and refused as a patch is, but not linted, so `added` and `removed`
/// say only what validation finds. The editor lints the state it shows once the deck is
/// written, as it does after a keystroke in the source.
pub fn typing(b: &Bundle, ops: &Value, what: Option<&str>) -> Result<(Patched, Option<Write>), OpsError> {
    made(b, ops, what, Judge::Valid)
}

/// What lays out the states a patch reaches, in a deck and its theme: what the layout rules find
/// there.
type Layout<'a> = dyn FnMut(&Deck, &Theme, &[&str]) -> Result<Vec<Finding>, OpsError> + 'a;

/// What a patch's findings are judged by, before it and after.
enum Judge<'a> {
    /// Validation alone: an edit as it is typed.
    Valid,
    /// Validation, the document rules, and the layout rules on every state.
    Whole,
    /// Validation and the document rules, and the layout rules on the states the patch
    /// reaches, laid out by an editor's engine ([`editing`]).
    Reached(&'a mut Layout<'a>),
}

fn made(b: &Bundle, ops: &Value, what: Option<&str>, judge: Judge) -> Result<(Patched, Option<Write>), OpsError> {
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
    let mut states = Vec::new();
    let (before, after) = if refused {
        (invalid, invalid_after)
    } else {
        let next = Deck::from_json(&text).context("the patched deck")?;
        states = changed(&b.deck, &next, placed(b.theme_json.as_deref(), &b.deck, &next).as_ref())?;
        let (before, after) = match judge {
            Judge::Valid => (invalid, invalid_after),
            Judge::Whole => (lint(b)?.findings, lint_in(&next, &View::of(b))?.findings),
            Judge::Reached(layout) => {
                let (then, now) = reached((&b.deck, &doc), (&next, &compiled.doc), &states, &compiled.renamed);
                let files = View::of(b);
                let (then, now): (Vec<&str>, Vec<&str>) =
                    (then.iter().map(String::as_str).collect(), now.iter().map(String::as_str).collect());
                let theme = files.theme(&b.deck)?;
                let before = lint_validated(&b.deck, invalid, theme.as_deref(), |t| layout(&b.deck, t, &then))?;
                let theme = files.theme(&next)?;
                let after = lint_validated(&next, invalid_after, theme.as_deref(), |t| layout(&next, t, &now))?;
                (before.findings, after.findings)
            }
        };
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
        states,
        refused,
    };
    Ok((patched, write))
}

/// The states `ops` would change what shows in, by id, as [`Patched::states`] says, with
/// nothing validated, linted, or written: what an editor says of a drag before it is dropped
/// ("in 3 states", ADR-0013). `files` is the bundle, for the ops that read the theme, and
/// `places` the theme's furniture, where it places the slides' numbers and footers.
pub fn reach(
    deck: &Deck,
    files: &dyn BundleFiles,
    places: Option<&Furniture>,
    ops: &[Value],
) -> Result<Vec<String>, OpsError> {
    let doc = deck.to_value()?;
    let compiled = scaena_core::patch::compile(&doc, ops, files).map_err(|e| OpsError {
        message: e.to_string(),
        plan: None,
        op: Some(e.index),
    })?;
    changed(deck, &Deck::from_value(&compiled.doc).map_err(OpsError::new)?, places)
}

/// The furniture `theme`, a theme's JSON, places (ADR-0028), read only where `before` or `after`
/// shows a slide's number or footer.
pub(crate) fn placed(theme: Option<&str>, before: &Deck, after: &Deck) -> Option<Furniture> {
    if before.furniture.is_none() && after.furniture.is_none() {
        return None;
    }
    let theme: Value = serde_json::from_str(theme?).ok()?;
    serde_json::from_value(theme.get("furniture")?.clone()).ok()
}

/// The states a patch reaches, by id, before it and after: each it changes (`changed`, ids of
/// `after`), and each whose cue starts from one it changes or from another state than it did,
/// in the order each deck has them; and before it, each it takes away. Every state, where it
/// changes what the deck holds besides its states and nodes, which every state is drawn from.
fn reached(
    (before, was): (&Deck, &Value),
    (after, is): (&Deck, &Value),
    changed: &[String],
    renamed: &[Renamed],
) -> (Vec<String>, Vec<String>) {
    // What a state's snapshot does not say: the deck's theme, canvas, formats, fonts, data, and
    // furniture.
    let drawn = |doc: &Value| {
        let mut doc = doc.clone();
        if let Some(doc) = doc.as_object_mut() {
            for key in ["nodes", "states", "overrides", "spine", "comment"] {
                doc.remove(key);
            }
        }
        doc
    };
    let every = drawn(was) != drawn(is);
    // The id a state of `after` had in `before`.
    let then = |id: &str| -> String {
        (renamed.iter().rev()).fold(id.to_string(), |id, r| match r {
            Renamed::State { from, to } if *to == id => from.clone(),
            _ => id,
        })
    };
    let had: Vec<&str> = before.states.iter().map(|s| s.id.as_str()).collect();
    let before_it = |id: &str| -> Option<&str> {
        let at = had.iter().position(|h| *h == id)?;
        at.checked_sub(1).map(|p| had[p])
    };
    let mut now = Vec::new();
    for (i, s) in after.states.iter().enumerate() {
        let prior = i.checked_sub(1).map(|p| after.states[p].id.as_str());
        let reaches = every
            || changed.contains(&s.id)
            || prior.is_some_and(|p| changed.iter().any(|c| c == p))
            || prior.map(then) != before_it(&then(&s.id)).map(String::from);
        if reaches {
            now.push(s.id.clone());
        }
    }
    let kept: Vec<String> = after.states.iter().map(|s| then(&s.id)).collect();
    let mut was_reached: Vec<String> = Vec::new();
    for id in had {
        let reaches = every || !kept.iter().any(|k| k == id) || now.iter().any(|n| then(n) == id);
        if reaches {
            was_reached.push(id.to_string());
        }
    }
    (was_reached, now)
}

/// The states of `after` that show otherwise than the same state of `before`, and those
/// `before` does not have, in `after`'s order: its nodes, resolved with the overrides, or its
/// layout; or how it comes in and holds, its own transition, choreography, hold, and notes; or
/// what its slide draws at its foot, its number and footer, where `places`, the theme's
/// furniture, puts them (ADR-0028).
pub(crate) fn changed(before: &Deck, after: &Deck, places: Option<&Furniture>) -> Result<Vec<String>, OpsError> {
    let was = scaena_core::resolve_states(before).context("tracking")?;
    let is = scaena_core::resolve_states(after).context("tracking")?;
    let own = |deck: &Deck, id: &str| {
        let s = deck.states.iter().find(|s| s.id == id)?;
        Some((s.transition.clone(), s.choreography.clone(), s.hold, s.notes.clone()))
    };
    let differs = |s: &scaena_core::Snapshot| {
        let then = was.iter().find(|w| w.state_id == s.state_id);
        then.is_none_or(|w| {
            w.layout != s.layout
                || own(before, &w.state_id) != own(after, &s.state_id)
                || furnished(before, places, w) != furnished(after, places, s)
                || with_overrides(before, w).nodes != with_overrides(after, s).nodes
        })
    };
    Ok(is.iter().filter(|s| differs(s)).map(|s| s.state_id.clone()).collect())
}

/// What the state `snap` draws at its slide's foot, as the engine draws it (ADR-0028): its
/// number, as `places` writes it, and the deck's footer, each where the theme places one and
/// the slide shows them.
fn furnished(
    deck: &Deck,
    places: Option<&Furniture>,
    snap: &scaena_core::Snapshot,
) -> (Option<String>, Option<String>) {
    let (Some(wants), Some(places)) = (deck.furniture.as_ref(), places) else { return (None, None) };
    let Some(slide) = scaena_core::furniture::slide(deck, &snap.state_id) else { return (None, None) };
    if !slide.shown(places, snap.layout.as_deref()) {
        return (None, None);
    }
    let number = (places.number.as_ref()).filter(|_| wants.number).map(|item| slide.written(item.pattern.as_deref()));
    let footer = wants.footer.clone().filter(|_| places.footer.is_some());
    (number, footer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A patch that numbers the slides, sets their footer, or hides a slide's, changes what each
    /// state of a slide that shows them draws at its foot (ADR-0028), and `states` says so; a
    /// state in a layout the theme leaves them off is not among them.
    #[test]
    fn a_patch_that_furnishes_the_slides_changes_the_states_that_draw_it() {
        let b = crate::open(std::path::Path::new("../../docs/examples/revenue.deck.json")).unwrap();
        let states = |b: &Bundle, ops: Value| patching(b, &ops, None).unwrap().0.states;
        assert_eq!(states(&b, json!([{ "op": "furnish", "number": true }])), ["revenue", "mix"]);
        assert_eq!(states(&b, json!([{ "op": "furnish", "footer": "Board" }])), ["revenue", "mix"]);
        // On a deck that draws neither, a slide that hides them changes nothing drawn.
        let hide = json!([{ "op": "set_state", "id": "revenue", "prop": "furniture", "value": false }]);
        assert!(states(&b, hide.clone()).is_empty());
        let (_, write) = patching(&b, &json!([{ "op": "furnish", "number": true }]), None).unwrap();
        let numbered = Bundle { deck: write.unwrap().deck, ..b.clone() };
        assert_eq!(states(&numbered, hide), ["revenue", "mix"]);
        // The title slide, which Dusk leaves them off, draws them when it asks.
        let show = json!([{ "op": "set_state", "id": "intro", "prop": "furniture", "value": true }]);
        assert_eq!(states(&numbered, show), ["intro"]);
    }
}

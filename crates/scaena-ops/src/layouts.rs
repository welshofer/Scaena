//! The layouts an editor suggests for a state (PLAN 2.92): each of the theme's layouts the state
//! may take, as `set_state` gives it one (`choices::state_choices`: a slot for each node placed
//! in one, in each state the choice reaches), with the state laid out in it, linted, and drawn.
//! They come fewest errors first, then fewest warnings, the layout the state takes now first of
//! those lint judges alike, then in the theme's order. Nothing is written: each comes with the
//! patch that gives the state its layout, written where the layout lives.
//!
//! A suggestion shows the state otherwise, or it is no suggestion. A layout is its slots: a state
//! that places no node in one is offered none. And a layout that draws the state as one before it
//! does, with what lint finds alike, is that one's `alike`, not a suggestion of its own: two
//! layouts whose slots differ only where the state's nodes do not reach draw it alike.

use crate::lint::{View, data_in, engine_in, layout_rules, lint_with};
use crate::patch::changed;
use crate::{Bundle, Context, OpsError};
use scaena_core::choices::{Takes, state_choices};
use scaena_core::model::theme::Theme;
use scaena_core::validate::BundleFiles;
use scaena_core::{Deck, Finding, Severity};
use scaena_engine::FrameRequest;
use scaena_paint::Assets;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::{Value, json};

/// One layout a state may take, and what lint finds in the state laid out in it.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Suggestion {
    pub layout: String,
    /// The layout the state takes now.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub current: bool,
    /// What lint finds in the state laid out in it, in every format the deck lists.
    pub errors: usize,
    pub warnings: usize,
    /// The layouts that draw the state as this one does, where it is drawn, with what lint
    /// finds alike: choosing one shows nothing new, so each is left out, in the theme's order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alike: Vec<String>,
    /// The states the patch changes, by id: a layout lives where it is set, and each state
    /// that takes it from there takes the new one. None for the layout the state takes now.
    pub reach: Vec<String>,
    /// The patch that gives the state this layout: one `set_state`, or none for the layout
    /// it takes now.
    pub patch: Vec<Value>,
}

/// A layout a state may take, with the deck its patch makes: what lint and a frame judge.
#[derive(Debug, Clone)]
pub struct Candidate {
    /// Its counts are none until it is [`counted`].
    pub suggestion: Suggestion,
    pub deck: Deck,
}

/// The layouts `state` may take in `theme`, in the theme's order, each with its patch made;
/// none where it places no node in a slot.
pub fn candidates(
    deck: &Deck,
    theme: &Theme,
    files: &dyn BundleFiles,
    state: &str,
) -> Result<Vec<Candidate>, OpsError> {
    let choices = state_choices(deck, theme, state).map_err(OpsError::new)?;
    let snapshots = scaena_core::resolve_states(deck).context("tracking")?;
    let slotted = |props: &scaena_core::document::Props| {
        let slot = props.get("at").and_then(|at| at.get("in")).and_then(Value::as_str);
        slot.is_some_and(|slot| !matches!(slot, "canvas" | "grid"))
    };
    if !snapshots.iter().any(|s| s.state_id == state && s.nodes.values().any(slotted)) {
        return Ok(Vec::new());
    }
    let Some(field) = choices.fields.iter().find(|f| f.prop == "layout") else { return Ok(Vec::new()) };
    let Takes::Name { names, .. } = &field.takes else { return Ok(Vec::new()) };
    let now = field.value.as_ref().and_then(Value::as_str);
    let doc = deck.to_value()?;
    let mut found = Vec::with_capacity(names.len());
    for layout in names {
        let current = now == Some(layout.as_str());
        let patch = match current {
            true => Vec::new(),
            false => vec![json!({ "op": "set_state", "id": state, "prop": "layout", "value": layout })],
        };
        let made = match current {
            true => deck.clone(),
            false => {
                let compiled = scaena_core::patch::compile(&doc, &patch, files).map_err(|e| OpsError {
                    message: e.to_string(),
                    plan: None,
                    op: Some(e.index),
                })?;
                Deck::from_value(&compiled.doc).map_err(OpsError::new)?
            }
        };
        let reach = if current { Vec::new() } else { changed(deck, &made)? };
        let (errors, warnings, alike) = (0, 0, Vec::new());
        let suggestion = Suggestion { layout: layout.clone(), current, errors, warnings, alike, reach, patch };
        found.push(Candidate { suggestion, deck: made });
    }
    Ok(found)
}

/// `suggestion` with what lint finds in `state` counted: `findings`' errors and warnings there.
pub fn counted(suggestion: Suggestion, state: &str, findings: &[Finding]) -> Suggestion {
    let here =
        |severity| findings.iter().filter(|f| f.state.as_deref() == Some(state) && f.severity == severity).count();
    Suggestion { errors: here(Severity::Error), warnings: here(Severity::Warning), ..suggestion }
}

/// `judged`, in the theme's order, each counted and with its drawing: best first, and each
/// drawn as one before it, with the same counts, folded into that one's `alike`.
pub fn ranked<D: PartialEq>(judged: Vec<(Suggestion, D)>) -> Vec<(Suggestion, D)> {
    let mut found: Vec<(usize, Suggestion, D)> = judged.into_iter().enumerate().map(|(i, (s, d))| (i, s, d)).collect();
    // Of layouts lint judges alike, the one the state takes now comes first.
    let rank = |order: &usize, s: &Suggestion| (s.errors, s.warnings, !s.current, *order);
    scaena_core::sort::by(&mut found, |(a, x, _), (b, y, _)| rank(a, x).cmp(&rank(b, y)));
    let mut kept: Vec<(Suggestion, D)> = Vec::with_capacity(found.len());
    for (_, suggestion, drawn) in found {
        let counts = |s: &Suggestion| (s.errors, s.warnings);
        match kept.iter_mut().find(|(k, d)| counts(k) == counts(&suggestion) && *d == drawn) {
            Some((like, _)) => like.alike.push(suggestion.layout),
            None => kept.push((suggestion, drawn)),
        }
    }
    kept
}

/// The layouts `state` may take in `theme`, best first, each with its drawing; none where it
/// places no node in a slot. `judge` lints a deck and draws `state` in it: what lint finds in
/// `state` counts, and a layout drawn as one before it, with the same counts, is that one's
/// `alike`.
pub fn suggesting<D: PartialEq>(
    deck: &Deck,
    theme: &Theme,
    files: &dyn BundleFiles,
    state: &str,
    mut judge: impl FnMut(&Deck) -> Result<(Vec<Finding>, D), OpsError>,
) -> Result<Vec<(Suggestion, D)>, OpsError> {
    let mut judged = Vec::new();
    for candidate in candidates(deck, theme, files, state)? {
        let (findings, drawn) = judge(&candidate.deck)?;
        judged.push((counted(candidate.suggestion, state, &findings), drawn));
    }
    Ok(ranked(judged))
}

/// The layouts `state` may take in the bundle's theme, best first, each judged by lint as
/// [`lint_state_in`](crate::lint::lint_state_in) judges the bundle with its patch made, and
/// drawn at rest in `format`, or on the deck's canvas (`scaena inspect --layouts`).
pub fn suggest(b: &Bundle, state: &str, format: Option<&str>) -> Result<Vec<Suggestion>, OpsError> {
    let (files, theme) = (View::of(b), crate::theme(b)?);
    let json = files.theme(&b.deck)?;
    // A layout names no font or image: one engine lays out the state in each.
    let mut assets = Assets::new();
    let mut engine = engine_in(&b.deck, &files, &theme, Some(&mut assets))?;
    let data = data_in(&b.deck, &files)?;
    let judged = suggesting(&b.deck, &theme, &files, state, |made| {
        let linted = lint_with(made, &files, json.as_deref(), |theme| {
            layout_rules(&mut engine, made, theme, &data, &assets, Some(state))
        })?;
        let req = FrameRequest { deck: made, theme: &theme, data: &data, state, t_ms: f64::INFINITY, format };
        Ok((linted.findings, engine.frame(&req)?.display_list))
    })?;
    Ok(judged.into_iter().map(|(suggestion, _)| suggestion).collect())
}

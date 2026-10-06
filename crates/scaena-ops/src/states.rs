//! The state strip's patches (PLAN 2.35, ADR-0013): a state added after the one shown, as a
//! step of its slide or as a slide of its own, with an id new to the deck, in the beat of the
//! state it follows. Moving, renaming, and removing a state are the ops themselves
//! (`move_state`, `rename_state`, `remove_state`, SPEC §7.3).

use crate::{Context, OpsError};
use scaena_core::Deck;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// What the strip adds after the state shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Adding {
    /// A step of its slide, right after it, tracking from it: it shows what that state shows
    /// until it is changed.
    Step,
    /// A slide of its own, after the last step of the state's slide: empty (`absolute`), in
    /// the state's layout.
    Slide,
}

/// A state a patch adds: its id, and the patch.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct AddedState {
    pub id: String,
    /// One `add_state`.
    pub patch: Vec<Value>,
}

/// The patch that adds a state after `shown` (PLAN 2.35). A step is named after the state it
/// follows (`revenue-2`, after `revenue`), a slide `slide`, `slide-2`, …: the first that names
/// no state.
pub fn adding(deck: &Deck, shown: &str, what: Adding) -> Result<AddedState, OpsError> {
    let i = deck
        .states
        .iter()
        .position(|s| s.id == shown)
        .ok_or_else(|| OpsError::new(format!("unknown state `{shown}`")))?;
    let slide = deck.states[i].slide.clone().unwrap_or_else(|| shown.to_string());
    let (id, after, state) = match what {
        Adding::Step => {
            let id = fresh(deck, stem(shown));
            (id.clone(), i, json!({ "id": id, "slide": slide }))
        }
        Adding::Slide => {
            let steps = deck.states[i + 1..].iter().take_while(|s| s.slide.as_deref() == Some(slide.as_str())).count();
            let snaps = scaena_core::resolve_states(deck).context("tracking")?;
            let id = fresh(deck, "slide");
            let mut state = json!({ "id": id, "mode": "absolute" });
            if let Some(layout) = &snaps[i].layout {
                state["layout"] = json!(layout);
            }
            (id, i + steps, state)
        }
    };
    let after = &deck.states[after].id;
    let mut op = json!({ "op": "add_state", "state": state, "after": after });
    let beats = deck.spine.iter().flat_map(|s| &s.sections).flat_map(|s| &s.beats);
    if let Some(beat) = beats.into_iter().find(|b| b.states.contains(after)) {
        op["beat"] = json!(beat.id);
    }
    Ok(AddedState { id, patch: vec![op] })
}

/// `id` without a step's number: `revenue` of `revenue-2`.
fn stem(id: &str) -> &str {
    match id.rsplit_once('-') {
        Some((stem, n)) if !stem.is_empty() && !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => stem,
        _ => id,
    }
}

/// The first of `base`, `base-2`, `base-3`, … that names no state of `deck`.
fn fresh(deck: &Deck, base: &str) -> String {
    let taken = |id: &str| deck.states.iter().any(|s| s.id == id);
    (1..)
        .map(|n| if n == 1 { base.to_string() } else { format!("{base}-{n}") })
        .find(|id| !taken(id))
        .expect("some number is free")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn revenue() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn a_step_follows_the_state_shown_in_its_slide_and_a_slide_follows_the_slide() {
        let deck = revenue();
        // `mix` builds on `revenue`'s slide: a step after `revenue` joins it, in the beat.
        let step = adding(&deck, "revenue", Adding::Step).unwrap();
        assert_eq!(step.id, "revenue-2");
        assert_eq!(
            step.patch,
            [
                json!({ "op": "add_state", "state": { "id": "revenue-2", "slide": "revenue" }, "after": "revenue", "beat": "doubled" })
            ]
        );
        // A slide goes after `mix`, the slide's last step, empty, in its layout.
        let slide = adding(&deck, "revenue", Adding::Slide).unwrap();
        assert_eq!(
            slide.patch,
            [
                json!({ "op": "add_state", "state": { "id": "slide", "mode": "absolute", "layout": "figure" }, "after": "mix", "beat": "doubled" })
            ]
        );
        assert_eq!(stem("revenue-2"), "revenue");
        assert_eq!(stem("q3-2026"), "q3");
        assert_eq!(stem("-2"), "-2");
        assert!(adding(&deck, "nowhere", Adding::Step).is_err());
    }
}

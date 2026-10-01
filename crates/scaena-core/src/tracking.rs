//! State tracking (SPEC §2.2): resolve the ordered cue list into absolute snapshots.
//!
//! A state declares deltas. Unchanged properties track forward from the previous
//! state (or from `from`). `mode: absolute` starts from nothing. `remove` exits
//! nodes. Per-state-only keys (currently `anim`) never track: an entrance track
//! must not replay on the next cue.
//!
//! Resolution is deterministic and depends only on the document.

use crate::document::{Deck, Props, State, StateMode};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

/// Property keys that belong to exactly one state and never track forward.
pub const NON_TRACKING_KEYS: &[&str] = &["anim"];

#[derive(Debug, Error, PartialEq)]
pub enum TrackingError {
    #[error("state `{state}` tracks from unknown state `{from}`")]
    UnknownFrom { state: String, from: String },
    #[error("state `{state}` tracks from `{from}`, which is not an earlier state")]
    ForwardFrom { state: String, from: String },
    #[error("state `{state}` references unknown node `{node}`")]
    UnknownNode { state: String, node: String },
}

/// A state resolved to absolute values: every visible node with its merged props.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Snapshot {
    pub state_id: String,
    pub slide_id: String,
    pub layout: Option<String>,
    /// Visible nodes in scene-graph order, with node defaults merged under state deltas.
    pub nodes: IndexMap<String, Props>,
    /// Nodes that became visible in this state.
    pub entered: Vec<String>,
    /// Nodes that stopped being visible in this state.
    pub exited: Vec<String>,
}

/// Resolve every state of `deck` into a [`Snapshot`], in order.
pub fn resolve_states(deck: &Deck) -> Result<Vec<Snapshot>, TrackingError> {
    let mut out: Vec<Snapshot> = Vec::with_capacity(deck.states.len());
    for (i, state) in deck.states.iter().enumerate() {
        let (base, base_layout): (IndexMap<String, Props>, Option<String>) = match (state.mode, &state.from) {
            (StateMode::Absolute, _) => (IndexMap::new(), None),
            (StateMode::Delta, Some(from)) => {
                let j = deck
                    .state_index(from)
                    .ok_or_else(|| TrackingError::UnknownFrom { state: state.id.clone(), from: from.clone() })?;
                if j >= i {
                    return Err(TrackingError::ForwardFrom { state: state.id.clone(), from: from.clone() });
                }
                (strip_non_tracking(&out[j].nodes), out[j].layout.clone())
            }
            (StateMode::Delta, None) => match out.last() {
                Some(prev) => (strip_non_tracking(&prev.nodes), prev.layout.clone()),
                None => (IndexMap::new(), None),
            },
        };
        let snap = apply_state(deck, state, base, base_layout, out.last())?;
        out.push(snap);
    }
    Ok(out)
}

fn strip_non_tracking(nodes: &IndexMap<String, Props>) -> IndexMap<String, Props> {
    nodes
        .iter()
        .map(|(id, props)| {
            let mut p = props.clone();
            for k in NON_TRACKING_KEYS {
                p.shift_remove(*k);
            }
            (id.clone(), p)
        })
        .collect()
}

fn apply_state(
    deck: &Deck,
    state: &State,
    mut nodes: IndexMap<String, Props>,
    base_layout: Option<String>,
    prev: Option<&Snapshot>,
) -> Result<Snapshot, TrackingError> {
    for id in &state.remove {
        if !deck.nodes.contains_key(id) {
            return Err(TrackingError::UnknownNode { state: state.id.clone(), node: id.clone() });
        }
        nodes.shift_remove(id);
    }
    for (id, delta) in &state.props {
        let Some(node) = deck.nodes.get(id) else {
            return Err(TrackingError::UnknownNode { state: state.id.clone(), node: id.clone() });
        };
        let entry = nodes.entry(id.clone()).or_insert_with(|| node.props.clone());
        merge_props(entry, delta);
    }
    // Keep scene-graph order (nodes map order) so painters get a stable z-list.
    let ordered: IndexMap<String, Props> =
        deck.nodes.keys().filter_map(|id| nodes.get(id).map(|p| (id.clone(), p.clone()))).collect();

    let prev_ids: Vec<&String> = prev.map(|p| p.nodes.keys().collect()).unwrap_or_default();
    let entered = ordered.keys().filter(|id| !prev_ids.contains(id)).cloned().collect();
    let exited = prev_ids.into_iter().filter(|id| !ordered.contains_key(*id)).cloned().collect();

    Ok(Snapshot {
        state_id: state.id.clone(),
        slide_id: deck.slide_of(state).to_string(),
        // The layout template tracks like any other property: a build on the same
        // slide need not restate it.
        layout: state.layout.clone().or(base_layout),
        nodes: ordered,
        entered,
        exited,
    })
}

/// Shallow-merge `delta` into `base`: top-level keys replace; object values merge
/// one level (so `at: {col}` can override just `col`); `null` deletes a key.
pub fn merge_props(base: &mut Props, delta: &Props) {
    for (k, v) in delta {
        match v {
            Value::Null => {
                base.shift_remove(k);
            }
            Value::Object(dm) => match base.get_mut(k) {
                Some(Value::Object(bm)) => {
                    for (dk, dv) in dm {
                        if dv.is_null() {
                            bm.remove(dk);
                        } else {
                            bm.insert(dk.clone(), dv.clone());
                        }
                    }
                }
                _ => {
                    base.insert(k.clone(), v.clone());
                }
            },
            _ => {
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn example() -> Deck {
        Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap()
    }

    #[test]
    fn tracks_forward_and_removes() {
        let deck = example();
        let snaps = resolve_states(&deck).unwrap();
        assert_eq!(snaps.len(), 4);
        // intro: bg, title, subtitle
        assert_eq!(snaps[0].nodes.keys().collect::<Vec<_>>(), vec!["bg", "title", "subtitle"]);
        assert_eq!(snaps[0].entered, vec!["bg", "title", "subtitle"]);
        // revenue: subtitle removed, rev + note entered, title text overridden
        assert!(!snaps[1].nodes.contains_key("subtitle"));
        assert_eq!(snaps[1].exited, vec!["subtitle"]);
        assert_eq!(snaps[1].entered, vec!["rev", "note"]);
        assert_eq!(snaps[1].nodes["title"]["text"], json!("Revenue doubled"));
        assert_eq!(snaps[1].nodes["title"]["at"], json!({"in": "header"}));
        // mix: rev kind changed, bg tracked all the way from intro
        assert_eq!(snaps[2].nodes["rev"]["kind"], json!("stackedBar"));
        assert_eq!(snaps[2].nodes["rev"]["data"], json!("@q3"));
        assert_eq!(snaps[2].nodes["bg"]["seed"], json!(7));
        assert_eq!(snaps[2].slide_id, "revenue");
        assert_eq!(snaps[2].layout.as_deref(), Some("full"), "layout tracks forward");
        // close: rev/note removed
        assert_eq!(snaps[3].nodes.keys().collect::<Vec<_>>(), vec!["bg", "title"]);
    }

    #[test]
    fn nested_objects_merge_one_level() {
        let mut base: Props = serde_json::from_value(json!({"at": {"col": [1, 6], "row": 2}})).unwrap();
        let delta: Props = serde_json::from_value(json!({"at": {"col": [7, 12], "align": "center"}})).unwrap();
        merge_props(&mut base, &delta);
        assert_eq!(base["at"], json!({"col": [7, 12], "row": 2, "align": "center"}));
        let del: Props = serde_json::from_value(json!({"at": {"row": null}})).unwrap();
        merge_props(&mut base, &del);
        assert_eq!(base["at"], json!({"col": [7, 12], "align": "center"}));
    }

    #[test]
    fn anim_does_not_track() {
        let mut deck = example();
        deck.states[0].props.get_mut("title").unwrap().insert("anim".into(), json!({"opacity": [{"t": 0, "v": 0}]}));
        let snaps = resolve_states(&deck).unwrap();
        assert!(snaps[0].nodes["title"].contains_key("anim"));
        assert!(!snaps[1].nodes["title"].contains_key("anim"));
    }

    #[test]
    fn from_must_be_earlier() {
        let mut deck = example();
        deck.states[1].from = Some("close".into());
        assert_eq!(
            resolve_states(&deck).unwrap_err(),
            TrackingError::ForwardFrom { state: "revenue".into(), from: "close".into() }
        );
    }
}

//! State tracking (SPEC §2.2): resolve the ordered cue list into absolute snapshots.
//!
//! A state declares deltas. Unchanged properties track forward from the previous
//! state (or from `from`). `mode: absolute` starts from nothing. `remove` exits
//! nodes. Per-state-only keys (`anim` and `emphasis`) never track: a motion must
//! not replay on the next cue.
//!
//! Resolution is deterministic and depends only on the document.

use crate::document::{Deck, Props, State, StateMode};
use indexmap::IndexMap;
use serde::Serialize;
use serde_json::Value;
use thiserror::Error;

/// Property keys that belong to exactly one state and never track forward.
pub const NON_TRACKING_KEYS: &[&str] = &["anim", "emphasis"];

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
#[derive(Debug, Clone, Serialize, PartialEq, schemars::JsonSchema)]
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
        if let (StateMode::Delta, Some(from)) = (state.mode, &state.from) {
            let j = deck
                .state_index(from)
                .ok_or_else(|| TrackingError::UnknownFrom { state: state.id.clone(), from: from.clone() })?;
            if j >= i {
                return Err(TrackingError::ForwardFrom { state: state.id.clone(), from: from.clone() });
            }
        }
        let (base, base_layout): (IndexMap<String, Props>, Option<String>) = match tracks_from(deck, i) {
            Some(j) => (strip_non_tracking(&out[j].nodes), out[j].layout.clone()),
            None => (IndexMap::new(), None),
        };
        let snap = apply_state(deck, state, base, base_layout, out.last())?;
        out.push(snap);
    }
    Ok(out)
}

/// The state that `deck.states[i]` tracks from: its `from`, else the state before it. None
/// in absolute mode, or for the first state.
pub fn tracks_from(deck: &Deck, i: usize) -> Option<usize> {
    let state = &deck.states[i];
    match (state.mode, &state.from) {
        (StateMode::Absolute, _) => None,
        (StateMode::Delta, Some(from)) => deck.state_index(from),
        (StateMode::Delta, None) => i.checked_sub(1),
    }
}

/// Where a state gets one of a node's properties from (ADR-0013: an edit changes a value
/// where it lives).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lives {
    /// The delta of the state at this index.
    State(usize),
    /// The node's own properties, which every state shows that sets nothing over them.
    Node,
}

/// Where `deck.states[i]` gets `node`'s property `prop` from: the latest delta that sets it,
/// from state `i` back along what it tracks, else the node's own properties. With `keys`, an
/// object property is set by a delta that sets any of those keys of it, or sets it whole. A
/// node that leaves comes back with its own properties, and an absolute state starts from
/// them. The deck's `overrides`, which win over every state, are not asked.
pub fn lives(deck: &Deck, i: usize, node: &str, prop: &str, keys: &[&str]) -> Lives {
    let mut at = Some(i);
    while let Some(j) = at {
        let state = &deck.states[j];
        if let Some(value) = state.props.get(node).and_then(|delta| delta.get(prop)) {
            let sets = match value {
                Value::Object(set) if !keys.is_empty() => keys.iter().any(|k| set.contains_key(*k)),
                _ => true,
            };
            if sets {
                return Lives::State(j);
            }
        }
        if state.remove.iter().any(|id| id == node) {
            return Lives::Node;
        }
        at = tracks_from(deck, j);
    }
    Lives::Node
}

/// Where `deck.states[i]` gets its layout template from, which tracks like a property: the
/// latest state that sets one, from state `i` back along what it tracks. `None` where none
/// does.
pub fn layout_lives(deck: &Deck, i: usize) -> Option<usize> {
    let mut at = Some(i);
    while let Some(j) = at {
        if deck.states[j].layout.is_some() {
            return Some(j);
        }
        // A state tracks from one before it; a `from` that does not is an error resolving.
        at = tracks_from(deck, j).filter(|&k| k < j);
    }
    None
}

/// The states that take their layout from state `w`, were it to set one: `w`, and each state
/// whose way back along what it tracks reaches `w` before a state that sets one.
pub fn layout_takers(deck: &Deck, w: usize) -> Vec<usize> {
    (0..deck.states.len())
        .filter(|&j| {
            let mut at = Some(j);
            while let Some(k) = at {
                if k == w {
                    return true;
                }
                if deck.states[k].layout.is_some() {
                    return false;
                }
                at = tracks_from(deck, k).filter(|&back| back < k);
            }
            false
        })
        .collect()
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
/// one level (so `at: {col}` can override just `col`); `null` deletes a key. An object
/// with nothing to merge into is taken as it is, less the keys it deletes. Deletes keep
/// the order of what remains. A text's `text` and `runs` are one property written two
/// ways: a delta that sets either takes the other away.
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
                            bm.shift_remove(dk);
                        } else {
                            bm.insert(dk.clone(), dv.clone());
                        }
                    }
                }
                _ => {
                    let kept = dm.iter().filter(|(_, dv)| !dv.is_null()).map(|(dk, dv)| (dk.clone(), dv.clone()));
                    base.insert(k.clone(), Value::Object(kept.collect()));
                }
            },
            _ => {
                if let Some(other) = other_spelling(k) {
                    base.shift_remove(other);
                }
                base.insert(k.clone(), v.clone());
            }
        }
    }
}

/// What a state in delta mode declares to resolve as `to` when it tracks from `from` (PLAN
/// 2.97): a delta for each node it shows, and the nodes it takes away. A node `from` does not
/// show enters with its own properties, so its delta is what `to` changes of them; `anim` and
/// `emphasis` never track, so they are the state's own. [`merge_props`] of each delta over
/// `from` gives `to`.
pub fn delta_to(deck: &Deck, from: &Snapshot, to: &Snapshot) -> (IndexMap<String, Props>, Vec<String>) {
    let base = strip_non_tracking(&from.nodes);
    let mut props = IndexMap::new();
    for (id, now) in &to.nodes {
        match base.get(id) {
            Some(was) => {
                let delta = diff(was, now);
                if !delta.is_empty() {
                    props.insert(id.clone(), delta);
                }
            }
            // It enters: its delta, even an empty one, shows it.
            None => {
                let own = deck.nodes.get(id).map(|n| n.props.clone()).unwrap_or_default();
                props.insert(id.clone(), diff(&own, now));
            }
        }
    }
    let remove = base.keys().filter(|id| !to.nodes.contains_key(*id)).cloned().collect();
    (props, remove)
}

/// What `to` sets over `from`, as [`merge_props`] reads a delta: each key that differs, an
/// object's keys one level down, and `null` for each key `to` lacks.
fn diff(from: &Props, to: &Props) -> Props {
    let mut out = Props::new();
    for (key, now) in to {
        match (from.get(key), now) {
            (Some(was), _) if was == now => {}
            (Some(Value::Object(was)), Value::Object(now)) => {
                let mut set = serde_json::Map::new();
                for (k, v) in now {
                    if was.get(k) != Some(v) {
                        set.insert(k.clone(), v.clone());
                    }
                }
                for k in was.keys().filter(|k| !now.contains_key(*k)) {
                    set.insert(k.clone(), Value::Null);
                }
                out.insert(key.clone(), Value::Object(set));
            }
            _ => {
                out.insert(key.clone(), now.clone());
            }
        }
    }
    for key in from.keys().filter(|k| !to.contains_key(*k)) {
        out.insert(key.clone(), Value::Null);
    }
    out
}

/// `states`, some of `deck`'s in a new order with states new to it among them, each showing
/// what it showed (PLAN 2.97). A state of `deck` in delta mode that would track from another
/// state than it did, or whose `from` is gone or no longer before it, has its delta written
/// again, without `from`, to resolve as it did from the state before it; where it takes its
/// layout from that state, it sets its own. A state new to `deck` stays as it is, and `like`
/// names the state of `deck` each resolves as. The states are resolved after to prove it: one
/// that would show otherwise is an error that says which.
pub fn keep_looks(deck: &Deck, mut states: Vec<State>, like: &[(String, String)]) -> Result<Vec<State>, String> {
    let before = resolve_states(deck).map_err(|e| e.to_string())?;
    let looked = |id: &str| -> Option<&Snapshot> {
        let id = like.iter().find(|(new, _)| new == id).map_or(id, |(_, old)| old.as_str());
        before.iter().find(|s| s.state_id == id)
    };
    let tracked = |id: &str| -> Option<&str> {
        let i = deck.state_index(id)?;
        tracks_from(deck, i).map(|j| deck.states[j].id.as_str())
    };
    for k in 0..states.len() {
        let state = &states[k];
        if state.mode == StateMode::Absolute || deck.state_index(&state.id).is_none() {
            continue;
        }
        let earlier = |id: &String| states[..k].iter().any(|s| &s.id == id);
        let from = match &state.from {
            Some(from) if earlier(from) => Some(from.clone()),
            _ => k.checked_sub(1).map(|j| states[j].id.clone()),
        };
        let kept = state.from.as_ref().is_none_or(earlier);
        if kept && from.as_deref() == tracked(&state.id) {
            continue;
        }
        let to = looked(&state.id).ok_or_else(|| format!("no state `{}`", state.id))?.clone();
        let (props, remove, layout) = match from.as_deref().and_then(looked) {
            Some(base) => {
                let (props, remove) = delta_to(deck, base, &to);
                let layout = if base.layout == to.layout { None } else { to.layout.clone() };
                (props, remove, layout)
            }
            // The first state tracks from nothing: each node it shows enters.
            None => {
                let empty = Snapshot { nodes: IndexMap::new(), ..to.clone() };
                let (props, remove) = delta_to(deck, &empty, &to);
                (props, remove, to.layout.clone())
            }
        };
        let state = &mut states[k];
        state.from = None;
        state.props = props;
        state.remove = remove;
        state.layout = state.layout.take().or(layout);
    }
    let made = Deck { states: states.clone(), ..deck.clone() };
    let after = resolve_states(&made).map_err(|e| e.to_string())?;
    for snap in &after {
        let was = looked(&snap.state_id);
        if was.is_some_and(|was| was.nodes != snap.nodes || was.layout != snap.layout) {
            return Err(format!("`{}` would not show what it showed", snap.state_id));
        }
    }
    Ok(states)
}

/// The other way a text's words are written: `runs` for `text`, `text` for `runs`.
pub fn other_spelling(prop: &str) -> Option<&'static str> {
    match prop {
        "text" => Some("runs"),
        "runs" => Some("text"),
        _ => None,
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
        // revenue: subtitle and background removed, rev + note entered, title text overridden
        assert!(!snaps[1].nodes.contains_key("subtitle"));
        assert_eq!(snaps[1].exited, vec!["bg", "subtitle"]);
        assert_eq!(snaps[1].entered, vec!["rev", "note"]);
        assert_eq!(snaps[1].nodes["title"]["text"], json!("Revenue doubled"));
        assert_eq!(snaps[1].nodes["title"]["at"], json!({"in": "header"}));
        // mix: rev kind changed, the layout tracked forward, the background still gone
        assert_eq!(snaps[2].nodes["rev"]["kind"], json!("stackedBar"));
        assert_eq!(snaps[2].nodes["rev"]["data"], json!("@q3"));
        assert!(!snaps[2].nodes.contains_key("bg"), "a removed node stays removed");
        assert_eq!(snaps[2].slide_id, "revenue");
        assert_eq!(snaps[2].layout.as_deref(), Some("figure"), "layout tracks forward");
        // close: rev/note removed; the background re-enters from its node defaults
        assert_eq!(snaps[3].nodes.keys().collect::<Vec<_>>(), vec!["bg", "title"]);
        assert_eq!(snaps[3].entered, vec!["bg"]);
        assert_eq!(snaps[3].nodes["bg"]["seed"], json!(7));
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
    fn text_and_runs_are_one_property() {
        let d = deck(
            json!({ "t": { "type": "text", "runs": [{ "text": "bold", "style": { "weight": 700 } }] } }),
            json!([
                { "id": "a", "props": { "t": {} } },
                { "id": "b", "props": { "t": { "text": "plain" } } },
                { "id": "c", "props": { "t": { "runs": [{ "text": "low", "emphasis": "low" }] } } },
            ]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert!(snaps[0].nodes["t"].contains_key("runs"));
        assert_eq!(snaps[1].nodes["t"].get("runs"), None, "a state's text is shown over the runs it tracks");
        assert_eq!(snaps[1].nodes["t"]["text"], json!("plain"));
        assert_eq!(snaps[2].nodes["t"].get("text"), None, "and its runs over the text");
        assert_eq!(snaps[2].nodes["t"]["runs"], json!([{ "text": "low", "emphasis": "low" }]));
    }

    #[test]
    fn anim_and_emphasis_do_not_track() {
        let mut deck = example();
        let title = deck.states[0].props.get_mut("title").unwrap();
        title.insert("anim".into(), json!({"opacity": [{"t": 0, "v": 0}]}));
        title.insert("emphasis".into(), json!("pulse"));
        let snaps = resolve_states(&deck).unwrap();
        for key in ["anim", "emphasis"] {
            assert!(snaps[0].nodes["title"].contains_key(key), "{key} in its own state");
            assert!(!snaps[1].nodes["title"].contains_key(key), "{key} in the next");
        }
    }

    /// A deck of `nodes` and `states`, written as JSON.
    fn deck(nodes: serde_json::Value, states: serde_json::Value) -> Deck {
        let doc = json!({ "scaena": crate::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "nodes": nodes, "states": states });
        serde_json::from_value(doc).unwrap()
    }

    #[test]
    fn an_object_with_nothing_to_merge_into_keeps_none_of_its_nulls() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x" } }),
            json!([{ "id": "a", "props": { "t": { "at": { "in": null, "col": [1, 6] } } } }]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert_eq!(snaps[0].nodes["t"]["at"], json!({ "col": [1, 6] }));
    }

    #[test]
    fn null_deletes_and_the_deletion_tracks() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x", "alt": "a caption" } }),
            json!([
                { "id": "a", "props": { "t": {} } },
                { "id": "b", "props": { "t": { "alt": null } } },
                { "id": "c" }
            ]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert_eq!(snaps[0].nodes["t"]["alt"], json!("a caption"));
        assert!(!snaps[1].nodes["t"].contains_key("alt"));
        assert!(!snaps[2].nodes["t"].contains_key("alt"), "a deletion tracks like any change");
    }

    #[test]
    fn a_state_with_no_changes_repeats_the_one_before() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x" } }),
            json!([{ "id": "a", "props": { "t": { "text": "y" } } }, { "id": "b" }]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert_eq!(snaps[1].nodes, snaps[0].nodes);
        assert!(snaps[1].entered.is_empty() && snaps[1].exited.is_empty());
    }

    #[test]
    fn a_node_that_reenters_starts_from_its_defaults() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x", "opacity": 1 } }),
            json!([
                { "id": "a", "props": { "t": { "text": "changed", "opacity": 0.5 } } },
                { "id": "b", "remove": ["t"] },
                { "id": "c", "props": { "t": { "opacity": 0.8 } } }
            ]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert!(!snaps[1].nodes.contains_key("t"));
        assert_eq!(snaps[1].exited, ["t"]);
        assert_eq!(snaps[2].nodes["t"]["text"], json!("x"), "not the text it had when it left");
        assert_eq!(snaps[2].nodes["t"]["opacity"], json!(0.8));
        assert_eq!(snaps[2].entered, ["t"]);
    }

    #[test]
    fn from_branches_from_an_earlier_state() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x" }, "u": { "type": "text", "text": "u" } }),
            json!([
                { "id": "a", "layout": "title", "props": { "t": { "text": "a" } } },
                { "id": "b", "layout": "full", "remove": ["t"], "props": { "u": {} } },
                { "id": "c", "from": "a", "props": { "t": { "opacity": 0.5 } } }
            ]),
        );
        let snaps = resolve_states(&d).unwrap();
        // `c` is `a` plus its delta, layout included: `b` does not reach it.
        assert_eq!(snaps[2].nodes.keys().collect::<Vec<_>>(), ["t"]);
        assert_eq!(snaps[2].nodes["t"]["text"], json!("a"));
        assert_eq!(snaps[2].nodes["t"]["opacity"], json!(0.5));
        assert_eq!(snaps[2].layout.as_deref(), Some("title"));
        // What enters and exits is against what was on screen: `b`.
        assert_eq!(snaps[2].entered, ["t"]);
        assert_eq!(snaps[2].exited, ["u"]);
    }

    #[test]
    fn a_value_lives_in_the_latest_delta_a_state_tracks_or_in_the_node() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x", "at": { "in": "title" } } }),
            json!([
                { "id": "a", "props": { "t": { "text": "a", "at": { "align": "center" } } } },
                { "id": "b", "props": { "t": { "at": { "in": "header" } } } },
                { "id": "c" },
                { "id": "d", "from": "a" },
                { "id": "e", "remove": ["t"] },
                { "id": "f", "props": { "t": {} } },
                { "id": "g", "mode": "absolute", "props": { "t": { "at": { "col": 2 } } } }
            ]),
        );
        let place = |i: usize| lives(&d, i, "t", "at", &["in", "col"]);
        // `a` sets `at`, but none of its keys that place: the node's own do.
        assert_eq!(
            [place(0), place(1), place(2), place(3)],
            [Lives::Node, Lives::State(1), Lives::State(1), Lives::Node]
        );
        // Back after leaving, from its own; in an absolute state, from that state.
        assert_eq!([place(5), place(6)], [Lives::Node, Lives::State(6)]);
        // Without keys, a delta that sets the property at all.
        assert_eq!(lives(&d, 3, "t", "at", &[]), Lives::State(0));
        assert_eq!(lives(&d, 2, "t", "text", &[]), Lives::State(0));
    }

    #[test]
    fn absolute_shows_exactly_its_own_props() {
        let d = deck(
            json!({ "t": { "type": "text", "text": "x" }, "u": { "type": "text", "text": "u" } }),
            json!([
                { "id": "a", "layout": "title", "props": { "t": { "text": "a" }, "u": {} } },
                { "id": "b", "mode": "absolute", "props": { "t": { "opacity": 0.5 } } }
            ]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert_eq!(snaps[1].nodes.keys().collect::<Vec<_>>(), ["t"]);
        assert_eq!(snaps[1].nodes["t"]["text"], json!("x"), "node defaults, not what `a` set");
        assert_eq!(snaps[1].nodes["t"]["opacity"], json!(0.5));
        assert_eq!(snaps[1].layout, None, "nothing tracks, the layout included");
        assert_eq!(snaps[1].exited, ["u"]);
    }

    #[test]
    fn scene_graph_order_not_cue_order() {
        let d = deck(
            json!({ "back": { "type": "group" }, "front": { "type": "group" } }),
            json!([{ "id": "a", "props": { "front": {} } }, { "id": "b", "props": { "back": {} } }]),
        );
        let snaps = resolve_states(&d).unwrap();
        assert_eq!(snaps[1].nodes.keys().collect::<Vec<_>>(), ["back", "front"]);
    }

    /// `delta` over the snapshot `from`, as resolving applies a state's.
    fn applied(
        deck: &Deck,
        from: &Snapshot,
        props: &IndexMap<String, Props>,
        remove: &[String],
    ) -> IndexMap<String, Props> {
        let mut nodes = strip_non_tracking(&from.nodes);
        for id in remove {
            nodes.shift_remove(id);
        }
        for (id, delta) in props {
            merge_props(nodes.entry(id.clone()).or_insert_with(|| deck.nodes[id].props.clone()), delta);
        }
        nodes
    }

    /// A delta written from two snapshots resolves as the second (PLAN 2.97): each state of
    /// the example from the one before it, and from the one after it, the way back too.
    #[test]
    fn a_delta_between_two_snapshots_resolves_as_the_second() {
        let deck = example();
        let snaps = resolve_states(&deck).unwrap();
        for (a, b) in [(0, 1), (1, 2), (2, 3), (3, 0), (2, 0), (1, 3)] {
            let (props, remove) = delta_to(&deck, &snaps[a], &snaps[b]);
            assert_eq!(applied(&deck, &snaps[a], &props, &remove), snaps[b].nodes, "{a} to {b}");
        }
        // `mix` sets only what changes from `revenue`: the title's words and the chart's data.
        let (props, remove) = delta_to(&deck, &snaps[1], &snaps[2]);
        assert!(remove.is_empty());
        assert_eq!(props.keys().collect::<Vec<_>>(), ["title", "rev"]);
    }

    /// States moved keep their looks (PLAN 2.97): `close` before `revenue` tracks from `intro`,
    /// and `revenue` from `close`, each written again; `mix` still builds on `revenue`. A copy
    /// of `close` resolves as `close` does.
    #[test]
    fn states_moved_keep_their_looks() {
        let deck = example();
        let by = |ids: &[&str]| ids.iter().map(|id| deck.states[deck.state_index(id).unwrap()].clone()).collect();
        let moved = keep_looks(&deck, by(&["intro", "close", "revenue", "mix"]), &[]).unwrap();
        let ids: Vec<&str> = moved.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["intro", "close", "revenue", "mix"]);
        let index = |id: &str| deck.state_index(id).unwrap();
        assert_eq!(moved[0].props, deck.states[index("intro")].props, "intro is first as it was");
        assert_ne!(moved[1].props, deck.states[index("close")].props, "close is written again");
        assert_eq!(moved[3].props, deck.states[index("mix")].props, "mix builds on revenue as it did");
        // Nothing of `close` leaves that `intro` showed but the subtitle.
        assert_eq!(moved[1].remove, ["subtitle"]);

        // A copy of `close`, after it, as absolute, shows what it shows.
        let mut copy = deck.states[index("close")].clone();
        copy.id = "close-2".into();
        let mut states: Vec<State> = by(&["intro", "revenue", "mix", "close"]);
        states.push(copy);
        assert!(keep_looks(&deck, states, &[("close-2".into(), "close".into())]).is_ok());

        // `revenue` gone, `mix` tracks from `intro`, written again to show what it showed.
        let gone = keep_looks(&deck, by(&["intro", "mix", "close"]), &[]).unwrap();
        assert_eq!(gone[1].layout.as_deref(), Some("figure"), "the layout it took from revenue, its own");

        // A state with no layout cannot keep that after one with a layout: it says which.
        let mut bare = deck.clone();
        bare.states[0].layout = None;
        let moved = vec![bare.states[1].clone(), bare.states[0].clone()];
        assert_eq!(keep_looks(&bare, moved, &[]).unwrap_err(), "`intro` would not show what it showed");
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

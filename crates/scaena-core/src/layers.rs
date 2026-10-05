//! A state's layers (PLAN 2.50, ADR-0013): its nodes nested as their containers and groups hold
//! them, in the order an editor's layers panel lists them.
//!
//! - Each list starts with the topmost: siblings paint by `z`, then the deck's order of nodes,
//!   and a container under what it holds (SPEC §3.4). A stack's children are listed in the order
//!   it lays them out (`at.index`, then the deck's order), its first first: they do not overlap,
//!   so that is the order the canvas shows.
//! - Beside what the state shows stand the nodes that leave in it, and those another state of
//!   its slide shows: each where the nearest state that shows it places it, so it can be shown
//!   here (`show_node`). A node no state shows stands in every state's, where its own `at`
//!   places it: hidden where it entered, it is shown again from here.
//! - A node whose container is not listed stands on the canvas.
//! - The deck's `overrides` win in every state, as the canvas shows them.

use crate::document::{Deck, NodeType, Props};
use crate::tracking::{Snapshot, merge_props};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashSet;

/// One node of a state's layers.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Layer {
    pub node: String,
    #[serde(rename = "type")]
    pub kind: NodeType,
    /// Whether the state shows it. One it does not leaves in it, another state of its slide
    /// shows it, or no state does.
    pub shown: bool,
    /// What it holds, a container's or a group's children, listed as the state's are.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Layer>,
}

/// The layers of the state at `state` in `snapshots`, which [`crate::resolve_states`] made of
/// `deck`: what stands on the canvas, the topmost first, each with what it holds.
pub fn layers(deck: &Deck, snapshots: &[Snapshot], state: usize) -> Vec<Layer> {
    let Some(here) = snapshots.get(state) else { return vec![] };
    // A node's props as the canvas shows them: the deck's overrides on them.
    let shown = |id: &str, props: &Props| {
        let mut props = props.clone();
        if let Some(over) = deck.overrides.get(id) {
            merge_props(&mut props, over);
        }
        props
    };
    // The props each node listed is placed by, and whether the state shows it.
    let mut listed: IndexMap<&str, (Props, bool)> =
        here.nodes.iter().map(|(id, props)| (id.as_str(), (shown(id, props), true))).collect();
    // A node the state does not show, as the nearest state that shows it places it; of two as
    // near, the one before.
    let mut others: Vec<usize> = (0..snapshots.len()).filter(|&i| i != state).collect();
    crate::sort::by_key(&mut others, |&i| (i.abs_diff(state), i > state));
    let nearest = |id: &str| others.iter().find_map(|&i| snapshots[i].nodes.get(id));
    let slide = (snapshots.iter()).filter(|s| s.slide_id == here.slide_id).flat_map(|s| s.nodes.keys());
    for id in here.exited.iter().chain(slide) {
        if !listed.contains_key(id.as_str())
            && let Some(props) = nearest(id)
        {
            listed.insert(id, (shown(id, props), false));
        }
    }
    for (id, node) in &deck.nodes {
        if !listed.contains_key(id.as_str()) && !snapshots.iter().any(|s| s.nodes.contains_key(id)) {
            listed.insert(id, (shown(id, &node.props), false));
        }
    }
    // Each node once, however its containers name one another.
    let mut placed = HashSet::new();
    nest(None, &listed, deck, &mut placed)
}

/// What holds a node placed by `props`, where it is listed: none on the canvas.
fn holder<'p>(props: &'p Props, listed: &IndexMap<&str, (Props, bool)>) -> Option<&'p str> {
    let parent = props.get("at").and_then(|at| at.get("parent")).and_then(Value::as_str);
    parent.filter(|p| listed.contains_key(p))
}

/// What `parent` holds among `listed` (the canvas, for `None`), as a layers panel lists it,
/// each with what it holds.
fn nest<'a>(
    parent: Option<&str>,
    listed: &IndexMap<&'a str, (Props, bool)>,
    deck: &Deck,
    placed: &mut HashSet<&'a str>,
) -> Vec<Layer> {
    let mut held: Vec<(&str, &Props, bool)> = (listed.iter())
        .filter(|(id, (props, _))| holder(props, listed) == parent && !placed.contains(**id))
        .map(|(id, (props, shown))| (*id, props, *shown))
        .collect();
    let order = |id: &str| deck.nodes.get_index_of(id).unwrap_or(usize::MAX);
    // As the engine reads them: `z` and `at.index` whole numbers, 0 where unset.
    let z = |props: &Props| props.get("z").and_then(Value::as_i64).unwrap_or(0);
    let index = |props: &Props| props.get("at").and_then(|at| at.get("index")).and_then(Value::as_u64).unwrap_or(0);
    if parent.and_then(|p| deck.nodes.get(p)).is_some_and(|p| p.node_type == NodeType::Stack) {
        crate::sort::by_key(&mut held, |&(id, props, _)| (index(props), order(id)));
    } else {
        crate::sort::by_key(&mut held, |&(id, props, _)| (z(props), order(id)));
        held.reverse();
    }
    placed.extend(held.iter().map(|(id, _, _)| *id));
    (held.into_iter())
        .filter_map(|(id, _, shown)| {
            let kind = deck.nodes.get(id)?.node_type;
            let children = if kind.is_container() { nest(Some(id), listed, deck, placed) } else { vec![] };
            Some(Layer { node: id.to_string(), kind, shown, children })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve_states;

    /// Each layer as `node`, `node (hidden)`, or `node [what it holds]`, as listed.
    fn said(layers: &[Layer]) -> Vec<String> {
        (layers.iter())
            .map(|l| {
                let mut s = l.node.clone();
                if !l.shown {
                    s.push_str(" (hidden)");
                }
                if !l.children.is_empty() {
                    s.push_str(&format!(" {:?}", said(&l.children)));
                }
                s
            })
            .collect()
    }

    fn layers_of(deck: &Deck, state: &str) -> Vec<String> {
        let snapshots = resolve_states(deck).unwrap();
        said(&layers(deck, &snapshots, deck.state_index(state).unwrap()))
    }

    #[test]
    fn a_state_lists_its_nodes_topmost_first_with_those_that_leave_in_it() {
        let deck = Deck::from_json(include_str!("../../../docs/examples/revenue.deck.json")).unwrap();
        // The background paints first (z -100), so it is listed last; the rest the deck's order
        // backwards.
        assert_eq!(layers_of(&deck, "intro"), ["subtitle", "title", "bg"]);
        // `revenue` hides the background and the subtitle: listed, hidden, where intro has them.
        assert_eq!(layers_of(&deck, "revenue"), ["note", "rev", "subtitle (hidden)", "title", "bg (hidden)"]);
        // `mix` is a step of `revenue`'s slide, and shows what it shows.
        assert_eq!(layers_of(&deck, "mix"), ["note", "rev", "title"]);
        assert_eq!(layers_of(&deck, "close"), ["note (hidden)", "rev (hidden)", "title", "bg"]);
    }

    #[test]
    fn a_container_holds_its_children_and_a_step_shows_what_its_slide_will() {
        let deck = Deck::from_json(
            &serde_json::json!({
                "scaena": crate::FORMAT_VERSION,
                "canvas": { "width": 1920, "height": 1080, "unit": "cu" },
                "nodes": {
                    "card": { "type": "stack", "at": { "col": [1, 6], "row": [1, 6] } },
                    "head": { "type": "text", "text": "Head", "role": "title", "at": { "parent": "card", "index": 1 } },
                    "body": { "type": "text", "text": "Body", "role": "body", "z": 5, "at": { "parent": "card", "index": 0 } },
                    "panel": { "type": "frame", "at": { "col": [7, 12], "row": [1, 6] } },
                    "front": { "type": "text", "text": "Front", "role": "body", "at": { "parent": "panel" } },
                    "back": { "type": "text", "text": "Back", "role": "body", "z": -1, "at": { "parent": "panel" } },
                    "aside": { "type": "text", "text": "Aside", "role": "caption", "at": { "col": [1, 12], "row": [7, 8] } },
                    "spare": { "type": "text", "text": "Spare", "role": "caption", "at": { "col": [1, 4], "row": [9, 10] } }
                },
                "states": [
                    { "id": "one", "mode": "delta", "props": { "card": {}, "head": {}, "body": {}, "panel": {}, "front": {}, "back": {} } },
                    { "id": "two", "slide": "one", "mode": "delta", "props": { "aside": {} } }
                ]
            })
            .to_string(),
        )
        .unwrap();
        // A stack's children in the order it lays them out, whatever their `z`; a frame's
        // topmost first. The step after shows the aside, which stands hidden in the first; no
        // state shows the spare, which stands hidden in both.
        let held = [r#"panel ["front", "back"]"#, r#"card ["body", "head"]"#];
        assert_eq!(layers_of(&deck, "one"), ["spare (hidden)", "aside (hidden)", held[0], held[1]]);
        assert_eq!(layers_of(&deck, "two"), ["spare (hidden)", "aside", held[0], held[1]]);
        // The deck's overrides win in every state: the front sent behind the back.
        let mut deck = deck;
        deck.overrides.insert("front".into(), serde_json::from_value(serde_json::json!({ "z": -2 })).unwrap());
        assert_eq!(layers_of(&deck, "two")[2], r#"panel ["back", "front"]"#);
    }
}

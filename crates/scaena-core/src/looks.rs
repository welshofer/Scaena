//! A node's look, picked up and put down (PLAN 2.58, ADR-0013): ⌥⌘C takes the look of the node
//! selected, as a state shows it, and ⌥⌘V gives it to each node selected that takes it, one
//! patch of `choose`s, each written where that node's own value lives (SPEC §7.3).
//!
//! A look is what [`look_props`] names for a node's type, of what an inspector edits
//! ([`crate::choices`]): a text's role and style, a shape's fill, stroke, and corners, an
//! image's corners, a shader's preset and palette, a chart's labels, a stack's or a grid's gap.
//! What a node is and holds (its words, its data, its file and crop), where it goes, how it
//! moves, and what a reader hears are not its look. A value the theme gives, which the deck sets
//! nowhere, is part of it too: put down, it takes the other node's own value away where it lives,
//! so the theme's shows there as well. A value written out, an override, is put down as one.

use crate::choices::choices;
use crate::data::SourceFiles;
use crate::document::{Deck, NodeType};
use crate::model::theme::Theme;
use crate::patch::SemanticOp;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// The properties a node's look is made of, by its type, in the order a look puts them down: a
/// text's role before the style that adjusts it.
pub fn look_props(node_type: NodeType) -> &'static [&'static str] {
    match node_type {
        NodeType::Text => {
            &["role", "style/family", "style/weight", "style/italic", "style/size", "style/color", "style/case"]
        }
        NodeType::Shape => &["fill", "stroke/paint", "stroke/width", "radius"],
        NodeType::Image => &["radius"],
        NodeType::Shader => &["preset", "palette"],
        NodeType::Chart => &["labels/show", "labels/role"],
        NodeType::Stack | NodeType::Grid => &["gap"],
        NodeType::Table | NodeType::Frame | NodeType::Group => &[],
    }
}

/// A node's look as a state shows it: what ⌥⌘C picks up.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Look {
    /// The node it was picked up from.
    pub node: String,
    #[serde(rename = "type")]
    pub node_type: NodeType,
    /// Each property of its type's look, with the value the state shows.
    pub props: Vec<Part>,
}

/// One property of a look.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Part {
    pub prop: String,
    /// As the deck sets it, tracked and then overridden; absent where nothing sets it and the
    /// theme's shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
}

/// What a look put down makes.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Put {
    /// The patch: one `choose` (SPEC §7.3) for each property a node takes whose value it does
    /// not show already, in the state the look is put down in. What `deck_patch` takes; empty
    /// when every node looks so already.
    pub patch: Vec<Value>,
    /// The nodes it changes, in the order asked.
    pub took: Vec<String>,
    /// The nodes that look so already.
    pub same: Vec<String>,
    /// The nodes that take none of it, each with why.
    pub refused: Vec<Refused>,
}

/// A node a look is not put on, and why.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Refused {
    pub node: String,
    pub why: String,
}

/// `node`'s look as `state` shows it, in `theme` (PLAN 2.58).
pub fn look(deck: &Deck, theme: &Theme, state: &str, node: &str, files: &dyn SourceFiles) -> Result<Look, String> {
    let c = choices(deck, theme, state, node, files)?;
    let props = (look_props(c.node_type).iter())
        .filter_map(|p| c.fields.iter().find(|f| f.prop == *p))
        .map(|f| Part { prop: f.prop.clone(), value: f.value.clone() })
        .collect();
    Ok(Look { node: node.into(), node_type: c.node_type, props })
}

/// `look` put down on each of `nodes` in `state` (PLAN 2.58): for each property of it a node's
/// type takes, a `choose` of its value where the node shows another, written where the node's
/// value lives, or taking it away there where the look's is the theme's. A node of another type
/// takes the properties its own look shares with it (an image a shape's corners); one that
/// shares none, or is where the look came from, is refused, with why. A node not on screen in
/// `state` is an error, as an inspector's is.
pub fn putting(
    deck: &Deck,
    theme: &Theme,
    state: &str,
    look: &Look,
    nodes: &[String],
    files: &dyn SourceFiles,
) -> Result<Put, String> {
    let mut put = Put { patch: Vec::new(), took: Vec::new(), same: Vec::new(), refused: Vec::new() };
    let kind =
        |t: NodeType| serde_json::to_value(t).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    for node in nodes {
        if *node == look.node {
            let why = "is where the look was picked up".to_string();
            put.refused.push(Refused { node: node.clone(), why });
            continue;
        }
        let c = choices(deck, theme, state, node, files)?;
        let own = look_props(c.node_type);
        let takes: Vec<&Part> = look.props.iter().filter(|p| own.contains(&p.prop.as_str())).collect();
        if takes.is_empty() {
            let why = format!("a {} takes none of a {}'s look", kind(c.node_type), kind(look.node_type));
            put.refused.push(Refused { node: node.clone(), why });
            continue;
        }
        let before = put.patch.len();
        for part in takes {
            let now = c.fields.iter().find(|f| f.prop == part.prop).and_then(|f| f.value.as_ref());
            if now == part.value.as_ref() {
                continue;
            }
            let op = SemanticOp::Choose {
                node: node.clone(),
                prop: part.prop.clone(),
                value: part.value.clone().unwrap_or(Value::Null),
                state: Some(state.to_string()),
                fork: false,
            };
            put.patch.push(serde_json::to_value(op).map_err(|e| e.to_string())?);
        }
        match put.patch.len() > before {
            true => put.took.push(node.clone()),
            false => put.same.push(node.clone()),
        }
    }
    Ok(put)
}

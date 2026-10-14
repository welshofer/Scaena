//! A node's look, picked up and put down (PLAN 2.58, ADR-0013): ⌥⌘C takes the look of the node
//! selected, as a state shows it, and ⌥⌘V gives it to each node selected that takes it, one
//! patch of `choose`s, each written where that node's own value lives (SPEC §7.3).
//!
//! A look is what [`look_props`] names for a node's type, of what an inspector edits
//! ([`crate::choices`]): a text's role and style, a shape's fill, stroke, and corners, an
//! image's corners, mask, border (PLAN 2.118), and adjustments (PLAN 2.117), a shader's preset and palette, a chart's
//! labels, a stack's or a grid's gap, and the shadow a shape, an image, or a container casts
//! (PLAN 2.115).
//! What a node is and holds (its words, its data, its file and crop), where it goes, how it
//! moves, and what a reader hears are not its look. A value the theme gives, which the deck sets
//! nowhere, is part of it too: put down, it takes the other node's own value away where it lives,
//! so the theme's shows there as well. A value written out, an override, is put down as one.

use crate::choices::choices;
use crate::data::SourceFiles;
use crate::document::{Deck, NodeType, Props};
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
        NodeType::Shape => &["fill", "stroke/paint", "stroke/width", "radius", "shadow"],
        NodeType::Image => &[
            "radius",
            "mask",
            "stroke/paint",
            "stroke/width",
            "shadow",
            "adjust/exposure",
            "adjust/contrast",
            "adjust/saturation",
            "adjust/warmth",
        ],
        NodeType::Shader => &["preset", "palette"],
        NodeType::Chart => &["labels/show", "labels/role"],
        NodeType::Stack | NodeType::Grid => &["gap", "shadow"],
        NodeType::Frame => &["shadow"],
        NodeType::Table | NodeType::Group => &[],
    }
}

/// Whether a node of `node_type`, as `props` show it, casts the shadow it names (PLAN 2.115): a
/// filled rectangle or ellipse, a container with a panel (its `fill`), and a picture under no mask
/// or an oval one that hides all under it (`clear`: one that may let it show, [`see_through`]).
/// What else names one draws none, which the engine draws by and an inspector offers by.
pub fn casts(node_type: NodeType, props: &Props, clear: bool) -> bool {
    let filled = props.get("fill").is_some_and(|f| !f.is_null());
    match node_type {
        NodeType::Shape => filled && matches!(shape_kind(props), "rect" | "ellipse"),
        NodeType::Image => !clear && matches!(mask_of(props), None | Some("ellipse")),
        NodeType::Stack | NodeType::Grid | NodeType::Frame => filled,
        _ => false,
    }
}

/// Whether `radius` rounds a node's corners as `props` show it (PLAN 2.68, 2.113): a rectangle's,
/// and a picture's under no mask, which takes their place.
pub fn rounds(node_type: NodeType, props: &Props) -> bool {
    match node_type {
        NodeType::Shape => shape_kind(props) == "rect",
        NodeType::Image => mask_of(props).is_none(),
        _ => false,
    }
}

/// A shape's kind as it draws: a `rect` where it names none, or a `path` where it gives one.
fn shape_kind(props: &Props) -> &str {
    let path = props.contains_key("path");
    props.get("kind").and_then(Value::as_str).unwrap_or(if path { "path" } else { "rect" })
}

/// The mask a picture is shown in, by name, if any.
fn mask_of(props: &Props) -> Option<&str> {
    props.get("mask").filter(|m| !m.is_null()).map(|m| m.as_str().unwrap_or_default())
}

/// Whether a picture's file may let what is under it show (PLAN 2.115): a PNG with an alpha
/// channel, or with a color it says is transparent (`tRNS`, which comes before its pixels). A
/// JPEG hides all under it.
pub fn see_through(bytes: &[u8]) -> bool {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return false;
    }
    // IHDR's color type: 4 is gray and 6 color, each with alpha.
    if matches!(bytes.get(25), Some(4 | 6)) {
        return true;
    }
    let mut at = 8usize;
    while let Some(head) = bytes.get(at..at.saturating_add(8)) {
        match &head[4..8] {
            b"tRNS" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => {}
        }
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        at = at.saturating_add(12).saturating_add(len);
    }
    false
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

//! `deck.json` document types (SPEC §3). The structural skeleton is typed; node
//! properties are an ordered JSON map (`Props`) so that tracking, validation, and
//! patching work generically today. Phase 1 task 1.1 introduces typed `NodeProps`
//! generated together with `docs/schema/deck.schema.json`; the merge semantics in
//! [`crate::tracking`] are written against `Props` so that change is local.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Ordered property bag for a node (whole, in `nodes`) or a delta (in `states[].props`).
pub type Props = IndexMap<String, Value>;

/// The canonical deck document.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Deck {
    /// Format version, e.g. `"0.1"`.
    pub scaena: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    pub canvas: Canvas,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub formats: Vec<String>,
    /// Path inside the bundle, or an inline theme object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub theme: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontRef>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub data: IndexMap<String, DataSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spine: Option<Spine>,
    /// The scene graph: node id → node.
    pub nodes: IndexMap<String, Node>,
    /// The cue list, in order.
    pub states: Vec<State>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub overrides: IndexMap<String, Props>,
    #[serde(default, rename = "_comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Anything else in `meta` is preserved verbatim.
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    pub width: f64,
    pub height: f64,
    #[serde(default = "default_unit")]
    pub unit: Unit,
}

fn default_unit() -> Unit {
    Unit::Cu
}

/// Canvas units; 1 cu = 1 px at 1080p.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Cu,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FontRef {
    pub family: String,
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<IndexMap<String, [f64; 2]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    pub source: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse: Option<IndexMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Spine {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    pub id: String,
    pub claim: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub states: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media: Option<Value>,
}

/// Node types (SPEC §3.3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Text,
    Shape,
    Image,
    Chart,
    Shader,
    Stack,
    Grid,
    Frame,
    Group,
}

impl NodeType {
    pub fn is_container(self) -> bool {
        matches!(self, NodeType::Stack | NodeType::Grid | NodeType::Frame | NodeType::Group)
    }
}

/// A node in the scene graph: a type plus its default properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(rename = "type")]
    pub node_type: NodeType,
    #[serde(flatten)]
    pub props: Props,
}

/// How a state relates to the one before it (SPEC §2.2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum StateMode {
    /// Declare changes only; unchanged properties track forward.
    #[default]
    Delta,
    /// Everything explicit; nothing tracks.
    Absolute,
}

/// One cue.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slide: Option<String>,
    /// Track from this state instead of the previous one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default)]
    pub mode: StateMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transition: Option<Value>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    pub props: IndexMap<String, Props>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choreography: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, rename = "_comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl Deck {
    /// Parse a `deck.json` string.
    pub fn from_json(s: &str) -> Result<Deck, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Serialize canonically (pretty, key order preserved).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Index of a state by id.
    pub fn state_index(&self, id: &str) -> Option<usize> {
        self.states.iter().position(|s| s.id == id)
    }

    /// The slide id a state belongs to (its own id when it has no `slide`).
    pub fn slide_of<'a>(&self, state: &'a State) -> &'a str {
        state.slide.as_deref().unwrap_or(&state.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../../docs/examples/revenue.deck.json");

    #[test]
    fn example_deck_round_trips() {
        let deck = Deck::from_json(EXAMPLE).expect("example parses");
        assert_eq!(deck.scaena, "0.1");
        assert_eq!(deck.states.len(), 4);
        assert_eq!(deck.nodes["rev"].node_type, NodeType::Chart);
        let again: Deck = serde_json::from_str(&deck.to_json().unwrap()).unwrap();
        assert_eq!(again.states[2].slide.as_deref(), Some("revenue"));
        assert_eq!(serde_json::to_value(&again).unwrap(), serde_json::to_value(&deck).unwrap());
    }

    #[test]
    fn unknown_top_level_keys_are_rejected() {
        let mut v: Value = serde_json::from_str(EXAMPLE).unwrap();
        v["bogus"] = Value::Bool(true);
        assert!(serde_json::from_value::<Deck>(v).is_err());
    }
}

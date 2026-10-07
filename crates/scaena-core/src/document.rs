//! `deck.json` document types (SPEC §3). The structural skeleton is typed; node
//! properties are an ordered JSON map (`Props`), so tracking, validation, and patching
//! work generically. What those maps may hold is typed in [`crate::model`], one struct per
//! node type, and these types and those together generate `docs/schema/deck.schema.json`
//! (PLAN 1.1).

use crate::model;
use indexmap::IndexMap;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::borrow::Cow;

/// Ordered property bag for a node (whole, in `nodes`) or a delta (in `states[].props`).
pub type Props = IndexMap<String, Value>;

/// How many containers deep a node may sit (SPEC §3.4): far past what a slide shows, and
/// well inside what laying out each level by recursion holds on a browser's stack.
pub const MAX_NESTING: usize = 64;

/// Canonical deck.json. A deck is a scene graph (nodes) plus an ordered cue list (states)
/// over a narrative spine, rendered against a theme. See docs/SPEC.md §3.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Scaena deck document")]
pub struct Deck {
    /// Format version (semver, major.minor[.patch]).
    // The schema's pattern for it comes from `crate::FORMAT_VERSION`.
    pub scaena: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Meta>,
    pub canvas: Canvas,
    /// Additional projections rendered from the same spine and nodes with other layout
    /// template sets.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<model::Format>", extend("uniqueItems" = true))]
    pub formats: Vec<String>,
    /// Path to a theme file inside the bundle, or an inline theme object (schema:
    /// theme.schema.json).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<model::ThemeRef>")]
    pub theme: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<FontRef>,
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(extend("propertyNames" = {"$ref": "#/$defs/Id"}))]
    pub data: IndexMap<String, DataSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spine: Option<Spine>,
    /// The scene graph. Keys are node ids, stable across the whole deck.
    #[schemars(extend("propertyNames" = {"$ref": "#/$defs/Id"}))]
    pub nodes: IndexMap<String, Node>,
    /// The cue list, in order. Every click is a state.
    #[schemars(length(min = 1))]
    pub states: Vec<State>,
    /// Per node, props that win over its theme, its defaults, and every state: a delta merged
    /// into the node as each state resolves it (SPEC §3.6). The only place raw pixel and
    /// color values are theme-legal (lint W300 elsewhere).
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(
        with = "IndexMap<String, model::StateDeltaRef>",
        extend("propertyNames" = {"$ref": "#/$defs/Id"})
    )]
    pub overrides: IndexMap<String, Props>,
    #[serde(default, rename = "_comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// `manifest.json` (SPEC §3.1): what a bundle held when it was last saved. Saving writes
/// it; nothing in the render path reads it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Scaena bundle manifest")]
pub struct Manifest {
    /// The deck format version the bundle was saved in.
    pub scaena: String,
    /// The sha256 of `deck.json` as saved.
    #[schemars(regex(pattern = r"^[0-9a-f]{64}$"))]
    pub deck: String,
    /// Every other file in the bundle, by its path in the bundle: its sha256.
    #[schemars(extend("additionalProperties" = {"type": "string", "pattern": "^[0-9a-f]{64}$"}))]
    pub files: IndexMap<String, String>,
    /// When the bundle was first saved.
    #[schemars(extend("format" = "date-time"))]
    pub created: String,
    /// When it was saved last.
    #[schemars(extend("format" = "date-time"))]
    pub modified: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, JsonSchema)]
pub struct Meta {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("format" = "date-time"))]
    pub created: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("format" = "date-time"))]
    pub modified: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("default" = "en-US"))]
    pub lang: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Anything else in `meta` is preserved verbatim.
    #[serde(flatten)]
    pub extra: IndexMap<String, Value>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Canvas {
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub width: f64,
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub height: f64,
    #[serde(default = "default_unit")]
    pub unit: Unit,
}

fn default_unit() -> Unit {
    Unit::Cu
}

/// Canvas units. 1 cu = 1 px at 1080p.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Unit {
    #[default]
    Cu,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FontRef {
    pub family: String,
    #[schemars(regex(pattern = r"^fonts/"))]
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 1000))]
    pub weight: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<model::FontStyle>")]
    pub style: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<IndexMap<String, [f64; 2]>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataSource {
    #[schemars(with = "model::SourceRef")]
    pub source: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<IndexMap<String, model::FieldType>>")]
    pub schema: Option<IndexMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parse: Option<IndexMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Spine {
    pub sections: Vec<Section>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Section {
    #[schemars(with = "model::Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[schemars(length(min = 1))]
    pub beats: Vec<Beat>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Beat {
    #[schemars(with = "model::Id")]
    pub id: String,
    /// One sentence the audience should leave with.
    pub claim: String,
    /// @data refs, asset refs, or URLs.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "model::IdList")]
    pub states: Vec<String>,
    /// Speaker notes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// Estimated seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub duration: Option<f64>,
    /// Projection hints (infographic priority, podcast script, …).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<serde_json::Map<String, Value>>")]
    pub media: Option<Value>,
}

/// Node types (SPEC §3.3).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Text,
    Shape,
    Image,
    Chart,
    Table,
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

/// What a JSON error says, without where in its text: the text of a value written out to
/// be parsed, a line no file holds.
pub(crate) fn unplaced(e: serde_json::Error) -> String {
    let message = e.to_string();
    match message.rsplit_once(" at line ") {
        Some((what, _)) if e.line() > 0 => what.to_string(),
        _ => message,
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

impl Node {
    /// This node through its type's view: every property checked against what its type
    /// may hold.
    pub fn typed(&self) -> Result<model::TypedNode, serde_json::Error> {
        serde_json::from_value(serde_json::to_value(self)?)
    }
}

/// A node's schema is its type's ([`model::TypedNode`]).
impl JsonSchema for Node {
    fn schema_name() -> Cow<'static, str> {
        model::TypedNode::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        model::TypedNode::schema_id()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        model::TypedNode::json_schema(generator)
    }
}

/// How a state relates to the one before it (SPEC §2.2).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum StateMode {
    /// Declare changes only; unchanged properties track forward.
    #[default]
    Delta,
    /// Everything explicit; nothing tracks.
    Absolute,
}

/// One cue.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct State {
    #[schemars(with = "model::Id")]
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// The slide this state builds on: its first state's id. A state without one starts a
    /// slide named by its own id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<model::Id>")]
    pub slide: Option<String>,
    /// Track from this state instead of the previous one (branch).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<model::Id>")]
    pub from: Option<String>,
    #[serde(default)]
    pub mode: StateMode,
    /// Layout template name from the theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(with = "Option<model::Transition>")]
    pub transition: Option<Value>,
    /// Per-node property deltas for this state. Keys are node ids.
    #[serde(default, skip_serializing_if = "IndexMap::is_empty")]
    #[schemars(
        with = "IndexMap<String, model::StateDeltaRef>",
        extend("propertyNames" = {"$ref": "#/$defs/Id"})
    )]
    pub props: IndexMap<String, Props>,
    /// Nodes that exit in this state.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "model::IdList")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(with = "Vec<model::ChoreoItem>")]
    pub choreography: Vec<Value>,
    /// Auto-advance dwell in ms (video and kiosk).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub hold: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, rename = "_comment", skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

impl Deck {
    /// The formats the deck lays out anew (ADR-0020): each it lists but its own canvas's
    /// shape, which a node's own props lay out. A node's layout is kept in these alone.
    pub fn anew(&self) -> Vec<&str> {
        let own = [self.canvas.width, self.canvas.height];
        (self.formats.iter())
            .filter(|f| model::Format::parse(f).is_some_and(|f| f.canvas(own) != own))
            .map(String::as_str)
            .collect()
    }

    /// The image files the deck's image nodes name (`src`), in their defaults, their
    /// states, and their overrides: sorted, each once.
    pub fn image_files(&self) -> Vec<String> {
        let images = |id: &String| self.nodes.get(id).is_some_and(|n| n.node_type == NodeType::Image);
        let props = self
            .nodes
            .iter()
            .filter(|(id, _)| images(id))
            .map(|(_, n)| &n.props)
            .chain(self.states.iter().flat_map(|s| s.props.iter().filter(|(id, _)| images(id)).map(|(_, p)| p)))
            .chain(self.overrides.iter().filter(|(id, _)| images(id)).map(|(_, p)| p));
        let mut out: Vec<String> = props.filter_map(|p| p.get("src")?.as_str().map(String::from)).collect();
        crate::sort::sort(&mut out);
        out.dedup();
        out
    }

    /// The props a node's overrides set, as JSON pointers into the node: what makes it not
    /// theme-safe (SPEC §3.6). An object counts by its keys, so `style: {size, color}` is
    /// two overrides.
    pub fn overridden(&self, id: &str) -> Vec<String> {
        let esc = |k: &str| k.replace('~', "~0").replace('/', "~1");
        let mut out = Vec::new();
        for (key, value) in self.overrides.get(id).into_iter().flatten() {
            let at = format!("/{}", esc(key));
            match value {
                Value::Object(map) if !map.is_empty() => out.extend(map.keys().map(|k| format!("{at}/{}", esc(k)))),
                _ => out.push(at),
            }
        }
        out
    }

    /// Parse a `deck.json` string.
    pub fn from_json(s: &str) -> Result<Deck, serde_json::Error> {
        serde_json::from_str(s)
    }

    /// Serialize canonically (pretty, key order preserved).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// The deck as a JSON value, through its canonical text: the one serializer of the
    /// model a WASM module carries, where `serde_json::to_value` would compile in a second
    /// (SPEC §15).
    pub fn to_value(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::from_str(&self.to_json()?)
    }

    /// A deck from a JSON value, through its text, with the one parser of the model
    /// ([`Deck::from_json`]); an error says what is wrong without a place in that text,
    /// which no file holds.
    pub fn from_value(value: &serde_json::Value) -> Result<Deck, String> {
        Deck::from_json(&value.to_string()).map_err(unplaced)
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
        assert_eq!(deck.scaena, crate::FORMAT_VERSION);
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

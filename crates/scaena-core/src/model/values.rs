//! The values node properties are made of (SPEC §3.2–§3.9): ids, lengths, colors and
//! paints, placement, motion references, chart encodings.

use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A slug: unique within its collection (SPEC §3.2).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Id(#[schemars(regex(pattern = r"^[a-z][a-z0-9_-]{0,63}$"))] pub String);

/// Ids, each at most once.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct IdList(#[schemars(extend("uniqueItems" = true))] pub Vec<Id>);

/// A length in canvas units, a token reference (e.g. "space.4"), or a percentage string.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Length {
    Cu(f64),
    Token(#[schemars(regex(pattern = r"^([a-z][a-z0-9_.-]*|-?[0-9.]+(cu|%))$"))] String),
}

/// Milliseconds, or a named theme duration ("fast", "standard", "slow").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Duration {
    Ms(#[schemars(range(min = 0))] f64),
    Named(#[schemars(length(min = 1))] String),
}

/// Named theme easing or a cubic Bézier [x1, y1, x2, y2].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Easing {
    Named(#[schemars(length(min = 1))] String),
    Bezier([f64; 4]),
}

/// Named theme spring or explicit parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Spring {
    Named(#[schemars(length(min = 1))] String),
    Params(SpringParams),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SpringParams {
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub stiffness: f64,
    #[schemars(range(min = 0))]
    pub damping: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0, "default" = 1))]
    pub mass: Option<f64>,
}

/// Theme token name (e.g. "accent", "color.ink") or a literal (#rrggbb[aa], oklch(...)).
/// Literals outside overrides are lint W300.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Color(#[schemars(length(min = 1))] pub String);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Paint {
    Color(Color),
    Solid(SolidPaint),
    Gradient(GradientPaint),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SolidPaint {
    pub solid: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GradientPaint {
    pub gradient: Gradient,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Gradient {
    pub kind: GradientKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<Point>,
    #[schemars(length(min = 2))]
    pub stops: Vec<GradientStop>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GradientKind {
    Linear,
    Radial,
    Conic,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GradientStop {
    #[schemars(range(min = 0, max = 1))]
    pub at: f64,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Stroke {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paint: Option<Paint>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap: Option<Cap>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub join: Option<Join>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dash: Option<Vec<NonNegative>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    Miter,
    Round,
    Bevel,
}

/// A number at or above 0.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct NonNegative(#[schemars(range(min = 0))] pub f64);

pub type Point = [f64; 2];

/// [x, y, w, h] in canvas units.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Rect(pub [f64; 4]);

/// A grid index or an inclusive [start, end] range (1-based).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Range {
    Index(#[schemars(range(min = 1))] u32),
    Span(#[schemars(extend("items" = {"type": "integer", "minimum": 1}))] [u32; 2]),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Transform {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translate: Option<Point>,
    /// Degrees.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skew: Option<Point>,
    /// Normalized [0..1, 0..1] within the node's box.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Point>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Scale {
    Uniform(f64),
    Xy(Point),
}

/// Alignment within a slot or cell. Vertical anchors include typographic ones (SPEC §3.4).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Align {
    Both(AlignX),
    Axes(AlignAxes),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AlignX {
    Start,
    Center,
    End,
    Stretch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum AlignY {
    Start,
    Center,
    End,
    Stretch,
    Cap,
    Baseline,
    XHeight,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AlignAxes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<AlignX>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<AlignY>,
}

/// Where a node goes. One placement wins: `rect`, else `in`, else `col`/`row` (SPEC §3.4).
/// Grid cells and slots are theme-managed; `rect` is an explicit override (lint W301 in
/// template-managed states). In a container (`parent`), `col`/`row` and `area` are its
/// grid's, `rect` is relative to a frame's padding, and a stack places its children itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<Range>,
    /// Named slot in the state's layout template, or `canvas` or `grid`.
    #[serde(rename = "in", default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// The container this node is in: a `stack`, `grid`, `frame`, or `group` (ADR-0008).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<Id>,
    /// Order among the container's children, as CSS `order`: ascending, default 0, ties
    /// in `nodes` order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    /// A named area of the grid container this node is in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub area: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<Rect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<Point>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Size {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub w: Option<SizeValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub h: Option<SizeValue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_w: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_w: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_h: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_h: Option<Length>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(regex(pattern = r"^[0-9.]+:[0-9.]+$"))]
    pub aspect: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum SizeValue {
    Length(Length),
    Keyword(SizeKeyword),
    Fraction(#[schemars(regex(pattern = r"^fraction\([0-9.]+\)$"))] String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SizeKeyword {
    Fit,
    Fill,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Keyframe {
    /// Milliseconds from the start of the track.
    #[schemars(range(min = 0))]
    pub t: f64,
    /// Value; its type must match the property.
    pub v: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<Easing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<Spring>,
}

/// Per-property keyframe tracks within a state. They never track to the next state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AnimTracks(
    #[schemars(extend("additionalProperties" = {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/Keyframe"}}))]
    pub IndexMap<String, Vec<Keyframe>>,
);

/// A theme motion preset by name, or a preset with parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum PresetRef {
    Named(#[schemars(length(min = 1))] String),
    With(Box<PresetCall>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetCall {
    pub preset: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<Easing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<Spring>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<IndexMap<String, Value>>,
}

/// What a choreographed entrance splits its target into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SplitUnit {
    Lines,
    Words,
    Glyphs,
    Children,
    Marks,
}

/// What a text node's own `split` divides it into for motion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextSplit {
    Lines,
    Words,
    Glyphs,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Run {
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emphasis: Option<Emphasis>,
    /// What this run changes about its role's look, after the node's `style` when the run
    /// is set in the node's role (SPEC §3.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<TextStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
}

/// What a text node, or a run, changes about its role's look (SPEC §3.6): the role's own
/// properties, by name; each replaces the role's. `family` names a theme family and
/// `color` a theme color token or role. A literal color, and any `size`, is a pixel value:
/// theme-legal only in `overrides` (lint W300 elsewhere).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TextStyle {
    /// Key in the theme's `type.families`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 1000))]
    pub weight: Option<u16>,
    /// Line height as a multiple of size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub leading: Option<f64>,
    /// Em units; negative tightens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opsz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<super::theme::Case>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Emphasis {
    High,
    Low,
}

/// One chart channel bound to a data field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Encoding {
    pub field: String,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub field_type: Option<FieldKind>,
    /// d3-format / strftime-style (Phase 1; PLAN 1.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("items" = {"type": ["number", "string", "null"]}))]
    pub domain: Option<[Value; 2]>,
    /// Theme data palette name for color encodings (categorical | sequential | diverging).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort: Option<Sort>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum FieldKind {
    Quantitative,
    Ordinal,
    Nominal,
    Temporal,
}

/// A sort direction, or a field to sort by.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Sort {
    Direction(SortDirection),
    Field(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SortDirection {
    Ascending,
    Descending,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub kind: AnnotationKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<IndexMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AnnotationKind {
    Callout,
    Rule,
    Band,
    Highlight,
}

/// OpenType features: on, off, or an alternate's index.
pub type Features = IndexMap<String, FeatureValue>;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum FeatureValue {
    Switch(bool),
    Alternate(i64),
}

/// Variable-font axis values by tag.
pub type FontAxes = IndexMap<String, f64>;

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

/// Per-property keyframe tracks within a state, each property a look's (SPEC §3.9):
/// `opacity`, `translate`, `scale`, `rotate`, and `progress`. They never track to the
/// next state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct AnimTracks(
    #[schemars(extend(
        "propertyNames" = {"enum": ["opacity", "translate", "scale", "rotate", "progress"]},
        "additionalProperties" = {"type": "array", "minItems": 1, "items": {"$ref": "#/$defs/Keyframe"}}
    ))]
    pub IndexMap<String, Vec<Keyframe>>,
);

/// A motion preset's look (SPEC §3.9), against the unit at rest: what an entrance starts
/// from, an exit ends at, and an emphasis goes out to and comes back from.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetLook {
    /// Multiplies the unit's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub opacity: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<LookTransform>,
    /// A theme color every paint the unit draws mixes toward, in Oklab, keeping its alpha.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<LookParams>,
}

/// A look's transform: scale, then rotate, about `anchor`, then translate.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookTransform {
    /// Canvas units.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translate: Option<Point>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Scale>,
    /// Degrees, clockwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rotate: Option<f64>,
    /// Fractions of the unit's box; its center by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<Point>,
}

/// Numbers a look sets on what its unit draws.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LookParams {
    /// How much of each outline a shape strokes is drawn, from where the outline starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub progress: Option<f64>,
}

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
    /// Over the preset's look's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<LookParams>,
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
    /// Set in the family's italic face (SPEC §3.5), or, `false`, upright. None is
    /// synthesized: a family without one sets the text upright (lint W231).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
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

/// A paragraph of a text as an item of a list (SPEC §3.5, ADR-0018): bulleted or numbered, at
/// a level, 0 the outermost. Its marker and indent are the theme's (`type.lists`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ListItem {
    pub kind: ListKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 8))]
    pub level: Option<u8>,
}

impl ListItem {
    /// Its level, 0 where it gives none.
    pub fn depth(&self) -> u8 {
        self.level.unwrap_or(0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ListKind {
    Bullet,
    Number,
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

/// Which of a line's or an area's rows are projected: a forecast, or an estimate (SPEC
/// §3.7). The rows whose `field` holds `value`, or a true one when no value is given. The
/// line runs dashed from the last actual point through them, the area under them is lighter,
/// and their value labels say they are estimates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Projected {
    pub field: String,
    /// What `field` holds in a projected row; `true` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("type" = ["string", "number", "boolean"]))]
    pub value: Option<Value>,
    /// What a projected value's label says after the value; the theme's
    /// `charts.projected.note` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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

/// A chart annotation (SPEC §3.7): a rule, a band, a callout, or a highlight, in the
/// theme's `charts.annotation` style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Annotation {
    pub kind: AnnotationKind,
    /// Where it stands, by kind: a rule at `x` or `y`, a band from one to another of
    /// either, a callout at `x` (and `y`, or a `series`), a highlight on categories `x`
    /// and series.
    pub at: AnnotationAt,
    /// What it says; a highlight says nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// The text's role; `charts.annotation.role` when unset.
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

/// An annotation's place: a category or an x value (a number, or a date as ISO 8601),
/// a value on the value axis, and a series, each one or several by kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AnnotationAt {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Place>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Place>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series: Option<Place>,
}

/// One value, or several: a band's two ends, or what a highlight picks out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Place {
    One(Scalar),
    Many(#[schemars(length(min = 1))] Vec<Scalar>),
}

/// A number, or text: a category, a series, or a date.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Scalar {
    Number(f64),
    Text(String),
}

impl Place {
    /// Its values, in order.
    pub fn values(&self) -> Vec<&Scalar> {
        match self {
            Place::One(v) => vec![v],
            Place::Many(vs) => vs.iter().collect(),
        }
    }
}

impl Scalar {
    /// The value as a category or a series reads with no format: text as is, a number
    /// in d3's default form (`Datum::label`).
    pub fn label(&self) -> String {
        match self {
            Scalar::Number(n) => crate::data::Datum::Number(*n).label(),
            Scalar::Text(s) => s.clone(),
        }
    }

    /// Where the value falls along a continuous x of numbers, or of dates (`dates`) in
    /// seconds, which it writes in ISO 8601; `None` if it is not one.
    pub fn position(&self, dates: bool) -> Option<f64> {
        match (self, dates) {
            (Scalar::Number(n), false) => Some(*n),
            (Scalar::Text(s), true) => crate::format::read_iso(s).ok().map(|t| t.0 as f64),
            _ => None,
        }
    }
}

impl Annotation {
    /// Whether its place fits its kind (SPEC §3.7): a rule stands at one `x` or one
    /// numeric `y`; a band spans two of either; a callout stands at one `x`, with one
    /// numeric `y` or one `series`, and says something; a highlight picks out
    /// categories, series, or both, and says nothing. A `donut`, with no axes and no
    /// series, takes only highlights of its slices. The message says what to write.
    pub fn check(&self, donut: bool) -> Result<(), String> {
        let at = &self.at;
        if donut && self.kind != AnnotationKind::Highlight {
            return Err("a donut has no axes: it takes highlights of its slices (`at.x`)".into());
        }
        if donut && at.series.is_some() {
            return Err("a donut has no series: pick out its slices with `at.x`".into());
        }
        let one = |p: &Option<Place>| matches!(p, Some(Place::One(_)));
        let number = |p: &Option<Place>| p.iter().flat_map(Place::values).all(|v| matches!(v, Scalar::Number(_)));
        if !number(&at.y) {
            return Err("`at.y` is a value on the value axis: a number".into());
        }
        match self.kind {
            AnnotationKind::Rule => match (&at.x, &at.y, &at.series) {
                (_, _, Some(_)) => Err("a rule stands at `at.x` or `at.y`; it takes no `at.series`".into()),
                (Some(_), None, None) if one(&at.x) => Ok(()),
                (None, Some(_), None) if one(&at.y) => Ok(()),
                (Some(_), Some(_), _) => Err("a rule stands at `at.x` or at `at.y`, not both".into()),
                (None, None, _) => Err("a rule stands at `at.x` or `at.y`".into()),
                _ => Err("a rule stands at one value: `\"at\": { \"y\": 30 }`".into()),
            },
            AnnotationKind::Band => {
                let pair = |p: &Option<Place>| matches!(p, Some(Place::Many(vs)) if vs.len() == 2);
                match (&at.x, &at.y, &at.series) {
                    (_, _, Some(_)) => Err("a band spans `at.x` or `at.y`; it takes no `at.series`".into()),
                    (Some(_), None, None) if pair(&at.x) => Ok(()),
                    (None, Some(_), None) if pair(&at.y) => Ok(()),
                    (Some(_), Some(_), _) => Err("a band spans `at.x` or `at.y`, not both".into()),
                    _ => Err("a band spans from one value to another: `\"at\": { \"y\": [20, 30] }`".into()),
                }
            }
            AnnotationKind::Callout => {
                if self.text.is_none() {
                    return Err("a callout says something: give it `text`".into());
                }
                match (&at.x, &at.y, &at.series) {
                    (None, ..) => Err("a callout stands at `at.x`, a category or an x value".into()),
                    (_, Some(_), Some(_)) => Err("a callout stands at `at.y` or on `at.series`' mark, not both".into()),
                    _ if !one(&at.x) || at.y.is_some() && !one(&at.y) || at.series.is_some() && !one(&at.series) => {
                        Err("a callout stands at one place: one `x`, and one `y` or one `series`".into())
                    }
                    _ => Ok(()),
                }
            }
            AnnotationKind::Highlight => match (&at.x, &at.y, &self.text) {
                (_, Some(_), _) => {
                    Err("a highlight picks out categories (`at.x`) and series; it takes no `at.y`".into())
                }
                (_, _, Some(_)) => Err("a highlight says nothing; a callout carries `text`".into()),
                (None, ..) if at.series.is_none() => Err("a highlight picks out `at.x`, `at.series`, or both".into()),
                _ => Ok(()),
            },
        }
    }
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

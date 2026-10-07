//! Node properties, one struct per node type (SPEC §3.3, §3.7, §3.8). A node in `nodes`
//! is one of [`TypedNode`]'s variants: its `type` picks the struct, and every property
//! must belong to that type.

use super::values::*;
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Communicative function, independent of visual role (SPEC §3.3). Reserved for narrative
/// lints; optional.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Semantic {
    Claim,
    Evidence,
    Annotation,
    Context,
    Comparison,
    Takeaway,
    Source,
    Navigation,
    Decoration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

/// How this node moves between two states that both show it (SPEC §3.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeTransition {
    Morph,
    Crossfade,
    Cut,
}

/// Clip to the node's box, or to SVG path data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Clip {
    Box(bool),
    Path(String),
}

/// One length for every side, or 2–4 lengths CSS-style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Padding {
    All(Length),
    Sides(#[schemars(length(min = 2, max = 4))] Vec<Length>),
}

/// A node's layout in one of the deck's formats (SPEC §3.4, ADR-0020): where it goes, how big,
/// whether it shows, a container's tracks and spacing, and a text's lines. Each property takes
/// the place of the node's own of the same name in that format; a property its type does not
/// have is E106, as anywhere on the node.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FormatLayout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Placement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<Size>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Align>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visible: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Transform>,
    /// A stack's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<Axis>,
    /// A stack's or a grid's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<Length>,
    /// A stack's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub distribute: Option<Distribute>,
    /// A container's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub padding: Option<Padding>,
    /// A grid's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<Tracks>,
    /// A grid's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<Tracks>,
    /// A grid's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub areas: Option<Vec<String>>,
    /// A text's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub max_lines: Option<u32>,
}

/// The properties every node type has, followed by the type's own: one struct per type.
/// (`#[serde(flatten)]` would lose `deny_unknown_fields`, so the shared fields are
/// written out by this macro instead.)
macro_rules! node {
    ($(#[$meta:meta])* $name:ident { $($body:tt)* }) => {
        #[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields, rename_all = "camelCase")]
        $(#[$meta])*
        pub struct $name {
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub name: Option<String>,
            /// Accessible description; text nodes default to their content (SPEC §3.12).
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub alt: Option<String>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub semantic: Option<Semantic>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub z: Option<i64>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub tags: Option<Vec<String>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub visible: Option<bool>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #[schemars(range(min = 0, max = 1))]
            pub opacity: Option<f64>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub transform: Option<Transform>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub fill: Option<Paint>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub stroke: Option<Stroke>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub blur: Option<NonNegative>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub shadow: Option<IndexMap<String, Value>>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub clip: Option<Clip>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub blend: Option<Blend>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub at: Option<Placement>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub size: Option<Size>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub align: Option<Align>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub transition: Option<NodeTransition>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub enter: Option<PresetRef>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub exit: Option<PresetRef>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub emphasis: Option<PresetRef>,
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub anim: Option<AnimTracks>,
            /// How it lays out in another of the deck's formats, by format (SPEC §3.4,
            /// ADR-0020): each property there takes the place of the node's own of the same
            /// name when the deck lays out in that format.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            #[schemars(extend("propertyNames" = {"$ref": "#/$defs/Format"}))]
            pub formats: Option<IndexMap<String, FormatLayout>>,
            #[serde(rename = "_comment", default, skip_serializing_if = "Option::is_none")]
            pub comment: Option<String>,
            $($body)*
        }
    };
}

node! {
    /// Typographic text (SPEC §3.5): `text` or `runs`, set in a theme role.
    #[schemars(extend("anyOf" = [{"required": ["text"]}, {"required": ["runs"]}]))]
    TextNode {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub role: Option<String>,
        /// What this node changes about its role's look (SPEC §3.6).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub style: Option<TextStyle>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub text: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(length(min = 1))]
        pub runs: Option<Vec<Run>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fit: Option<TextFit>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub wrap: Option<Wrap>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1))]
        pub max_lines: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(extend("exclusiveMinimum" = 0))]
        pub min_size: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(extend("exclusiveMinimum" = 0))]
        pub max_size: Option<f64>,
        #[serde(rename = "box", default, skip_serializing_if = "Option::is_none")]
        pub text_box: Option<TextBox>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub features: Option<Features>,
        /// Variable-font axis values by tag.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub axes: Option<FontAxes>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub lang: Option<String>,
        /// Max line length in characters (overrides the role's).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(extend("exclusiveMinimum" = 0))]
        pub measure: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hyphenate: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub optical_margins: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub hanging_punctuation: Option<bool>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub numeric: Option<Numeric>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(range(min = 1))]
        pub min_last_line_words: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub split: Option<TextSplit>,
        /// Its paragraphs as a list's items (SPEC §3.5, ADR-0018): one entry for each
        /// paragraph a hard line break ends, in order, `null` for one that is no item.
        /// Paragraphs past its end are no items.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub list: Option<Vec<Option<ListItem>>>,
    }
}

/// What a text node does when its text does not fit (SPEC §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextFit {
    Wrap,
    Shrink,
    Grow,
    Clip,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Wrap {
    Greedy,
    Pretty,
    Balance,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum TextBox {
    Cap,
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Numeric {
    TabularLining,
    TabularOldstyle,
    ProportionalLining,
    ProportionalOldstyle,
}

node! {
    /// Vector geometry: `path` (SVG path data) or a `kind`.
    ShapeNode {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub kind: Option<ShapeKind>,
        /// SVG path data.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub path: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub points: Option<Vec<Point>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
    Arrow,
    Polygon,
    Path,
}

node! {
    /// An image from the bundle, a PNG or a JPEG, placed in its box (SPEC §3.3).
    ImageNode {
        /// The image file's path in the bundle; `assets/<sha256>.<ext>` once saved.
        pub src: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fit: Option<ImageFit>,
        /// The point of the image that lines up with the same point of the box, in
        /// fractions of the crop, as CSS `object-position`; default [0.5, 0.5].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub focal: Option<Point>,
        /// The part of the image to show, [x, y, w, h] in fractions of the image.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub crop: Option<Rect>,
        /// Rounds the corners of what shows.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
    }
}

/// How an image meets its box.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ImageFit {
    /// Fills the box, cutting what overflows (the default).
    Cover,
    /// Fits inside the box, whole.
    Contain,
    /// Stretched to the box.
    Fill,
}

node! {
    /// A data-bound chart (SPEC §3.7): a declarative spec compiled to marks.
    ChartNode {
        pub kind: ChartKind,
        /// A data source, `@name`.
        #[schemars(regex(pattern = r"^@[a-z][a-z0-9_-]*$"))]
        pub data: String,
        /// Steps run in order before the chart reads its data: filter, derive, sort, limit,
        /// aggregate, fold, pivot (SPEC §3.10; expressions in docs/spec/expr.md).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub data_transform: Option<Vec<IndexMap<String, Value>>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub x: Option<Encoding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub y: Option<Encoding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub series: Option<Encoding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub color: Option<Encoding>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub size_encoding: Option<Encoding>,
        /// Which of a line's or an area's rows are a forecast or an estimate (PLAN 1.28).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub projected: Option<Projected>,
        /// The field that identifies a mark across states.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub key: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub axes: Option<ChartAxes>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub labels: Option<ChartLabels>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub legend: Option<Legend>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub annotations: Option<Vec<Annotation>>,
    }
}

node! {
    /// A data-bound table (SPEC §3.3): rows from a data source, set in the theme's
    /// `tables` styles, each row matched by `key` across states.
    TableNode {
        /// A data source, `@name`.
        #[schemars(regex(pattern = r"^@[a-z][a-z0-9_-]*$"))]
        pub data: String,
        /// Steps run in order before the table reads its data (SPEC §3.10).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub data_transform: Option<Vec<IndexMap<String, Value>>>,
        /// Its columns, in order; every column of the data when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(length(min = 1))]
        pub columns: Option<Vec<TableColumn>>,
        /// The field that identifies a row across states; the first column's when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub key: Option<String>,
        /// The header row, shown unless false.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub header: Option<bool>,
    }
}

/// One column of a table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TableColumn {
    pub field: String,
    /// The header's text; the field's name when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// How its numbers or dates print (docs/spec/format.md).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Where its text sits across the column; `end` for numbers, else `start`, when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<ColumnAlign>,
}

/// Where a column's text sits across it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ColumnAlign {
    Start,
    Center,
    End,
}

/// v1 chart kinds (SPEC §3.7); slope, waffle, range, and heatmap are deferred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum ChartKind {
    Bar,
    StackedBar,
    Line,
    Area,
    Scatter,
    Dot,
    Donut,
}

/// The chart's axes (PLAN 1.9 draws them).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartAxes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<AxisSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<AxisSpec>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AxisSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gridlines: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Value labels on the marks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartLabels {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<LabelShow>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// How labels that collide resolve (lint W310 otherwise).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collide: Option<LabelCollide>,
}

/// Which values a chart prints. `auto` (the theme's `charts.label.show`, else by kind)
/// puts the data on the marks: every bar, dot, and slice; a line's or an area's first
/// and last; a scatter's none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelShow {
    Auto,
    All,
    Ends,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LabelCollide {
    Hide,
    Nudge,
}

/// Where a chart's legend stands, alone or with a title (SPEC §3.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Legend {
    Placed(LegendPlace),
    Spec(LegendSpec),
}

/// A legend with a title: `{ "place": "right", "title": "Product" }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LegendSpec {
    /// `top` when unset: a titled legend is a legend, not labels at the series' ends.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<LegendPlace>,
    /// Printed before the entries, in the legend's role.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

/// Where a legend stands. `auto` is the theme's `charts.legend.place`, else `direct`.
/// `direct` names each series at its end, beside the plot, where a chart has ends: a
/// line, an area, or a stacked bar; any other chart places it `top`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LegendPlace {
    None,
    Auto,
    Direct,
    Top,
    Bottom,
    Right,
}

node! {
    /// A parametric background or fill (SPEC §3.8): a kind, a seed, a theme palette, and
    /// typed params, its own or a theme preset's. No shader source, ever.
    #[schemars(extend("allOf" = [
        {"if": {"properties": {"kind": {"const": "mesh"}}, "required": ["kind"]},
         "then": {"properties": {"params": {"$ref": "#/$defs/MeshParams"}}}},
        {"if": {"properties": {"kind": {"const": "gradient"}}, "required": ["kind"]},
         "then": {"properties": {"params": {"$ref": "#/$defs/GradientParams"}}}},
        {"if": {"properties": {"kind": {"const": "noise"}}, "required": ["kind"]},
         "then": {"properties": {"params": {"$ref": "#/$defs/NoiseParams"}}}},
        {"if": {"properties": {"kind": {"const": "grain"}}, "required": ["kind"]},
         "then": {"properties": {"params": {"$ref": "#/$defs/GrainParams"}}}},
        {"if": {"properties": {"kind": {"const": "particles"}}, "required": ["kind"]},
         "then": {"properties": {"params": {"$ref": "#/$defs/ParticlesParams"}}}}
    ]))]
    ShaderNode {
        pub kind: ShaderKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub seed: Option<u64>,
        /// A shader palette from the theme; the preset's when unset.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub palette: Option<String>,
        /// A shader preset from the theme, of this kind: its palette and params are this
        /// node's, unless the node sets its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub preset: Option<String>,
        /// Typed per kind (SPEC §3.8): `mesh` takes [`MeshParams`], `gradient`
        /// [`GradientParams`], `noise` [`NoiseParams`], `grain` [`GrainParams`], and
        /// `particles` [`ParticlesParams`].
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub params: Option<IndexMap<String, ShaderParam>>,
    }
}

/// A shader parameter: a number, or a name its kind lists (a gradient's `shape`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ShaderParam {
    Number(f64),
    Name(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ShaderKind {
    Mesh,
    Gradient,
    Noise,
    Grain,
    Particles,
}

/// The `gradient` shader's params (SPEC §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GradientParams {
    /// `linear` (the default), `radial`, or `conic`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shape: Option<GradientShape>,
    /// Degrees clockwise from up: a linear gradient's direction (180, the default, runs
    /// top to bottom), a conic one's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -360, max = 360))]
    pub angle: Option<f64>,
    /// A radial or conic gradient's center across the rect, 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub x: Option<f64>,
    /// A radial or conic gradient's center down the rect, 0 to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub y: Option<f64>,
    /// A radial gradient's radius, in shorter sides of the rect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 4), extend("exclusiveMinimum" = 0))]
    pub radius: Option<f64>,
    /// Degrees a second a linear or conic gradient turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -360, max = 360))]
    pub speed: Option<f64>,
    /// In Oklab lightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 0.25))]
    pub grain: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum GradientShape {
    Linear,
    Radial,
    Conic,
}

/// The `noise` shader's params (SPEC §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NoiseParams {
    /// Cycles per canvas unit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 0.1), extend("exclusiveMinimum" = 0))]
    pub scale: Option<f64>,
    /// Layers of finer noise over the first (fractal Brownian motion); 1 is plain
    /// simplex noise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 8))]
    pub octaves: Option<u8>,
    /// How fast the field changes, in noise cycles a second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 2))]
    pub speed: Option<f64>,
    /// How far the field spreads across the palette.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 4))]
    pub contrast: Option<f64>,
    /// In Oklab lightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 0.25))]
    pub grain: Option<f64>,
}

/// The `grain` shader's params (SPEC §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct GrainParams {
    /// The strongest grain's opacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub amount: Option<f64>,
    /// How many times a second the grain changes; 0 holds it still.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 60))]
    pub fps: Option<f64>,
}

/// The `particles` shader's params (SPEC §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ParticlesParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 64))]
    pub count: Option<u8>,
    /// A particle's radius, in shorter sides of the rect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 0.25), extend("exclusiveMinimum" = 0))]
    pub size: Option<f64>,
    /// In shorter sides a second.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub speed: Option<f64>,
    /// How much of a particle's radius its edge fades over.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub softness: Option<f64>,
}

/// The `mesh` shader's params (SPEC §3.8).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MeshParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 2, max = 16))]
    pub points: Option<u8>,
    /// In shorter sides of the rect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub drift: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(max = 1), extend("exclusiveMinimum" = 0))]
    pub softness: Option<f64>,
    /// In Oklab lightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 0.25))]
    pub grain: Option<f64>,
}

node! {
    /// A layout container along one axis (CSS flex, SPEC §3.4). Its children name it in
    /// `at.parent`.
    StackNode {
        /// `y` (the default) stacks children top to bottom, `x` left to right.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub axis: Option<Axis>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub gap: Option<Length>,
        /// Where leftover space along the axis goes: `start` (the default), `center`,
        /// `end`, or between and around the children.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub distribute: Option<Distribute>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub padding: Option<Padding>,
        /// Rounds the corners of the container's `fill` and `stroke`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Axis {
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Distribute {
    Start,
    Center,
    End,
    Between,
    Around,
    Evenly,
}

node! {
    /// A layout container on a grid of its own (CSS grid, SPEC §3.4). Its children name it
    /// in `at.parent` and take an `area`, a `col`/`row` range, or the next free cell.
    GridNode {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub cols: Option<Tracks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub rows: Option<Tracks>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub gap: Option<Length>,
        /// Named areas, one string per row of cells, as CSS `grid-template-areas`:
        /// `["head head", "left right"]`; `.` leaves a cell unnamed.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub areas: Option<Vec<String>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub padding: Option<Padding>,
        /// Rounds the corners of the container's `fill` and `stroke`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
    }
}

/// A count of equal tracks, or each track's size.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Tracks {
    Count(#[schemars(range(min = 1))] u32),
    Sizes(Vec<SizeValue>),
}

node! {
    /// A layout container whose children place themselves by `at.rect`, relative to its
    /// padding (SPEC §3.4). Its children name it in `at.parent`.
    FrameNode {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub padding: Option<Padding>,
        /// Rounds the corners of the container's `fill` and `stroke`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
    }
}

node! {
    /// Nodes drawn together, each placed on the slide by its own `at` (SPEC §3.4). Its
    /// children name it in `at.parent`.
    GroupNode {}
}

/// A node in the scene graph: its `type` and that type's properties. A node's type never
/// changes across states (E104).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "type", rename_all = "lowercase")]
#[schemars(rename = "Node")]
pub enum TypedNode {
    Text(Box<TextNode>),
    Shape(Box<ShapeNode>),
    Image(Box<ImageNode>),
    Chart(Box<ChartNode>),
    Table(Box<TableNode>),
    Shader(Box<ShaderNode>),
    Stack(Box<StackNode>),
    Grid(Box<GridNode>),
    Frame(Box<FrameNode>),
    Group(Box<GroupNode>),
}

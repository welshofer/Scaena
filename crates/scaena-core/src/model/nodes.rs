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
/// how it is turned, a container's tracks and spacing, and a text's lines. Each property takes
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
            /// Locked in an editor (PLAN 2.95): its canvas passes over the node, and moves,
            /// resizes, turns, retypes, and deletes nothing of it; its layers and its inspector
            /// select it and unlock it. The node's own, in every state: a state and `overrides`
            /// cannot set it (E104). It draws as it would unlocked.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub locked: Option<bool>,
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
            /// The shadow it casts (PLAN 2.115): one of the theme's by name, or one written out.
            #[serde(default, skip_serializing_if = "Option::is_none")]
            pub shadow: Option<ShadowRef>,
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
        /// The paragraphs a state draws (PLAN 2.129, deck format 0.27), counted from 1: one, or
        /// the first and the last of a run. The rest are laid out where they stand and not drawn,
        /// so a list built by bullet keeps its place as its items come and go. Without it, all.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub paragraphs: Option<Range>,
        /// The text is code (SPEC §3.5, ADR-0029; deck format 0.28): set as written, its lines
        /// broken only where it breaks them, its tabs standing to stops, grown then shrunk to fit
        /// its box, never under 12 pt.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub code: Option<Code>,
    }
}

/// What makes a text code (PLAN 2.131, ADR-0029): the language its words are written in.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Code {
    /// The language its words are written in, a lowercase name: one Scaena knows, by its name
    /// or an alias (`rust`, `rs`, `c++`), `plain` for none, or any other, which the generic
    /// scanner colors (`scaena_core::code`).
    #[schemars(regex(pattern = r"^[a-z0-9][a-z0-9+#._-]{0,31}$"))]
    pub language: String,
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

impl TextFit {
    /// The fit a text is set by (SPEC §3.4, ADR-0029): its own, else its slot's, else `shrink`
    /// for a shape's `words` and for `code`, else `wrap`. Code never wraps, so a `wrap` of its
    /// own (E106) or of its slot's is passed over. Layout, lint, and the editors read this one.
    pub fn effective(own: Option<TextFit>, slot: Option<TextFit>, words: bool, code: bool) -> TextFit {
        let set = |fit: Option<TextFit>| fit.filter(|f| !(code && *f == TextFit::Wrap));
        set(own).or(set(slot)).unwrap_or(if words || code { TextFit::Shrink } else { TextFit::Wrap })
    }
}

/// How a text's lines break where its words run past its width (SPEC §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Wrap {
    Greedy,
    Pretty,
    Balance,
    // Never: its lines break only where the text breaks them, as verse is set (PLAN O.4b), and
    // under `fit: shrink` a line too wide shrinks the whole text, as code does (ADR-0029).
    None,
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
        /// The box of the path's own coordinates that fits the node's box, [x, y, w, h], as
        /// SVG's `viewBox`: the path is scaled alike both ways to fit it and centered in the
        /// node's box, so icons drawn on one view keep one weight beside each other (PLAN
        /// 2.104). Without one, the path's own bounds fit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub view: Option<Rect>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub points: Option<Vec<Point>>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub radius: Option<Length>,
        /// The two nodes a line or an arrow joins (PLAN 2.109): it runs from `from`'s box to
        /// `to`'s as each state lays them out, and so follows them, and it leaves wherever either
        /// does. What it joins places it: it takes no `at` and no `points` of its own.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub connect: Option<Connect>,
        /// What a line's or an arrow's ends are drawn with (PLAN 2.128, deck format 0.27): its
        /// `start`, where its first point is, and its `end`. An end it does not name is a line's
        /// bare end, and an arrow's last a filled head.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub ends: Option<LineEnds>,
    }
}

/// A line's or an arrow's two ends (PLAN 2.128), as a presentation app's line endpoints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LineEnds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start: Option<LineEnd>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end: Option<LineEnd>,
}

/// What one end of a line is drawn with, sized from its stroke: nothing, a filled `arrow`head, an
/// `open` one of two strokes, or a filled `circle`, `square`, or `diamond` on the end.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum LineEnd {
    None,
    Arrow,
    Open,
    Circle,
    Square,
    Diamond,
}

/// The shadow a node casts (PLAN 2.115): the theme's shadow of this name (`tokens.shadow`), or
/// one written out, an override: `x`, `y`, and `blur` in canvas units, a `color`, and an
/// `opacity`. A filled rectangle or ellipse, an image, and a container's panel cast one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum ShadowRef {
    Named(String),
    Written(WrittenShadow),
}

/// A shadow written out (PLAN 2.115): these keys and no others, so one written as CSS writes
/// one (`spread`) is an error at the key, where it is written.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WrittenShadow {
    /// How far right of the node it falls; left where it is negative. None by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub x: Option<Length>,
    /// How far below the node it falls; above where it is negative. None by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub y: Option<Length>,
    /// How soft its edge is: the blur's radius, twice the deviation of the Gaussian it is
    /// drawn with. None by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blur: Option<Length>,
    /// A theme color: black by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<Color>,
    /// How dark it is, from 0 to 1: 1 by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub opacity: Option<f64>,
}

/// What a connector joins (PLAN 2.109): the node it runs from, the node it runs to, and how.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Connect {
    pub from: Id,
    pub to: Id,
    /// `straight`, the default, runs between the two boxes along the line from middle to middle;
    /// `elbow` leaves one box by the side that faces the other and turns at right angles; `curved`
    /// leaves and meets the boxes as an elbow does, a cubic curve through the elbow's corners
    /// (PLAN 2.128, deck format 0.27).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub route: Option<Route>,
}

/// How a connector runs between what it joins (PLAN 2.109, 2.128).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Route {
    Straight,
    Elbow,
    Curved,
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
        /// Shows the image only inside a shape (PLAN 2.113), fitted to what shows, as a
        /// presentation app masks a picture with a shape: an oval, or a polygon of the shape
        /// library. It takes the place of `radius`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub mask: Option<ImageMask>,
        /// Its light and color adjusted (PLAN 2.117), as a presentation app's Adjust Image
        /// adjusts a picture.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub adjust: Option<ImageAdjust>,
        /// Where the picture came from (ADR-0030, deck format 0.29): a map fetched once from a
        /// service, with when, what was asked, and the credit its terms ask shown with it, which
        /// the engine draws.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub fetched: Option<crate::fetched::Fetched>,
        /// A pin the engine draws on the picture (ADR-0030, decision 13), with its label.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pin: Option<crate::fetched::Pin>,
        /// Where its `fetched` credit is drawn: on a card inside the picture (the default), or
        /// as a line below it inside the box.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub credit: Option<crate::fetched::CreditPlace>,
    }
}

/// How an image's light and color are adjusted (PLAN 2.117): each from -1 to 1, where 0, or
/// none, leaves the picture as it is.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImageAdjust {
    /// Brighter or darker, as a camera's exposure: 1 is two stops brighter, -1 two darker.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -1, max = 1))]
    pub exposure: Option<f64>,
    /// The lights lighter and the darks darker, or both nearer the middle: -1 is a flat gray.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -1, max = 1))]
    pub contrast: Option<f64>,
    /// More color, or less: -1 is black and white, 1 twice the color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -1, max = 1))]
    pub saturation: Option<f64>,
    /// Warmer, toward orange, or cooler, toward blue, as a camera's white balance.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = -1, max = 1))]
    pub warmth: Option<f64>,
}

/// The shape an image shows inside (PLAN 2.113): an oval, or a polygon of the shape library
/// (`scaena_core::inserts::polygons`), by its name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ImageMask {
    Ellipse,
    Triangle,
    RightTriangle,
    Diamond,
    Parallelogram,
    Trapezoid,
    Pentagon,
    Hexagon,
    Octagon,
    Star,
    Chevron,
    RightArrow,
    LeftArrow,
    UpArrow,
    DownArrow,
    Plus,
    Callout,
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
        /// Which way a `bar`'s or a `stackedBar`'s bars run (PLAN 1.29): up from the
        /// baseline, the default, or across it, the categories down the side and their names
        /// read across. Any other kind is E106.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub orient: Option<ChartOrient>,
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
        /// A `range`'s interval about each value (PLAN 1.30): the fields that hold its low
        /// and high ends. Any other kind is E106.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub interval: Option<Interval>,
        /// Small multiples (PLAN 1.31): a panel for each value of a field, on one shared
        /// scale, each named by its value.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub facet: Option<Facet>,
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
        /// A `weather` chart's fields, units, and parts (deck format 0.32, ADR-0030): any other
        /// kind is E106.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub weather: Option<Weather>,
    }
}

/// What a `weather` chart draws (SPEC §3.7, ADR-0030 decision 19): a column for each period of a
/// forecast, across the box, or a row down a box taller than wide, each its name, its sky's icon,
/// and its figures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Weather {
    /// The columns each part reads, where they are not the forecast's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fields: Option<WeatherFields>,
    /// The units it shows its figures in: the source's by default. They convert at layout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<crate::weather::Units>,
    /// The parts each period shows, in this order; `period`, `sky`, `high`, `low`, and `chance`
    /// when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<Vec<WeatherPart>>,
    /// Hours between columns of a forecast by the hour; unset, 1, 2, 3, or 6, the least that fits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 24))]
    pub every: Option<u32>,
    /// The theme color a highlighted period stands on: `surface-2` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub band: Option<String>,
}

/// The columns a `weather` chart reads its parts from (ADR-0030, decision 18).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct WeatherFields {
    /// The WMO code of each period's sky: `sky`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sky: Option<String>,
    /// The high: `high` by the day, `temperature` by the hour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high: Option<String>,
    /// The low: `low`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub low: Option<String>,
    /// The chance of rain, in percent: `rain`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chance: Option<String>,
    /// The wind's speed: `wind`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wind: Option<String>,
    /// Whether an hour is in daylight: `day`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub day: Option<String>,
}

/// A part of a `weather` chart's period.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum WeatherPart {
    /// Its name: the weekday, or the hour.
    Period,
    /// The sky's icon.
    Sky,
    /// The sky in words.
    Words,
    High,
    Low,
    /// The chance of rain, with a drop where it is 10% or more.
    Chance,
    Wind,
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

/// v1 chart kinds (SPEC §3.7); waffle and heatmap are deferred. A `slope` compares two
/// states, a line a series from the first to the second, both ends labeled and the change
/// said; a `range` spans the value axis at each category, a dumbbell between its series or
/// each value with its `interval` (PLAN 1.30); a `weather` chart draws a forecast, a column for
/// each period, its sky's icon and its figures (ADR-0030).
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
    Slope,
    Range,
    // A forecast: a column for each period, its sky's icon and its figures (ADR-0030).
    Weather,
}

/// How a chart splits into small multiples (SPEC §3.7, PLAN 1.31).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Facet {
    /// The field whose values split the data, a panel each, in the order they first appear.
    pub field: String,
    /// How many panels stand in a row; unset, the count that makes each panel nearest 3:2.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub columns: Option<u32>,
}

/// The fields that hold each value's interval on a `range` (SPEC §3.7, PLAN 1.30).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Interval {
    /// The field of its low end.
    pub low: String,
    /// The field of its high end.
    pub high: String,
}

/// Which way a chart's bars run (SPEC §3.7, PLAN 1.29).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ChartOrient {
    /// Up from the baseline, the categories along the foot.
    Vertical,
    /// Across from the baseline, the categories down the side.
    Horizontal,
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

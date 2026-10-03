//! The theme, a design system (SPEC §3.6): tokens, typographic roles, the grid, layout
//! templates, motion, shader palettes, chart styling. Documents reference these by name.

use super::nodes::{LabelShow, LegendPlace, ShaderKind, ShaderParam};
use super::values::{Features, FontAxes, NonNegative, Range, SplitUnit, SpringParams};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Tokens, typographic roles, layout templates, motion, and palettes. Documents reference
/// these by name; they never store resolved values. See docs/SPEC.md §3.6.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(title = "Scaena theme (design system)")]
pub struct Theme {
    /// Theme format version.
    // The schema's pattern for it comes from `crate::THEME_FORMAT_VERSION`.
    #[serde(rename = "scaena-theme")]
    pub version: String,
    #[schemars(length(min = 1))]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub tokens: Tokens,
    #[serde(rename = "type")]
    pub typography: Typography,
    pub grid: Grid,
    #[schemars(extend("required" = ["title", "full"], "minProperties" = 2))]
    pub layouts: IndexMap<String, Layout>,
    /// The grid in other formats than the deck's own (SPEC §3.4): `"9:16": { "grid": … }`.
    /// Each layout says its own slots there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<IndexMap<super::Format, ThemeFormat>>,
    pub motion: Motion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shaders: Option<Shaders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charts: Option<Charts>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tables: Option<Tables>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
}

/// `#rrggbb[aa]`, `oklch(...)`, or `oklab(...)`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct ColorLiteral(
    #[schemars(regex(pattern = r"^(#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?|oklch\(.*\)|oklab\(.*\))$"))] pub String,
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Tokens {
    #[schemars(extend("minProperties" = 2))]
    pub color: IndexMap<String, ColorLiteral>,
    /// Color roles, each the name of a color token.
    #[schemars(extend("required" = ["surface", "onSurface", "accent", "onAccent"]))]
    pub roles: IndexMap<String, String>,
    pub data: DataPalettes,
    pub space: Space,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub radius: Option<Radius>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<IndexMap<String, NonNegative>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct DataPalettes {
    #[schemars(length(min = 3))]
    pub categorical: Vec<ColorLiteral>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 2))]
    pub sequential: Option<Vec<ColorLiteral>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 3))]
    pub diverging: Option<Vec<ColorLiteral>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Space {
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub unit: f64,
    #[schemars(length(min = 4))]
    pub scale: Vec<NonNegative>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Radius {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<Vec<NonNegative>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Typography {
    #[schemars(extend("minProperties" = 1))]
    pub families: IndexMap<String, Family>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scale: Option<TypeScale>,
    #[schemars(extend("required" = ["display", "headline", "body", "caption", "label", "numeral"]))]
    pub roles: IndexMap<String, Role>,
}

/// A font family: its file in the bundle and how it may vary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Family {
    pub family: String,
    #[schemars(regex(pattern = r"^fonts/"))]
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<IndexMap<String, [f64; 2]>>,
    /// Other families in this theme, in order. Bundle-only; system fonts are never consulted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<Features>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypeScale {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 1))]
    pub ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub base: Option<f64>,
}

/// What a text role looks like (SPEC §3.5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Role {
    /// Key in `type.families`.
    pub family: String,
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub size: f64,
    #[schemars(range(min = 1, max = 1000))]
    pub weight: u16,
    /// Line height as a multiple of size.
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub leading: f64,
    /// Em units; negative tightens.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tracking: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opsz: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<FontAxes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<Features>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wrap: Option<super::nodes::Wrap>,
    #[serde(rename = "box", default, skip_serializing_if = "Option::is_none")]
    pub text_box: Option<super::nodes::TextBox>,
    /// What of the text sits on the baseline grid (`grid.baseline`, SPEC §3.4): every
    /// baseline, or the first line's cap height. Nothing snaps when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snap: Option<Snap>,
    /// Max characters per line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub measure: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub max_lines: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub min_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub max_size: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub case: Option<Case>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub numeric: Option<super::nodes::Numeric>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1))]
    pub min_last_line_words: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub optical_margins: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hanging_punctuation: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hyphenate: Option<bool>,
    /// Color role or token; defaults to onSurface.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// What of a text role sits on the baseline grid (SPEC §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Snap {
    /// Every line's baseline, the lines whole grid lines apart: body text.
    Baseline,
    /// The first line's cap height, the lines below at the role's leading: display text.
    Cap,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Case {
    None,
    Upper,
    Lower,
    Title,
    Smallcaps,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Grid {
    #[schemars(range(min = 1))]
    pub columns: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1), extend("default" = 6))]
    pub rows: Option<u32>,
    #[schemars(range(min = 0))]
    pub gutter: f64,
    pub margin: Margin,
    /// Baseline grid: the distance between its lines in cu, which run from the top
    /// margin. Text roles with `snap` sit on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub baseline: Option<f64>,
}

/// One margin for every side, or 2–4 CSS-style.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Margin {
    All(#[schemars(range(min = 0))] f64),
    Sides(#[schemars(length(min = 2, max = 4))] Vec<NonNegative>),
}

/// A layout template: named slots on the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Layout {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The slots that move in other formats (SPEC §3.4): `"9:16": { "slots": … }`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<IndexMap<super::Format, LayoutFormat>>,
    #[schemars(extend("minProperties" = 1))]
    pub slots: IndexMap<String, Slot>,
}

/// A theme in another format: the grid there.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ThemeFormat {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grid: Option<Grid>,
}

/// A layout in another format: slots that take the place of its slots of the same
/// names there. The rest stay as they are.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct LayoutFormat {
    #[schemars(extend("minProperties" = 1))]
    pub slots: IndexMap<String, Slot>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub col: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<Range>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<super::values::Align>,
    /// Default text role for nodes placed in this slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Motion {
    #[schemars(extend("required" = ["fast", "standard", "slow"]))]
    pub durations: IndexMap<String, NonNegative>,
    #[schemars(extend("required" = ["standard", "in", "out"]))]
    pub easings: IndexMap<String, [f64; 4]>,
    #[schemars(extend("minProperties" = 1))]
    pub springs: IndexMap<String, SpringParams>,
    #[schemars(extend("required" = ["fade", "rise", "grow"]))]
    pub presets: IndexMap<String, Preset>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1), extend("default" = 12))]
    pub max_concurrent: Option<u32>,
    /// Max total choreography per state, ms (lint W321).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0), extend("default" = 2500))]
    pub max_build: Option<f64>,
}

/// A named entrance, exit, or emphasis: keyframes with parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Preset {
    /// The look an entrance starts from, or an exit ends at.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<super::values::PresetLook>,
    /// The look an emphasis goes out to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<super::values::PresetLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration: Option<super::values::Duration>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ease: Option<super::values::Easing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spring: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger: Option<NonNegative>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split: Option<SplitUnit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shaders {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("additionalProperties" = {"type": "array", "minItems": 2, "items": {"$ref": "#/$defs/ColorLiteral"}}))]
    pub palettes: Option<IndexMap<String, Vec<ColorLiteral>>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presets: Option<IndexMap<String, ShaderPreset>>,
}

/// A shader a node can name (SPEC §3.8): its kind, and the palette and params a node of
/// that kind takes unless it sets its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
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
pub struct ShaderPreset {
    pub kind: ShaderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<IndexMap<String, ShaderParam>>,
    /// A shader palette from this theme.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette: Option<String>,
}

/// Chart styling: every chart draws with these, never with literals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Charts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axis: Option<ChartRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gridlines: Option<ChartRule>,
    /// Value labels: their role (`label` when unset), and which values print when a
    /// chart does not say (`auto` when unset: by kind).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<ChartValues>,
    /// Axis titles; their role defaults to the axis's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<ChartLabel>,
    /// About how many ticks a value axis shows (d3's tick count); 5 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 2, max = 20))]
    pub tick_count: Option<u32>,
    /// At most how many ticks, and so reference lines, a value axis shows; 5 when unset.
    /// The axis asks for `tickCount`, then for fewer, down to two, until its ticks fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 2, max = 20))]
    pub max_ticks: Option<u32>,
    /// The signal color: the one hue a chart spends on what matters, the data a
    /// highlight picks out; `accent` when unset. The rest of the data is the
    /// categorical palette, which a theme keeps neutral.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signal: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub corner_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub bar_gap: Option<f64>,
    /// Between the bars of one category's group, a fraction of each bar's slot; 0.1 when
    /// unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub group_gap: Option<f64>,
    /// A dot on each point of a line; none when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub point_radius: Option<f64>,
    /// A dot plot's dots, and a scatter's at its largest size; 8 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub dot_radius: Option<f64>,
    /// A donut's hole, a fraction of its radius; 0.6 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub donut_hole: Option<f64>,
    /// Legends: their role (the axis's when unset), and where a chart that does not say
    /// places one (`direct` when unset).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legend: Option<ChartLegend>,
    /// Annotations: rules, bands, callouts, and highlights.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub annotation: Option<ChartAnnotation>,
}

/// How a chart's annotations look (SPEC §3.7), every one a role, a token, or a fraction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ChartAnnotation {
    /// The text's role; the value labels' when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// A rule's and a callout's leader's stroke; `thin` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    /// The rules, the leaders, the bands, and the text; `accent` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// A rule's and a leader's opacity; 1 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub opacity: Option<f64>,
    /// A band's fill opacity; 0.12 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub band: Option<f64>,
    /// The opacity a highlight leaves the rest of the chart's marks at; 0.5 when unset.
    /// Their value labels and names dim half as far, so they stay legible.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub dimmed: Option<f64>,
}

/// Table styles (SPEC §3.6), every one a role, a token, or a length.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Tables {
    /// The header row's text: role `label` in `onSurfaceMuted` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub header: Option<TableText>,
    /// The cells' text: role `body` in its own color when unset. Numbers set in tabular
    /// lining figures.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell: Option<TableText>,
    /// The rule under the header: the `hairline` stroke in `onSurfaceMuted` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<ChartRule>,
    /// Rules between rows; none when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_rule: Option<ChartRule>,
    /// Canvas units above and below each row's text; half a space unit when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub row_gap: Option<f64>,
    /// Canvas units between columns; an em of the cell text when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub column_gap: Option<f64>,
    /// Whether a table spans its cell, its first column taking the room the cell has to
    /// spare; false when unset: a table is as wide as its columns, at the cell's start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stretch: Option<bool>,
}

/// A table's text: a role, and a color token or role.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TableText {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// A chart rule: the axis or the gridlines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChartRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ChartLabel {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// How value labels read, and which print when a chart does not say (SPEC §3.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartValues {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub show: Option<LabelShow>,
}

/// How legends read, and where they stand when a chart does not say (SPEC §3.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartLegend {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// `direct` when unset; `auto` means the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub place: Option<LegendPlace>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Density {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1), extend("default" = 40))]
    pub max_words_per_state: Option<u32>,
}

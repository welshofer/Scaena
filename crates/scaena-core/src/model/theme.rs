//! The theme, a design system (SPEC §3.6): tokens, typographic roles, the grid, layout
//! templates, motion, shader palettes, chart styling. Documents reference these by name.

use super::nodes::ShaderKind;
use super::values::{Features, FontAxes, NonNegative, Range, SplitUnit, SpringParams};
use indexmap::IndexMap;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    pub motion: Motion,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shaders: Option<Shaders>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charts: Option<Charts>,
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
    /// Baseline grid in cu; text leading snaps to multiples.
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
    /// Per-format overrides of slots (e.g. "9:16").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub formats: Option<IndexMap<String, Value>>,
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
    /// Property values at the start (enter) or end (exit).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<IndexMap<String, Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<IndexMap<String, Value>>,
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

/// A shader a node can name (PLAN 1.10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShaderPreset {
    pub kind: ShaderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<IndexMap<String, Value>>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<ChartLabel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_width: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub corner_radius: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub bar_gap: Option<f64>,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Density {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1), extend("default" = 40))]
    pub max_words_per_state: Option<u32>,
}

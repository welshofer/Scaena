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
    // The schema's pattern for it comes from `crate::THEME_FORMAT_VERSION`. An older format this
    // build reads is read as the current one (`crate::version`).
    #[serde(rename = "scaena-theme", deserialize_with = "read_format")]
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
    /// How code is set and colored (theme format 0.18, ADR-0029): the role a code text is set in,
    /// its tab stops, and each kind of token's look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<CodeLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
    /// A slide's number and footer: where each stands and in what type, and the layouts a slide
    /// shows neither on (theme format 0.14, ADR-0028). The deck says whether they show.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub furniture: Option<Furniture>,
}

/// A theme's format as read: an older one this build reads is the current one (SPEC §3.1).
fn read_format<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    let saved = String::deserialize(d)?;
    Ok(crate::version::THEME.read(&saved).to_string())
}

impl Theme {
    /// A theme from its JSON. Every crate parses a theme here, so the model's parser is
    /// compiled once: a WASM module carries one copy of it rather than one per crate that
    /// asks (SPEC §15).
    pub fn from_json(text: &str) -> Result<Theme, serde_json::Error> {
        serde_json::from_str(text)
    }

    /// A theme from a JSON value, through its text, as [`Theme::from_json`]; an error
    /// says what is wrong without a place in that text, which no file holds.
    pub fn from_value(value: &serde_json::Value) -> Result<Theme, String> {
        Theme::from_json(&value.to_string()).map_err(crate::document::unplaced)
    }

    /// The text role the theme keeps for code (ADR-0029): its `code.role`, else its `code` role,
    /// if it has one. A code text that names no role is set in it, else in `body`; and a deck's
    /// script stays out of its family's fallbacks, so code keeps its columns (ADR-0027).
    pub fn code_role(&self) -> Option<&str> {
        match self.code.as_ref().and_then(|c| c.role.as_deref()) {
            Some(role) => Some(role),
            None => self.typography.roles.contains_key("code").then_some("code"),
        }
    }

    /// The names this theme defines of one kind, in its order: what a deck may call them.
    /// A color is a color role or a token, the roles first, a name both have once; a step of the space or radius
    /// scale is `space.N` or `radius.N`; a data palette is `categorical`, then `sequential`
    /// and `diverging` where the theme has them.
    pub fn names(&self, of: Vocabulary) -> Vec<String> {
        fn keys<V>(map: &IndexMap<String, V>) -> Vec<String> {
            map.keys().cloned().collect()
        }
        fn steps(scale: Option<&Vec<NonNegative>>, token: &str) -> Vec<String> {
            (0..scale.map_or(0, Vec::len)).map(|i| format!("{token}.{i}")).collect()
        }
        let shaders = self.shaders.as_ref();
        match of {
            Vocabulary::TextRole => keys(&self.typography.roles),
            Vocabulary::FontFamily => keys(&self.typography.families),
            Vocabulary::Color => {
                let mut names = keys(&self.tokens.roles);
                names.extend(self.tokens.color.keys().filter(|k| !self.tokens.roles.contains_key(*k)).cloned());
                names
            }
            Vocabulary::Layout => keys(&self.layouts),
            Vocabulary::MotionPreset => keys(&self.motion.presets),
            Vocabulary::Duration => keys(&self.motion.durations),
            Vocabulary::Easing => keys(&self.motion.easings),
            Vocabulary::Spring => keys(&self.motion.springs),
            Vocabulary::ShaderPreset => shaders.and_then(|s| s.presets.as_ref()).map(keys).unwrap_or_default(),
            Vocabulary::ShaderPalette => shaders.and_then(|s| s.palettes.as_ref()).map(keys).unwrap_or_default(),
            Vocabulary::DataPalette => {
                let data = &self.tokens.data;
                let more = [("sequential", data.sequential.is_some()), ("diverging", data.diverging.is_some())];
                let more = more.into_iter().filter(|(_, has)| *has).map(|(name, _)| name);
                ["categorical"].into_iter().chain(more).map(String::from).collect()
            }
            Vocabulary::Stroke => self.tokens.stroke.as_ref().map(keys).unwrap_or_default(),
            Vocabulary::Shadow => self.tokens.shadow.as_ref().map(keys).unwrap_or_default(),
            Vocabulary::Radius => steps(self.tokens.radius.as_ref().and_then(|r| r.scale.as_ref()), "radius"),
            Vocabulary::Space => steps(Some(&self.tokens.space.scale), "space"),
        }
    }
}

/// A kind of name a theme defines, which a deck calls it by (SPEC §3.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Vocabulary {
    TextRole,
    FontFamily,
    /// A color role or a color token.
    Color,
    Layout,
    MotionPreset,
    Duration,
    Easing,
    Spring,
    ShaderPreset,
    ShaderPalette,
    /// A chart's color scale.
    DataPalette,
    /// A stroke token: a width.
    Stroke,
    /// A shadow token (PLAN 2.115).
    Shadow,
    /// A step of the radius scale.
    Radius,
    /// A step of the space scale.
    Space,
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
    /// The shadows a node may cast (PLAN 2.115), by name: `soft`, `raised`, and `floating` in
    /// the shipped themes, as a presentation app offers an object's shadow.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shadow: Option<IndexMap<String, Shadow>>,
}

/// A shadow a node casts (PLAN 2.115): how far it falls from the node, right and down, how soft
/// its edge is, its color, and how dark it is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Shadow {
    /// How far right of the node it falls, canvas units; left where it is negative.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub x: f64,
    /// How far below the node it falls, canvas units; above where it is negative.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y: f64,
    /// How soft its edge is, canvas units: the blur's radius, twice the deviation of the
    /// Gaussian it is drawn with.
    pub blur: NonNegative,
    /// Its color: black where it names none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<ColorLiteral>,
    /// How dark it is, from 0 to 1.
    #[schemars(range(min = 0, max = 1))]
    pub opacity: f64,
}

/// A shadow's offset of none is written as none, as the shipped themes write it.
fn is_zero(v: &f64) -> bool {
    *v == 0.0
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
    /// How a text's list items are marked and indented (SPEC §3.5, ADR-0018).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lists: Option<Lists>,
}

/// A theme's lists (ADR-0018): how far each level's words start from the one above, and the
/// least room between a marker and its words, both in ems of the text's size; a bullet for
/// each level, and a pattern for each level's numbers, whose first `1`, `a`, `A`, `i`, or `I`
/// is the item's number in digits, letters, or roman numerals. A level past a list's end
/// takes its last.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Lists {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("exclusiveMinimum" = 0))]
    pub indent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0))]
    pub gap: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub bullets: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1), inner(regex(pattern = "[1aAiI]")))]
    pub numbers: Option<Vec<String>>,
}

impl Lists {
    /// A level's indent and the gap, in ems, and its bullet and number pattern: the theme's,
    /// else 1.2 em, 0.4 em, `• – ·`, and `1. a. i.`.
    pub fn at(&self, level: u8) -> (f64, f64, &str, &str) {
        const BULLETS: [&str; 3] = ["\u{2022}", "\u{2013}", "\u{B7}"];
        const NUMBERS: [&str; 3] = ["1.", "a.", "i."];
        fn pick<'a>(list: Option<&'a Vec<String>>, default: &'static [&'static str; 3], level: u8) -> &'a str {
            match list.filter(|l| !l.is_empty()) {
                Some(l) => l[(level as usize).min(l.len() - 1)].as_str(),
                None => default[(level as usize).min(2)],
            }
        }
        let bullet = pick(self.bullets.as_ref(), &BULLETS, level);
        let number = pick(self.numbers.as_ref(), &NUMBERS, level);
        (self.indent.unwrap_or(1.2), self.gap.unwrap_or(0.4), bullet, number)
    }
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
    /// Its italic face (SPEC §3.5), which text set `italic` takes. A family without one sets
    /// such text upright: no italic is synthesized (lint W231).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<Face>,
    /// Other families in this theme, in order. Bundle-only; system fonts are never consulted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<Features>,
}

/// Another face of a family, in a file of its own: its italic (PLAN 2.40). Its file names
/// the family's name, as the family's own file does.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Face {
    #[schemars(regex(pattern = r"^fonts/"))]
    pub file: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub axes: Option<IndexMap<String, [f64; 2]>>,
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
    /// Set in its family's italic face (SPEC §3.5); upright when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
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
    /// What a person calls it in an editor (theme format 0.12, PLAN 3.31): `SWOT`, `Two by two`,
    /// `Q&A`. An editor without one makes a name of its key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Where an editor's gallery of slides to start offers it (theme format 0.11, PLAN 3.30): a
    /// section named for what the layouts in it hold (`Titles`, `Words`, `Pictures`), in the order
    /// the theme first names each.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
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
    /// How far in from its cells the slot stands on each side, canvas units (theme format 0.11,
    /// PLAN 3.30): words on a card that fills the same cells.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inset: Option<f64>,
    /// What goes in the slot (theme format 0.11, PLAN 3.30). Words a person types over: a new
    /// slide in the layout puts a text in the slot's `role` there, with these words, each item of
    /// a list a paragraph. A slot with no role says in words what goes in it, a picture or a
    /// figure, and a new slide leaves it empty, an editor outlining it. A shape: a new slide puts
    /// one there, under what is placed in the slots over it, a card or a rule. A format's slot
    /// keeps the layout's prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<Prompt>,
    /// The slot reaches the canvas's edge on each side where its tracks meet the edge of the grid
    /// (theme format 0.11, PLAN 3.30): a picture that bleeds, off the page or one side of it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bleed: Option<bool>,
    /// What the layout's picture shows in a slot that waits for a picture or a figure (theme
    /// format 0.12, PLAN 3.31): a picture, drawn by one of the theme's shader presets; or a chart
    /// of a kind, or a table, drawn from a few rows of their own. An editor draws each layout with
    /// these in its slots, so its picture shows what the slot is for. A new slide in the layout
    /// leaves the slot waiting all the same.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub example: Option<Example>,
    /// How a text placed in the slot fits it where the text does not say (theme format 0.13):
    /// `shrink` sets a number too wide for its slot smaller, down to its role's `minSize`, as a
    /// presentation app's placeholder shrinks what is typed in it to fit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<super::nodes::TextFit>,
}

impl Slot {
    /// Whether a node of `kind` goes in the slot, as Insert and a paste fill it (PLAN 2.34): a
    /// text where words go, in the slot's role; a picture or a figure where the slot says in
    /// words what goes there with no role to type it in (PLAN 3.30); a shape where its prompt is
    /// an icon, for another icon in its place (PLAN G.23); nothing where its prompt is another
    /// shape, a card or a rule made with the slide; and anything where it says nothing of what
    /// goes there.
    pub fn takes(&self, kind: crate::document::NodeType) -> bool {
        use crate::document::NodeType;
        match (&self.role, &self.prompt) {
            (_, Some(Prompt::Shape(shape))) => shape.shape == PromptShapeKind::Icon && kind == NodeType::Shape,
            (Some(_), _) => kind == NodeType::Text,
            (None, Some(_)) => matches!(kind, NodeType::Image | NodeType::Chart | NodeType::Table | NodeType::Shader),
            (None, None) => true,
        }
    }
}

/// What a layout's picture shows in a slot that waits (PLAN 3.31): `{ "picture": "backdrop" }`,
/// `{ "chart": "bar" }`, or `{ "table": 3 }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum Example {
    /// A picture, drawn by the theme's shader preset of this name.
    Picture(#[schemars(length(min = 1))] String),
    /// A chart of this kind, of a few rows made up for it.
    Chart(super::nodes::ChartKind),
    /// A table of this many columns, of a few rows made up for it.
    Table(#[schemars(range(min = 2, max = 6))] u32),
}

/// What goes in a slot (PLAN 3.30): words, a paragraph to each line; a list's items; or a shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Prompt {
    Words(#[schemars(length(min = 1))] String),
    Items(PromptItems),
    Shape(PromptShape),
}

/// A shape a new slide puts in a slot (PLAN 3.30): a card behind words, a rule between them, an
/// arrow from one to the next, or an icon over an item's words (PLAN G.23).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptShape {
    pub shape: PromptShapeKind,
    /// What fills a rectangle, an ellipse, a polygon, or an icon: a color token or role. A line
    /// and an arrow take the theme's rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
    /// What strokes its outline, a color token or role, in the theme's thin rule (theme format
    /// 0.12, PLAN 3.31): a ring, or circles that overlap and leave what is under them seen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke: Option<String>,
    /// Its points, as fractions of the slot: `[0, 0]` its top left, `[1, 1]` its bottom right
    /// (theme format 0.12, PLAN 3.31). A line or an arrow with none crosses the slot's middle,
    /// left to right; one down the slot, or back, gives them. A polygon gives three or more.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<super::values::Point>>,
    /// An icon's name, as the set it is drawn from names it (theme format 0.15, PLAN G.23): what
    /// the icon a new slide puts there is called, by a reader and in the layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub icon: Option<String>,
    /// An icon's outline, as SVG path data (theme format 0.15, PLAN G.23), drawn on its `view`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1))]
    pub path: Option<String>,
    /// The box of an icon's `path` that fits the slot, `[x, y, w, h]`, as a path shape's `view`
    /// fits its node (theme format 0.15, PLAN G.23): the icon as large as the slot takes, in its
    /// middle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub view: Option<super::values::Rect>,
}

impl PromptShape {
    /// The node an icon's prompt makes, as an inserted icon is made
    /// ([`crate::inserts::icon_node`]): its path on its view, filled as the prompt says or in the
    /// theme's accent, decoration named by the icon. None for another shape, or an icon with no
    /// path or view.
    pub fn icon_node(&self) -> Option<serde_json::Value> {
        let (PromptShapeKind::Icon, Some(path), Some(view)) = (self.shape, &self.path, &self.view) else {
            return None;
        };
        let name = self.icon.as_deref().unwrap_or("icon");
        Some(crate::inserts::icon_node(name, path, view.0, self.fill.as_deref().unwrap_or("accent")))
    }
}

/// The shapes a slot's prompt makes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PromptShapeKind {
    Rect,
    Ellipse,
    Line,
    Arrow,
    /// Theme format 0.12: a chevron, a triangle, or any outline its `points` give.
    Polygon,
    /// Theme format 0.15 (PLAN G.23): an icon, its `path` drawn on its `view` and named by
    /// `icon`. A new slide puts it there for a person to swap for another; a deck made from a
    /// prompt puts there the icon its item names.
    Icon,
}

/// A list's items, as a slot's prompt gives them: `{ "bullet": [...] }` or `{ "number": [...] }`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PromptItems {
    Bullet(#[schemars(length(min = 1))] Vec<String>),
    Number(#[schemars(length(min = 1))] Vec<String>),
}

impl Prompt {
    /// The words, a paragraph to each line, and the kind of list each paragraph is an item of,
    /// if any; none for a shape.
    pub fn text(&self) -> Option<(String, Option<super::values::ListKind>)> {
        use super::values::ListKind;
        match self {
            Prompt::Words(words) => Some((words.clone(), None)),
            Prompt::Items(PromptItems::Bullet(items)) => Some((items.join("\n"), Some(ListKind::Bullet))),
            Prompt::Items(PromptItems::Number(items)) => Some((items.join("\n"), Some(ListKind::Number))),
            Prompt::Shape(_) => None,
        }
    }
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
    /// Forecasts and estimates (PLAN 1.28): how a line dashes through what is projected,
    /// how much lighter an area is under it, and what its value labels add.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projected: Option<ChartProjected>,
}

/// How a chart shows the rows its `projected` marks (SPEC §3.7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChartProjected {
    /// A line's dash and the gap after it, in widths of the line; [3, 2] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("items" = {"type": "number", "exclusiveMinimum": 0}))]
    pub dash: Option<[f64; 2]>,
    /// An area's fill under projected rows, a fraction of its own; 0.5 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 1))]
    pub opacity: Option<f64>,
    /// What a projected value's label says after the value; `est.` when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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

/// How code is set and colored (SPEC §3.6, ADR-0029; theme format 0.18). A kind of token the
/// theme names no color for takes one derived from its accent, and every color code is drawn in
/// is held, where it is drawn, to read on its ground.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CodeLook {
    /// The text role a code text is set in where it names none: the theme's `code` role when
    /// unset, else `body`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    /// The columns between tab stops; 4 when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 16))]
    pub tab: Option<u8>,
    /// Each kind of token's look, by the kind's name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub syntax: Option<Syntax>,
}

/// Each kind of token's look in code (ADR-0029), by the kind's name: the 16 kinds the scanners
/// read. A kind left out is derived.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Syntax {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyword: Option<SyntaxLook>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub function: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub string: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub constant: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub punctuation: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub property: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attribute: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tag: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub variable: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inserted: Option<SyntaxLook>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deleted: Option<SyntaxLook>,
}

impl Syntax {
    /// The look the theme gives `class`, if it gives one: none for plain text, which is set in the
    /// text's own look.
    pub fn of(&self, class: crate::code::Class) -> Option<&SyntaxLook> {
        use crate::code::Class;
        match class {
            Class::Plain => None,
            Class::Keyword => self.keyword.as_ref(),
            Class::Type => self.type_.as_ref(),
            Class::Function => self.function.as_ref(),
            Class::String => self.string.as_ref(),
            Class::Number => self.number.as_ref(),
            Class::Constant => self.constant.as_ref(),
            Class::Comment => self.comment.as_ref(),
            Class::Operator => self.operator.as_ref(),
            Class::Punctuation => self.punctuation.as_ref(),
            Class::Property => self.property.as_ref(),
            Class::Attribute => self.attribute.as_ref(),
            Class::Tag => self.tag.as_ref(),
            Class::Variable => self.variable.as_ref(),
            Class::Meta => self.meta.as_ref(),
            Class::Inserted => self.inserted.as_ref(),
            Class::Deleted => self.deleted.as_ref(),
        }
    }
}

/// A kind of token's look in code: its color, weight, and italic, each the derived one where
/// unset.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SyntaxLook {
    /// A color token or role, or a literal, which a theme may hold: drawn a step lighter or darker
    /// where its ground needs it to read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// Its weight: else the text's, and 600 at least for a keyword.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 1000))]
    pub weight: Option<u16>,
    /// Whether it is set in its family's italic face, where the family has one: else as the text
    /// is, and in italic for a comment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub italic: Option<bool>,
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

/// A slide's furniture (theme format 0.14, ADR-0028): its number and its footer, each set in a
/// text role in the margin at the canvas's foot or head, across columns of the grid.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Furniture {
    /// Where the slide's number stands, where the deck shows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub number: Option<FurnitureItem>,
    /// Where the deck's footer stands, where the deck has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub footer: Option<FurnitureItem>,
    /// The layouts a slide in shows neither, unless it says so (a title, a divider, a picture
    /// that bleeds to the foot).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub except: Vec<String>,
}

/// One piece of a slide's furniture: a line of words in the margin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FurnitureItem {
    /// The grid's columns it spans, in each format's grid.
    pub col: Range,
    /// The margin it stands in: the canvas's foot, or its head.
    #[serde(default, skip_serializing_if = "Edge::is_foot")]
    pub edge: Edge,
    /// The text role it is set in.
    pub role: String,
    /// How it lines up across its columns; start by default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub align: Option<Across>,
    /// A color token or role it is set in, in place of its role's color.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    /// How the number is written: `{n}` the slide's number, `{total}` how many slides the deck
    /// has (`"{n} / {total}"`); `{n}` by default. The footer's words are the deck's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

/// How a piece of furniture lines up across its columns.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Across {
    #[default]
    Start,
    Center,
    End,
}

/// The margin a piece of furniture stands in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    #[default]
    Foot,
    Head,
}

impl Edge {
    fn is_foot(&self) -> bool {
        *self == Edge::Foot
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct Density {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1), extend("default" = 40))]
    pub max_words_per_state: Option<u32>,
}

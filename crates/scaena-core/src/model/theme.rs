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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub density: Option<Density>,
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
}

/// What goes in a slot (PLAN 3.30): words, a paragraph to each line; a list's items; or a shape.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum Prompt {
    Words(#[schemars(length(min = 1))] String),
    Items(PromptItems),
    Shape(PromptShape),
}

/// A shape a new slide puts in a slot (PLAN 3.30): a card behind words, or a rule between them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PromptShape {
    pub shape: PromptShapeKind,
    /// What fills a rectangle or an ellipse: a color token or role. A line and an arrow take the
    /// theme's rule.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fill: Option<String>,
}

/// The shapes a slot's prompt makes: those that need no points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum PromptShapeKind {
    Rect,
    Ellipse,
    Line,
    Arrow,
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

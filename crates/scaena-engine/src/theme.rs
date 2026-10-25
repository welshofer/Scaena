//! The theme in the engine (SPEC §3.6, ADR-0005): the typed design system from
//! `scaena-core::model`, and the lookups layout, text, charts, shaders, and motion make in
//! it. What a node's role, style, and overrides add up to is [`crate::cascade`].

use crate::EngineError;
use indexmap::IndexMap;
use scaena_core::displaylist::Color;
use scaena_core::model::Format;
use scaena_core::model::nodes as node_model;
use scaena_core::model::theme as model;
use scaena_core::model::values::FeatureValue;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::ops::Deref;

/// A loaded theme: the design system, typed. It reads as the model it wraps
/// (`theme.grid`, `theme.tokens`), with the lookups the engine makes in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme(model::Theme);

impl Deref for Theme {
    type Target = model::Theme;
    fn deref(&self) -> &model::Theme {
        &self.0
    }
}

/// A typographic role as text is set in it (SPEC §3.5): the theme's role, resolved, in
/// the units the text engine takes.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRole {
    /// Key in `type.families`.
    pub family: String,
    pub size: f32,
    pub weight: f32,
    /// Asks for the family's italic face (SPEC §3.5); a family without one sets it upright.
    pub italic: bool,
    /// Line height as a multiple of `size`.
    pub leading: f32,
    /// Letter spacing in em; negative tightens.
    pub tracking: f32,
    pub opsz: Option<f32>,
    pub axes: BTreeMap<String, f32>,
    /// OpenType features: 0 off, 1 on, or an alternate's index.
    pub features: BTreeMap<String, u16>,
    pub wrap: Wrap,
    pub text_box: TextBox,
    /// What of the text sits on the baseline grid, if anything (SPEC §3.4).
    pub snap: Option<model::Snap>,
    pub measure: Option<f32>,
    pub max_lines: Option<u32>,
    pub min_size: Option<f32>,
    pub max_size: Option<f32>,
    pub case: Option<model::Case>,
    pub numeric: Option<Numeric>,
    pub min_last_line_words: Option<u32>,
    pub optical_margins: bool,
    pub hanging_punctuation: bool,
    pub hyphenate: bool,
    /// Color role or token; `onSurface` when the role names none.
    pub color: Option<String>,
}

impl TextRole {
    fn from_model(name: &str, r: &model::Role) -> Result<TextRole, EngineError> {
        Ok(TextRole {
            family: r.family.clone(),
            size: r.size as f32,
            weight: f32::from(r.weight),
            italic: r.italic.unwrap_or(false),
            leading: r.leading as f32,
            tracking: r.tracking.unwrap_or(0.0) as f32,
            opsz: r.opsz.map(|v| v as f32),
            axes: scaena_core::sort::map(r.axes.iter().flatten().map(|(k, v)| (k.clone(), *v as f32))),
            features: features(r.features.as_ref()).map_err(|e| EngineError::Theme(format!("role `{name}`: {e}")))?,
            wrap: r.wrap.map_or(Wrap::Greedy, Wrap::from),
            text_box: r.text_box.map_or(TextBox::Line, TextBox::from),
            snap: r.snap,
            measure: r.measure.map(|v| v as f32),
            max_lines: r.max_lines,
            min_size: r.min_size.map(|v| v as f32),
            max_size: r.max_size.map(|v| v as f32),
            case: r.case,
            numeric: r.numeric.map(Numeric::from),
            min_last_line_words: r.min_last_line_words,
            optical_margins: r.optical_margins.unwrap_or(false),
            hanging_punctuation: r.hanging_punctuation.unwrap_or(false),
            hyphenate: r.hyphenate.unwrap_or(false),
            color: r.color.clone(),
        })
    }
}

/// OpenType feature settings as the shaper takes them: 0 off, 1 on, or an index.
pub fn features(f: Option<&IndexMap<String, FeatureValue>>) -> Result<BTreeMap<String, u16>, String> {
    scaena_core::sort::try_map(f.into_iter().flatten().map(|(tag, v)| {
        let n = match *v {
            FeatureValue::Switch(on) => u16::from(on),
            FeatureValue::Alternate(i) => {
                u16::try_from(i).map_err(|_| format!("feature `{tag}`: {i} is not an alternate's index"))?
            }
        };
        Ok((tag.clone(), n))
    }))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wrap {
    #[default]
    Greedy,
    Pretty,
    Balance,
    /// Lines break only where the text breaks them (PLAN O.4b).
    None,
}

impl From<node_model::Wrap> for Wrap {
    fn from(w: node_model::Wrap) -> Wrap {
        match w {
            node_model::Wrap::Greedy => Wrap::Greedy,
            node_model::Wrap::Pretty => Wrap::Pretty,
            node_model::Wrap::Balance => Wrap::Balance,
            node_model::Wrap::None => Wrap::None,
        }
    }
}

/// Which box a text node's height is measured against (SPEC §3.4 `box`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBox {
    Cap,
    #[default]
    Line,
}

impl From<node_model::TextBox> for TextBox {
    fn from(b: node_model::TextBox) -> TextBox {
        match b {
            node_model::TextBox::Cap => TextBox::Cap,
            node_model::TextBox::Line => TextBox::Line,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Numeric {
    TabularLining,
    TabularOldstyle,
    ProportionalLining,
    ProportionalOldstyle,
}

impl From<node_model::Numeric> for Numeric {
    fn from(n: node_model::Numeric) -> Numeric {
        match n {
            node_model::Numeric::TabularLining => Numeric::TabularLining,
            node_model::Numeric::TabularOldstyle => Numeric::TabularOldstyle,
            node_model::Numeric::ProportionalLining => Numeric::ProportionalLining,
            node_model::Numeric::ProportionalOldstyle => Numeric::ProportionalOldstyle,
        }
    }
}

impl Numeric {
    /// The OpenType features that select this numeral style.
    pub fn features(self) -> [(&'static str, u16); 4] {
        let (tabular, lining) = match self {
            Numeric::TabularLining => (true, true),
            Numeric::TabularOldstyle => (true, false),
            Numeric::ProportionalLining => (false, true),
            Numeric::ProportionalOldstyle => (false, false),
        };
        [
            ("tnum", u16::from(tabular)),
            ("pnum", u16::from(!tabular)),
            ("lnum", u16::from(lining)),
            ("onum", u16::from(!lining)),
        ]
    }
}

impl Theme {
    pub fn from_json(s: &str) -> Result<Theme, EngineError> {
        scaena_core::model::theme::Theme::from_json(s).map(Theme).map_err(|e| EngineError::Theme(e.to_string()))
    }

    /// The color a token or color role names, following role → token: as written, and
    /// as sRGB. `color.` before the name is allowed.
    pub fn color_literal(&self, name: &str) -> Option<&str> {
        let tokens = &self.tokens;
        let name = name.strip_prefix("color.").unwrap_or(name);
        let literal = tokens.color.get(name).or_else(|| tokens.color.get(tokens.roles.get(name)?))?;
        Some(literal.0.as_str())
    }

    /// A color as sRGB: a token or color role the theme names, or a literal written out
    /// (hex, `oklch(…)`, or `oklab(…)`), which only `overrides` should hold (lint W300).
    pub fn color(&self, name: &str) -> Result<Color, EngineError> {
        let written = name.starts_with('#') || name.starts_with("oklch(") || name.starts_with("oklab(");
        let literal = match written {
            true => name,
            false => self.color_literal(name).ok_or_else(|| EngineError::Theme(format!("unknown color `{name}`")))?,
        };
        scaena_core::color::parse(literal).map_err(|e| EngineError::Theme(format!("color `{name}`: {e}")))
    }

    /// How a code text set in `text` draws each class of its tokens on `ground` (ADR-0029, PLAN
    /// 2.132), by `Class as usize`, plain first. A kind's color is the one the theme's `code.syntax`
    /// names, else the one `palette::syntax` derives from its accent; plain words take the text's
    /// own. Every color is held to read on `ground` by the least step of lightness: at 4.5:1, and
    /// a keyword at 7:1 where the ground allows. A kind's weight and italic are the theme's, else
    /// the text's, but for a keyword, at 600 at least, and a comment, in italic; italic only where
    /// the family has an italic face. A color with alpha is taken as it lies on `ground`.
    pub fn code_looks(&self, ground: [u8; 3], text: &TextRole) -> Result<crate::code::Looks, EngineError> {
        use crate::code::TokenLook;
        use scaena_core::code::Class;
        use scaena_core::palette;
        let on = |c: Color| -> [u8; 3] {
            let [r, g, b, a] = c.0.map(u32::from);
            let mix = |c: u32, g: u8| ((c * a + u32::from(g) * (255 - a) + 127) / 255) as u8;
            [mix(r, ground[0]), mix(g, ground[1]), mix(b, ground[2])]
        };
        let own = on(self.color(text.color.as_deref().unwrap_or("onSurface"))?);
        let accent = self.color("accent").map_or(own, on);
        let second = self.color("accent-2").ok().map(on);
        let muted = self.color("onSurfaceMuted").map_or(own, on);
        let derived = palette::syntax(ground, own, accent, second, muted);
        let syntax = self.code.as_ref().and_then(|c| c.syntax.as_ref());
        let face = self.has_italic(&text.family);
        let opaque = |[r, g, b]: [u8; 3]| [r, g, b, 255];
        let plain = TokenLook {
            color: opaque(palette::held(own, ground, palette::READS)),
            weight: text.weight,
            italic: text.italic && face,
        };
        let mut looks = [plain; 17];
        for (i, class) in Class::KINDS.into_iter().enumerate() {
            let named = syntax.and_then(|s| s.of(class));
            let color = match named.and_then(|l| l.color.as_deref()) {
                Some(name) if class == Class::Keyword => palette::held_strong(on(self.color(name)?), ground),
                Some(name) => palette::held(on(self.color(name)?), ground, palette::READS),
                None => derived[i],
            };
            let weight = match class {
                Class::Keyword => text.weight.max(600.0),
                _ => text.weight,
            };
            let italic = named.and_then(|l| l.italic).unwrap_or(class == Class::Comment || text.italic);
            looks[class as usize] = TokenLook {
                color: opaque(color),
                weight: named.and_then(|l| l.weight).map_or(weight, f32::from),
                italic: italic && face,
            };
        }
        Ok(looks)
    }

    /// The card a code text stands on (PLAN 2.136, ADR-0029): the theme's `code.card`, its fill
    /// taken as it lies on the surface, else `surface-2` stepped from the surface by the least
    /// lightness to stand 1.3:1 off it, so it shows on a projector.
    pub fn code_card(&self) -> Result<crate::code::CardLook, EngineError> {
        use scaena_core::palette;
        let [r, g, b, _] = self.color("surface")?.0;
        let surface = [r, g, b];
        let card = self.code.as_ref().and_then(|c| c.card.as_ref());
        let fill = match card.and_then(|c| c.fill.as_deref()) {
            Some(name) => over(self.color(name)?, surface),
            None => palette::held(
                self.color("surface-2").map_or(surface, |c| over(c, surface)),
                surface,
                crate::code::CARD_STEP,
            ),
        };
        Ok(crate::code::CardLook {
            fill: [fill[0], fill[1], fill[2], 255],
            radius: card.and_then(|c| c.radius.as_ref()).map(|r| serde_json::to_value(r).unwrap_or_default()),
            inset: card.and_then(|c| c.inset).map_or(crate::code::INSET, |i| i as f32),
            stroke: card.and_then(|c| c.stroke.clone()),
            shadow: card.and_then(|c| c.shadow.clone()),
        })
    }

    /// How a code text set in `text` numbers its lines on `ground`, its first line going by
    /// `start` (PLAN 2.136): in the theme's `code.numbers.color`, else the muted text color, held
    /// to read on `ground` as its code is, `code.numbers.gap` ems from the code.
    pub fn code_numbers(&self, ground: [u8; 3], start: u32) -> Result<crate::code::Numbers, EngineError> {
        use scaena_core::palette;
        let numbers = self.code.as_ref().and_then(|c| c.numbers.as_ref());
        let color = over(self.color(numbers.and_then(|n| n.color.as_deref()).unwrap_or("onSurfaceMuted"))?, ground);
        let [r, g, b] = palette::held(color, ground, palette::READS);
        let gap = numbers.and_then(|n| n.gap).map_or(crate::code::GAP, |g| g as f32);
        Ok(crate::code::Numbers { start, color: [r, g, b, 255], gap })
    }

    /// A typographic role, as text is set in it.
    pub fn text_role(&self, name: &str) -> Result<TextRole, EngineError> {
        let role =
            self.typography.roles.get(name).ok_or_else(|| EngineError::Theme(format!("unknown text role `{name}`")))?;
        TextRole::from_model(name, role)
    }

    /// Every font family, in theme order.
    pub fn families(&self) -> &IndexMap<String, model::Family> {
        &self.typography.families
    }

    /// Whether the family keyed `key` has an italic face (PLAN 2.40): what text it sets in
    /// italic takes. Without one, such text is set upright.
    pub fn has_italic(&self, key: &str) -> bool {
        self.families().get(key).is_some_and(|f| f.italic.is_some())
    }

    /// The font stack for a family key: the family, then its fallbacks in order, as
    /// family names, without repeats. Only the family's own `fallback` list is
    /// followed; fallbacks of fallbacks are not, so the stack is exactly what the
    /// theme author wrote.
    pub fn family_stack(&self, key: &str) -> Result<Vec<String>, EngineError> {
        let lookup = |k: &str| {
            self.families()
                .get(k)
                .ok_or_else(|| EngineError::Theme(format!("unknown font family `{k}` in type.families")))
        };
        let primary = lookup(key)?;
        let mut stack = vec![primary.family.clone()];
        for k in primary.fallback.iter().flatten() {
            let name = &lookup(k)?.family;
            if !stack.contains(name) {
                stack.push(name.clone());
            }
        }
        Ok(stack)
    }

    /// A named duration in ms, or a number of ms as it is.
    pub fn duration(&self, v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => self.motion.durations.get(s).map(|d| d.0),
            _ => None,
        }
    }

    /// A named spring.
    pub fn spring(&self, name: &str) -> Option<scaena_core::timeline::Spring> {
        let s = self.motion.springs.get(name)?;
        Some(scaena_core::timeline::Spring { stiffness: s.stiffness, damping: s.damping, mass: s.mass.unwrap_or(1.0) })
    }

    /// A named motion preset.
    pub fn preset(&self, name: &str) -> Option<&model::Preset> {
        self.motion.presets.get(name)
    }

    /// A named easing as a cubic Bézier.
    pub fn easing(&self, name: &str) -> Option<scaena_core::timeline::CubicBezier> {
        let [a, b, c, d] = *self.motion.easings.get(name)?;
        Some(scaena_core::timeline::CubicBezier(a, b, c, d))
    }

    /// A layout template's slots, by name.
    pub fn slots(&self, layout: &str) -> Option<&IndexMap<String, model::Slot>> {
        Some(&self.layouts.get(layout)?.slots)
    }

    /// This theme as it lays out in `format` (SPEC §3.4): the format's grid, if it gives
    /// one, and each layout's slots with those it names in that format in their place.
    pub fn in_format(&self, format: Format) -> Theme {
        let mut theme = self.0.clone();
        if let Some(grid) = (self.formats.as_ref()).and_then(|f| f.get(&format)).and_then(|f| f.grid.clone()) {
            theme.grid = grid;
        }
        for layout in theme.layouts.values_mut() {
            if let Some(there) = layout.formats.as_ref().and_then(|f| f.get(&format)) {
                let slots = there.slots.clone();
                layout.slots.extend(slots);
            }
        }
        Theme(theme)
    }

    /// A shader preset by name (SPEC §3.8).
    pub fn shader_preset(&self, name: &str) -> Result<&model::ShaderPreset, EngineError> {
        (self.shaders.as_ref().and_then(|s| s.presets.as_ref()).and_then(|p| p.get(name)))
            .ok_or_else(|| EngineError::Theme(format!("no shader preset `{name}`")))
    }

    /// A shader palette's colors, as sRGB.
    pub fn palette(&self, name: &str) -> Result<Vec<Color>, EngineError> {
        let colors = self
            .shaders
            .as_ref()
            .and_then(|s| s.palettes.as_ref())
            .and_then(|p| p.get(name))
            .ok_or_else(|| EngineError::Theme(format!("no shader palette `{name}`")))?;
        colors
            .iter()
            .map(|c| scaena_core::color::parse(&c.0).map_err(|e| EngineError::Theme(format!("palette `{name}`: {e}"))))
            .collect()
    }

    /// A length (SPEC §3.2), in canvas units: a number, `"12cu"`, a percentage of `whole`,
    /// or a theme token: `space.N` or `radius.N` (step N of that scale), or a stroke token
    /// (`hairline`, `thin`, …).
    pub fn length(&self, v: &Value, whole: f32) -> Result<f32, EngineError> {
        let bad = || EngineError::Layout(format!("{v} is not a length: canvas units, a percentage, or a theme token"));
        let s = match v {
            Value::Number(n) => return n.as_f64().map(|n| n as f32).ok_or_else(bad),
            Value::String(s) => s.as_str(),
            _ => return Err(bad()),
        };
        if let Some(n) = s.strip_suffix("cu") {
            return n.parse::<f32>().map_err(|_| bad());
        }
        if let Some(n) = s.strip_suffix('%') {
            return n.parse::<f32>().map(|p| p / 100.0 * whole).map_err(|_| bad());
        }
        let step = |scale: Option<&Vec<scaena_core::model::values::NonNegative>>, n: &str, what: &str| {
            let i: usize = n.parse().map_err(|_| bad())?;
            scale
                .and_then(|s| s.get(i))
                .map(|v| v.0 as f32)
                .ok_or_else(|| EngineError::Theme(format!("{what} scale has no step {i}")))
        };
        if let Some(n) = s.strip_prefix("space.") {
            return step(Some(&self.tokens.space.scale), n, "the space");
        }
        if let Some(n) = s.strip_prefix("radius.") {
            return step(self.tokens.radius.as_ref().and_then(|r| r.scale.as_ref()), n, "the radius");
        }
        self.stroke(s).map_err(|_| EngineError::Theme(format!("unknown length token `{s}`")))
    }

    /// The theme's shadow `name` (PLAN 2.115): an unknown one says which the theme has.
    pub fn shadow(&self, name: &str) -> Result<&model::Shadow, EngineError> {
        let shadows = self.tokens.shadow.as_ref();
        shadows.and_then(|s| s.get(name)).ok_or_else(|| {
            let names: Vec<&str> = shadows.into_iter().flat_map(|s| s.keys()).map(String::as_str).collect();
            let has = if names.is_empty() { "none".to_string() } else { names.join(", ") };
            EngineError::Theme(format!("unknown shadow `{name}`: the theme's shadows are {has}"))
        })
    }

    /// How the theme frames its pictures (PLAN T.3a): its keyline and its mat, each a width and
    /// a color, from the picture out; none where it says none.
    pub fn frame(&self) -> Result<Option<crate::images::Frame>, EngineError> {
        let Some(pictures) = &self.pictures else { return Ok(None) };
        let band = |b: Option<&model::Band>| b.map(|b| Ok::<_, EngineError>((b.width as f32, self.color(&b.color)?)));
        let (keyline, mat) = pictures.frame.bands();
        let (keyline, mat) = (band(keyline).transpose()?, band(mat).transpose()?);
        Ok((keyline.is_some() || mat.is_some()).then_some(crate::images::Frame { keyline, mat }))
    }

    /// A stroke token's width.
    pub fn stroke(&self, name: &str) -> Result<f32, EngineError> {
        self.tokens
            .stroke
            .as_ref()
            .and_then(|s| s.get(name))
            .map(|w| w.0 as f32)
            .ok_or_else(|| EngineError::Theme(format!("unknown stroke token `{name}`")))
    }
}

/// `c` as it lies on `ground`, its alpha composited.
fn over(c: Color, ground: [u8; 3]) -> [u8; 3] {
    let [r, g, b, a] = c.0.map(u32::from);
    let mix = |c: u32, g: u8| ((c * a + u32::from(g) * (255 - a) + 127) / 255) as u8;
    [mix(r, ground[0]), mix(g, ground[1]), mix(b, ground[2])]
}

#[cfg(test)]
mod tests {
    use super::*;
    const DUSK: &str = include_str!("../../../docs/examples/themes/dusk.theme.json");
    const TORTURE: &str = include_str!("../../../tests/fixtures/torture.scaena/theme.json");

    #[test]
    fn resolves_roles_tokens_motion() {
        let t = Theme::from_json(DUSK).unwrap();
        assert_eq!(t.name, "Dusk");
        assert_eq!(t.color_literal("accent"), Some("#FF6A3D"));
        assert_eq!(t.color("onSurface").unwrap().to_hex(), "#F2F0E9FF");
        assert_eq!(t.color_literal("color.line"), Some("#2A2A33"));
        assert_eq!(t.text_role("display").unwrap().size, 144.0);
        assert_eq!(t.duration(&Value::String("standard".into())), Some(420.0));
        assert_eq!(t.spring("snappy").unwrap().stiffness, 420.0);
        assert_eq!(t.easing("standard").unwrap().0, 0.2);
        assert!(t.slots("split").unwrap().contains_key("main"));
    }

    #[test]
    fn every_role_in_both_themes_types_cleanly() {
        for src in [DUSK, TORTURE] {
            let t = Theme::from_json(src).unwrap();
            for name in t.typography.roles.keys() {
                t.text_role(name).unwrap_or_else(|e| panic!("{}: {e}", t.name));
            }
        }
        let t = Theme::from_json(TORTURE).unwrap();
        let dlig = t.text_role("dlig").unwrap();
        assert_eq!(dlig.features["dlig"], 1);
        assert_eq!(t.text_role("wdth-150").unwrap().axes["wdth"], 150.0);
        assert_eq!(t.text_role("pretty-missing").unwrap_err().to_string(), "theme: unknown text role `pretty-missing`");
    }

    #[test]
    fn family_stack_follows_one_level_of_fallback_in_order() {
        let t = Theme::from_json(TORTURE).unwrap();
        assert_eq!(
            t.family_stack("serif").unwrap(),
            ["Roboto Serif", "EB Garamond", "Noto Sans Hebrew", "Noto Sans Arabic", "Noto Color Emoji"]
        );
        assert_eq!(t.family_stack("hebrew").unwrap(), ["Noto Sans Hebrew", "Roboto Serif", "Noto Color Emoji"]);
        assert!(t.family_stack("nope").is_err());
    }

    #[test]
    fn numeric_styles_map_to_opentype_features() {
        assert_eq!(Numeric::TabularOldstyle.features(), [("tnum", 1), ("pnum", 0), ("lnum", 0), ("onum", 1)]);
        assert_eq!(Numeric::ProportionalLining.features(), [("tnum", 0), ("pnum", 1), ("lnum", 1), ("onum", 0)]);
    }
}

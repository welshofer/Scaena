//! Theme loading and the style cascade (SPEC §3.6, ADR-0005).
//!
//! Cascade (later wins): theme role defaults → node `style` → state props → `overrides`.
//! Only `overrides` may carry raw literals; literals elsewhere are lint W300.
//!
//! Phase 1 task 1.6 replaces the `serde_json::Value`-backed theme with typed
//! structs generated alongside `docs/schema/theme.schema.json`. Phase 0 types the
//! parts text layout needs: [`TextRole`] and [`FamilyDef`].

use crate::EngineError;
use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;

/// A loaded theme. Until Phase 1 this is the raw JSON plus a few typed accessors.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub raw: Value,
}

/// A typographic role, `type.roles.<name>` (SPEC §3.5, theme schema `Role`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TextRole {
    /// Key in `type.families`.
    pub family: String,
    pub size: f32,
    pub weight: f32,
    /// Line height as a multiple of `size`.
    pub leading: f32,
    /// Letter spacing in em; negative tightens.
    #[serde(default)]
    pub tracking: f32,
    pub opsz: Option<f32>,
    #[serde(default)]
    pub axes: BTreeMap<String, f32>,
    #[serde(default)]
    pub features: BTreeMap<String, FeatureValue>,
    #[serde(default)]
    pub wrap: Wrap,
    #[serde(rename = "box", default)]
    pub text_box: TextBox,
    pub measure: Option<f32>,
    pub max_lines: Option<u32>,
    pub min_size: Option<f32>,
    pub max_size: Option<f32>,
    pub case: Option<String>,
    pub numeric: Option<Numeric>,
    pub min_last_line_words: Option<u32>,
    #[serde(default)]
    pub optical_margins: bool,
    #[serde(default)]
    pub hanging_punctuation: bool,
    #[serde(default)]
    pub hyphenate: bool,
    /// Color role or token; `onSurface` when absent.
    pub color: Option<String>,
}

/// An OpenType feature setting: `true`/`false` or an alternate index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum FeatureValue {
    Flag(bool),
    Index(u16),
}

impl FeatureValue {
    pub fn value(self) -> u16 {
        match self {
            FeatureValue::Flag(on) => u16::from(on),
            FeatureValue::Index(i) => i,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wrap {
    #[default]
    Greedy,
    Pretty,
    Balance,
}

/// Which box a text node's height is measured against (SPEC §3.4 `box`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TextBox {
    Cap,
    #[default]
    Line,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Numeric {
    TabularLining,
    TabularOldstyle,
    ProportionalLining,
    ProportionalOldstyle,
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

/// A font family, `type.families.<key>`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FamilyDef {
    /// Family name as the font's name table spells it.
    pub family: String,
    /// Bundle path of the font file.
    pub file: String,
    #[serde(default)]
    pub axes: BTreeMap<String, [f32; 2]>,
    /// Other family keys of this theme, in fallback order. Bundle-only.
    #[serde(default)]
    pub fallback: Vec<String>,
    /// Feature defaults for every role set in this family.
    #[serde(default)]
    pub features: BTreeMap<String, FeatureValue>,
}

impl Theme {
    pub fn from_json(s: &str) -> Result<Theme, EngineError> {
        let raw: Value = serde_json::from_str(s).map_err(|e| EngineError::Theme(e.to_string()))?;
        let name = raw.get("name").and_then(Value::as_str).unwrap_or("untitled").to_string();
        Ok(Theme { name, raw })
    }

    /// Resolve a color token or role name to its literal, following role → token.
    pub fn color(&self, name: &str) -> Option<&str> {
        let tokens = self.raw.get("tokens")?;
        let name = name.strip_prefix("color.").unwrap_or(name);
        if let Some(lit) = tokens.get("color").and_then(|c| c.get(name)).and_then(Value::as_str) {
            return Some(lit);
        }
        let token = tokens.get("roles")?.get(name)?.as_str()?;
        tokens.get("color")?.get(token)?.as_str()
    }

    /// A typographic role's raw definition.
    pub fn role(&self, name: &str) -> Option<&Value> {
        self.raw.get("type")?.get("roles")?.get(name)
    }

    /// A typographic role, typed.
    pub fn text_role(&self, name: &str) -> Result<TextRole, EngineError> {
        let raw = self.role(name).ok_or_else(|| EngineError::Theme(format!("unknown text role `{name}`")))?;
        serde_json::from_value(raw.clone()).map_err(|e| EngineError::Theme(format!("role `{name}`: {e}")))
    }

    /// Every font family, in theme order.
    pub fn families(&self) -> Result<IndexMap<String, FamilyDef>, EngineError> {
        let raw = self.raw.get("type").and_then(|t| t.get("families")).cloned().unwrap_or(Value::Null);
        serde_json::from_value(raw).map_err(|e| EngineError::Theme(format!("type.families: {e}")))
    }

    /// The font stack for a family key: the family, then its fallbacks in order, as
    /// family names, without repeats. Only the family's own `fallback` list is
    /// followed; fallbacks of fallbacks are not, so the stack is exactly what the
    /// theme author wrote.
    pub fn family_stack(&self, key: &str) -> Result<Vec<String>, EngineError> {
        let families = self.families()?;
        let lookup = |k: &str| {
            families.get(k).ok_or_else(|| EngineError::Theme(format!("unknown font family `{k}` in type.families")))
        };
        let primary = lookup(key)?;
        let mut stack = vec![primary.family.clone()];
        for k in &primary.fallback {
            let name = &lookup(k)?.family;
            if !stack.contains(name) {
                stack.push(name.clone());
            }
        }
        Ok(stack)
    }

    /// A named duration in ms, or parse a numeric value.
    pub fn duration(&self, v: &Value) -> Option<f64> {
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => self.raw.get("motion")?.get("durations")?.get(s)?.as_f64(),
            _ => None,
        }
    }

    /// A named spring.
    pub fn spring(&self, name: &str) -> Option<scaena_core::timeline::Spring> {
        let v = self.raw.get("motion")?.get("springs")?.get(name)?;
        serde_json::from_value(v.clone()).ok()
    }

    /// A named easing as a cubic Bézier.
    pub fn easing(&self, name: &str) -> Option<scaena_core::timeline::CubicBezier> {
        let v = self.raw.get("motion")?.get("easings")?.get(name)?.as_array()?;
        if v.len() != 4 {
            return None;
        }
        let f = |i: usize| v[i].as_f64();
        Some(scaena_core::timeline::CubicBezier(f(0)?, f(1)?, f(2)?, f(3)?))
    }

    /// Layout template slots for a layout name, keyed by slot name.
    pub fn slots(&self, layout: &str) -> Option<IndexMap<String, Value>> {
        let slots = self.raw.get("layouts")?.get(layout)?.get("slots")?.as_object()?;
        Some(slots.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }
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
        assert_eq!(t.color("accent"), Some("#FF6A3D"));
        assert_eq!(t.color("onSurface"), Some("#F2F0E9"));
        assert_eq!(t.color("color.line"), Some("#2A2A33"));
        assert_eq!(t.role("display").unwrap()["size"], 144);
        assert_eq!(t.duration(&Value::String("standard".into())), Some(420.0));
        assert_eq!(t.spring("snappy").unwrap().stiffness, 420.0);
        assert_eq!(t.easing("standard").unwrap().0, 0.2);
        assert!(t.slots("split").unwrap().contains_key("right"));
    }

    #[test]
    fn every_role_in_both_themes_types_cleanly() {
        for src in [DUSK, TORTURE] {
            let t = Theme::from_json(src).unwrap();
            let roles = t.raw["type"]["roles"].as_object().unwrap();
            for name in roles.keys() {
                t.text_role(name).unwrap_or_else(|e| panic!("{}: {e}", t.name));
            }
        }
        let t = Theme::from_json(TORTURE).unwrap();
        let dlig = t.text_role("dlig").unwrap();
        assert_eq!(dlig.features["dlig"].value(), 1);
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

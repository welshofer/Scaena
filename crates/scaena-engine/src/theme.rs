//! Theme loading and the style cascade (SPEC §3.6, ADR-0005).
//!
//! Cascade (later wins): theme role defaults → node `style` → state props → `overrides`.
//! Only `overrides` may carry raw literals; literals elsewhere are lint W300.
//!
//! Phase 1 task 1.6 replaces the `serde_json::Value`-backed theme with typed
//! structs generated alongside `docs/schema/theme.schema.json`.

use crate::EngineError;
use indexmap::IndexMap;
use serde_json::Value;

/// A loaded theme. Until Phase 1 this is the raw JSON plus a few typed accessors.
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub raw: Value,
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
}

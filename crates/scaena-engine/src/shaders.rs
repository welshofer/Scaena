//! Shader nodes (SPEC §3.8) to shader ops. What each kind draws lives in
//! `scaena_core::shader`, CPU reference and WGSL twin side by side, because painters
//! run it. The engine resolves a node to its op's parameters: the kind, the seed, the
//! theme palette's colors, typed params, and the rect it covers. A frame adds the
//! time: the frame's place on the global timeline (SPEC §3.9). No arbitrary shader
//! source, ever.

use crate::EngineError;
use crate::theme::Theme;
use scaena_core::displaylist::{Color, Op, Rect, ShaderKind};
use scaena_core::document::Props;
use scaena_core::shader;
use serde_json::Value;
use std::collections::BTreeMap;

/// A shader node, resolved: everything its op carries but the time.
#[derive(Debug, Clone, PartialEq)]
pub struct ShaderNode {
    pub kind: ShaderKind,
    pub seed: u64,
    pub palette: Vec<Color>,
    pub params: BTreeMap<String, f32>,
    /// What it covers, canvas units.
    pub rect: Rect,
}

impl ShaderNode {
    /// A shader node's props over `rect`. A theme preset the node names gives its palette
    /// and params unless the node sets its own. Params are typed here, so a bad one is
    /// the document's error, reported with its node, not a painter's.
    pub fn resolve(props: &Props, theme: &Theme, rect: Rect) -> Result<ShaderNode, EngineError> {
        let kind = match props.get("kind").and_then(Value::as_str) {
            Some("mesh") => ShaderKind::Mesh,
            Some("gradient") => ShaderKind::Gradient,
            Some("noise") => ShaderKind::Noise,
            Some("grain") => ShaderKind::Grain,
            Some("particles") => ShaderKind::Particles,
            other => return Err(EngineError::Layout(format!("unknown shader kind {other:?}"))),
        };
        let seed = match props.get("seed") {
            None => 0,
            Some(v) => v.as_u64().ok_or_else(|| EngineError::Layout(format!("`seed` {v}: expected a whole number")))?,
        };
        let preset = match props.get("preset").and_then(Value::as_str) {
            None => None,
            Some(name) => {
                let preset = theme.shader_preset(name)?;
                let theirs = serde_json::to_value(preset.kind).unwrap_or_default();
                if theirs != props["kind"] {
                    return Err(EngineError::Layout(format!(
                        "shader preset `{name}` is a {} shader, and this one is a {}",
                        theirs.as_str().unwrap_or_default(),
                        props["kind"].as_str().unwrap_or_default()
                    )));
                }
                Some(preset)
            }
        };
        let name = (props.get("palette").and_then(Value::as_str))
            .or_else(|| preset.and_then(|p| p.palette.as_deref()))
            .ok_or_else(|| EngineError::Layout("a shader needs `palette`, a theme shader palette".into()))?;
        let palette = theme.palette(name)?;
        // The preset's params, then the node's over them.
        let mut written: Vec<(String, Value)> = Vec::new();
        if let Some(p) = preset.and_then(|p| p.params.as_ref()) {
            written.extend(p.iter().map(|(k, v)| (k.clone(), serde_json::to_value(v).unwrap_or_default())));
        }
        if let Some(p) = props.get("params") {
            let p = p.as_object().ok_or_else(|| EngineError::Layout(format!("`params` {p}: expected an object")))?;
            written.extend(p.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        let mut params = BTreeMap::new();
        for (k, v) in written {
            let v = match &v {
                Value::Number(n) => n.as_f64().unwrap_or(f64::NAN) as f32,
                Value::String(name) => shader::named(kind, &k, name).map_err(|e| EngineError::Layout(e.to_string()))?,
                other => return Err(EngineError::Layout(format!("shader param `{k}` {other}: expected a number"))),
            };
            params.insert(k, v);
        }
        shader::check(kind, &params).map_err(|e| EngineError::Layout(e.to_string()))?;
        Ok(ShaderNode { kind, seed, palette, params, rect })
    }

    /// Its op at `t` seconds on the global timeline, in the coordinates of a layer at
    /// its rect's corner.
    pub fn op(&self, t: f64) -> Op {
        Op::Shader {
            kind: self.kind,
            seed: self.seed,
            t: t as f32,
            rect: [0.0, 0.0, self.rect[2], self.rect[3]],
            palette: self.palette.clone(),
            params: self.params.clone(),
        }
    }

    /// The same shader moved to `rect`.
    pub fn at(&self, rect: Rect) -> ShaderNode {
        ShaderNode { rect, ..self.clone() }
    }

    /// Whether `other` is this shader, wherever it sits.
    pub fn same_shader(&self, other: &ShaderNode) -> bool {
        (self.kind, self.seed, &self.palette, &self.params) == (other.kind, other.seed, &other.palette, &other.params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn theme() -> Theme {
        let mut t: serde_json::Value =
            serde_json::from_str(include_str!("../../../tests/fixtures/torture.scaena/theme.json")).unwrap();
        t["shaders"]["palettes"]["lab"] = json!(["oklch(70% 0.1 200)", "#000000"]);
        t["shaders"]["presets"] = json!({
            "soft": { "kind": "noise", "palette": "torture", "params": { "octaves": 3, "scale": 0.002 } },
            "sunrise": { "kind": "gradient", "palette": "lab", "params": { "shape": "radial", "radius": 1.2 } }
        });
        Theme::from_json(&t.to_string()).unwrap()
    }

    #[test]
    fn a_preset_gives_its_palette_and_params_and_the_node_wins() {
        let node = props(json!({"kind": "noise", "preset": "soft", "params": {"scale": 0.004}}));
        let s = ShaderNode::resolve(&node, &theme(), [0.0, 0.0, 100.0, 100.0]).unwrap();
        assert_eq!((s.palette.len(), s.params["octaves"], s.params["scale"]), (4, 3.0, 0.004));
        // A name a kind lists is a number in the op.
        let node = props(json!({"kind": "gradient", "preset": "sunrise", "params": {"shape": "conic"}}));
        let s = ShaderNode::resolve(&node, &theme(), [0.0, 0.0, 100.0, 100.0]).unwrap();
        assert_eq!((s.params["shape"], s.params["radius"], s.palette.len()), (2.0, 1.2, 2));
    }

    fn props(v: serde_json::Value) -> Props {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_mesh_resolves_its_palette_and_types_its_params() {
        let node =
            props(json!({"kind": "mesh", "seed": 7, "palette": "torture", "params": {"points": 5, "grain": 0.035}}));
        let s = ShaderNode::resolve(&node, &theme(), [0.0, 0.0, 1920.0, 1080.0]).unwrap();
        assert_eq!((s.seed, s.palette.len(), s.params["points"]), (7, 4, 5.0));
        assert_eq!(s.palette[0], Color::from_hex("#0F766E").unwrap());
        let Op::Shader { t, rect, .. } = s.op(0.84) else { unreachable!() };
        assert_eq!((t, rect), (0.84, [0.0, 0.0, 1920.0, 1080.0]));
    }

    #[test]
    fn a_palette_may_be_written_in_oklch() {
        let node = props(json!({"kind": "mesh", "palette": "lab"}));
        let s = ShaderNode::resolve(&node, &theme(), [0.0, 0.0, 1920.0, 1080.0]).unwrap();
        assert_eq!(s.palette, [Color::from_hex("#40B1B7").unwrap(), Color::from_hex("#000000").unwrap()]);
    }

    #[test]
    fn what_a_shader_cannot_draw_says_why() {
        let err = |v| ShaderNode::resolve(&props(v), &theme(), [0.0, 0.0, 10.0, 10.0]).unwrap_err().to_string();
        assert!(err(json!({"kind": "mesh"})).contains("palette"));
        assert!(
            err(json!({"kind": "gradient", "palette": "torture", "params": {"shape": "spiral"}})).contains("shape")
        );
        assert!(err(json!({"kind": "noise", "palette": "torture", "params": {"octaves": 9}})).contains("octaves"));
        assert!(err(json!({"kind": "mesh", "preset": "soft"})).contains("noise shader, and this one is a mesh"));
        assert!(err(json!({"kind": "mesh", "preset": "nope"})).contains("nope"));
        assert!(err(json!({"kind": "mesh", "palette": "nope"})).contains("no shader palette `nope`"));
        assert!(err(json!({"kind": "mesh", "palette": "torture", "params": {"points": 40}})).contains("points"));
        assert!(err(json!({"kind": "mesh", "palette": "torture", "seed": -1})).contains("seed"));
    }
}

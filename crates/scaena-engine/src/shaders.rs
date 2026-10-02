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
use scaena_core::shader::mesh;
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
    /// A shader node's props over `rect`. Params are typed here, so a bad one is the
    /// document's error, reported with its node, not a painter's.
    pub fn resolve(props: &Props, theme: &Theme, rect: Rect) -> Result<ShaderNode, EngineError> {
        let kind = match props.get("kind").and_then(Value::as_str) {
            Some("mesh") => ShaderKind::Mesh,
            Some("gradient" | "noise" | "grain" | "particles") => {
                return Err(EngineError::NotImplemented("shader kinds other than mesh — PLAN 1.10"));
            }
            other => return Err(EngineError::Layout(format!("unknown shader kind {other:?}"))),
        };
        let seed = match props.get("seed") {
            None => 0,
            Some(v) => v.as_u64().ok_or_else(|| EngineError::Layout(format!("`seed` {v}: expected a whole number")))?,
        };
        let name = props
            .get("palette")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::Layout("a shader needs `palette`, a theme shader palette".into()))?;
        let palette = theme.palette(name)?;
        let mut params = BTreeMap::new();
        if let Some(p) = props.get("params") {
            let p = p.as_object().ok_or_else(|| EngineError::Layout(format!("`params` {p}: expected an object")))?;
            for (k, v) in p {
                let v = v
                    .as_f64()
                    .ok_or_else(|| EngineError::Layout(format!("shader param `{k}` {v}: expected a number")))?;
                params.insert(k.clone(), v as f32);
            }
        }
        mesh::Params::from_map(&params).map_err(|e| EngineError::Layout(e.to_string()))?;
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
        Theme::from_json(&t.to_string()).unwrap()
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
    fn what_a_mesh_cannot_draw_says_why() {
        let err = |v| ShaderNode::resolve(&props(v), &theme(), [0.0, 0.0, 10.0, 10.0]).unwrap_err().to_string();
        assert!(err(json!({"kind": "noise", "palette": "torture"})).contains("PLAN 1.10"));
        assert!(err(json!({"kind": "mesh"})).contains("palette"));
        assert!(err(json!({"kind": "mesh", "palette": "nope"})).contains("no shader palette `nope`"));
        assert!(err(json!({"kind": "mesh", "palette": "torture", "params": {"points": 40}})).contains("points"));
        assert!(err(json!({"kind": "mesh", "palette": "torture", "seed": -1})).contains("seed"));
    }
}

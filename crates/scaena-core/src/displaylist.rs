//! The display list (SPEC §6): a serializable, painter-agnostic frame description.
//!
//! Coordinates are canvas units. Glyph positions are final (post-shaping,
//! post-kerning): painters never shape text. Shader ops carry parameters, not
//! pixels. This type is the unit of golden testing.

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const DL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DisplayList {
    pub dl: u32,
    pub viewport: [f64; 2],
    pub ops: Vec<Op>,
}

impl DisplayList {
    pub fn new(viewport: [f64; 2]) -> Self {
        Self { dl: DL_VERSION, viewport, ops: Vec::new() }
    }
}

/// A 2D affine matrix `[a, b, c, d, e, f]` (column-major, CSS/SVG convention).
pub type Affine = [f64; 6];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Op {
    /// Group with opacity/blend/clip; children paint into an isolated layer when needed.
    Layer {
        opacity: f64,
        blend: Blend,
        #[serde(skip_serializing_if = "Option::is_none")]
        clip: Option<String>,
        ops: Vec<Op>,
    },
    Transform {
        m: Affine,
    },
    Fill {
        path: String,
        paint: Paint,
    },
    Stroke {
        path: String,
        paint: Paint,
        width: f64,
        cap: Cap,
        join: Join,
        #[serde(skip_serializing_if = "Option::is_none")]
        dash: Option<Vec<f64>>,
    },
    Glyphs {
        font: String,
        size: f64,
        color: String,
        /// `[glyph_id, x, y]` triples; positions are absolute canvas units.
        glyphs: Vec<[f64; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        axes: Option<Value>,
    },
    Image {
        asset: String,
        src: [f64; 4],
        dst: [f64; 4],
        quality: Quality,
    },
    Shader {
        kind: String,
        seed: u64,
        params: Value,
        t: f64,
        rect: [f64; 4],
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Paint {
    Solid(String),
    Gradient {
        kind: GradientKind,
        stops: Vec<(f64, String)>,
        #[serde(skip_serializing_if = "Option::is_none")]
        angle: Option<f64>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum GradientKind {
    Linear,
    Radial,
    Conic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Cap {
    #[default]
    Butt,
    Round,
    Square,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Join {
    #[default]
    Miter,
    Round,
    Bevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Quality {
    Low,
    #[default]
    High,
}

/// Round every coordinate to 1/64 cu so golden comparisons are exact across platforms (SPEC §13.4).
pub fn quantize(dl: &mut DisplayList) {
    fn q(v: &mut f64) {
        *v = (*v * 64.0).round() / 64.0;
    }
    fn walk(ops: &mut [Op]) {
        for op in ops {
            match op {
                Op::Layer { ops, .. } => walk(ops),
                Op::Transform { m } => m.iter_mut().for_each(q),
                Op::Glyphs { glyphs, .. } => glyphs.iter_mut().for_each(|g| {
                    q(&mut g[1]);
                    q(&mut g[2]);
                }),
                Op::Image { src, dst, .. } => {
                    src.iter_mut().for_each(q);
                    dst.iter_mut().for_each(q);
                }
                Op::Shader { rect, .. } => rect.iter_mut().for_each(q),
                Op::Fill { .. } | Op::Stroke { .. } => {}
            }
        }
    }
    walk(&mut dl.ops);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_with_tagged_ops() {
        let mut dl = DisplayList::new([1920.0, 1080.0]);
        dl.ops.push(Op::Fill { path: "M0 0H1920V1080Z".into(), paint: Paint::Solid("#101014".into()) });
        dl.ops.push(Op::Glyphs {
            font: "f0".into(),
            size: 128.0,
            color: "#F2F0E9".into(),
            glyphs: vec![[42.0, 96.0123, 300.0]],
            axes: None,
        });
        let s = serde_json::to_string(&dl).unwrap();
        assert!(s.contains(r#""op":"fill""#) && s.contains(r#""op":"glyphs""#));
        let back: DisplayList = serde_json::from_str(&s).unwrap();
        assert_eq!(back, dl);
        quantize(&mut dl);
        if let Op::Glyphs { glyphs, .. } = &dl.ops[1] {
            assert_eq!(glyphs[0][1], 96.015625);
        }
    }
}

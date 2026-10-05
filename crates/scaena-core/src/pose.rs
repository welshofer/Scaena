//! A node's pose (SPEC §3.3, PLAN 2.51): its `transform` drawn at rest, where it stands, as a
//! cue's looks draw theirs. Scale, then skew, then rotate (degrees, clockwise), about its
//! anchor (fractions of its box, its middle unless it says), then translate (canvas units).
//! The sine, cosine, and tangent are `libm`'s (SPEC §13).
//!
//! A pose keeps its map finite: a skew within 89° of either side, a turn of a hundred turns at
//! most, a scale and a translation no larger than a canvas could use.

use crate::model::values::{Scale, Transform};
use serde_json::Value;

/// How far a skew leans, degrees, either way: short of a right angle, whose tangent has no end.
const LEAN: f64 = 89.0;
/// How far a node may turn, degrees, either way: a hundred turns, which a move between two
/// states may make.
const TURNS: f64 = 36_000.0;
/// How large a scale may grow, either way.
const GROW: f64 = 1000.0;
/// How far a translation may go, canvas units, either way.
const REACH: f64 = 100_000.0;

/// A node's transform at rest, each part as a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    pub translate: [f64; 2],
    pub rotate: f64,
    pub scale: [f64; 2],
    pub skew: [f64; 2],
    pub anchor: [f64; 2],
}

impl Default for Pose {
    fn default() -> Self {
        Pose::REST
    }
}

impl Pose {
    /// The node as it is laid out.
    pub const REST: Pose = Pose { translate: [0.0; 2], rotate: 0.0, scale: [1.0; 2], skew: [0.0; 2], anchor: [0.5; 2] };

    /// The pose a node's `transform`, as it resolves, gives it: `None` where it has none, or
    /// one that leaves it as it is laid out.
    pub fn of(transform: Option<&Value>) -> Option<Pose> {
        let t: Transform = serde_json::from_value(transform?.clone()).ok()?;
        Some(Pose::from(&t)).filter(|p| !p.is_rest())
    }

    /// Whether this pose leaves the node as it is laid out.
    pub fn is_rest(&self) -> bool {
        self.translate == [0.0; 2] && self.rotate == 0.0 && self.scale == [1.0; 2] && self.skew == [0.0; 2]
    }

    /// `p` of the way from this pose to `to`, each part on its own, as a node moving between
    /// two states turns: a rotation from 0 to 350 goes most of the way round, and one to 720
    /// twice round.
    pub fn lerp(&self, to: &Pose, p: f64) -> Pose {
        let mix = |a: f64, b: f64| a + (b - a) * p;
        let mix2 = |a: [f64; 2], b: [f64; 2]| [mix(a[0], b[0]), mix(a[1], b[1])];
        Pose {
            translate: mix2(self.translate, to.translate),
            rotate: mix(self.rotate, to.rotate),
            scale: mix2(self.scale, to.scale),
            skew: mix2(self.skew, to.skew),
            anchor: mix2(self.anchor, to.anchor),
        }
    }

    /// `p` of the way from `a` to `b`, where either may leave the node as it is laid out: that
    /// side takes the other's anchor, which a pose at rest has no use for, so a node grows or
    /// turns about the point it is drawn about.
    pub fn between(a: Option<Pose>, b: Option<Pose>, p: f64) -> Pose {
        let rest = |other: Option<Pose>| Pose { anchor: other.map_or(Pose::REST.anchor, |o| o.anchor), ..Pose::REST };
        a.unwrap_or(rest(b)).lerp(&b.unwrap_or(rest(a)), p)
    }

    /// The pose as a map of canvas points, `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`), for a
    /// node whose box as laid out is `rect` (`[x, y, width, height]`).
    pub fn affine(&self, rect: [f32; 4]) -> [f64; 6] {
        let [x, y, w, h] = rect.map(f64::from);
        let (ax, ay) = (x + self.anchor[0] * w, y + self.anchor[1] * h);
        let turn = self.rotate.to_radians();
        let (sin, cos) = if self.rotate == 0.0 { (0.0, 1.0) } else { (libm::sin(turn), libm::cos(turn)) };
        let lean = |k: f64| if k == 0.0 { 0.0 } else { libm::tan(k.to_radians()) };
        let (kx, ky) = (lean(self.skew[0]), lean(self.skew[1]));
        let [sx, sy] = self.scale;
        // Rotate · skew · scale: x leans by `kx` per unit of y, y by `ky` per unit of x.
        let (a, b) = ((cos - sin * ky) * sx, (sin + cos * ky) * sx);
        let (c, d) = ((cos * kx - sin) * sy, (sin * kx + cos) * sy);
        let e = ax + self.translate[0] - (a * ax + c * ay);
        let f = ay + self.translate[1] - (b * ax + d * ay);
        [a, b, c, d, e, f]
    }
}

impl From<&Transform> for Pose {
    fn from(t: &Transform) -> Pose {
        let within = |v: f64, max: f64| if v.is_finite() { v.clamp(-max, max) } else { 0.0 };
        let pair = |p: [f64; 2], max: f64| [within(p[0], max), within(p[1], max)];
        let scale = match t.scale {
            Some(Scale::Uniform(k)) => [k, k],
            Some(Scale::Xy(xy)) => xy,
            None => [1.0, 1.0],
        };
        Pose {
            translate: pair(t.translate.unwrap_or([0.0; 2]), REACH),
            rotate: within(t.rotate.unwrap_or(0.0), TURNS),
            scale: pair(scale, GROW),
            skew: pair(t.skew.unwrap_or([0.0; 2]), LEAN),
            anchor: pair(t.anchor.unwrap_or([0.5; 2]), REACH),
        }
    }
}

/// `m` applied to the point `p`.
pub fn apply(m: &[f64; 6], p: [f64; 2]) -> [f64; 2] {
    [m[0] * p[0] + m[2] * p[1] + m[4], m[1] * p[0] + m[3] * p[1] + m[5]]
}

/// The map that undoes `m`, where one does: `None` for one that flattens the plane.
pub fn invert(m: &[f64; 6]) -> Option<[f64; 6]> {
    let det = m[0] * m[3] - m[1] * m[2];
    if det.abs() < 1e-12 || !det.is_finite() {
        return None;
    }
    let (a, b, c, d) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det);
    Some([a, b, c, d, -(a * m[4] + c * m[5]), -(b * m[4] + d * m[5])])
}

/// How `m` sizes what it draws: how long it draws a unit along x, and how far apart two lines
/// along x a unit apart, which is the size a text and a plot's height read at. A pose's turn
/// and lean change neither; its scale does.
pub fn stretch(m: &[f64; 6]) -> [f64; 2] {
    let along = libm::hypot(m[0], m[1]);
    let det = (m[0] * m[3] - m[1] * m[2]).abs();
    if along < 1e-12 { [0.0, 0.0] } else { [along, det / along] }
}

/// `rect`'s corners through `m`, in order round it: where the box is drawn.
pub fn corners(m: &[f64; 6], rect: [f32; 4]) -> [[f64; 2]; 4] {
    let [x, y, w, h] = rect.map(f64::from);
    [[x, y], [x + w, y], [x + w, y + h], [x, y + h]].map(|p| apply(m, p))
}

/// The box `m` maps `rect` into, `[x, y, width, height]`: its four corners' bounds.
pub fn bounds(m: &[f64; 6], rect: [f32; 4]) -> [f32; 4] {
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for [px, py] in corners(m, rect) {
        (x0, y0, x1, y1) = (x0.min(px), y0.min(py), x1.max(px), y1.max(py));
    }
    [x0 as f32, y0 as f32, (x1 - x0) as f32, (y1 - y0) as f32]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn near(a: [f64; 2], b: [f64; 2]) -> bool {
        (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9
    }

    #[test]
    fn a_pose_turns_scales_and_leans_about_its_anchor() {
        let rect = [100.0, 100.0, 200.0, 100.0];
        // A quarter turn clockwise about the middle (200, 150): the top left corner goes to
        // the top right of the turned box.
        let turn = Pose::of(Some(&json!({ "rotate": 90 }))).unwrap();
        assert!(near(apply(&turn.affine(rect), [100.0, 100.0]), [250.0, 50.0]));
        assert!(near(apply(&turn.affine(rect), [200.0, 150.0]), [200.0, 150.0]), "the anchor stays");
        // Doubled about the top left corner, then moved.
        let grow = Pose::of(Some(&json!({ "scale": 2, "anchor": [0, 0], "translate": [10, -5] }))).unwrap();
        assert!(near(apply(&grow.affine(rect), [300.0, 200.0]), [510.0, 295.0]));
        // Leaning 45° across: a point a unit lower goes a unit right, about the middle.
        let lean = Pose::of(Some(&json!({ "skew": [45, 0] }))).unwrap();
        let p = apply(&lean.affine(rect), [200.0, 151.0]);
        assert!(near(p, [201.0, 151.0]), "{p:?}");
        // A pose that moves nothing is none at all; nor is a transform the engine cannot read.
        assert_eq!(Pose::of(Some(&json!({ "rotate": 0, "anchor": [0, 0] }))), None);
        assert_eq!(Pose::of(Some(&json!({ "spin": 3 }))), None);
        assert_eq!(Pose::of(None), None);
    }

    #[test]
    fn a_pose_without_a_skew_is_the_map_a_look_makes() {
        let rect = [40.0, 60.0, 300.0, 80.0];
        let pose =
            Pose::of(Some(&json!({ "rotate": 30, "scale": [1.5, 0.5], "anchor": [0.25, 1], "translate": [7, 9] })));
        let look = crate::timeline::Look {
            rotate: 30.0,
            scale: [1.5, 0.5],
            anchor: [0.25, 1.0],
            translate: [7.0, 9.0],
            ..crate::timeline::Look::REST
        };
        assert_eq!(pose.unwrap().affine(rect), look.affine(rect));
    }

    #[test]
    fn a_map_undoes_and_bounds_what_it_maps() {
        let pose = Pose::of(Some(&json!({ "rotate": 30, "skew": [10, -20], "scale": [2, 1] }))).unwrap();
        let rect = [10.0, 20.0, 100.0, 50.0];
        let m = pose.affine(rect);
        let back = invert(&m).unwrap();
        assert!(near(apply(&back, apply(&m, [33.0, 44.0])), [33.0, 44.0]));
        // A scale of 0 flattens the plane: nothing undoes it.
        assert_eq!(invert(&Pose { scale: [0.0, 1.0], ..Pose::REST }.affine(rect)), None);
        // A quarter turn of a 100 × 50 box about its middle stands 50 × 100.
        let turned = bounds(&Pose { rotate: 90.0, ..Pose::REST }.affine(rect), rect);
        assert!(turned.iter().zip([35.0, -5.0, 50.0, 100.0]).all(|(a, b)| (a - b).abs() < 1e-3), "{turned:?}");
        // Out of reach: a skew of 90° leans 89°, a scale of a million is a thousand, a million
        // degrees a hundred turns.
        let far = serde_json::from_value::<Transform>(json!({ "skew": [90, 0], "scale": 1e6, "rotate": 1e6 }));
        let far = Pose::from(&far.unwrap());
        assert_eq!((far.skew, far.scale, far.rotate), ([89.0, 0.0], [1000.0, 1000.0], 36_000.0));
        assert!(far.affine(rect).iter().all(|v| v.is_finite()));
    }

    #[test]
    fn a_pose_moves_from_rest_about_its_own_anchor() {
        let half = Pose::of(Some(&json!({ "scale": 0.5, "anchor": [0, 0] })));
        let rect = [100.0, 100.0, 200.0, 100.0];
        // Growing back to its box from its top left corner, it keeps that corner where it is.
        for p in [0.0, 0.25, 0.5, 1.0] {
            let corner = apply(&Pose::between(half, None, p).affine(rect), [100.0, 100.0]);
            assert!(near(corner, [100.0, 100.0]), "{p}: {corner:?}");
        }
        assert_eq!(Pose::between(None, half, 0.5).scale, [0.75, 0.75]);
        assert_eq!(Pose::between(None, None, 0.5), Pose::REST);
    }

    #[test]
    fn a_turn_or_a_lean_keeps_a_text_its_size_and_a_scale_does_not() {
        let rect = [0.0, 0.0, 100.0, 40.0];
        let size = |t: serde_json::Value| stretch(&Pose::of(Some(&t)).unwrap().affine(rect));
        let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-9 && (a[1] - b[1]).abs() < 1e-9;
        assert!(close(size(json!({ "rotate": 37 })), [1.0, 1.0]));
        assert!(close(size(json!({ "skew": [30, 0] })), [1.0, 1.0]), "a lean along the line keeps its lines apart");
        assert!(close(size(json!({ "scale": [2, 0.5], "rotate": 90 })), [2.0, 0.5]));
        assert_eq!(stretch(&Pose { scale: [0.0, 1.0], ..Pose::REST }.affine(rect)), [0.0, 0.0]);
    }
}

//! `gradient`: linear, radial, or conic (SPEC §3.8).
//!
//! The palette's colors run evenly spaced across the gradient, blended in Oklab. A
//! linear gradient runs along `angle`, clockwise from up, over the rect's whole extent in
//! that direction, as a CSS linear gradient does. A radial one runs out from (`x`, `y`)
//! to `radius` shorter sides of the rect. A conic one runs around (`x`, `y`) from
//! `angle`, back to its first color. A linear or conic gradient turns `speed` degrees a
//! second. `grain` adds seeded noise to Oklab lightness, per device pixel.
//!
//! The angle's sine and cosine come once per frame from `libm`. Per pixel, a linear
//! gradient is `+ − × ÷`; a radial one adds a square root, and a conic one an arctangent
//! (`libm` here, WGSL's own in `gradient.wgsl`), whose last bits a GPU may round
//! differently.

use super::Color;
use super::{
    MAX_STOPS, ShaderError, Words, frame_box, grain_noise, local_map, oklab_rgba8, ramp, sample, seed_key, thresholds,
    within,
};
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("gradient.wgsl");

/// A gradient's shape, as its shader op carries it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Linear = 0,
    Radial = 1,
    Conic = 2,
}

impl Shape {
    /// The shape a document names.
    pub fn named(name: &str) -> Option<Shape> {
        match name {
            "linear" => Some(Shape::Linear),
            "radial" => Some(Shape::Radial),
            "conic" => Some(Shape::Conic),
            _ => None,
        }
    }
}

/// Typed gradient parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    pub shape: Shape,
    /// Degrees clockwise from up: a linear gradient's direction, a conic one's start.
    pub angle: f32,
    /// A radial or conic gradient's center, as fractions of the rect.
    pub x: f32,
    pub y: f32,
    /// A radial gradient's radius, in shorter sides.
    pub radius: f32,
    /// Degrees a second a linear or conic gradient turns.
    pub speed: f32,
    /// Noise amplitude in Oklab lightness.
    pub grain: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { shape: Shape::Linear, angle: 180.0, x: 0.5, y: 0.5, radius: 0.75, speed: 0.0, grain: 0.0 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            match name.as_str() {
                "shape" => {
                    p.shape = match v {
                        0.0 => Shape::Linear,
                        1.0 => Shape::Radial,
                        2.0 => Shape::Conic,
                        _ => {
                            let problem = format!("{v}: expected linear, radial, or conic");
                            return Err(ShaderError::Param { name: name.clone(), problem });
                        }
                    }
                }
                "angle" => p.angle = within(name, v, -360.0, 360.0)?,
                "x" => p.x = within(name, v, 0.0, 1.0)?,
                "y" => p.y = within(name, v, 0.0, 1.0)?,
                "radius" if v > 0.0 && v <= 4.0 => p.radius = v,
                "radius" => {
                    let problem = format!("{v}: expected above 0, up to 4");
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
                "speed" => p.speed = within(name, v, -360.0, 360.0)?,
                "grain" => p.grain = within(name, v, 0.0, 0.25)?,
                _ => {
                    let problem = "unknown; a gradient takes shape, angle, x, y, radius, speed, and grain".to_string();
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
            }
        }
        Ok(p)
    }
}

/// One frame of a gradient over a box of device pixels: everything [`Frame::pixel`]
/// reads, which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// A device pixel's center to `(u, v)`, `[a, b, c, d, e, f]` (`u = a·px + c·py +
    /// e`): for a linear gradient, `u` is how far along it the pixel is, 0 to 1; for a
    /// radial one, `(u, v)` is the pixel from the center in radii; for a conic one, in
    /// shorter sides.
    pub map: [f32; 6],
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    pub shape: Shape,
    /// A conic gradient's start, in turns clockwise from up.
    pub start: f32,
    pub grain: f32,
    /// Seeds the grain.
    pub key: u32,
    /// Per stop: Oklab L, a, b, and alpha 0–1.
    pub colors: Vec<[f32; 4]>,
}

impl Frame {
    /// The gradient at `t` seconds over `rect` (canvas units), drawn through `device`
    /// into a raster `size` pixels across. `None` when it covers no pixel.
    pub fn new(
        seed: u64,
        t: f32,
        palette: &[Color],
        params: &Params,
        rect: [f32; 4],
        device: [f64; 6],
        size: [u32; 2],
    ) -> Result<Option<Frame>, ShaderError> {
        let colors = ramp("gradient", palette)?;
        let Some((bbox, inverse)) = frame_box(rect, device, size)? else { return Ok(None) };
        let [_, _, w, h] = rect.map(f64::from);
        let side = w.min(h);
        let turned = f64::from(params.angle) + f64::from(params.speed) * f64::from(t);
        let map = match params.shape {
            // Along the direction (sin θ, −cos θ), y down, through the rect's center and
            // as long as the rect's extent that way, as CSS sets a linear gradient.
            Shape::Linear => {
                let theta = turned.to_radians();
                let (sin, cos) = (libm::sin(theta), libm::cos(theta));
                let length = (w * sin).abs() + (h * cos).abs();
                let [a, b, c, d, e, f] = local_map(inverse, rect, 1.0);
                let (cx, cy) = (0.5 * w, 0.5 * h);
                [
                    (sin * a - cos * b) / length,
                    0.0,
                    (sin * c - cos * d) / length,
                    0.0,
                    (sin * (e - cx) - cos * (f - cy)) / length + 0.5,
                    0.0,
                ]
            }
            Shape::Radial | Shape::Conic => {
                let k =
                    if params.shape == Shape::Radial { 1.0 / (f64::from(params.radius) * side) } else { 1.0 / side };
                let [a, b, c, d, e, f] = local_map(inverse, rect, k);
                let (cx, cy) = (f64::from(params.x) * w * k, f64::from(params.y) * h * k);
                [a, b, c, d, e - cx, f - cy]
            }
        };
        let start = turned / 360.0;
        Ok(Some(Frame {
            map: map.map(|v| v as f32),
            bbox,
            shape: params.shape,
            start: (start - start.floor()) as f32,
            grain: params.grain,
            key: seed_key(seed),
            colors,
        }))
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `gradient.wgsl` is
    /// this function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let [a, b, c, d, e, f] = self.map;
        let px = (self.bbox[0] + gx) as f32 + 0.5;
        let py = (self.bbox[1] + gy) as f32 + 0.5;
        let u = a * px + c * py + e;
        let v = b * px + d * py + f;
        let at = match self.shape {
            Shape::Linear => u,
            Shape::Radial => libm::sqrtf(u * u + v * v),
            Shape::Conic => {
                // 1 / τ, written out so both twins multiply by the same f32.
                let turn = libm::atan2f(u, -v) * 0.159_154_94 - self.start;
                turn - turn.floor()
            }
        };
        let [l, ca, cb, alpha] = sample(&self.colors, at.clamp(0.0, 1.0), self.shape == Shape::Conic);
        let l = l + self.grain * grain_noise(self.key, gx, gy);
        oklab_rgba8(l, ca, cb, alpha)
    }

    /// The uniform buffer `gradient.wgsl` declares as `Gradient`, for output rows
    /// `stride` words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let mut out = Words(Vec::with_capacity(1344));
        let [a, b, c, d, e, f] = self.map;
        out.f32s([a, b, c, d, e, f, self.grain, self.start]);
        out.u32s(self.bbox);
        out.u32s([self.shape as u32, self.colors.len() as u32, self.key, stride]);
        (0..MAX_STOPS).for_each(|i| out.f32s(self.colors.get(i).copied().unwrap_or_default()));
        out.f32s(*thresholds());
        out.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Color; 3] = [Color([15, 118, 110, 255]), Color([67, 56, 202, 255]), Color([194, 65, 12, 255])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(params: Params) -> Frame {
        Frame::new(7, 0.0, &PALETTE, &params, [0.0, 0.0, 200.0, 100.0], ID, [200, 100]).unwrap().unwrap()
    }

    /// Within a few steps of `b`: a pixel's center is half a pixel in from an end, which
    /// in Oklab can move a channel several steps toward the next color.
    fn near(a: [u8; 4], b: Color) -> bool {
        a.iter().zip(b.0).all(|(x, y)| x.abs_diff(y) <= 10)
    }

    #[test]
    fn params_take_defaults_and_refuse_what_they_cannot_draw() {
        let map = |pairs: &[(&str, f32)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
        assert_eq!(Params::from_map(&map(&[])).unwrap(), Params::default());
        assert_eq!(Params::from_map(&map(&[("shape", 2.0)])).unwrap().shape, Shape::Conic);
        for (k, v) in [("shape", 3.0), ("angle", 400.0), ("x", 1.5), ("radius", 0.0), ("grain", 0.3), ("hue", 1.0)] {
            let err = Params::from_map(&map(&[(k, v)])).unwrap_err().to_string();
            assert!(err.contains(k), "{k} = {v}: {err}");
        }
    }

    #[test]
    fn a_linear_gradient_runs_its_colors_end_to_end() {
        // 180°: top to bottom. The first row is the first color, the last row the last,
        // and the middle row the middle color.
        let f = frame(Params::default());
        assert!(near(f.pixel(100, 0), PALETTE[0]), "{:?}", f.pixel(100, 0));
        assert!(near(f.pixel(100, 99), PALETTE[2]), "{:?}", f.pixel(100, 99));
        // Row 49's center is 49.5 of 100: just before the middle.
        let mid = f.pixel(0, 49);
        assert!(mid.iter().zip(PALETTE[1].0).all(|(x, y)| x.abs_diff(y) <= 3), "{mid:?}");
        // 90°: left to right, the same down every column.
        let across = frame(Params { angle: 90.0, ..Params::default() });
        assert!(near(across.pixel(0, 50), PALETTE[0]) && near(across.pixel(199, 50), PALETTE[2]));
        assert_eq!(across.pixel(10, 0), across.pixel(10, 99));
    }

    #[test]
    fn a_radial_gradient_runs_out_from_its_center_and_a_conic_one_around_it() {
        let radial = frame(Params { shape: Shape::Radial, radius: 0.5, ..Params::default() });
        assert!(near(radial.pixel(100, 50), PALETTE[0]), "the center is the first color");
        assert!(near(radial.pixel(199, 50), PALETTE[2]), "past the radius, the last");
        assert_eq!(radial.pixel(90, 50), radial.pixel(109, 50), "round");
        let conic = frame(Params { shape: Shape::Conic, ..Params::default() });
        // From 180° (straight down) clockwise: down is the first color, up a half turn on.
        assert!(near(conic.pixel(100, 99), PALETTE[0]), "{:?}", conic.pixel(100, 99));
        let up = conic.pixel(100, 0);
        assert!(!near(up, PALETTE[0]) && !near(up, PALETTE[2]), "{up:?}");
    }

    #[test]
    fn a_gradient_turns_with_time() {
        let still = Frame::new(1, 2.0, &PALETTE, &Params::default(), [0.0, 0.0, 200.0, 100.0], ID, [200, 100]);
        let turning = Params { speed: 45.0, ..Params::default() };
        let moved = Frame::new(1, 2.0, &PALETTE, &turning, [0.0, 0.0, 200.0, 100.0], ID, [200, 100]);
        let (still, moved) = (still.unwrap().unwrap(), moved.unwrap().unwrap());
        assert_eq!(still.pixel(0, 0), frame(Params::default()).pixel(0, 0), "no speed, no motion");
        // At 2 s it has turned 90° on from 180°: right to left.
        let back = frame(Params { angle: 270.0, ..Params::default() });
        assert_eq!(moved.pixel(0, 50), back.pixel(0, 50));
        assert!(near(moved.pixel(0, 50), PALETTE[2]) && near(moved.pixel(199, 50), PALETTE[0]));
        assert_eq!(moved.uniforms(200).len(), 1344, "the WGSL `Gradient` struct's size");
    }
}

//! `mesh`: a mesh gradient (SPEC §3.8).
//!
//! `points` palette colors sit at seeded, evenly spread positions over the shader's
//! rect and drift on slow Lissajous paths. Each pixel blends them in Oklab, weighting
//! a point at distance `d` by `1 / (1 + d²/σ²)²`, so colors pool around their points
//! and run smoothly into each other; `softness` sets σ. Distances are in units of the
//! rect's shorter side, so a circle stays round at any aspect. `grain` adds seeded
//! noise to Oklab lightness, per device pixel.
//!
//! Positions come from `seed` and `t` once per frame (`libm` sine and cosine); the
//! per-pixel function is `+ − × ÷` and comparisons, here and in `mesh.wgsl` alike.

use super::{
    ShaderError, SplitMix64, Words, device_box, encode, invert, linear, linear_to_oklab, lowbias32, thresholds,
};
use crate::displaylist::Color;
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("mesh.wgsl");

/// The most points a mesh takes (the WGSL uniform arrays' length).
pub const MAX_POINTS: usize = 16;

/// The R2 sequence (Roberts): the plastic number's inverse and its square. Points
/// stepped by it spread evenly over the rect for any count.
const R2: [f64; 2] = [0.754_877_666_246_692_7, 0.569_840_290_998_053_2];
/// Drift angular speeds, radians per second: one loop every 14 to 31 seconds.
const SPEED: [f64; 2] = [0.2, 0.25];
/// σ, in units of the rect's shorter side, at `softness` 1.
const SIGMA: f64 = 0.6;

/// Typed mesh parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Color points, 2–16.
    pub points: u32,
    /// How far a point wanders from its place, in units of the rect's shorter side, 0–1.
    pub drift: f32,
    /// How far each color spreads into its neighbors, above 0 to 1.
    pub softness: f32,
    /// Noise amplitude in Oklab lightness, 0–0.25.
    pub grain: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { points: 5, drift: 0.1, softness: 0.6, grain: 0.03 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            let bad = |problem: &str| ShaderError::Param { name: name.clone(), problem: problem.to_string() };
            match name.as_str() {
                "points" if v.fract() == 0.0 && (2.0..=MAX_POINTS as f32).contains(&v) => p.points = v as u32,
                "points" => return Err(bad(&format!("{v}: expected a whole number from 2 to {MAX_POINTS}"))),
                "drift" if (0.0..=1.0).contains(&v) => p.drift = v,
                "drift" => return Err(bad(&format!("{v}: expected 0 to 1"))),
                "softness" if v > 0.0 && v <= 1.0 => p.softness = v,
                "softness" => return Err(bad(&format!("{v}: expected above 0, up to 1"))),
                "grain" if (0.0..=0.25).contains(&v) => p.grain = v,
                "grain" => return Err(bad(&format!("{v}: expected 0 to 0.25"))),
                _ => return Err(bad("unknown; a mesh takes points, drift, softness, and grain")),
            }
        }
        Ok(p)
    }
}

/// One frame of a mesh over a box of device pixels: everything [`Frame::pixel`] reads,
/// which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// A device pixel's center to shader space, `[a, b, c, d, e, f]` (`x = a·px + c·py
    /// + e`): the rect's top-left corner is the origin and its shorter side is 1.
    pub map: [f32; 6],
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    pub grain: f32,
    pub inv_sigma2: f32,
    /// Seeds the grain.
    pub key: u32,
    /// Per point: x and y in shader space, alpha 0–1, and 0.
    pub points: Vec<[f32; 4]>,
    /// Per point: Oklab L, a, b, and 0.
    pub colors: Vec<[f32; 4]>,
}

impl Frame {
    /// The mesh at `t` seconds over `rect` (canvas units), drawn through `device` into
    /// a raster `size` pixels across. `None` when it covers no pixel.
    pub fn new(
        seed: u64,
        t: f32,
        palette: &[Color],
        params: &Params,
        rect: [f32; 4],
        device: [f64; 6],
        size: [u32; 2],
    ) -> Result<Option<Frame>, ShaderError> {
        if palette.is_empty() {
            return Err(ShaderError::EmptyPalette);
        }
        let [rx, ry, w, h] = rect;
        if !(rx.is_finite() && ry.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
            return Err(ShaderError::Rect(rect));
        }
        let (Some(bbox), Some([a, b, c, d, e, f])) = (device_box(rect, device, size), invert(device)) else {
            return Ok(None);
        };
        let side = f64::from(w.min(h));
        let (x0, y0) = (f64::from(rx), f64::from(ry));
        let map = [a / side, b / side, c / side, d / side, (e - x0) / side, (f - y0) / side].map(|v| v as f32);

        let (width, height) = (f64::from(w) / side, f64::from(h) / side);
        let mut rng = SplitMix64(seed);
        let start = [rng.unit(), rng.unit()];
        let (t, drift) = (f64::from(t), f64::from(params.drift));
        let mut points = Vec::with_capacity(params.points as usize);
        let mut colors = Vec::with_capacity(params.points as usize);
        for i in 0..params.points as usize {
            let n = (i + 1) as f64;
            let place = [0, 1].map(|k| {
                let v = start[k] + n * R2[k];
                v - v.floor()
            });
            let speed = [0, 1].map(|_| SPEED[0] + SPEED[1] * rng.unit());
            let phase = [0, 1].map(|_| std::f64::consts::TAU * rng.unit());
            let x = place[0] * width + drift * libm::cos(speed[0] * t + phase[0]);
            let y = place[1] * height + drift * libm::sin(speed[1] * t + phase[1]);
            let Color([r, g, bl, alpha]) = palette[i % palette.len()];
            let [l, ca, cb] = linear_to_oklab([linear(r), linear(g), linear(bl)]);
            points.push([x as f32, y as f32, f32::from(alpha) / 255.0, 0.0]);
            colors.push([l as f32, ca as f32, cb as f32, 0.0]);
        }
        let sigma = SIGMA * f64::from(params.softness);
        Ok(Some(Frame {
            map,
            bbox,
            grain: params.grain,
            inv_sigma2: (1.0 / (sigma * sigma)) as f32,
            key: lowbias32(seed as u32 ^ lowbias32((seed >> 32) as u32)),
            points,
            colors,
        }))
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `mesh.wgsl` is
    /// this function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let [a, b, c, d, e, f] = self.map;
        let px = (self.bbox[0] + gx) as f32 + 0.5;
        let py = (self.bbox[1] + gy) as f32 + 0.5;
        let x = a * px + c * py + e;
        let y = b * px + d * py + f;
        let (mut sum, mut l, mut ca, mut cb, mut alpha) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for (p, k) in self.points.iter().zip(&self.colors) {
            let dx = x - p[0];
            let dy = y - p[1];
            let u = 1.0 + (dx * dx + dy * dy) * self.inv_sigma2;
            let w = 1.0 / (u * u);
            sum += w;
            l += w * k[0];
            ca += w * k[1];
            cb += w * k[2];
            alpha += w * p[2];
        }
        let inv = 1.0 / sum;
        let n = (lowbias32(self.key ^ lowbias32(gx ^ lowbias32(gy))) >> 8) as f32 * (1.0 / 16_777_216.0) - 0.5;
        let lg = l * inv + self.grain * n;
        let ca = ca * inv;
        let cb = cb * inv;
        let lm = lg + 0.396_337_78 * ca + 0.215_803_76 * cb;
        let mm = lg - 0.105_561_346 * ca - 0.063_854_17 * cb;
        let sm = lg - 0.089_484_18 * ca - 1.291_485_5 * cb;
        let lc = lm * lm * lm;
        let mc = mm * mm * mm;
        let sc = sm * sm * sm;
        let r = 4.076_741_7 * lc - 3.307_711_6 * mc + 0.230_969_94 * sc;
        let g = -1.268_438 * lc + 2.609_757_4 * mc - 0.341_319_38 * sc;
        let bl = -0.004_196_086_4 * lc - 0.703_418_6 * mc + 1.707_614_7 * sc;
        let a8 = (alpha * inv).clamp(0.0, 1.0) * 255.0 + 0.5;
        [encode(r) as u8, encode(g) as u8, encode(bl) as u8, a8 as u8]
    }

    /// The whole box, row-major.
    pub fn render(&self) -> Vec<u8> {
        let [_, _, w, h] = self.bbox;
        let mut out = Vec::with_capacity(w as usize * h as usize * 4);
        for gy in 0..h {
            for gx in 0..w {
                out.extend_from_slice(&self.pixel(gx, gy));
            }
        }
        out
    }

    /// The uniform buffer `mesh.wgsl` declares as `Mesh`, for output rows `stride`
    /// words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let mut out = Words(Vec::with_capacity(1600));
        let [a, b, c, d, e, f] = self.map;
        out.f32s([a, b, c, d, e, f, self.grain, self.inv_sigma2]);
        out.u32s(self.bbox);
        out.u32s([self.points.len() as u32, self.key, stride, 0]);
        for list in [&self.points, &self.colors] {
            (0..MAX_POINTS).for_each(|i| out.f32s(list.get(i).copied().unwrap_or_default()));
        }
        out.f32s(*thresholds());
        out.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Color; 4] =
        [Color([15, 118, 110, 255]), Color([67, 56, 202, 255]), Color([194, 65, 12, 255]), Color([245, 196, 81, 255])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(seed: u64, t: f32, params: Params) -> Frame {
        Frame::new(seed, t, &PALETTE, &params, [0.0, 0.0, 160.0, 90.0], ID, [160, 90]).unwrap().unwrap()
    }

    #[test]
    fn params_take_defaults_and_refuse_what_they_cannot_draw() {
        let map = |pairs: &[(&str, f32)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
        assert_eq!(Params::from_map(&map(&[])).unwrap(), Params::default());
        let p = Params::from_map(&map(&[("points", 7.0), ("grain", 0.0)])).unwrap();
        assert_eq!((p.points, p.grain, p.drift), (7, 0.0, 0.1));
        for (k, v) in [
            ("points", 1.0),
            ("points", 2.5),
            ("points", 17.0),
            ("drift", -0.1),
            ("softness", 0.0),
            ("grain", 0.3),
            ("grain", f32::NAN),
            ("speed", 1.0),
        ] {
            let err = Params::from_map(&map(&[(k, v)])).unwrap_err().to_string();
            assert!(err.contains(k), "{k} = {v}: {err}");
        }
    }

    #[test]
    fn a_seed_and_a_time_make_one_image() {
        let p = Params::default();
        assert_eq!(frame(7, 1.5, p).render(), frame(7, 1.5, p).render());
        assert_ne!(frame(7, 1.5, p).render(), frame(8, 1.5, p).render(), "another seed, another mesh");
        assert_ne!(frame(7, 1.5, p).render(), frame(7, 3.0, p).render(), "the points drift");
        let still = Params { drift: 0.0, ..p };
        assert_eq!(frame(7, 1.5, still).render(), frame(7, 3.0, still).render(), "no drift, no motion");
    }

    #[test]
    fn a_pixel_on_a_point_takes_its_color_when_the_blend_is_tight() {
        let p = Params { points: 2, softness: 0.01, grain: 0.0, drift: 0.0 };
        let f = frame(3, 0.0, p);
        for (i, pt) in f.points.iter().enumerate() {
            // The pixel whose center is nearest the point, in a 1:1 device space.
            let (gx, gy) = ((pt[0] * 90.0).floor() as u32, (pt[1] * 90.0).floor() as u32);
            let want = PALETTE[i].0;
            let got = f.pixel(gx.min(159), gy.min(89));
            assert!(got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 1), "point {i}: {got:?} vs {want:?}");
        }
    }

    #[test]
    fn grain_moves_lightness_evenly_around_the_smooth_mesh() {
        let smooth = frame(5, 0.0, Params { grain: 0.0, ..Params::default() });
        let grainy = frame(5, 0.0, Params { grain: 0.1, ..Params::default() });
        let (a, b) = (smooth.render(), grainy.render());
        let diffs: Vec<i32> = a.iter().zip(&b).step_by(4).map(|(x, y)| i32::from(*y) - i32::from(*x)).collect();
        let mean = diffs.iter().sum::<i32>() as f64 / diffs.len() as f64;
        assert!(diffs.iter().any(|d| *d != 0), "grain changes pixels");
        assert!(mean.abs() < 0.5, "and averages out: mean shift {mean}");
    }

    #[test]
    fn the_box_maps_device_pixels_into_the_rect() {
        // At 2× with the rect at (10, 5) cu, device pixel (20, 10)'s center is
        // 0.25 cu inside the rect's corner, in units of its 90 cu shorter side.
        let f = Frame::new(
            1,
            0.0,
            &PALETTE,
            &Params::default(),
            [10.0, 5.0, 160.0, 90.0],
            [2.0, 0.0, 0.0, 2.0, 0.0, 0.0],
            [400, 400],
        )
        .unwrap()
        .unwrap();
        assert_eq!(f.bbox, [20, 10, 320, 180]);
        let [a, b, c, d, e, g] = f.map;
        let (px, py) = (20.5_f32, 10.5_f32);
        assert!(
            ((a * px + c * py + e) - 0.25 / 90.0).abs() < 1e-6 && ((b * px + d * py + g) - 0.25 / 90.0).abs() < 1e-6
        );
        assert_eq!(f.uniforms(320).len(), 1600, "the WGSL `Mesh` struct's size");
    }
}

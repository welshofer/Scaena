//! `particles`: a seeded field of soft discs drifting across the rect (SPEC §3.8).
//!
//! `count` particles start at seeded, evenly spread places (the R2 sequence, as a mesh's
//! points do) and drift in seeded directions at `speed` shorter sides a second, each at
//! its own pace (½ to 1½ times). A particle that leaves one side comes back in at the
//! other, once it is wholly out. Each is a disc `size` shorter sides in radius (½ to 1½
//! times, seeded), its edge fading over `softness` of its radius, in the palette's
//! colors in turn. Later particles lie over earlier ones, blended in linear light.
//!
//! Places come from `seed` and `t` once per frame (`libm` sine and cosine); per pixel it
//! is `+ − × ÷` and comparisons, here and in `particles.wgsl` alike.

use super::{
    ShaderError, SplitMix64, Words, encode, frame_box, linear, local_map, mesh::R2, thresholds, whole, within,
};
use crate::displaylist::Color;
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("particles.wgsl");

/// The most particles a field takes (the WGSL uniform arrays' length).
pub const MAX_PARTICLES: usize = 64;

/// Typed particle parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Particles, 1–64.
    pub count: u32,
    /// A particle's radius, in shorter sides of the rect.
    pub size: f32,
    /// Shorter sides a second.
    pub speed: f32,
    /// How much of a particle's radius its edge fades over, 0–1.
    pub softness: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { count: 24, size: 0.01, speed: 0.02, softness: 0.5 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            match name.as_str() {
                "count" => p.count = whole(name, v, 1, MAX_PARTICLES as u32)?,
                "size" if v > 0.0 && v <= 0.25 => p.size = v,
                "size" => {
                    let problem = format!("{v}: expected above 0, up to 0.25 shorter sides");
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
                "speed" => p.speed = within(name, v, 0.0, 1.0)?,
                "softness" => p.softness = within(name, v, 0.0, 1.0)?,
                _ => {
                    let problem = "unknown; particles take count, size, speed, and softness".to_string();
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
            }
        }
        Ok(p)
    }
}

/// One frame of particles over a box of device pixels: everything [`Frame::pixel`]
/// reads, which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// A device pixel's center to shader space, `[a, b, c, d, e, f]` (`x = a·px + c·py +
    /// e`): the rect's top-left corner is the origin and its shorter side is 1.
    pub map: [f32; 6],
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    /// Per particle: its center's x and y, its radius squared, and 1 over the part of
    /// that its edge fades over.
    pub discs: Vec<[f32; 4]>,
    /// Per particle: its color in linear light, and its alpha 0–1.
    pub colors: Vec<[f32; 4]>,
}

impl Frame {
    /// The particles at `t` seconds over `rect` (canvas units), drawn through `device`
    /// into a raster `size` pixels across. `None` when they cover no pixel.
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
        let Some((bbox, inverse)) = frame_box(rect, device, size)? else { return Ok(None) };
        let [_, _, w, h] = rect.map(f64::from);
        let side = w.min(h);
        let (width, height) = (w / side, h / side);
        let (t, speed, radius) = (f64::from(t), f64::from(params.speed), f64::from(params.size));
        // A particle wraps once it is wholly out: around the rect grown by the largest
        // radius.
        let reach = 1.5 * radius;
        let wrap = |v: f64, extent: f64| {
            let span = extent + 2.0 * reach;
            let v = v + reach;
            v - (v / span).floor() * span - reach
        };
        let mut rng = SplitMix64(seed);
        let start = [rng.unit(), rng.unit()];
        let mut discs = Vec::with_capacity(params.count as usize);
        let mut colors = Vec::with_capacity(params.count as usize);
        for i in 0..params.count as usize {
            let n = (i + 1) as f64;
            let place = [0, 1].map(|k| {
                let v = start[k] + n * R2[k];
                v - v.floor()
            });
            let heading = std::f64::consts::TAU * rng.unit();
            let pace = 0.5 + rng.unit();
            let r = radius * (0.5 + rng.unit());
            let x = wrap(place[0] * width + speed * pace * libm::cos(heading) * t, width);
            let y = wrap(place[1] * height + speed * pace * libm::sin(heading) * t, height);
            let inner = r * (1.0 - f64::from(params.softness));
            let band = r * r - inner * inner;
            discs.push([x as f32, y as f32, (r * r) as f32, if band > 0.0 { (1.0 / band) as f32 } else { 1e30 }]);
            let Color([cr, cg, cb, ca]) = palette[i % palette.len()];
            colors.push([linear(cr) as f32, linear(cg) as f32, linear(cb) as f32, f32::from(ca) / 255.0]);
        }
        Ok(Some(Frame { map: local_map(inverse, rect, 1.0 / side).map(|v| v as f32), bbox, discs, colors }))
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `particles.wgsl` is
    /// this function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let [a, b, c, d, e, f] = self.map;
        let px = (self.bbox[0] + gx) as f32 + 0.5;
        let py = (self.bbox[1] + gy) as f32 + 0.5;
        let x = a * px + c * py + e;
        let y = b * px + d * py + f;
        let (mut r, mut g, mut bl, mut alpha) = (0.0_f32, 0.0_f32, 0.0_f32, 0.0_f32);
        for (disc, k) in self.discs.iter().zip(&self.colors) {
            let dx = x - disc[0];
            let dy = y - disc[1];
            let d2 = dx * dx + dy * dy;
            if d2 < disc[2] {
                let cover = ((disc[2] - d2) * disc[3]).min(1.0) * k[3];
                let rest = 1.0 - cover;
                r = k[0] * cover + r * rest;
                g = k[1] * cover + g * rest;
                bl = k[2] * cover + bl * rest;
                alpha = cover + alpha * rest;
            }
        }
        if alpha <= 0.0 {
            return [0, 0, 0, 0];
        }
        let inv = 1.0 / alpha;
        let a8 = alpha.min(1.0) * 255.0 + 0.5;
        [encode(r * inv) as u8, encode(g * inv) as u8, encode(bl * inv) as u8, a8 as u8]
    }

    /// The uniform buffer `particles.wgsl` declares as `Particles`, for output rows
    /// `stride` words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let mut out = Words(Vec::with_capacity(3136));
        let [a, b, c, d, e, f] = self.map;
        out.f32s([a, b, c, d, e, f, 0.0, 0.0]);
        out.u32s(self.bbox);
        out.u32s([self.discs.len() as u32, stride, 0, 0]);
        for list in [&self.discs, &self.colors] {
            (0..MAX_PARTICLES).for_each(|i| out.f32s(list.get(i).copied().unwrap_or_default()));
        }
        out.f32s(*thresholds());
        out.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Color; 2] = [Color([194, 65, 12, 255]), Color([67, 56, 202, 128])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(t: f32, params: Params) -> Frame {
        Frame::new(11, t, &PALETTE, &params, [0.0, 0.0, 200.0, 100.0], ID, [200, 100]).unwrap().unwrap()
    }

    #[test]
    fn params_take_defaults_and_refuse_what_they_cannot_draw() {
        let map = |pairs: &[(&str, f32)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
        assert_eq!(Params::from_map(&map(&[])).unwrap(), Params::default());
        for (k, v) in [("count", 0.0), ("count", 65.0), ("size", 0.0), ("speed", 2.0), ("softness", 1.5)] {
            let err = Params::from_map(&map(&[(k, v)])).unwrap_err().to_string();
            assert!(err.contains(k), "{k} = {v}: {err}");
        }
    }

    #[test]
    fn particles_are_discs_in_their_palette_colors_over_a_clear_field() {
        let f = frame(0.0, Params { count: 2, size: 0.1, softness: 0.0, speed: 0.0 });
        // A disc's center takes its color; far from both, nothing.
        let center = |i: usize| ((f.discs[i][0] * 100.0) as u32, (f.discs[i][1] * 100.0) as u32);
        let (x, y) = center(0);
        assert_eq!(f.pixel(x, y), [194, 65, 12, 255]);
        let (x, y) = center(1);
        let second = f.pixel(x, y);
        assert_eq!((second[..3].to_vec(), second[3]), (vec![67, 56, 202], 128), "{second:?}");
        let empty =
            (0..200).flat_map(|x| (0..100).map(move |y| (x, y))).filter(|&(x, y)| f.pixel(x, y)[3] == 0).count();
        assert!(empty > 15_000, "mostly clear: {empty} of 20000");
        assert_eq!(f.uniforms(200).len(), 3136, "the WGSL `Particles` struct's size");
    }

    #[test]
    fn particles_drift_and_come_back_round() {
        let p = Params { count: 8, speed: 0.5, ..Params::default() };
        let (a, b) = (frame(0.0, p), frame(1.0, p));
        assert_ne!(a.discs, b.discs, "they move");
        // Every center stays within the rect grown by the largest radius.
        let reach = 1.5 * p.size;
        for t in [0.0, 3.0, 50.0, 3600.0] {
            for d in frame(t, p).discs {
                assert!((-reach..=2.0 + reach).contains(&d[0]) && (-reach..=1.0 + reach).contains(&d[1]), "{t}: {d:?}");
            }
        }
        let still = Params { speed: 0.0, ..p };
        assert_eq!(frame(0.0, still).discs, frame(9.0, still).discs);
    }
}

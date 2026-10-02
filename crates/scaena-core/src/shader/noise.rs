//! `noise`: simplex noise in layers (SPEC §3.8).
//!
//! Each pixel samples 3D simplex noise (Gustavson's, its gradients picked by a seeded
//! integer hash) at its place in canvas units times `scale`, and at `speed` × t along the
//! third axis, so the field changes as time runs instead of sliding. `octaves` layers
//! finer noise over it, each at twice the frequency and half the amplitude (fractal
//! Brownian motion). The sum, spread by `contrast`, picks a color along the palette's
//! ramp, blended in Oklab. `grain` adds seeded noise to Oklab lightness, per device
//! pixel.
//!
//! The seed moves the field and keys its hashes. Per pixel the noise is `+ − × ÷`,
//! `floor`, comparisons, and integer hashing, here and in `noise.wgsl` alike.

use super::{
    MAX_STOPS, ShaderError, SplitMix64, Words, frame_box, grain_noise, local_map, lowbias32, oklab_rgba8, ramp, sample,
    seed_key, thresholds, whole, within,
};
use crate::displaylist::Color;
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("noise.wgsl");

/// How far the seed moves the field, in noise cycles along each axis.
const SPREAD: f64 = 256.0;

/// Typed noise parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// Cycles per canvas unit.
    pub scale: f32,
    /// Layers, 1–8; 1 is plain simplex noise.
    pub octaves: u32,
    /// Noise cycles a second along the time axis.
    pub speed: f32,
    /// How far the field spreads across the palette, 0–4.
    pub contrast: f32,
    /// Noise amplitude in Oklab lightness.
    pub grain: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { scale: 0.0015, octaves: 4, speed: 0.05, contrast: 1.0, grain: 0.0 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            match name.as_str() {
                "scale" if v > 0.0 && v <= 0.1 => p.scale = v,
                "scale" => {
                    let problem = format!("{v}: expected above 0, up to 0.1 cycles a canvas unit");
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
                "octaves" => p.octaves = whole(name, v, 1, 8)?,
                "speed" => p.speed = within(name, v, 0.0, 2.0)?,
                "contrast" => p.contrast = within(name, v, 0.0, 4.0)?,
                "grain" => p.grain = within(name, v, 0.0, 0.25)?,
                _ => {
                    let problem = "unknown; noise takes scale, octaves, speed, contrast, and grain".to_string();
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
            }
        }
        Ok(p)
    }
}

/// One frame of noise over a box of device pixels: everything [`Frame::pixel`] reads,
/// which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// A device pixel's center to noise space, `[a, b, c, d, e, f]` (`x = a·px + c·py +
    /// e`).
    pub map: [f32; 6],
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    /// Where the frame is along the time axis.
    pub z: f32,
    pub octaves: u32,
    pub contrast: f32,
    pub grain: f32,
    /// Keys the lattice hashes and the grain.
    pub key: u32,
    /// Per stop: Oklab L, a, b, and alpha 0–1.
    pub colors: Vec<[f32; 4]>,
}

impl Frame {
    /// The noise at `t` seconds over `rect` (canvas units), drawn through `device` into a
    /// raster `size` pixels across. `None` when it covers no pixel.
    pub fn new(
        seed: u64,
        t: f32,
        palette: &[Color],
        params: &Params,
        rect: [f32; 4],
        device: [f64; 6],
        size: [u32; 2],
    ) -> Result<Option<Frame>, ShaderError> {
        let colors = ramp("noise", palette)?;
        let Some((bbox, inverse)) = frame_box(rect, device, size)? else { return Ok(None) };
        let mut rng = SplitMix64(seed);
        let offset = [rng.unit(), rng.unit(), rng.unit()].map(|u| u * SPREAD);
        let [a, b, c, d, e, f] = local_map(inverse, rect, f64::from(params.scale));
        let map = [a, b, c, d, e + offset[0], f + offset[1]];
        Ok(Some(Frame {
            map: map.map(|v| v as f32),
            bbox,
            z: (offset[2] + f64::from(params.speed) * f64::from(t)) as f32,
            octaves: params.octaves,
            contrast: params.contrast,
            grain: params.grain,
            key: seed_key(seed),
            colors,
        }))
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `noise.wgsl` is this
    /// function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let [a, b, c, d, e, f] = self.map;
        let px = (self.bbox[0] + gx) as f32 + 0.5;
        let py = (self.bbox[1] + gy) as f32 + 0.5;
        let x = a * px + c * py + e;
        let y = b * px + d * py + f;
        let (mut sum, mut norm, mut amp, mut freq) = (0.0_f32, 0.0_f32, 1.0_f32, 1.0_f32);
        for o in 0..self.octaves {
            sum += amp * simplex(x * freq, y * freq, self.z * freq, lowbias32(self.key.wrapping_add(o)));
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        let n = sum / norm;
        let u = (0.5 + 0.5 * self.contrast * n).clamp(0.0, 1.0);
        let [l, ca, cb, alpha] = sample(&self.colors, u, false);
        let l = l + self.grain * grain_noise(self.key, gx, gy);
        oklab_rgba8(l, ca, cb, alpha)
    }

    /// The uniform buffer `noise.wgsl` declares as `Noise`, for output rows `stride`
    /// words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let mut out = Words(Vec::with_capacity(1360));
        let [a, b, c, d, e, f] = self.map;
        out.f32s([a, b, c, d, e, f, self.z, self.grain]);
        out.u32s(self.bbox);
        out.u32s([self.octaves, self.colors.len() as u32, self.key, stride]);
        out.f32s([self.contrast, 0.0, 0.0, 0.0]);
        (0..MAX_STOPS).for_each(|i| out.f32s(self.colors.get(i).copied().unwrap_or_default()));
        out.f32s(*thresholds());
        out.0
    }
}

/// Ken Perlin's gradient pick: one of twelve edge directions (four twice), dotted with
/// `(x, y, z)`.
fn grad(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let h = hash & 15;
    let u = if h < 8 { x } else { y };
    let v = if h < 4 {
        y
    } else if h == 12 || h == 14 {
        x
    } else {
        z
    };
    (if h & 1 == 0 { u } else { -u }) + (if h & 2 == 0 { v } else { -v })
}

/// One corner's share of the noise: its gradient, fading to nothing 0.6 away.
fn corner(key: u32, lattice: [u32; 3], x: f32, y: f32, z: f32) -> f32 {
    let t = 0.6 - x * x - y * y - z * z;
    if t < 0.0 {
        return 0.0;
    }
    let h = lowbias32(key ^ lowbias32(lattice[0] ^ lowbias32(lattice[1] ^ lowbias32(lattice[2]))));
    let t2 = t * t;
    t2 * t2 * grad(h, x, y, z)
}

/// 3D simplex noise at `(x, y, z)`, about −1 to 1 (Gustavson).
fn simplex(x: f32, y: f32, z: f32, key: u32) -> f32 {
    // 1/3 and 1/6, written out so both twins use the same f32.
    const F3: f32 = 0.333_333_34;
    const G3: f32 = 0.166_666_67;
    let s = (x + y + z) * F3;
    let i = (x + s).floor();
    let j = (y + s).floor();
    let k = (z + s).floor();
    let t = (i + j + k) * G3;
    let x0 = x - (i - t);
    let y0 = y - (j - t);
    let z0 = z - (k - t);
    // Which of the six simplices the point is in: the corners after the first.
    let (o1, o2) = if x0 >= y0 {
        if y0 >= z0 {
            ([1, 0, 0], [1, 1, 0])
        } else if x0 >= z0 {
            ([1, 0, 0], [1, 0, 1])
        } else {
            ([0, 0, 1], [1, 0, 1])
        }
    } else if y0 < z0 {
        ([0, 0, 1], [0, 1, 1])
    } else if x0 < z0 {
        ([0, 1, 0], [0, 1, 1])
    } else {
        ([0, 1, 0], [1, 1, 0])
    };
    let x1 = x0 - o1[0] as f32 + G3;
    let y1 = y0 - o1[1] as f32 + G3;
    let z1 = z0 - o1[2] as f32 + G3;
    let x2 = x0 - o2[0] as f32 + F3;
    let y2 = y0 - o2[1] as f32 + F3;
    let z2 = z0 - o2[2] as f32 + F3;
    let x3 = x0 - 0.5;
    let y3 = y0 - 0.5;
    let z3 = z0 - 0.5;
    let at = [i as i32 as u32, j as i32 as u32, k as i32 as u32];
    let plus = |o: [u32; 3]| [at[0].wrapping_add(o[0]), at[1].wrapping_add(o[1]), at[2].wrapping_add(o[2])];
    let n0 = corner(key, at, x0, y0, z0);
    let n1 = corner(key, plus(o1), x1, y1, z1);
    let n2 = corner(key, plus(o2), x2, y2, z2);
    let n3 = corner(key, plus([1, 1, 1]), x3, y3, z3);
    32.0 * (n0 + n1 + n2 + n3)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Color; 2] = [Color([14, 12, 20, 255]), Color([255, 200, 87, 255])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(seed: u64, t: f32, params: Params) -> Frame {
        Frame::new(seed, t, &PALETTE, &params, [0.0, 0.0, 160.0, 90.0], ID, [160, 90]).unwrap().unwrap()
    }

    #[test]
    fn params_take_defaults_and_refuse_what_they_cannot_draw() {
        let map = |pairs: &[(&str, f32)]| pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect::<BTreeMap<_, _>>();
        assert_eq!(Params::from_map(&map(&[])).unwrap(), Params::default());
        let p = Params::from_map(&map(&[("octaves", 1.0), ("scale", 0.01)])).unwrap();
        assert_eq!((p.octaves, p.scale), (1, 0.01));
        for (k, v) in [("scale", 0.0), ("octaves", 0.0), ("octaves", 2.5), ("speed", 3.0), ("contrast", -1.0)] {
            let err = Params::from_map(&map(&[(k, v)])).unwrap_err().to_string();
            assert!(err.contains(k), "{k} = {v}: {err}");
        }
    }

    #[test]
    fn simplex_noise_is_continuous_bounded_and_zero_on_the_lattice() {
        assert_eq!(simplex(0.0, 0.0, 0.0, 9), 0.0, "every corner's gradient is dotted with zero");
        let mut most = 0.0_f32;
        for i in 0..2000 {
            let (x, y, z) = (i as f32 * 0.0137, i as f32 * 0.0291, i as f32 * 0.0053);
            let (n, m) = (simplex(x, y, z, 9), simplex(x + 1e-3, y, z, 9));
            assert!(n.abs() <= 1.0, "{n} at {x}, {y}, {z}");
            assert!((n - m).abs() < 0.02, "continuous: {n} then {m}");
            most = most.max(n.abs());
        }
        assert!(most > 0.5, "and it uses its range: {most}");
        assert_ne!(simplex(0.3, 0.7, 0.1, 9), simplex(0.3, 0.7, 0.1, 10), "the key picks the gradients");
    }

    #[test]
    fn a_seed_and_a_time_make_one_field() {
        let p = Params { scale: 0.02, ..Params::default() };
        let render = |f: Frame| super::super::render(f.bbox, |x, y| f.pixel(x, y));
        assert_eq!(render(frame(3, 1.0, p)), render(frame(3, 1.0, p)));
        assert_ne!(render(frame(3, 1.0, p)), render(frame(4, 1.0, p)), "another seed, another field");
        assert_ne!(render(frame(3, 1.0, p)), render(frame(3, 9.0, p)), "the field changes with time");
        let still = Params { speed: 0.0, ..p };
        assert_eq!(render(frame(3, 1.0, still)), render(frame(3, 9.0, still)), "no speed, no motion");
        // Contrast 0 is the ramp's middle everywhere.
        let flat = render(frame(3, 1.0, Params { contrast: 0.0, ..p }));
        assert!(flat.chunks(4).all(|px| px == &flat[..4]));
        assert_eq!(frame(3, 0.0, p).uniforms(160).len(), 1360, "the WGSL `Noise` struct's size");
    }
}

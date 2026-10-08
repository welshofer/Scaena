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

use super::Color;
use super::{
    BLOCK, Encoder, MAX_STOPS, ShaderError, SplitMix64, Words, frame_box, grain_noise, local_map, lowbias32,
    oklab_linear, oklab_rgba8, ramp, sample, seed_key, thresholds, whole, within,
};
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

    /// The whole box, row-major: [`Frame::pixel`]'s bytes, worked out a [`BLOCK`] of a
    /// row at a time. Each octave goes across the block in three passes: each pixel's
    /// cell and corners, as SIMD, its simplex picked by masks rather than branches; the
    /// corners' lattice hashes, worked out once for each cell ([`Lattice`]); and the
    /// corners' shares, as SIMD. Then the ramp's colors, Oklab to linear light as SIMD, and the
    /// bytes. Each pixel does `pixel`'s arithmetic in its order, so the bytes are
    /// `pixel`'s (`render_is_pixel_for_pixel`).
    pub fn render(&self) -> Vec<u8> {
        let [_, _, width, height] = self.bbox;
        let mut out = vec![0; width as usize * height as usize * 4];
        self.render_rows(0, &mut out);
        out
    }

    /// The box's rows from `first`, as many as `out` holds whole: those rows of
    /// [`Frame::render`], byte for byte, since no row reads another.
    pub fn render_rows(&self, first: u32, out: &mut [u8]) {
        // 1/3 and 1/6, as `simplex` writes them.
        const F3: f32 = 0.333_333_34;
        const G3: f32 = 0.166_666_67;
        let [left, top, width, _] = self.bbox;
        if width == 0 {
            return;
        }
        let enc = Encoder::new();
        let [a, b, c, d, e, f] = self.map;
        // Each octave's weight, frequency, and lattice, and their weights' sum, as `pixel`
        // steps them.
        let (mut norm, mut amp, mut freq) = (0.0_f32, 1.0_f32, 1.0_f32);
        let mut octaves = Vec::with_capacity(self.octaves as usize);
        for o in 0..self.octaves {
            octaves.push((amp, freq, Lattice::new(lowbias32(self.key.wrapping_add(o)))));
            norm += amp;
            amp *= 0.5;
            freq *= 2.0;
        }
        let spread = 0.5 * self.contrast;
        let (mut px, mut py, mut sum) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK], [0.0_f32; BLOCK]);
        // Per pixel: its cell, its two middle corners as offsets in the cube, each corner's
        // place from the pixel, and each corner's hash.
        let mut cell = [[0_u32; BLOCK]; 3];
        let mut steps = [[0_u32; BLOCK]; 2];
        let mut at = [[[0.0_f32; BLOCK]; 3]; 4];
        let mut hashes = [[0_u32; BLOCK]; 4];
        // Per pixel: its color in Oklab and alpha, then in linear light, and its alpha's byte.
        let mut lab = [[0.0_f32; BLOCK]; 4];
        let mut rgb = [[0.0_f32; BLOCK]; 3];
        let mut a8 = [0_u8; BLOCK];
        for (gy, row) in (first..).zip(out.chunks_exact_mut(width as usize * 4)) {
            let fy = (top + gy) as f32 + 0.5;
            let (cfy, dfy) = (c * fy, d * fy);
            let hy = lowbias32(gy);
            for (gx, pixels) in (0..).step_by(BLOCK).zip(row.chunks_mut(BLOCK * 4)) {
                let n = pixels.len() / 4;
                let (px, py, sum) = (&mut px[..n], &mut py[..n], &mut sum[..n]);
                for i in 0..n {
                    let fx = (left + gx + i as u32) as f32 + 0.5;
                    px[i] = a * fx + cfy + e;
                    py[i] = b * fx + dfy + f;
                }
                sum.fill(0.0);
                for (amp, freq, lattice) in &mut octaves {
                    let z = self.z * *freq;
                    let [[x0, y0, z0], [x1, y1, z1], [x2, y2, z2], [x3, y3, z3]] =
                        at.each_mut().map(|corner| corner.each_mut().map(|axis| &mut axis[..n]));
                    let [ci_, cj_, ck_] = cell.each_mut().map(|axis| &mut axis[..n]);
                    let [s1, s2] = steps.each_mut().map(|o| &mut o[..n]);
                    for i in 0..n {
                        let (x, y) = (px[i] * *freq, py[i] * *freq);
                        let s = (x + y + z) * F3;
                        let ((i_, ci), (j, cj), (k, ck)) = (floor(x + s), floor(y + s), floor(z + s));
                        let t = (i_ + j + k) * G3;
                        let (cx, cy, cz) = (x - (i_ - t), y - (j - t), z - (k - t));
                        // `simplex`'s six cases, each its own mask, and its two middle corners.
                        let (xy, yz, xz) = (cx >= cy, cy >= cz, cx >= cz);
                        let (yz_, xz_) = (cy < cz, cx < cz);
                        let m =
                            [xy & yz, xy & !yz & xz, xy & !yz & !xz, !xy & yz_, !xy & !yz_ & xz_, !xy & !yz_ & !xz_];
                        let o1 = [m[0] | m[1], m[4] | m[5], m[2] | m[3]];
                        let o2 = [m[0] | m[1] | m[2] | m[5], m[0] | m[3] | m[4] | m[5], m[1] | m[2] | m[3] | m[4]];
                        let one = |b: bool| if b { 1.0 } else { 0.0 };
                        (x0[i], y0[i], z0[i]) = (cx, cy, cz);
                        (x1[i], y1[i], z1[i]) = (cx - one(o1[0]) + G3, cy - one(o1[1]) + G3, cz - one(o1[2]) + G3);
                        (x2[i], y2[i], z2[i]) = (cx - one(o2[0]) + F3, cy - one(o2[1]) + F3, cz - one(o2[2]) + F3);
                        (x3[i], y3[i], z3[i]) = (cx - 0.5, cy - 0.5, cz - 0.5);
                        (ci_[i], cj_[i], ck_[i]) = (ci as u32, cj as u32, ck as u32);
                        let code = |o: [bool; 3]| u32::from(o[0]) | u32::from(o[1]) << 1 | u32::from(o[2]) << 2;
                        (s1[i], s2[i]) = (code(o1), code(o2));
                    }
                    let [h0, h1, h2, h3] = hashes.each_mut().map(|h| &mut h[..n]);
                    for i in 0..n {
                        let corners = lattice.at([ci_[i], cj_[i], ck_[i]]);
                        let (o1, o2) = ((s1[i] & 7) as usize, (s2[i] & 7) as usize);
                        (h0[i], h1[i], h2[i], h3[i]) = (corners[0], corners[o1], corners[o2], corners[7]);
                    }
                    for i in 0..n {
                        let n0 = share(h0[i], x0[i], y0[i], z0[i]);
                        let n1 = share(h1[i], x1[i], y1[i], z1[i]);
                        let n2 = share(h2[i], x2[i], y2[i], z2[i]);
                        let n3 = share(h3[i], x3[i], y3[i], z3[i]);
                        sum[i] += *amp * (32.0 * (n0 + n1 + n2 + n3));
                    }
                }
                let [l, ca, cb, alpha] = &mut lab;
                for i in 0..n {
                    let u = (0.5 + spread * (sum[i] / norm)).clamp(0.0, 1.0);
                    let [sl, sa, sb, salpha] = sample(&self.colors, u, false);
                    let hash = lowbias32(self.key ^ lowbias32((gx + i as u32) ^ hy));
                    let grain = (hash >> 8) as f32 * (1.0 / 16_777_216.0) - 0.5;
                    (l[i], ca[i], cb[i], alpha[i]) = (sl + self.grain * grain, sa, sb, salpha);
                }
                let [r, g, bl] = &mut rgb;
                for i in 0..n {
                    [r[i], g[i], bl[i]] = oklab_linear(l[i], ca[i], cb[i]);
                    // `pixel` casts this to a byte. Clamped, it is NaN or from 0.5 to 255.5,
                    // which a cast through `i32` takes where `as u8` does, and as SIMD.
                    a8[i] = (alpha[i].clamp(0.0, 1.0) * 255.0 + 0.5) as i32 as u8;
                }
                for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    *px = [enc.byte(r[i]), enc.byte(g[i]), enc.byte(bl[i]), a8[i]];
                }
            }
        }
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

/// [`corner`] with its hash in hand, and nothing branching on the point: every lane of a
/// SIMD pass works out the share, and takes it or 0 as `corner` does.
#[inline(always)]
fn share(hash: u32, x: f32, y: f32, z: f32) -> f32 {
    let t = 0.6 - x * x - y * y - z * z;
    let t2 = t * t;
    let g = grad(hash, x, y, z);
    if t < 0.0 { 0.0 } else { t2 * t2 * g }
}

/// `x.floor()`, and that floor `as i32`, from one conversion, written so the compiler can
/// run it as SIMD on every target: below 2²³ a float's whole part fits an `i32`, and from
/// 2²³ up every float is whole. A whole float, either zero, an infinity, or NaN is its own
/// floor (`floor_is_floor_for_every_float`).
#[inline(always)]
fn floor(x: f32) -> (f32, i32) {
    let whole = x as i32;
    let t = whole as f32;
    let (below, small) = (t > x, x.abs() < 8_388_608.0);
    let f = if below { t - 1.0 } else { t };
    (if small && f != x { f } else { x }, whole - i32::from(below & small))
}

/// One octave's lattice hashes at the last cell it was asked about: the eight corners of
/// the cube from it, each hashed as [`corner`] hashes it. Neighboring pixels mostly fall
/// in one cell, so a frame hashes each cell's corners once rather than each pixel's four.
struct Lattice {
    key: u32,
    at: [u32; 3],
    corners: [u32; 8],
}

impl Lattice {
    /// The lattice `key` picks the gradients of, at the cell at the origin.
    fn new(key: u32) -> Self {
        let mut lattice = Lattice { key, at: [0; 3], corners: [0; 8] };
        lattice.hash();
        lattice
    }

    /// The hashes of the corners of the cell at `at`, numbered by the axes each steps
    /// along from it: bit 0 for x, 1 for y, 2 for z.
    fn at(&mut self, at: [u32; 3]) -> &[u32; 8] {
        if (at[0] ^ self.at[0]) | (at[1] ^ self.at[1]) | (at[2] ^ self.at[2]) != 0 {
            self.at = at;
            self.hash();
        }
        &self.corners
    }

    fn hash(&mut self) {
        let [x, y, z] = self.at;
        for dz in 0..2 {
            let hz = lowbias32(z.wrapping_add(dz));
            for dy in 0..2 {
                let hy = lowbias32(y.wrapping_add(dy) ^ hz);
                for dx in 0..2 {
                    let o = (dx | dy << 1 | dz << 2) as usize;
                    self.corners[o] = lowbias32(self.key ^ lowbias32(x.wrapping_add(dx) ^ hy));
                }
            }
        }
    }
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

    /// `render` is `pixel` at every pixel, bit for bit: at each count of octaves, with
    /// cells far wider than a pixel and far narrower, in a box turned and offset whose rows
    /// are not whole blocks, with grain and without, colors opaque and not, and a field
    /// moved to where its cells are negative.
    #[test]
    fn render_is_pixel_for_pixel() {
        let clear = [Color([14, 12, 20, 255]), Color([90, 40, 200, 128]), Color([255, 200, 87, 0])];
        let turned = [0.8, 0.6, -0.6, 0.8, 37.0, 11.0];
        let render = |f: &Frame| super::super::render(f.bbox, |x, y| f.pixel(x, y));
        for octaves in 1..=8 {
            // `scale` is at most 0.1 in a deck; 3 puts a cell edge between most pixels.
            for (scale, grain, contrast) in [(0.0015, 0.0, 0.6), (0.02, 0.1, 1.0), (3.0, 0.25, 4.0)] {
                let params = Params { scale, octaves, speed: 0.4, contrast, grain };
                let palette: &[Color] = if octaves % 2 == 0 { &PALETTE } else { &clear };
                let rect = [3.5, 1.0, 297.0, 61.0];
                let f =
                    Frame::new(u64::from(octaves), 2.75, palette, &params, rect, turned, [320, 120]).unwrap().unwrap();
                assert!(f.bbox[2] > 2 * BLOCK as u32 && !f.bbox[2].is_multiple_of(BLOCK as u32), "{:?}", f.bbox);
                assert!(f.render() == render(&f), "{octaves} octaves at scale {scale}");
                let below = Frame { map: [0.37, -0.11, 0.13, 0.41, -50.3, -20.7], z: -3.9, ..f };
                assert!(below.render() == render(&below), "{octaves} octaves at scale {scale}, below zero");
            }
        }
    }

    /// [`floor`] is `f32::floor`, bit for bit, on a sweep of floats and on each side of
    /// every edge it has: whole numbers near zero, 2²³ and 2²⁴, `i32`'s ends, and the
    /// largest float.
    #[test]
    fn floor_is_floor() {
        let edges = [0.5_f32, 1.0, 2.0, 3.0, 8_388_608.0, 16_777_216.0, 2_147_483_648.0, f32::MAX];
        let near = edges.into_iter().flat_map(|v| [v, -v]).flat_map(|v| {
            let bits = v.to_bits();
            (-3..=3).map(move |k| f32::from_bits(bits.wrapping_add_signed(k)))
        });
        let sweep = (0..=u32::MAX).step_by(4099).map(f32::from_bits);
        for v in near.chain(sweep).chain([0.0, -0.0, f32::INFINITY, f32::NEG_INFINITY, f32::NAN]) {
            let ((got, whole), want) = (floor(v), v.floor());
            assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "{v:?} ({:#010x})", v.to_bits());
            assert_eq!(whole, want as i32, "{v:?} ({:#010x})", v.to_bits());
        }
    }

    /// Every one of the 2³² floats. Slow unoptimized: `cargo test --release -p scaena-core
    /// every_float -- --ignored`.
    #[test]
    #[ignore]
    fn floor_is_floor_for_every_float() {
        for bits in 0..=u32::MAX {
            let v = f32::from_bits(bits);
            let ((got, whole), want) = (floor(v), v.floor());
            assert!(got.to_bits() == want.to_bits() || got.is_nan() && want.is_nan(), "{v:?} ({bits:#010x})");
            assert_eq!(whole, want as i32, "{v:?} ({bits:#010x})");
        }
    }
}

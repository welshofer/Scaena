//! `caustics`: light on water (SPEC §3.8).
//!
//! Two layers of cells, the second finer than the first, turned against it, and flowing
//! faster, so the light never reads as a grid; each layer bent by two slow waves, so its lines
//! curve and waver. Each cell holds one seeded point, which wobbles about its place on a slow
//! Lissajous path, as a mesh's points drift; a pixel's light in a layer comes from how near it
//! lies to the edge between the two points nearest it (a Voronoi cell's edge: F2 − F1, the
//! second-nearest's distance less the nearest's), sharpened by `sharpness`. The coarse layer's
//! light is the network, and the fine layer's, its lines softer, swells it, so each line is
//! uneven and brightest where the two layers' lines cross. Times `intensity`, the light picks a
//! color along the palette's ramp, blended in Oklab: the first color is the water, the rest the
//! light, dim to bright. `grain` adds seeded noise to Oklab lightness, per device pixel.
//!
//! Each frame works out the paths' places once (`libm` sine and cosine); a cell's point is
//! hashed from its cell and picks two of them. Per pixel it is `+ − × ÷`, square roots,
//! rounding, `floor`, comparisons, and integer hashing, here and in `caustics.wgsl` alike.

use super::Color;
use super::{
    BLOCK, Encoder, MAX_STOPS, ShaderError, SplitMix64, Words, frame_box, grain_noise, local_map, lowbias32,
    noise::floor, oklab_linear, oklab_rgba8, ramp, seed_key, thresholds, within,
};
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("caustics.wgsl");

/// The paths a cell's point may wobble on, in each layer (the WGSL uniform array's length).
pub const PATHS: usize = 16;
/// Where a cell's point rests across and down its cell: from `REST` to `REST + SPREAD`. With
/// its wobble it keeps 0.1 of a cell from the cell's edges, so the two points nearest any
/// pixel lie among the nine cells around its own.
const REST: f32 = 0.2;
const SPREAD: f32 = 0.6;
/// How far a point wobbles from its place, in cells.
const WOBBLE: f64 = 0.1;
/// The fine layer: its cells against the coarse layer's, and how much faster it moves.
const FINE: f64 = 0.55;
const FASTER: f64 = 1.4;
/// How far each layer's cells are bent, in cells, by waves `1 / BEND` cells long that run
/// across them `RIPPLE` waves a second at `speed` 1: so the lines curve and waver.
const WARP: f32 = 0.4;
const BEND: f32 = 0.31;
const RIPPLE: f64 = 0.5;
/// How much of the coarse layer's light shows where the fine layer is dark: the rest comes
/// only where the fine layer's light crosses it, so each line is uneven and brightest where the
/// two layers' lines meet.
const FLOOR: f32 = 0.3;
/// How much wider the fine layer's lines are than the coarse layer's: soft swells along them.
const SOFTER: f32 = 2.5;
/// A line's half width, in cells, at `sharpness` 1.
const WIDTH: f32 = 0.25;
/// Cells a second the light flows at `speed` 1.
const FLOW: f64 = 0.5;
/// How far the seed moves each layer, in cells.
const SCATTER: f64 = 256.0;

/// Typed caustics parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// A cell's size, in shorter sides of the rect, 0.02–2.
    pub scale: f32,
    /// How fast the light wobbles and flows, 0–2.
    pub speed: f32,
    /// How bright the light is: how far along the ramp a line's middle reaches, 0–2.
    pub intensity: f32,
    /// How thin the lines are, 1–16.
    pub sharpness: f32,
    /// Noise amplitude in Oklab lightness, 0–0.25.
    pub grain: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { scale: 0.18, speed: 0.1, intensity: 1.0, sharpness: 6.0, grain: 0.0 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            match name.as_str() {
                "scale" => p.scale = within(name, v, 0.02, 2.0)?,
                "speed" => p.speed = within(name, v, 0.0, 2.0)?,
                "intensity" => p.intensity = within(name, v, 0.0, 2.0)?,
                "sharpness" => p.sharpness = within(name, v, 1.0, 16.0)?,
                "grain" => p.grain = within(name, v, 0.0, 0.25)?,
                _ => {
                    let problem = "unknown; caustics take scale, speed, intensity, sharpness, and grain".to_string();
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
            }
        }
        Ok(p)
    }
}

/// One frame of caustics over a box of device pixels: everything [`Frame::pixel`] reads,
/// which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// For each layer, a device pixel's center to its cells, `[a, b, c, d, e, f]` (`u = a·px +
    /// c·py + e`): a cell is 1 across.
    pub maps: [[f32; 6]; 2],
    /// For each layer, the whole cells its map leaves out, across and down: added to a cell's
    /// place before it is hashed.
    pub cells: [[i32; 2]; 2],
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    /// For each layer, where its two waves are along their length, 0 to 1: the one that bends
    /// it across, then the one that bends it down.
    pub waves: [[f32; 2]; 2],
    /// Per path, where it has taken a point from its place: across and down in the coarse
    /// layer, then in the fine one, in cells.
    pub paths: [[f32; 4]; PATHS],
    /// Each layer's key, which its cells' points are hashed from.
    pub keys: [u32; 2],
    /// Seeds the grain.
    pub key: u32,
    /// For each layer, 1 over a line's half width, in its cells.
    pub inv_widths: [f32; 2],
    pub intensity: f32,
    pub grain: f32,
    /// Per stop: Oklab L, a, b, and alpha 0–1.
    pub colors: Vec<[f32; 4]>,
}

impl Frame {
    /// The caustics at `t` seconds over `rect` (canvas units), drawn through `device` into a
    /// raster `size` pixels across. `None` when they cover no pixel.
    pub fn new(
        seed: u64,
        t: f32,
        palette: &[Color],
        params: &Params,
        rect: [f32; 4],
        device: [f64; 6],
        size: [u32; 2],
    ) -> Result<Option<Frame>, ShaderError> {
        let colors = ramp("caustics", palette)?;
        let Some((bbox, inverse)) = frame_box(rect, device, size)? else { return Ok(None) };
        let side = f64::from(rect[2].min(rect[3]));
        let [a, b, c, d, e, f] = local_map(inverse, rect, 1.0 / side);
        let (t, speed) = (f64::from(t), f64::from(params.speed));
        let mut rng = SplitMix64(seed);
        let mut maps = [[0.0; 6]; 2];
        let mut keys = [0; 2];
        let mut cells = [[0; 2]; 2];
        for (k, ((map, key), cell_at)) in maps.iter_mut().zip(&mut keys).zip(&mut cells).enumerate() {
            // Each layer turned its own way, flowing its own way, from its own place.
            let cell = f64::from(params.scale) * if k == 0 { 1.0 } else { FINE };
            let turn = std::f64::consts::TAU * rng.unit();
            let (cos, sin) = (libm::cos(turn) / cell, libm::sin(turn) / cell);
            let toward = std::f64::consts::TAU * rng.unit();
            let flow = FLOW * speed * t * if k == 0 { 1.0 } else { FASTER };
            let shift = [0, 1].map(|_| SCATTER * rng.unit());
            let shift = [shift[0] + flow * libm::cos(toward), shift[1] + flow * libm::sin(toward)];
            // The shift's whole cells go to the cells' hashes, and only its part of a cell to the
            // map, so a frame an hour in measures in cells as small as its first frame's.
            let whole = shift.map(f64::floor);
            *cell_at = whole.map(|w| w.rem_euclid(4_294_967_296.0) as u32 as i32);
            let m = [
                cos * a - sin * b,
                sin * a + cos * b,
                cos * c - sin * d,
                sin * c + cos * d,
                cos * e - sin * f + (shift[0] - whole[0]),
                sin * e + cos * f + (shift[1] - whole[1]),
            ];
            *map = m.map(|v| v as f32);
            *key = lowbias32(seed_key(seed) ^ lowbias32(k as u32 + 1));
        }
        let mut paths = [[0.0; 4]; PATHS];
        for path in &mut paths {
            for k in 0..2 {
                let faster = if k == 0 { 1.0 } else { FASTER };
                let pace = [0, 1].map(|_| std::f64::consts::TAU * speed * faster * (0.5 + rng.unit()));
                let phase = [0, 1].map(|_| std::f64::consts::TAU * rng.unit());
                path[2 * k] = (WOBBLE * libm::cos(pace[0] * t + phase[0])) as f32;
                path[2 * k + 1] = (WOBBLE * libm::sin(pace[1] * t + phase[1])) as f32;
            }
        }
        let mut waves = [[0.0; 2]; 2];
        for (k, wave) in waves.iter_mut().enumerate() {
            let faster = if k == 0 { 1.0 } else { FASTER };
            *wave = [0, 1].map(|_| {
                let at = rng.unit() + RIPPLE * speed * faster * t;
                (at - at.floor()) as f32
            });
        }
        Ok(Some(Frame {
            maps,
            cells,
            bbox,
            waves,
            paths,
            keys,
            key: seed_key(seed),
            inv_widths: [params.sharpness / WIDTH, params.sharpness / (WIDTH * SOFTER)],
            intensity: params.intensity,
            grain: params.grain,
            colors,
        }))
    }

    /// Where the point of cell `(i, j)` of layer `k` is in its cell, across and down.
    #[inline(always)]
    fn point(&self, k: usize, i: i32, j: i32) -> [f32; 2] {
        let [oi, oj] = self.cells[k];
        let (i, j) = (i.wrapping_add(oi) as u32, j.wrapping_add(oj) as u32);
        let h = lowbias32(self.keys[k] ^ lowbias32(i ^ lowbias32(j)));
        let g = lowbias32(h);
        let across = self.paths[(g & 15) as usize][2 * k];
        let down = self.paths[((g >> 4) & 15) as usize][2 * k + 1];
        let x = REST + SPREAD * ((h & 0xffff) as f32 * (1.0 / 65_536.0)) + across;
        let y = REST + SPREAD * ((h >> 16) as f32 * (1.0 / 65_536.0)) + down;
        [x, y]
    }

    /// Where the point of cell `(i, j)` of layer `k` is, in the layer's cells: the same two
    /// numbers whichever pixel asks, so the two nearest a pixel are the same however many cells'
    /// points it is measured against, as long as they include the nine around its own.
    #[inline(always)]
    fn at(&self, k: usize, i: i32, j: i32) -> [f32; 2] {
        let [qx, qy] = self.point(k, i, j);
        [i as f32 + qx, j as f32 + qy]
    }

    /// Layer `k`'s light at `(u, v)`, in its cells: 1 on an edge, falling away from it.
    fn light(&self, k: usize, u: f32, v: f32) -> f32 {
        let ((_, i), (_, j)) = (floor(u), floor(v));
        let (mut f1, mut f2) = (f32::MAX, f32::MAX);
        for cj in j.wrapping_sub(1)..=j.wrapping_add(1) {
            for ci in i.wrapping_sub(1)..=i.wrapping_add(1) {
                let [x, y] = self.at(k, ci, cj);
                let (dx, dy) = (u - x, v - y);
                let dd = dx * dx + dy * dy;
                f2 = less(f2, more(f1, dd));
                f1 = less(f1, dd);
            }
        }
        lit(f1, f2, self.inv_widths[k])
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `caustics.wgsl` is this
    /// function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let px = (self.bbox[0] + gx) as f32 + 0.5;
        let py = (self.bbox[1] + gy) as f32 + 0.5;
        let mut light = [0.0_f32; 2];
        for (k, l) in light.iter_mut().enumerate() {
            let [a, b, c, d, e, f] = self.maps[k];
            let [u, v] = bent(a * px + c * py + e, b * px + d * py + f, self.waves[k]);
            *l = self.light(k, u, v);
        }
        let lit = light[0] * (FLOOR + (1.0 - FLOOR) * light[1]);
        let s = (self.intensity * lit).clamp(0.0, 1.0) * (self.colors.len() - 1) as f32;
        let [mut l, mut ca, mut cb, mut alpha] = [0.0_f32; 4];
        for (j, c) in self.colors.iter().enumerate() {
            let w = more(0.0, 1.0 - (s - j as f32).abs());
            (l, ca, cb, alpha) = (l + w * c[0], ca + w * c[1], cb + w * c[2], alpha + w * c[3]);
        }
        let l = l + self.grain * grain_noise(self.key, gx, gy);
        oklab_rgba8(l, ca, cb, alpha)
    }

    /// The whole box, row-major: [`Frame::pixel`]'s bytes, worked out a [`BLOCK`] of a row at
    /// a time. For each layer, each pixel's place in its cells, bent, goes across the block as
    /// SIMD; then, a [`PART`] of the block at a time, each point of the cells the part lies in,
    /// and of the ring around them, is hashed once and measured against eight of its pixels at
    /// once, their two nearest kept in registers. That is more points than a pixel's nine, but the
    /// two nearest are the same two, the same distances away, since a point's place is the same
    /// whichever pixel asks and the nine hold the two nearest. Then each stop's share of the ramp,
    /// Oklab to linear light as SIMD, and the bytes. Each pixel's arithmetic is `pixel`'s, so the
    /// bytes are `pixel`'s (`render_is_pixel_for_pixel`).
    pub fn render(&self) -> Vec<u8> {
        let [_, _, width, height] = self.bbox;
        let mut out = vec![0; width as usize * height as usize * 4];
        self.render_rows(0, &mut out);
        out
    }

    /// The box's rows from `first`, as many as `out` holds whole: those rows of
    /// [`Frame::render`], byte for byte, since no row reads another.
    pub fn render_rows(&self, first: u32, out: &mut [u8]) {
        let [left, top, width, _] = self.bbox;
        if width == 0 {
            return;
        }
        let enc = Encoder::new();
        // Per pixel: its place in each layer's cells, the squared distances to the two nearest
        // points, then each layer's light.
        let (mut fu, mut fv) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
        let (mut f1, mut f2) = ([0.0_f32; BLOCK], [0.0_f32; BLOCK]);
        let mut light = [[0.0_f32; BLOCK]; 2];
        let mut along = [0.0_f32; BLOCK];
        let mut lab = [[0.0_f32; BLOCK]; 4];
        let spans = (self.colors.len() - 1) as f32;
        let mut rgb = [[0.0_f32; BLOCK]; 3];
        let mut a8 = [0_u8; BLOCK];
        for (gy, row) in (first..).zip(out.chunks_exact_mut(width as usize * 4)) {
            let fy = (top + gy) as f32 + 0.5;
            let hy = lowbias32(gy);
            for (gx, pixels) in (0..).step_by(BLOCK).zip(row.chunks_mut(BLOCK * 4)) {
                let n = pixels.len() / 4;
                for (k, light) in light.iter_mut().enumerate() {
                    let [a, b, c, d, e, f] = self.maps[k];
                    let (cfy, dfy) = (c * fy, d * fy);
                    let (u, v) = (&mut fu[..n], &mut fv[..n]);
                    for i in 0..n {
                        let fx = (left + gx + i as u32) as f32 + 0.5;
                        [u[i], v[i]] = bent(a * fx + cfy + e, b * fx + dfy + f, self.waves[k]);
                    }
                    let (f1, f2) = (&mut f1[..n], &mut f2[..n]);
                    f1.fill(f32::MAX);
                    f2.fill(f32::MAX);
                    // A part of the block at a time: the cells its pixels lie in, and the ring
                    // around them, a superset of each pixel's nine, which `pixel` measures it
                    // against. A part spans fewer cells than the block, so fewer points.
                    for (((u, v), f1), f2) in
                        u.chunks(PART).zip(v.chunks(PART)).zip(f1.chunks_mut(PART)).zip(f2.chunks_mut(PART))
                    {
                        let ([ulo, uhi], [vlo, vhi]) = (span(u), span(v));
                        let (i0, i1) = (floor(ulo).1.wrapping_sub(1), floor(uhi).1.wrapping_add(1));
                        let (j0, j1) = (floor(vlo).1.wrapping_sub(1), floor(vhi).1.wrapping_add(1));
                        // The part's points, a batch at a time, each measured against eight
                        // pixels at once whose two nearest stay in registers across the batch.
                        let mut cells = (j0..=j1).flat_map(|cj| (i0..=i1).map(move |ci| (ci, cj)));
                        let mut batch = [[0.0_f32; 2]; BATCH];
                        loop {
                            let mut len = 0;
                            for (p, (ci, cj)) in batch.iter_mut().zip(cells.by_ref()) {
                                *p = self.at(k, ci, cj);
                                len += 1;
                            }
                            if len == 0 {
                                break;
                            }
                            let points = &batch[..len];
                            let (u8s, u_rest) = u.as_chunks::<8>();
                            let (v8s, v_rest) = v.as_chunks::<8>();
                            let (f18s, f1_rest) = f1.as_chunks_mut::<8>();
                            let (f28s, f2_rest) = f2.as_chunks_mut::<8>();
                            for (((u, v), f1), f2) in u8s.iter().zip(v8s).zip(f18s).zip(f28s) {
                                let (mut a, mut b) = (*f1, *f2);
                                for &[x, y] in points {
                                    for l in 0..8 {
                                        let (dx, dy) = (u[l] - x, v[l] - y);
                                        let dd = dx * dx + dy * dy;
                                        b[l] = less(b[l], more(a[l], dd));
                                        a[l] = less(a[l], dd);
                                    }
                                }
                                (*f1, *f2) = (a, b);
                            }
                            for (((u, v), f1), f2) in u_rest.iter().zip(v_rest).zip(f1_rest).zip(f2_rest) {
                                for &[x, y] in points {
                                    let (dx, dy) = (u - x, v - y);
                                    let dd = dx * dx + dy * dy;
                                    *f2 = less(*f2, more(*f1, dd));
                                    *f1 = less(*f1, dd);
                                }
                            }
                            if len < BATCH {
                                break;
                            }
                        }
                    }
                    let light = &mut light[..n];
                    for i in 0..n {
                        light[i] = lit(f1[i], f2[i], self.inv_widths[k]);
                    }
                }
                // Where each pixel is along the ramp, then each stop's share of it, as SIMD.
                let [l, ca, cb, alpha] = lab.each_mut().map(|v| &mut v[..n]);
                let (coarse, fine) = (&light[0][..n], &light[1][..n]);
                let along = &mut along[..n];
                for i in 0..n {
                    let lit = coarse[i] * (FLOOR + (1.0 - FLOOR) * fine[i]);
                    along[i] = (self.intensity * lit).clamp(0.0, 1.0) * spans;
                }
                for v in [&mut *l, &mut *ca, &mut *cb, &mut *alpha] {
                    v.fill(0.0);
                }
                for (j, c) in self.colors.iter().enumerate() {
                    for i in 0..n {
                        let w = more(0.0, 1.0 - (along[i] - j as f32).abs());
                        (l[i], ca[i], cb[i], alpha[i]) =
                            (l[i] + w * c[0], ca[i] + w * c[1], cb[i] + w * c[2], alpha[i] + w * c[3]);
                    }
                }
                for (x, l) in (gx..).zip(l.iter_mut()) {
                    let hash = lowbias32(self.key ^ lowbias32(x ^ hy));
                    let grain = (hash >> 8) as f32 * (1.0 / 16_777_216.0) - 0.5;
                    *l += self.grain * grain;
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

    /// The uniform buffer `caustics.wgsl` declares as `Caustics`, for output rows `stride`
    /// words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let mut out = Words(Vec::with_capacity(1680));
        let [[a0, b0, c0, d0, e0, f0], [a1, b1, c1, d1, e1, f1]] = self.maps;
        out.f32s([a0, b0, c0, d0, a1, b1, c1, d1, e0, f0, e1, f1]);
        out.f32s([self.inv_widths[0], self.intensity, self.grain, self.inv_widths[1]]);
        out.u32s(self.bbox);
        out.u32s([self.keys[0], self.keys[1], self.key, stride]);
        out.u32s([self.colors.len() as u32, 0, 0, 0]);
        out.u32s(self.cells.as_flattened().iter().map(|&c| c as u32));
        out.f32s(self.waves.as_flattened().iter().copied());
        self.paths.iter().for_each(|p| out.f32s(*p));
        (0..MAX_STOPS).for_each(|i| out.f32s(self.colors.get(i).copied().unwrap_or_default()));
        out.f32s(*thresholds());
        out.0
    }
}

/// How many pixels of a block [`Frame::render_rows`] measures against one set of points.
const PART: usize = 64;

/// The most points [`Frame::render_rows`] measures pixels against in one pass.
const BATCH: usize = 32;

/// The least and the greatest of `values`, eight lanes at a time, as SIMD.
#[inline(always)]
fn span(values: &[f32]) -> [f32; 2] {
    let (mut lo, mut hi) = ([f32::MAX; 8], [f32::MIN; 8]);
    let (chunks, rest) = values.as_chunks::<8>();
    for c in chunks {
        for l in 0..8 {
            (lo[l], hi[l]) = (less(lo[l], c[l]), more(hi[l], c[l]));
        }
    }
    for &x in rest {
        (lo[0], hi[0]) = (less(lo[0], x), more(hi[0], x));
    }
    [lo.into_iter().fold(f32::MAX, less), hi.into_iter().fold(f32::MIN, more)]
}

/// `(u, v)` bent by a layer's two waves: across by a wave down the layer, and down by a wave
/// across it, each [`wave`] at its place along its length.
#[inline(always)]
fn bent(u: f32, v: f32, [across, down]: [f32; 2]) -> [f32; 2] {
    [u + WARP * wave(v * BEND + across), v + WARP * wave(u * BEND + down)]
}

/// A smooth wave, 1 long, from −1 to 1: two halves of a parabola, which meet with the same slope,
/// as near a sine as rounding and `+ − ×` make one. `x` is rounded to the nearest whole number,
/// ties to even, as WGSL's `round` rounds, by adding and taking away 1.5 × 2²³: exact for every
/// `x` a wave is asked at, whose size is under 2²², and as SIMD.
#[inline(always)]
fn wave(x: f32) -> f32 {
    const ROUND: f32 = 12_582_912.0;
    let y = 2.0 * (x - ((x + ROUND) - ROUND));
    4.0 * y * (1.0 - y.abs())
}

/// The less of two distances, as one SIMD instruction takes it.
#[inline(always)]
fn less(a: f32, b: f32) -> f32 {
    if a < b { a } else { b }
}

/// The greater of two distances.
#[inline(always)]
fn more(a: f32, b: f32) -> f32 {
    if a > b { a } else { b }
}

/// A layer's light from the squared distances to the two points nearest a pixel, in cells: half
/// of F2 − F1, about how far the pixel is from the edge between them, over a line's half width,
/// falling off as `1 / (1 + s²)²`.
#[inline(always)]
fn lit(f1: f32, f2: f32, inv_width: f32) -> f32 {
    let s = 0.5 * (f2.sqrt() - f1.sqrt()) * inv_width;
    let w = 1.0 + s * s;
    1.0 / (w * w)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WATER: [Color; 3] = [Color([6, 40, 61, 255]), Color([37, 109, 133, 255]), Color([200, 240, 240, 255])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(seed: u64, t: f32, params: Params) -> Frame {
        Frame::new(seed, t, &WATER, &params, [0.0, 0.0, 320.0, 180.0], ID, [320, 180]).unwrap().unwrap()
    }

    fn map(pairs: &[(&str, f32)]) -> BTreeMap<String, f32> {
        pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn params_take_defaults_and_refuse_what_they_cannot_draw() {
        assert_eq!(Params::from_map(&map(&[])).unwrap(), Params::default());
        let p = Params::from_map(&map(&[("scale", 0.5), ("sharpness", 9.0)])).unwrap();
        assert_eq!((p.scale, p.sharpness, p.speed), (0.5, 9.0, 0.1));
        for (k, v) in [
            ("scale", 0.0),
            ("scale", 2.5),
            ("speed", -0.1),
            ("intensity", 3.0),
            ("sharpness", 0.5),
            ("sharpness", 17.0),
            ("grain", 0.3),
            ("grain", f32::NAN),
            ("points", 5.0),
        ] {
            let err = Params::from_map(&map(&[(k, v)])).unwrap_err().to_string();
            assert!(err.contains(k), "{k} = {v}: {err}");
        }
    }

    #[test]
    fn a_seed_and_a_time_make_one_image() {
        let p = Params::default();
        assert_eq!(frame(7, 1.5, p).render(), frame(7, 1.5, p).render());
        assert_ne!(frame(7, 1.5, p).render(), frame(8, 1.5, p).render(), "another seed, other water");
        assert_ne!(frame(7, 1.5, p).render(), frame(7, 3.0, p).render(), "the light moves");
        let still = Params { speed: 0.0, ..p };
        assert_eq!(frame(7, 1.5, still).render(), frame(7, 30.0, still).render(), "no speed, no motion");
    }

    /// The light runs in thin lines over the water: most of the box is near the water's color,
    /// and a little of it reaches the light's.
    #[test]
    fn the_light_is_thin_lines_over_the_water() {
        let px = frame(3, 0.0, Params::default()).render();
        let lum: Vec<u32> =
            px.as_chunks::<4>().0.iter().map(|p| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2])).collect();
        let water = 6 + 40 + 61;
        let dark = lum.iter().filter(|&&l| l < water + 60).count() as f64 / lum.len() as f64;
        let bright = lum.iter().filter(|&&l| l > 400).count() as f64 / lum.len() as f64;
        assert!(dark > 0.5, "most is water: {dark}");
        assert!(bright > 0.005 && bright < 0.2, "a little is light: {bright}");
        // Sharper lines light less of it.
        let sharp = frame(3, 0.0, Params { sharpness: 12.0, ..Params::default() }).render();
        let lit =
            |px: &[u8]| px.as_chunks::<4>().0.iter().filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 300).count();
        assert!(lit(&sharp) < lit(&px));
        // With no intensity it is the water alone.
        let flat = frame(3, 0.0, Params { intensity: 0.0, ..Params::default() }).render();
        assert!(flat.as_chunks::<4>().0.iter().all(|p| p[..3] == WATER[0].0[..3]));
    }

    /// The nine cells around a pixel's own hold the two points nearest it, in either layer:
    /// against the twenty-five around it, at every pixel of a frame at several times.
    #[test]
    fn nine_cells_hold_the_two_nearest_points() {
        for t in [0.0, 3.0, 17.0] {
            let f = frame(11, t, Params { scale: 0.05, speed: 1.0, ..Params::default() });
            for gy in 0..180 {
                for gx in 0..320 {
                    let (px, py) = (gx as f32 + 0.5, gy as f32 + 0.5);
                    for k in 0..2 {
                        let [a, b, c, d, e, g] = f.maps[k];
                        let [u, v] = bent(a * px + c * py + e, b * px + d * py + g, f.waves[k]);
                        let (i, j) = (u.floor() as i32, v.floor() as i32);
                        let (fu, fv) = (u - u.floor(), v - v.floor());
                        let mut ds: Vec<(f32, i32)> = Vec::new();
                        for dj in -2..=2 {
                            for di in -2..=2 {
                                let [qx, qy] = f.point(k, i + di, j + dj);
                                let (dx, dy) = (fu - (di as f32 + qx), fv - (dj as f32 + qy));
                                ds.push((dx * dx + dy * dy, di.abs().max(dj.abs())));
                            }
                        }
                        ds.sort_by(|x, y| x.0.total_cmp(&y.0));
                        assert!(ds[0].1 <= 1 && ds[1].1 <= 1, "t {t}, pixel ({gx}, {gy}), layer {k}: {:?}", &ds[..3]);
                    }
                }
            }
        }
    }

    /// `render` is `pixel` at every pixel, bit for bit: in boxes whose rows are not whole
    /// blocks, scaled, offset, and turned onto the device, with cells small and large, an hour
    /// in, and palettes opaque and not.
    #[test]
    fn render_is_pixel_for_pixel() {
        let clear = [Color([15, 118, 110, 0]), Color([67, 56, 202, 128]), Color([255, 255, 255, 255])];
        let turned = [0.8, 0.6, -0.6, 0.8, 150.0, 20.0];
        type Case<'a> = (&'a [Color], Params, f32, [f32; 4], [f64; 6], [u32; 2]);
        let cases: [Case; 5] = [
            (&WATER, Params::default(), 2.5, [0.0, 0.0, 320.0, 180.0], ID, [320, 180]),
            (
                &WATER,
                Params { scale: 0.02, sharpness: 16.0, grain: 0.25, ..Params::default() },
                3600.0,
                [3.5, 7.25, 301.0, 77.0],
                [1.5, 0.0, 0.0, 1.5, 0.25, 0.0],
                [600, 200],
            ),
            (
                &clear,
                Params { scale: 2.0, intensity: 2.0, ..Params::default() },
                0.0,
                [0.0, 0.0, 300.0, 300.0],
                turned,
                [400, 400],
            ),
            (&WATER[..1], Params { sharpness: 1.0, ..Params::default() }, 9.0, [10.0, 10.0, 129.0, 3.0], ID, [200, 20]),
            (&WATER, Params { speed: 2.0, ..Params::default() }, 1.0, [-40.0, -40.0, 200.0, 120.0], ID, [160, 90]),
        ];
        for (i, (palette, params, t, rect, device, size)) in cases.into_iter().enumerate() {
            let f = Frame::new(i as u64 * 7919, t, palette, &params, rect, device, size).unwrap().unwrap();
            let [_, _, w, h] = f.bbox;
            let want: Vec<u8> =
                (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| f.pixel(x, y)).collect();
            assert!(f.render() == want, "case {i}, box {:?}", f.bbox);
        }
    }

    /// How long a full-bleed 1080p frame takes beside a mesh's, on one thread: `cargo test
    /// --release -p scaena-pixels timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timing() {
        let dusk = [Color([0x1B, 0x14, 0x30, 255]), Color([0x3A, 0x1F, 0x4F, 255]), Color([0xB0, 0x45, 0x2C, 255])];
        let rect = [0.0, 0.0, 1920.0, 1080.0];
        let caustics = Params { scale: 0.22, speed: 0.12, intensity: 0.9, sharpness: 5.0, grain: 0.0 };
        let c = Frame::new(1, 2.0, &dusk, &caustics, rect, ID, [1920, 1080]).unwrap().unwrap();
        let m = super::super::mesh::Params { points: 6, drift: 0.14, softness: 0.85, grain: 0.02 };
        let m = super::super::mesh::Frame::new(1, 2.0, &dusk, &m, rect, ID, [1920, 1080]).unwrap().unwrap();
        let (c, m) = (super::super::Job::Caustics(Box::new(c)), super::super::Job::Mesh(m));
        for (name, job) in [("mesh", &m), ("caustics", &c)] {
            let mut best = f64::MAX;
            for _ in 0..12 {
                let t = std::time::Instant::now();
                std::hint::black_box(job.render_on(1));
                best = best.min(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("{name}: {best:.1} ms");
        }
    }

    #[test]
    fn the_uniforms_are_the_wgsl_structs_size() {
        assert_eq!(frame(1, 0.0, Params::default()).uniforms(320).len(), 1680, "the WGSL `Caustics` struct's size");
    }
}

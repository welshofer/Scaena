//! What a shader op draws (SPEC §3.8): each kind's CPU reference implementation, and
//! its WGSL twin beside it. Painters run one or the other (SPEC §6); both read the
//! same uniforms, computed here once per frame with `libm`, so per pixel they do the
//! same arithmetic in the same order and differ only by GPU rounding. They live in
//! core because every painter runs them and painters depend only on core.
//!
//! A [`Job`] is one shader op made ready to draw over a box of device pixels: the
//! CPU painter calls [`Job::render`]; a GPU painter dispatches [`Job::wgsl`] over the
//! same box with [`Job::uniforms`] and copies the bytes it writes into a texture.
//!
//! Every v1 kind is here (PLAN 0.11, 1.10): `mesh`, `gradient`, `noise`, `grain`, and
//! `particles`.

pub mod gradient;
pub mod grain;
pub mod mesh;
pub mod noise;
pub mod particles;

use crate::displaylist::{Color, Op, ShaderKind};
use std::collections::BTreeMap;
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ShaderError {
    #[error("shader param `{name}`: {problem}")]
    Param { name: String, problem: String },
    #[error("a shader needs at least one palette color")]
    EmptyPalette,
    #[error("a {kind} shader runs through at most {max} palette colors; this palette has {len}")]
    Palette { kind: &'static str, max: usize, len: usize },
    #[error("shader rect {0:?}: expected a finite width and height above 0")]
    Rect([f32; 4]),
}

/// One shader op over a box of device pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Mesh(mesh::Frame),
    Gradient(gradient::Frame),
    Noise(noise::Frame),
    Grain(grain::Frame),
    Particles(particles::Frame),
}

impl Job {
    /// The job for shader op `op` drawn through `device` (canvas units to device pixels,
    /// `[a, b, c, d, e, f]` as in the display list) into a raster `size` pixels across.
    /// `None` when the op covers no pixel of the raster.
    pub fn new(op: &Op, device: [f64; 6], size: [u32; 2]) -> Result<Option<Job>, ShaderError> {
        let Op::Shader { kind, seed, t, rect, palette, params } = op else {
            return Ok(None);
        };
        let (seed, t, rect) = (*seed, *t, *rect);
        Ok(match kind {
            ShaderKind::Mesh => {
                let params = mesh::Params::from_map(params)?;
                mesh::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Mesh)
            }
            ShaderKind::Gradient => {
                let params = gradient::Params::from_map(params)?;
                gradient::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Gradient)
            }
            ShaderKind::Noise => {
                let params = noise::Params::from_map(params)?;
                noise::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Noise)
            }
            ShaderKind::Grain => {
                let params = grain::Params::from_map(params)?;
                grain::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Grain)
            }
            ShaderKind::Particles => {
                let params = particles::Params::from_map(params)?;
                particles::Frame::new(seed, t, palette, &params, rect, device, size)?.map(Job::Particles)
            }
        })
    }

    /// `[x, y, width, height]` in device pixels: where [`Job::render`]'s pixels go.
    pub fn bbox(&self) -> [u32; 4] {
        match self {
            Job::Mesh(f) => f.bbox,
            Job::Gradient(f) => f.bbox,
            Job::Noise(f) => f.bbox,
            Job::Grain(f) => f.bbox,
            Job::Particles(f) => f.bbox,
        }
    }

    /// The CPU reference: the box's pixels, row-major sRGB RGBA8 with straight alpha.
    pub fn render(&self) -> Vec<u8> {
        match self {
            Job::Mesh(f) => f.render(),
            Job::Gradient(f) => render(f.bbox, |x, y| f.pixel(x, y)),
            Job::Noise(f) => render(f.bbox, |x, y| f.pixel(x, y)),
            Job::Grain(f) => render(f.bbox, |x, y| f.pixel(x, y)),
            Job::Particles(f) => render(f.bbox, |x, y| f.pixel(x, y)),
        }
    }

    /// The WGSL twin. Entry point `main`, workgroup 8 × 8, one invocation per pixel of
    /// the box: binding 0 is the uniform buffer, binding 1 a storage buffer of `u32`
    /// that receives each pixel's RGBA8 bytes, `stride` words per row.
    pub fn wgsl(&self) -> &'static str {
        match self {
            Job::Mesh(_) => mesh::WGSL,
            Job::Gradient(_) => gradient::WGSL,
            Job::Noise(_) => noise::WGSL,
            Job::Grain(_) => grain::WGSL,
            Job::Particles(_) => particles::WGSL,
        }
    }

    /// The WGSL uniform buffer, little-endian, for an output `stride` words per row.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        match self {
            Job::Mesh(f) => f.uniforms(stride),
            Job::Gradient(f) => f.uniforms(stride),
            Job::Noise(f) => f.uniforms(stride),
            Job::Grain(f) => f.uniforms(stride),
            Job::Particles(f) => f.uniforms(stride),
        }
    }
}

/// Whether `params` are ones a `kind` shader takes, each in its range (SPEC §3.8). The
/// engine asks when it resolves a node, so a bad param is the document's error.
pub fn check(kind: ShaderKind, params: &BTreeMap<String, f32>) -> Result<(), ShaderError> {
    match kind {
        ShaderKind::Mesh => mesh::Params::from_map(params).map(drop),
        ShaderKind::Gradient => gradient::Params::from_map(params).map(drop),
        ShaderKind::Noise => noise::Params::from_map(params).map(drop),
        ShaderKind::Grain => grain::Params::from_map(params).map(drop),
        ShaderKind::Particles => particles::Params::from_map(params).map(drop),
    }
}

/// The number a shader op carries for a param a document writes as a name: a gradient's
/// `shape`. Shader ops carry numbers only (SPEC §6).
pub fn named(kind: ShaderKind, param: &str, name: &str) -> Result<f32, ShaderError> {
    let bad = |problem: String| ShaderError::Param { name: param.to_string(), problem };
    match (kind, param) {
        (ShaderKind::Gradient, "shape") => gradient::Shape::named(name)
            .map(|s| s as u32 as f32)
            .ok_or_else(|| bad(format!("`{name}`: expected linear, radial, or conic"))),
        _ => Err(bad(format!("`{name}`: expected a number"))),
    }
}

/// Whether `param` of `kind` moves through the values between when a shader morphs
/// from one state to the next (SPEC §3.8). A name (a gradient's `shape`) and a count
/// (mesh `points`, noise `octaves`, particles `count`) have nothing between, and a rate
/// (`speed`, grain `fps`) on the global clock would race its phase through (b − a)·t
/// on the way (SPEC §3.9): two shaders that differ in one of these cross-fade.
pub fn interpolates(kind: ShaderKind, param: &str) -> bool {
    !matches!(
        (kind, param),
        (ShaderKind::Mesh, "points")
            | (ShaderKind::Gradient, "shape" | "speed")
            | (ShaderKind::Noise, "octaves" | "speed")
            | (ShaderKind::Grain, "fps")
            | (ShaderKind::Particles, "count" | "speed")
    )
}

/// The value `param` of `kind` takes when a document leaves it out, as an op carries it;
/// `None` for a param the kind does not take.
pub fn default(kind: ShaderKind, param: &str) -> Option<f32> {
    let values: Vec<(&str, f32)> = match kind {
        ShaderKind::Mesh => {
            let p = mesh::Params::default();
            vec![("points", p.points as f32), ("drift", p.drift), ("softness", p.softness), ("grain", p.grain)]
        }
        ShaderKind::Gradient => {
            let p = gradient::Params::default();
            vec![
                ("shape", p.shape as u32 as f32),
                ("angle", p.angle),
                ("x", p.x),
                ("y", p.y),
                ("radius", p.radius),
                ("speed", p.speed),
                ("grain", p.grain),
            ]
        }
        ShaderKind::Noise => {
            let p = noise::Params::default();
            vec![
                ("scale", p.scale),
                ("octaves", p.octaves as f32),
                ("speed", p.speed),
                ("contrast", p.contrast),
                ("grain", p.grain),
            ]
        }
        ShaderKind::Grain => {
            let p = grain::Params::default();
            vec![("amount", p.amount), ("fps", p.fps)]
        }
        ShaderKind::Particles => {
            let p = particles::Params::default();
            vec![("count", p.count as f32), ("size", p.size), ("speed", p.speed), ("softness", p.softness)]
        }
    };
    values.into_iter().find(|(name, _)| *name == param).map(|(_, v)| v)
}

/// The box's pixels, row-major, from `pixel(x, y)`.
fn render(bbox: [u32; 4], pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
    let [_, _, w, h] = bbox;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for gy in 0..h {
        for gx in 0..w {
            out.extend_from_slice(&pixel(gx, gy));
        }
    }
    out
}

/// A param in `[lo, hi]`, or the error that says so.
fn within(name: &str, v: f32, lo: f32, hi: f32) -> Result<f32, ShaderError> {
    match (lo..=hi).contains(&v) {
        true => Ok(v),
        false => Err(ShaderError::Param { name: name.to_string(), problem: format!("{v}: expected {lo} to {hi}") }),
    }
}

/// A whole-number param in `[lo, hi]`, or the error that says so.
fn whole(name: &str, v: f32, lo: u32, hi: u32) -> Result<u32, ShaderError> {
    match v.fract() == 0.0 && (lo as f32..=hi as f32).contains(&v) {
        true => Ok(v as u32),
        false => {
            let problem = format!("{v}: expected a whole number from {lo} to {hi}");
            Err(ShaderError::Param { name: name.to_string(), problem })
        }
    }
}

/// The most colors a ramp runs through: the WGSL uniform arrays' length.
pub const MAX_STOPS: usize = 16;

/// A palette as a ramp: each color in Oklab, with its alpha, for [`sample`].
fn ramp(kind: &'static str, palette: &[Color]) -> Result<Vec<[f32; 4]>, ShaderError> {
    if palette.is_empty() {
        return Err(ShaderError::EmptyPalette);
    }
    if palette.len() > MAX_STOPS {
        return Err(ShaderError::Palette { kind, max: MAX_STOPS, len: palette.len() });
    }
    Ok(palette
        .iter()
        .map(|&Color([r, g, b, a])| {
            let [l, ca, cb] = linear_to_oklab([linear(r), linear(g), linear(b)]);
            [l as f32, ca as f32, cb as f32, f32::from(a) / 255.0]
        })
        .collect())
}

/// The ramp at `u`, 0 to 1: its colors evenly spaced, blended in Oklab. A `cyclic` ramp
/// runs on from its last color back to its first.
fn sample(stops: &[[f32; 4]], u: f32, cyclic: bool) -> [f32; 4] {
    let n = stops.len() as u32;
    if n == 1 {
        return stops[0];
    }
    let spans = if cyclic { n } else { n - 1 };
    let s = u * spans as f32;
    let k = (s.floor().max(0.0) as u32).min(spans - 1);
    let f = s - k as f32;
    let (a, b) = (stops[k as usize], stops[((k + 1) % n) as usize]);
    [a[0] + f * (b[0] - a[0]), a[1] + f * (b[1] - a[1]), a[2] + f * (b[2] - a[2]), a[3] + f * (b[3] - a[3])]
}

/// An Oklab color and its alpha, 0 to 1, as sRGB RGBA8 with straight alpha: the same
/// conversion `mesh.wgsl` makes, and every ramp kind's WGSL after it.
fn oklab_rgba8(l: f32, ca: f32, cb: f32, alpha: f32) -> [u8; 4] {
    let lm = l + 0.396_337_78 * ca + 0.215_803_76 * cb;
    let mm = l - 0.105_561_346 * ca - 0.063_854_17 * cb;
    let sm = l - 0.089_484_18 * ca - 1.291_485_5 * cb;
    let lc = lm * lm * lm;
    let mc = mm * mm * mm;
    let sc = sm * sm * sm;
    let r = 4.076_741_7 * lc - 3.307_711_6 * mc + 0.230_969_94 * sc;
    let g = -1.268_438 * lc + 2.609_757_4 * mc - 0.341_319_38 * sc;
    let b = -0.004_196_086_4 * lc - 0.703_418_6 * mc + 1.707_614_7 * sc;
    let a8 = alpha.clamp(0.0, 1.0) * 255.0 + 0.5;
    [encode(r) as u8, encode(g) as u8, encode(b) as u8, a8 as u8]
}

/// Seeded noise for pixel `(gx, gy)` of a box, uniform in `[-0.5, 0.5)`: the grain
/// every kind adds the same way.
fn grain_noise(key: u32, gx: u32, gy: u32) -> f32 {
    (lowbias32(key ^ lowbias32(gx ^ lowbias32(gy))) >> 8) as f32 * (1.0 / 16_777_216.0) - 0.5
}

/// A shader's seed as the key its grain and hashes start from.
fn seed_key(seed: u64) -> u32 {
    lowbias32(seed as u32 ^ lowbias32((seed >> 32) as u32))
}

/// A device pixel's center to canvas units relative to `rect`'s corner, scaled by `k`,
/// as `[a, b, c, d, e, f]` (`x = a·px + c·py + e`), from `device`'s inverse.
fn local_map(inverse: [f64; 6], rect: [f32; 4], k: f64) -> [f64; 6] {
    let [a, b, c, d, e, f] = inverse;
    let (x0, y0) = (f64::from(rect[0]), f64::from(rect[1]));
    [a * k, b * k, c * k, d * k, (e - x0) * k, (f - y0) * k]
}

/// The device pixels a shader covers, `[x, y, width, height]`, and the inverse of the
/// device map, which takes those pixels back to canvas units.
type Framed = ([u32; 4], [f64; 6]);

/// Checks a rect and finds the device box it covers and the device map's inverse, or
/// `Ok(None)` when it covers no pixel.
fn frame_box(rect: [f32; 4], device: [f64; 6], size: [u32; 2]) -> Result<Option<Framed>, ShaderError> {
    let [rx, ry, w, h] = rect;
    if !(rx.is_finite() && ry.is_finite() && w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0) {
        return Err(ShaderError::Rect(rect));
    }
    Ok(device_box(rect, device, size).zip(invert(device)))
}

/// The inverse of the affine map `m` (`[a, b, c, d, e, f]`), if it has one.
fn invert([a, b, c, d, e, f]: [f64; 6]) -> Option<[f64; 6]> {
    let det = a * d - b * c;
    if det == 0.0 || !det.is_finite() {
        return None;
    }
    Some([d / det, -b / det, -c / det, a / det, (c * f - d * e) / det, (b * e - a * f) / det])
}

/// The device pixels `[x, y, width, height]` that `rect` covers through `device`,
/// within a raster `size` pixels across; `None` if none.
fn device_box(rect: [f32; 4], device: [f64; 6], size: [u32; 2]) -> Option<[u32; 4]> {
    let [x, y, w, h] = rect.map(f64::from);
    let [a, b, c, d, e, f] = device;
    let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)].map(|(u, v)| (a * u + c * v + e, b * u + d * v + f));
    let (mut x0, mut y0, mut x1, mut y1) = (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    for (u, v) in corners {
        (x0, y0, x1, y1) = (x0.min(u), y0.min(v), x1.max(u), y1.max(v));
    }
    let clamp = |v: f64, hi: u32| v.clamp(0.0, f64::from(hi)) as u32;
    let (x0, y0) = (clamp(x0.floor(), size[0]), clamp(y0.floor(), size[1]));
    let (x1, y1) = (clamp(x1.ceil(), size[0]), clamp(y1.ceil(), size[1]));
    (x1 > x0 && y1 > y0).then_some([x0, y0, x1 - x0, y1 - y0])
}

/// sRGB-encoded 0–1 to linear light.
fn decode(v: f64) -> f64 {
    if v <= 0.04045 { v / 12.92 } else { libm::pow((v + 0.055) / 1.055, 2.4) }
}

/// An sRGB byte to linear light.
fn linear(c: u8) -> f64 {
    decode(f64::from(c) / 255.0)
}

/// Linear sRGB to Oklab `[L, a, b]` (Ottosson), in `f64` through `libm`.
fn linear_to_oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = libm::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = libm::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = libm::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// The 255 linear-light values halfway, in sRGB, between adjacent bytes: byte `k` is
/// the number of them at or below a linear value, which is that value encoded and
/// rounded to the nearest byte, with no `pow` per pixel. The 256th entry pads the
/// table to 64 `vec4`s in the WGSL uniform buffer.
fn thresholds() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut t = [2.0; 256];
        for (k, v) in t.iter_mut().enumerate().take(255) {
            *v = decode((k as f64 + 0.5) / 255.0) as f32;
        }
        t
    })
}

/// Linear light to an sRGB byte: eight comparisons against [`thresholds`], the same
/// eight the WGSL makes.
fn encode(v: f32) -> u32 {
    let t = thresholds();
    let mut k = 0;
    let mut step = 128;
    while step > 0 {
        if t[k + step - 1] <= v {
            k += step;
        }
        step /= 2;
    }
    k as u32
}

/// Chris Wellons' `lowbias32` integer hash; WGSL's `u32` arithmetic wraps the same way.
fn lowbias32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^ (x >> 16)
}

/// SplitMix64: the document's `seed` to a stream of numbers, the same everywhere.
struct SplitMix64(u64);

impl SplitMix64 {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1_u64 << 53) as f64
    }
}

/// Little-endian words, the WGSL uniform layout's unit.
struct Words(Vec<u8>);

impl Words {
    fn f32s(&mut self, v: impl IntoIterator<Item = f32>) {
        v.into_iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
    }

    fn u32s(&mut self, v: impl IntoIterator<Item = u32>) {
        v.into_iter().for_each(|x| self.0.extend_from_slice(&x.to_le_bytes()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoding_by_thresholds_rounds_like_the_transfer_function() {
        for byte in 0..=255_u8 {
            // Every byte's own linear value encodes back to it.
            assert_eq!(encode(linear(byte) as f32), u32::from(byte), "byte {byte}");
        }
        assert_eq!((encode(-0.5), encode(0.0), encode(1.0), encode(7.0)), (0, 0, 255, 255));
        // Halfway in sRGB, not in linear light: 0.5 linear is 188 (187.5 rounds up).
        assert_eq!(encode(0.5), 188);
    }

    #[test]
    fn oklab_matches_the_published_reference() {
        // Ottosson's table: white is L = 1, a = b = 0; pure red is (0.628, 0.225, 0.126).
        let white = linear_to_oklab([1.0, 1.0, 1.0]);
        assert!((white[0] - 1.0).abs() < 1e-6 && white[1].abs() < 1e-6 && white[2].abs() < 1e-6, "{white:?}");
        let red = linear_to_oklab([1.0, 0.0, 0.0]);
        assert!((red[0] - 0.627_955).abs() < 1e-5 && (red[1] - 0.224_863).abs() < 1e-5, "{red:?}");
    }

    #[test]
    fn device_boxes_cover_the_rect_and_stop_at_the_raster() {
        let id = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(device_box([0.0, 0.0, 1920.0, 1080.0], id, [1920, 1080]), Some([0, 0, 1920, 1080]));
        assert_eq!(device_box([10.5, 0.0, 10.0, 4.0], id, [100, 100]), Some([10, 0, 11, 4]));
        assert_eq!(
            device_box([-50.0, -50.0, 100.0, 100.0], [2.0, 0.0, 0.0, 2.0, 0.0, 0.0], [64, 64]),
            Some([0, 0, 64, 64])
        );
        assert_eq!(device_box([200.0, 0.0, 10.0, 10.0], id, [100, 100]), None);
        assert_eq!(invert([2.0, 0.0, 0.0, 4.0, 1.0, 1.0]), Some([0.5, 0.0, 0.0, 0.25, -0.5, -0.25]));
        assert_eq!(invert([1.0, 2.0, 2.0, 4.0, 0.0, 0.0]), None);
    }

    #[test]
    fn defaults_are_what_each_kind_takes_when_a_param_is_left_out() {
        let kinds = [
            (ShaderKind::Mesh, &["points", "drift", "softness", "grain"][..]),
            (ShaderKind::Gradient, &["shape", "angle", "x", "y", "radius", "speed", "grain"]),
            (ShaderKind::Noise, &["scale", "octaves", "speed", "contrast", "grain"]),
            (ShaderKind::Grain, &["amount", "fps"]),
            (ShaderKind::Particles, &["count", "size", "speed", "softness"]),
        ];
        for (kind, params) in kinds {
            let all: BTreeMap<String, f32> = params
                .iter()
                .map(|p| (p.to_string(), default(kind, p).unwrap_or_else(|| panic!("{kind:?} {p}"))))
                .collect();
            // Written out, the defaults are a valid op, and draw what leaving them out draws.
            check(kind, &all).unwrap_or_else(|e| panic!("{kind:?}: {e}"));
            let rect = [0.0, 0.0, 8.0, 8.0];
            let op = |params: BTreeMap<String, f32>| Op::Shader {
                kind,
                seed: 3,
                t: 1.5,
                rect,
                palette: vec![Color([230, 50, 25, 255]), Color([25, 75, 200, 255])],
                params,
            };
            let draw = |op: &Op| Job::new(op, [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], [8, 8]).unwrap().unwrap().render();
            assert_eq!(draw(&op(all)), draw(&op(BTreeMap::new())), "{kind:?}");
            assert_eq!(default(kind, "nonsense"), None);
        }
        assert!(interpolates(ShaderKind::Gradient, "angle") && !interpolates(ShaderKind::Gradient, "speed"));
    }

    #[test]
    fn seeds_give_the_same_numbers_everywhere() {
        // Vigna's reference implementation's first output for seed 1234567.
        assert_eq!(SplitMix64(1_234_567).next(), 6_457_827_717_110_365_317);
        // Pinned: goldens depend on this stream.
        let mut r = SplitMix64(7);
        assert_eq!(r.next(), 0x63cb_e1e4_5932_0dd7);
        assert!((0..1000).map(|_| r.unit()).all(|u| (0.0..1.0).contains(&u)));
        assert_eq!(lowbias32(0), 0);
        assert_ne!(lowbias32(1), lowbias32(2));
    }
}

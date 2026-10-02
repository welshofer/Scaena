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
//! Phase 0 (PLAN 0.11) implements `mesh`; the other kinds are PLAN 1.10.

pub mod mesh;

use crate::displaylist::{Op, ShaderKind};
use std::sync::OnceLock;
use thiserror::Error;

#[derive(Debug, Error, PartialEq)]
pub enum ShaderError {
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("shader param `{name}`: {problem}")]
    Param { name: String, problem: String },
    #[error("a shader needs at least one palette color")]
    EmptyPalette,
    #[error("shader rect {0:?}: expected a finite width and height above 0")]
    Rect([f32; 4]),
}

/// One shader op over a box of device pixels.
#[derive(Debug, Clone, PartialEq)]
pub enum Job {
    Mesh(mesh::Frame),
}

impl Job {
    /// The job for shader op `op` drawn through `device` (canvas units to device pixels,
    /// `[a, b, c, d, e, f]` as in the display list) into a raster `size` pixels across.
    /// `None` when the op covers no pixel of the raster.
    pub fn new(op: &Op, device: [f64; 6], size: [u32; 2]) -> Result<Option<Job>, ShaderError> {
        let Op::Shader { kind, seed, t, rect, palette, params } = op else {
            return Ok(None);
        };
        match kind {
            ShaderKind::Mesh => {
                let params = mesh::Params::from_map(params)?;
                Ok(mesh::Frame::new(*seed, *t, palette, &params, *rect, device, size)?.map(Job::Mesh))
            }
            ShaderKind::Gradient | ShaderKind::Noise | ShaderKind::Grain | ShaderKind::Particles => {
                Err(ShaderError::NotImplemented("shader kinds other than mesh — PLAN 1.10"))
            }
        }
    }

    /// `[x, y, width, height]` in device pixels: where [`Job::render`]'s pixels go.
    pub fn bbox(&self) -> [u32; 4] {
        match self {
            Job::Mesh(f) => f.bbox,
        }
    }

    /// The CPU reference: the box's pixels, row-major sRGB RGBA8 with straight alpha.
    pub fn render(&self) -> Vec<u8> {
        match self {
            Job::Mesh(f) => f.render(),
        }
    }

    /// The WGSL twin. Entry point `main`, workgroup 8 × 8, one invocation per pixel of
    /// the box: binding 0 is the uniform buffer, binding 1 a storage buffer of `u32`
    /// that receives each pixel's RGBA8 bytes, `stride` words per row.
    pub fn wgsl(&self) -> &'static str {
        match self {
            Job::Mesh(_) => mesh::WGSL,
        }
    }

    /// The WGSL uniform buffer, little-endian, for an output `stride` words per row.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        match self {
            Job::Mesh(f) => f.uniforms(stride),
        }
    }
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

//! `grain`: film grain over what lies under it (SPEC §3.8).
//!
//! Each device pixel takes seeded noise, uniform between −½ and ½: below zero it shows
//! the palette's first color, above it the last, at an opacity up to `amount` as far as
//! the noise is from zero. The grain changes `fps` times a second (0 holds it still), so
//! it shimmers as film does rather than crawling. It is per device pixel, as SPEC §6
//! says per-pixel noise is.
//!
//! Per pixel it is integer hashing and `+ − × ÷`, here and in `grain.wgsl` alike.

use super::{ShaderError, Words, frame_box, grain_noise, lowbias32, seed_key, within};
use crate::displaylist::Color;
use std::collections::BTreeMap;

/// The WGSL twin of [`Frame::pixel`].
pub const WGSL: &str = include_str!("grain.wgsl");

/// Typed grain parameters (SPEC §3.8), with their defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Params {
    /// The strongest grain's opacity, 0–1.
    pub amount: f32,
    /// How many times a second the grain changes, 0–60; 0 holds it still.
    pub fps: f32,
}

impl Default for Params {
    fn default() -> Self {
        Params { amount: 0.12, fps: 12.0 }
    }
}

impl Params {
    /// A shader op's `params`; absent keys take their defaults.
    pub fn from_map(map: &BTreeMap<String, f32>) -> Result<Params, ShaderError> {
        let mut p = Params::default();
        for (name, &v) in map {
            match name.as_str() {
                "amount" => p.amount = within(name, v, 0.0, 1.0)?,
                "fps" => p.fps = within(name, v, 0.0, 60.0)?,
                _ => {
                    let problem = "unknown; grain takes amount and fps".to_string();
                    return Err(ShaderError::Param { name: name.clone(), problem });
                }
            }
        }
        Ok(p)
    }
}

/// One frame of grain over a box of device pixels: everything [`Frame::pixel`] reads,
/// which [`Frame::uniforms`] lays out for the WGSL.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// `[x, y, width, height]` in device pixels.
    pub bbox: [u32; 4],
    /// Keys this frame's grain: the seed's, and which change of the grain it is.
    pub key: u32,
    /// The palette's first and last colors.
    pub dark: Color,
    pub light: Color,
    pub amount: f32,
}

impl Frame {
    /// The grain at `t` seconds over `rect` (canvas units), drawn through `device` into a
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
        let (Some(&dark), Some(&light)) = (palette.first(), palette.last()) else {
            return Err(ShaderError::EmptyPalette);
        };
        let Some((bbox, _)) = frame_box(rect, device, size)? else { return Ok(None) };
        let change = (f64::from(t) * f64::from(params.fps)).floor().max(0.0) as u32;
        Ok(Some(Frame { bbox, key: lowbias32(seed_key(seed) ^ change), dark, light, amount: params.amount }))
    }

    /// The pixel `(gx, gy)` of the box: sRGB RGBA8, straight alpha. `grain.wgsl` is this
    /// function, line for line.
    pub fn pixel(&self, gx: u32, gy: u32) -> [u8; 4] {
        let n = grain_noise(self.key, gx, gy);
        let (Color([r, g, b, a]), far) = if n < 0.0 { (self.dark, -n) } else { (self.light, n) };
        let alpha = far * 2.0 * self.amount * (f32::from(a) / 255.0);
        [r, g, b, (alpha * 255.0 + 0.5) as u8]
    }

    /// The uniform buffer `grain.wgsl` declares as `Grain`, for output rows `stride`
    /// words apart.
    pub fn uniforms(&self, stride: u32) -> Vec<u8> {
        let pack = |Color([r, g, b, _]): Color| u32::from(r) | u32::from(g) << 8 | u32::from(b) << 16;
        let alpha = |Color([_, _, _, a]): Color| f32::from(a) / 255.0;
        let mut out = Words(Vec::with_capacity(48));
        out.u32s(self.bbox);
        out.u32s([self.key, stride, pack(self.dark), pack(self.light)]);
        out.f32s([self.amount, alpha(self.dark), alpha(self.light), 0.0]);
        out.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PALETTE: [Color; 2] = [Color([0, 0, 0, 255]), Color([255, 255, 255, 255])];
    const ID: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn frame(t: f32, params: Params) -> Frame {
        Frame::new(5, t, &PALETTE, &params, [0.0, 0.0, 64.0, 64.0], ID, [64, 64]).unwrap().unwrap()
    }

    #[test]
    fn grain_is_dark_and_light_specks_at_most_amount_opaque() {
        let f = frame(0.0, Params { amount: 0.5, fps: 0.0 });
        let pixels: Vec<[u8; 4]> =
            (0..64).flat_map(|y| (0..64).map(move |x| (x, y))).map(|(x, y)| f.pixel(x, y)).collect();
        assert!(pixels.iter().all(|p| (p[..3] == [0, 0, 0] || p[..3] == [255, 255, 255]) && p[3] <= 128));
        let dark = pixels.iter().filter(|p| p[0] == 0).count();
        assert!((1600..2500).contains(&dark), "about half dark: {dark} of 4096");
    }

    #[test]
    fn grain_changes_at_its_frame_rate() {
        let p = Params::default();
        let render = |f: Frame| super::super::render(f.bbox, |x, y| f.pixel(x, y));
        assert_eq!(render(frame(0.0, p)), render(frame(1.0 / 24.0, p)), "within one of 12 changes a second");
        assert_ne!(render(frame(0.0, p)), render(frame(1.0 / 6.0, p)), "two changes on");
        let still = Params { fps: 0.0, ..p };
        assert_eq!(render(frame(0.0, still)), render(frame(9.0, still)));
        assert_eq!(frame(0.0, p).uniforms(64).len(), 48, "the WGSL `Grain` struct's size");
    }
}

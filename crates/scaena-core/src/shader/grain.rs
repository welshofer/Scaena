//! `grain`: film grain over what lies under it (SPEC §3.8).
//!
//! Each device pixel takes seeded noise, uniform between −½ and ½: below zero it shows
//! the palette's first color, above it the last, at an opacity up to `amount` as far as
//! the noise is from zero. The grain changes `fps` times a second (0 holds it still), so
//! it shimmers as film does rather than crawling. It is per device pixel, as SPEC §6
//! says per-pixel noise is.
//!
//! Per pixel it is integer hashing and `+ − × ÷`, here and in `grain.wgsl` alike.

use super::{BLOCK, ShaderError, Words, frame_box, grain_noise, lowbias32, seed_key, within};
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

    /// The whole box, row-major: [`Frame::pixel`]'s bytes, worked out a [`BLOCK`] of a
    /// row at a time, one step of `pixel` across the block before the next. Each pixel
    /// does `pixel`'s arithmetic in its order, so the bytes are the same
    /// (`render_is_pixel_for_pixel`), and each step runs as SIMD.
    pub fn render(&self) -> Vec<u8> {
        let [_, _, width, height] = self.bbox;
        let mut out = vec![0; width as usize * height as usize * 4];
        self.render_rows(0, &mut out);
        out
    }

    /// The box's rows from `first`, as many as `out` holds whole: those rows of
    /// [`Frame::render`], byte for byte, since no row reads another.
    pub fn render_rows(&self, first: u32, out: &mut [u8]) {
        let width = self.bbox[2];
        if width == 0 {
            return;
        }
        let (Color(dark), Color(light)) = (self.dark, self.light);
        let opacity = [f32::from(dark[3]) / 255.0, f32::from(light[3]) / 255.0];
        let (mut noise, mut a8) = ([0.0_f32; BLOCK], [0_u8; BLOCK]);
        for (gy, row) in (first..).zip(out.chunks_exact_mut(width as usize * 4)) {
            let hy = lowbias32(gy);
            for (gx, pixels) in (0..).step_by(BLOCK).zip(row.chunks_mut(BLOCK * 4)) {
                let n = pixels.len() / 4;
                let (noise, a8) = (&mut noise[..n], &mut a8[..n]);
                for (i, noise) in noise.iter_mut().enumerate() {
                    let hash = lowbias32(self.key ^ lowbias32((gx + i as u32) ^ hy));
                    *noise = (hash >> 8) as f32 * (1.0 / 16_777_216.0) - 0.5;
                }
                for i in 0..n {
                    let (far, opacity) = if noise[i] < 0.0 { (-noise[i], opacity[0]) } else { (noise[i], opacity[1]) };
                    let alpha = far * 2.0 * self.amount * opacity;
                    // `pixel` casts this to a byte. It is at least 0.5 and at most 255.5, which
                    // a cast through `i32` takes where `as u8` does, and as SIMD.
                    a8[i] = (alpha * 255.0 + 0.5) as i32 as u8;
                }
                for (i, px) in pixels.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                    let [r, g, b, _] = if noise[i] < 0.0 { dark } else { light };
                    *px = [r, g, b, a8[i]];
                }
            }
        }
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

    /// `render` is `pixel` at every pixel, bit for bit, in boxes whose rows are not whole
    /// blocks, with colors opaque and not.
    #[test]
    fn render_is_pixel_for_pixel() {
        let clear = [Color([20, 10, 40, 200]), Color([255, 240, 220, 90])];
        // A palette, params, a rect, the device transform, and the raster's size.
        type Case<'a> = (&'a [Color], Params, [f32; 4], [f64; 6], [u32; 2]);
        let cases: [Case; 3] = [
            (&PALETTE, Params::default(), [0.0, 0.0, 64.0, 64.0], ID, [64, 64]),
            (
                &clear,
                Params { amount: 1.0, fps: 24.0 },
                [5.5, 2.0, 300.0, 40.0],
                [1.25, 0.0, 0.0, 1.25, 0.0, 0.0],
                [500, 80],
            ),
            (&PALETTE[1..], Params { amount: 0.3, fps: 0.0 }, [0.0, 0.0, 129.0, 2.0], ID, [129, 2]),
        ];
        for (i, (palette, params, rect, device, size)) in cases.into_iter().enumerate() {
            let f = Frame::new(i as u64 * 104_729, 1.3, palette, &params, rect, device, size).unwrap().unwrap();
            let [_, _, w, h] = f.bbox;
            let want: Vec<u8> =
                (0..h).flat_map(|y| (0..w).map(move |x| (x, y))).flat_map(|(x, y)| f.pixel(x, y)).collect();
            assert!(f.render() == want, "case {i}, box {:?}", f.bbox);
        }
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

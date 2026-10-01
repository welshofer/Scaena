//! Raster comparison (SPEC §13.5): ΔE in Oklab per pixel, anti-aliased edges
//! excluded by a 1-px dilation mask. Golden rasters (PLAN 0.6) and the parity
//! harness (PLAN 0.9) both use it.
//!
//! - ΔE is the Euclidean distance between Oklab colors, scaled by 100 (about 2 is a
//!   just-noticeable difference). Pixels are composited over opaque white first, so
//!   alpha differences count as color differences.
//! - An edge pixel differs from a 4-neighbor by more than [`EDGE_STEP`] in some
//!   channel, in either raster. The mask is the edges grown by one pixel. Masked
//!   pixels are left out of the ΔE rule but still counted in `differing` and
//!   `max_channel`, so a change inside the mask stays visible.

use crate::{PaintError, Raster};

/// SPEC §13.5: at most this ΔE (Oklab × 100)…
pub const MAX_DELTA_E: f32 = 1.0;
/// …on all but this fraction of the compared pixels.
pub const MAX_OVER_FRACTION: f64 = 0.001;
/// Channel step (of 255) between 4-neighbors that marks an edge.
pub const EDGE_STEP: u8 = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Diff {
    pub pixels: usize,
    /// Pixels outside the edge mask: the ones the ΔE rule judges.
    pub compared: usize,
    /// Compared pixels with ΔE above [`MAX_DELTA_E`].
    pub over: usize,
    /// Largest ΔE among compared pixels.
    pub max_delta_e: f32,
    /// Largest per-channel difference anywhere, masked or not.
    pub max_channel: u8,
    /// Pixels that differ at all, anywhere.
    pub differing: usize,
}

impl Diff {
    /// SPEC §13.5: ΔE ≤ 1.0 on at least 99.9% of the compared pixels.
    pub fn passes(&self) -> bool {
        self.over as f64 <= MAX_OVER_FRACTION * self.compared as f64
    }
}

impl std::fmt::Display for Diff {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} of {} compared px over ΔE {MAX_DELTA_E} (max ΔE {:.2}); {} px differ anywhere (max channel step {})",
            self.over, self.compared, self.max_delta_e, self.differing, self.max_channel
        )
    }
}

pub fn compare(a: &Raster, b: &Raster) -> Result<Diff, PaintError> {
    let (w, h) = (a.width as usize, a.height as usize);
    let n = w * h;
    if (a.width, a.height) != (b.width, b.height) || a.rgba.len() != n * 4 || b.rgba.len() != n * 4 {
        return Err(PaintError::Mismatch { a: (a.width, a.height), b: (b.width, b.height) });
    }
    let (pa, pb) = (packed(a), packed(b));
    // Edges in either raster. The neighbor relation is symmetric, so each horizontal and
    // vertical pair is visited once and marks both of its pixels.
    let mut edges = vec![false; n];
    for px in [&pa, &pb] {
        for i in 0..n {
            if (i + 1) % w != 0 && step(px[i], px[i + 1]) > EDGE_STEP {
                edges[i] = true;
                edges[i + 1] = true;
            }
            if i + w < n && step(px[i], px[i + w]) > EDGE_STEP {
                edges[i] = true;
                edges[i + w] = true;
            }
        }
    }
    // The mask is the edges grown by one pixel in all eight directions.
    let mut mask = edges.clone();
    for i in (0..n).filter(|&i| edges[i]) {
        let (x, y) = (i % w, i / w);
        for ny in y.saturating_sub(1)..(y + 2).min(h) {
            for nx in x.saturating_sub(1)..(x + 2).min(w) {
                mask[ny * w + nx] = true;
            }
        }
    }
    let mut diff = Diff { pixels: n, compared: 0, over: 0, max_delta_e: 0.0, max_channel: 0, differing: 0 };
    for i in 0..n {
        let channel = step(pa[i], pb[i]);
        diff.max_channel = diff.max_channel.max(channel);
        diff.differing += usize::from(channel > 0);
        if mask[i] {
            continue;
        }
        diff.compared += 1;
        if channel > 0 {
            let delta = delta_e(pa[i].to_le_bytes(), pb[i].to_le_bytes());
            diff.max_delta_e = diff.max_delta_e.max(delta);
            diff.over += usize::from(delta > MAX_DELTA_E);
        }
    }
    Ok(diff)
}

/// One `u32` per pixel, so the common case (equal pixels) is a single comparison.
fn packed(r: &Raster) -> Vec<u32> {
    r.rgba.as_chunks::<4>().0.iter().map(|&p| u32::from_le_bytes(p)).collect()
}

/// Largest per-channel difference between two packed pixels.
fn step(p: u32, q: u32) -> u8 {
    if p == q {
        return 0;
    }
    let (p, q) = (p.to_le_bytes(), q.to_le_bytes());
    p[0].abs_diff(q[0]).max(p[1].abs_diff(q[1])).max(p[2].abs_diff(q[2])).max(p[3].abs_diff(q[3]))
}

/// ΔE (Oklab × 100) between two straight-alpha sRGB pixels composited over white.
pub fn delta_e(p: [u8; 4], q: [u8; 4]) -> f32 {
    let (lp, lq) = (oklab(p), oklab(q));
    100.0 * ((lp[0] - lq[0]).powi(2) + (lp[1] - lq[1]).powi(2) + (lp[2] - lq[2]).powi(2)).sqrt()
}

/// Oklab (Björn Ottosson's matrices) of a straight-alpha sRGB pixel over white.
fn oklab([r, g, b, a]: [u8; 4]) -> [f32; 3] {
    let alpha = f32::from(a) / 255.0;
    let linear = |c: u8| {
        let c = (f32::from(c) / 255.0) * alpha + (1.0 - alpha);
        if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    };
    let (r, g, b) = (linear(r), linear(g), linear(b));
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [
        0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s,
        1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s,
        0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: u32, h: u32, px: [u8; 4]) -> Raster {
        Raster { width: w, height: h, rgba: px.repeat((w * h) as usize) }
    }

    fn set(r: &mut Raster, x: u32, y: u32, px: [u8; 4]) {
        let i = ((y * r.width + x) * 4) as usize;
        r.rgba[i..i + 4].copy_from_slice(&px);
    }

    #[test]
    fn identical_rasters_pass_with_nothing_over() {
        let a = flat(16, 16, [250, 248, 242, 255]);
        let d = compare(&a, &a).unwrap();
        assert_eq!((d.compared, d.over, d.differing, d.max_channel), (256, 0, 0, 0));
        assert!(d.passes());
    }

    #[test]
    fn a_wrong_flat_region_fails_and_a_single_stray_pixel_is_masked_as_an_edge() {
        let a = flat(40, 40, [255, 255, 255, 255]);
        // A visibly different 20×20 block: its interior is compared, and fails.
        let mut b = a.clone();
        for y in 10..30 {
            for x in 10..30 {
                set(&mut b, x, y, [235, 235, 235, 255]);
            }
        }
        let d = compare(&a, &b).unwrap();
        assert!(!d.passes(), "{d}");
        assert!(d.max_delta_e > 5.0, "{d}");
        // One stray pixel reads as an edge in `b`, so the mask hides it from ΔE; the
        // anywhere counters still see it.
        let mut c = a.clone();
        set(&mut c, 20, 20, [0, 0, 0, 255]);
        let d = compare(&a, &c).unwrap();
        assert_eq!((d.over, d.differing, d.max_channel), (0, 1, 255), "{d}");
    }

    #[test]
    fn delta_e_is_oklab_scaled_by_100() {
        assert_eq!(delta_e([0, 0, 0, 255], [0, 0, 0, 255]), 0.0);
        // Black to white is the whole lightness range: ΔE ≈ 100.
        assert!((delta_e([0, 0, 0, 255], [255, 255, 255, 255]) - 100.0).abs() < 0.1);
        // One step of one 8-bit channel is below the 1.0 threshold.
        assert!(delta_e([128, 128, 128, 255], [129, 128, 128, 255]) < 1.0);
    }

    #[test]
    fn mismatched_sizes_are_an_error() {
        assert!(compare(&flat(2, 2, [0; 4]), &flat(2, 3, [0; 4])).is_err());
    }
}

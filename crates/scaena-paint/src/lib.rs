//! # scaena-paint
//!
//! Painters consume a [`scaena_core::displaylist::DisplayList`] and produce pixels.
//! They never shape text or lay anything out (SPEC §6).
//!
//! - `cpu` (default feature): `vello_cpu` — headless, deterministic; CI, agents, export.
//! - `gpu`: `vello` on `wgpu` — WebGPU in the browser, Metal on the Mac.
//!
//! Phase 0 tasks 0.6–0.7 implement both for the spike fixture.

use scaena_core::displaylist::DisplayList;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaintError {
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("unsupported display list version {0}")]
    Version(u32),
}

/// RGBA8, premultiplied, row-major.
#[derive(Debug, Clone, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

pub trait Painter {
    fn name(&self) -> &'static str;
    fn paint(&mut self, dl: &DisplayList, scale: f64) -> Result<Raster, PaintError>;
}

#[cfg(feature = "cpu")]
pub mod cpu {
    use super::*;

    /// `vello_cpu`-backed painter. Phase 0 task 0.6.
    #[derive(Default)]
    pub struct CpuPainter;

    impl Painter for CpuPainter {
        fn name(&self) -> &'static str {
            "cpu"
        }
        fn paint(&mut self, dl: &DisplayList, _scale: f64) -> Result<Raster, PaintError> {
            if dl.dl != scaena_core::displaylist::DL_VERSION {
                return Err(PaintError::Version(dl.dl));
            }
            Err(PaintError::NotImplemented("cpu painter — PLAN 0.6"))
        }
    }

    #[cfg(test)]
    mod tests {
        use vello_cpu::color::{AlphaColor, Srgb};
        use vello_cpu::kurbo::Rect;
        use vello_cpu::{Level, Pixmap, RenderContext, RenderSettings, Resources};

        /// PLAN 0.1 smoke test: `vello_cpu` links and rasterizes. A pixel-aligned opaque
        /// fill lands exactly, with nothing outside it, both at the architecture's
        /// baseline SIMD level and at whatever level the host detects.
        #[test]
        fn vello_cpu_fills_pixel_aligned_rect_exactly() {
            const ACCENT: [u8; 4] = [0xFF, 0x6A, 0x3D, 0xFF];
            for level in [Level::baseline(), Level::new()] {
                let mut ctx = RenderContext::new_with(4, 4, RenderSettings { level, ..Default::default() });
                ctx.set_paint(AlphaColor::<Srgb>::from_rgba8(ACCENT[0], ACCENT[1], ACCENT[2], ACCENT[3]));
                ctx.fill_rect(&Rect::new(0.0, 0.0, 2.0, 2.0));
                ctx.flush();
                let mut pixmap = Pixmap::new(4, 4);
                ctx.render(&mut pixmap, &mut Resources::new());
                let px = |x: usize, y: usize| -> [u8; 4] {
                    let i = (y * 4 + x) * 4;
                    pixmap.data_as_u8_slice()[i..i + 4].try_into().unwrap()
                };
                for (x, y) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    assert_eq!(px(x, y), ACCENT, "inside ({x},{y}) at {level:?}");
                }
                for (x, y) in [(2, 0), (0, 2), (2, 2), (3, 3)] {
                    assert_eq!(px(x, y), [0; 4], "outside ({x},{y}) at {level:?}");
                }
            }
        }
    }
}

#[cfg(feature = "gpu")]
pub mod gpu {
    //! `vello` on `wgpu`. Phase 0 task 0.7. Surface creation is the client's job
    //! (OffscreenCanvas/WebGPU in the browser, CAMetalLayer on the Mac).
}

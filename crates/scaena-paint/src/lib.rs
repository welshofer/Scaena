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
}

#[cfg(feature = "gpu")]
pub mod gpu {
    //! `vello` on `wgpu`. Phase 0 task 0.7. Surface creation is the client's job
    //! (OffscreenCanvas/WebGPU in the browser, CAMetalLayer on the Mac).
}

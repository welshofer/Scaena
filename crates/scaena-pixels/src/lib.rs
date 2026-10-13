//! The work done pixel by pixel, the same bytes on every target: each shader kind's CPU
//! reference and WGSL twin ([`shader`], SPEC §3.8), the JPEG decoder ([`jpeg`], ADR-0017), and
//! an image's adjustments ([`adjust`], PLAN 2.117).
//!
//! It is a crate of its own so the browser's modules can build it for speed while they build
//! the rest of `scaena-core` for size (PLAN 2.98). Core re-exports them, as
//! `scaena_core::shader`, `scaena_core::jpeg`, and `scaena_core::adjust`, and is how everything
//! else reaches them.

pub mod adjust;
pub mod jpeg;
pub mod shader;

/// The longest side an image may have, in pixels: vello's image atlas is 8192 px square, and
/// an image that does not fit it is not drawn on the GPU at all. The display list's
/// (`scaena_core::displaylist::MAX_IMAGE_SIDE`), which the decoder refuses past.
pub const MAX_IMAGE_SIDE: u32 = 8192;

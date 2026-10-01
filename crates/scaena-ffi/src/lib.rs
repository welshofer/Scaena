//! # scaena-ffi
//!
//! C ABI for the Swift client (SPEC §9.3, PLAN 3.1): opaque engine handle,
//! `scaena_open`, `scaena_states`, `scaena_frame(state, t, w, h)` returning a
//! display list or painting into a `CAMetalLayer`-backed `wgpu` surface,
//! `scaena_lint`, `scaena_patch`. Header generated with `cbindgen`.
//! SwiftUI owns chrome only; no TextKit in the render path.

pub const PHASE: &str = "3.1";

//! # scaena-wasm
//!
//! `wasm-bindgen` surface for the browser player/editor (SPEC §9.2):
//! `load(bundle_bytes)`, `states()`, `timeline(state)`, `frame(state, t, w, h)`
//! → display list (postcard bytes) or paints directly to an `OffscreenCanvas`
//! via `vello` on WebGPU. Phase 0 task 0.8 ships the bare page; budget ≤ 3 MB gz.

pub const PHASE: &str = "0.8";

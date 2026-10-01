//! Text layout (SPEC §3.5): shaping via `harfrust` through `parley`, line breaking
//! (`greedy` / `pretty` / `balance`), cap-height boxes, hanging punctuation,
//! optical margins, split units for animation.
//!
//! Phase 0 tasks 0.4–0.5. Fonts come from the bundle only (SPEC §13.3): the
//! font context is [`crate::fonts::bundle_font_context`], never the system.
//!
//! Output is a list of glyph runs with final positions — the display list's
//! `glyphs` op — so painters never shape.

use crate::EngineError;

#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub font: String,
    pub size: f64,
    pub glyphs: Vec<[f64; 3]>,
    pub line: usize,
    pub word: usize,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextLayout {
    pub runs: Vec<GlyphRun>,
    pub lines: usize,
    pub overflow: bool,
    pub cap_height: f64,
    pub x_height: f64,
    pub baseline_first: f64,
}

pub fn shape_and_break(_text: &str, _role: &serde_json::Value, _measure_cu: f64) -> Result<TextLayout, EngineError> {
    Err(EngineError::NotImplemented("text — PLAN 0.4"))
}

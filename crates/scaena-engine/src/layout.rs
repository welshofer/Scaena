//! Layout (SPEC §3.4): templates, slots, grid placement, containers via `taffy`,
//! typographic alignment anchors (cap / baseline / x-height).
//!
//! Phase 0 task 0.5 and Phase 1 task 1.7. The contract:
//!
//! - `layout(snapshot, theme, viewport) -> Layout` produces a box per visible node
//!   in canvas units, plus per-text-node glyph runs from [`crate::text`].
//! - Layout is per snapshot, never per frame; frames only sample (SPEC §5).

use crate::EngineError;

/// Resolved geometry for one node.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeBox {
    pub id: String,
    pub rect: [f64; 4],
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Layout {
    pub boxes: Vec<NodeBox>,
}

pub fn layout(
    _snapshot: &scaena_core::Snapshot,
    _theme: &crate::theme::Theme,
    _viewport: [f64; 2],
) -> Result<Layout, EngineError> {
    Err(EngineError::NotImplemented("layout — PLAN 0.5 / 1.7"))
}

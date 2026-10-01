//! `frame(request) -> DisplayList`: the engine's one public entry point.

use crate::EngineError;
use scaena_core::displaylist::DisplayList;

#[derive(Debug, Clone)]
pub struct FrameRequest<'a> {
    pub deck: &'a scaena_core::Deck,
    pub theme: &'a crate::theme::Theme,
    pub state: &'a str,
    /// Milliseconds since the start of the transition into `state`.
    pub t_ms: f64,
    pub viewport: [f64; 2],
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub display_list: DisplayList,
    /// Total transition + choreography duration for this state, ms.
    pub duration_ms: f64,
}

/// Render one frame. Deterministic: same inputs → identical display list (SPEC §13).
pub fn frame(req: &FrameRequest) -> Result<Frame, EngineError> {
    let snapshots = scaena_core::resolve_states(req.deck)?;
    let _snap = snapshots
        .iter()
        .find(|s| s.state_id == req.state)
        .ok_or_else(|| EngineError::UnknownState(req.state.to_string()))?;
    Err(EngineError::NotImplemented("render::frame — PLAN 0.3–0.10"))
}

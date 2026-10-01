//! # scaena-engine
//!
//! The pure pipeline (SPEC §5):
//!
//! ```text
//! resolve theme → resolve states → load data → charts→marks → layout + text
//! → resolve timeline → sample(t) → display list
//! ```
//!
//! Status: scaffold. Each module declares its contract; Phase 0 (PLAN §0) fills
//! `text`, `layout`, `shaders::mesh`, and `render::frame` for the spike fixture.
//! Nothing here may read a clock, system fonts, or the filesystem (SPEC §13).

pub mod charts;
pub mod error;
pub mod layout;
pub mod render;
pub mod sample;
pub mod shaders;
pub mod text;
pub mod theme;

pub use error::EngineError;
pub use render::{Frame, FrameRequest, frame};

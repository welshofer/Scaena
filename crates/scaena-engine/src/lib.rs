//! # scaena-engine
//!
//! The pure pipeline (SPEC §5):
//!
//! ```text
//! resolve theme → resolve states → load data → charts→marks → layout + text
//! → resolve timeline → sample(t) → display list
//! ```
//!
//! Status (PLAN 0.4): text nodes lay out on the theme grid with parley over bundle
//! fonts only, and `Engine::frame` emits their display list at rest. Charts (0.10),
//! shaders (0.11), sampling (0.10/1.12), and containers (1.7) are still to come.
//! Nothing here may read a clock, system fonts, or the filesystem (SPEC §13).

pub mod charts;
pub mod error;
pub mod fonts;
pub mod layout;
pub mod render;
pub mod sample;
pub mod shaders;
pub mod text;
pub mod theme;

pub use error::EngineError;
pub use render::{Engine, Frame, FrameRequest, PlacedText};

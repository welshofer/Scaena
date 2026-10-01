//! # scaena-engine
//!
//! The pure pipeline (SPEC §5):
//!
//! ```text
//! resolve theme → resolve states → load data → charts→marks → layout + text
//! → resolve timeline → sample(t) → display list
//! ```
//!
//! Status (PLAN 0.10): text nodes lay out on the theme grid with parley over bundle
//! fonts only; one-series bar charts compile to keyed marks from data files the
//! caller hands over; each snapshot lays out once into a [`sample::Scene`], and a
//! [`sample::Transition`] samples two of them, moving chart data by key (frames never
//! lay out or shape). Shaders (0.11), choreography (1.11), containers (1.7), and the
//! chart and table sprint (1.9) are still to come. Nothing here may read a clock,
//! system fonts, or the filesystem (SPEC §13).

pub mod charts;
pub mod data;
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

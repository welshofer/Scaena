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
//! lay out or shape). `mesh` shader nodes resolve to shader ops at their time on the
//! global timeline. Every node passes through the theme cascade (1.6, [`cascade`]): its
//! role, its `style`, the state, then the deck's `overrides`. Shapes and PNG images draw
//! in their boxes, and `stack`, `grid`, and `frame` containers lay their children out
//! with `taffy` (1.7, [`containers`]). Other shader kinds (1.10), choreography (1.11),
//! and the chart and table sprint (1.9) are still to come. Nothing here may read a
//! clock, system fonts, or the filesystem (SPEC §13).

pub mod cascade;
pub mod charts;
pub mod containers;
pub mod data;
pub mod error;
pub mod fonts;
pub mod images;
pub mod layout;
pub mod render;
pub mod sample;
pub mod scale;
pub mod shaders;
pub mod shapes;
pub mod tables;
pub mod text;
pub mod theme;

pub use error::EngineError;
pub use render::{Engine, Frame, FrameRequest, PlacedText};

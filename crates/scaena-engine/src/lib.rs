//! # scaena-engine
//!
//! The pure pipeline (SPEC §5):
//!
//! ```text
//! resolve theme → resolve states → load data → charts→marks → layout + text
//! → resolve timeline → sample(t) → display list
//! ```
//!
//! Status (PLAN 1.11): text nodes lay out on the theme grid with parley over bundle
//! fonts only; charts and tables compile from data files the caller hands over (1.9,
//! [`charts`], [`tables`]); each snapshot lays out once into a [`sample::Scene`], and a
//! [`sample::Transition`] samples two of them, moving chart data by key (frames never
//! lay out or shape). A state's motions (presets, `anim` tracks, choreography) resolve
//! against the theme ([`motion`]) and run on its cue's clock with the transition, and
//! [`Engine::timeline`] lays the states end to end with their holds (1.11). Shader nodes
//! of every kind resolve to shader ops at their time on that timeline, from the node or
//! a theme preset (1.10, [`shaders`]). Every node passes through the theme cascade (1.6,
//! [`cascade`]): its role, its `style`, the state, then the deck's `overrides`. Shapes,
//! in colors or gradients, and PNG images draw in their boxes, and `stack`, `grid`, and
//! `frame` containers lay their children out with `taffy` (1.7, [`containers`]). A state at
//! rest says what stands where, for a client that edits by pointing ([`geometry`], ADR-0013).
//! Nothing here may read a clock, system fonts, or the filesystem (SPEC §13).

pub mod cascade;
pub mod charts;
pub mod containers;
pub mod data;
pub mod error;
pub mod fonts;
pub mod geometry;
pub mod images;
pub mod layout;
pub mod lint;
pub mod motion;
pub mod render;
pub mod sample;
pub mod scale;
pub mod shaders;
pub mod shapes;
pub mod tables;
pub mod text;
pub mod theme;

pub use error::EngineError;
pub use render::{Engine, Frame, FrameRequest, PlacedText, project};

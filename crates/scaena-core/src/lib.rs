//! # scaena-core
//!
//! The document model and the pure logic that needs no fonts, no layout, and no GPU:
//!
//! - [`document`] — `deck.json` types (SPEC §3). Structural parts are typed; node
//!   properties are an ordered JSON map, so tracking and patching work generically.
//! - [`dsl`] — the `.scn` authoring projection: compile and canonical decompile (SPEC §4).
//! - [`model`] — what those maps may hold, one struct per node type, and the theme; with
//!   [`document`] they generate `docs/schema/*.json` (PLAN 1.1).
//! - [`tracking`] — resolves the ordered cue list into absolute snapshots (SPEC §2.2).
//! - [`timeline`] — easing curves and springs with settle-time computation (SPEC §3.9).
//! - [`displaylist`] — the painter-agnostic frame description (SPEC §6).
//! - [`color`] — a theme's color literals, hex and Oklab, to the bytes a display list holds.
//! - [`data`] — data sources read and typed (SPEC §3.10), from bytes the caller hands over.
//! - [`format`] — number and date formats, d3's grammar (`docs/spec/format.md`).
//! - [`shader`] — what a shader op draws: each kind's CPU reference and its WGSL twin
//!   (SPEC §3.8), here because every painter runs them.
//! - [`lint`] — findings, rules, and the document-level rule set (SPEC §7.4–7.5).
//! - [`patch`] — JSON Patch, and the semantic ops that compile to it (SPEC §7.3).
//! - [`reading`] — how a deck reads to someone who hears it: each node's part, and a state as
//!   HTML (SPEC §3.12), for a tagged PDF, a single file, and the web player.
//! - [`spine`] — the spine projection the pipelines beyond the deck read (SPEC §10).
//! - [`validate`] — semantic validation (ids, references), surfaced as lint findings.
//!
//! Invariant: nothing in this crate reads a clock, a font, or the filesystem.

pub mod choices;
pub mod color;
pub mod data;
pub mod displaylist;
pub mod document;
pub mod dsl;
pub mod expr;
pub mod format;
pub mod ids;
pub mod lint;
pub mod model;
pub mod patch;
pub mod reading;
pub mod shader;
pub mod spine;
pub mod timeline;
pub mod tracking;
pub mod transform;
pub mod validate;

pub use document::Deck;
pub use lint::{Finding, Severity};
pub use tracking::{Snapshot, resolve_states};

/// The `deck.json` format version this crate reads and writes (its `scaena` key). Bump per
/// SPEC §3.1; `docs/schema/deck.schema.json` takes its `$id` and version pattern from it.
pub const FORMAT_VERSION: &str = "0.10";

/// The theme format version (a theme's `scaena-theme` key), versioned apart from decks.
pub const THEME_FORMAT_VERSION: &str = "0.8";

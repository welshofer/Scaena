//! # scaena-core
//!
//! The document model and the pure logic that needs no fonts, no layout, and no GPU:
//!
//! - [`document`] — `deck.json` types (SPEC §3). Structural parts are typed; node
//!   properties are an ordered JSON map until Phase 1 task 1.1 replaces them with
//!   typed `NodeProps` generated alongside the JSON Schema.
//! - [`tracking`] — resolves the ordered cue list into absolute snapshots (SPEC §2.2).
//! - [`timeline`] — easing curves and springs with settle-time computation (SPEC §3.9).
//! - [`displaylist`] — the painter-agnostic frame description (SPEC §6).
//! - [`shader`] — what a shader op draws: each kind's CPU reference and its WGSL twin
//!   (SPEC §3.8), here because every painter runs them.
//! - [`lint`] — findings, rules, and the document-level rule set (SPEC §7.4–7.5).
//! - [`validate`] — semantic validation (ids, references), surfaced as lint findings.
//!
//! Invariant: nothing in this crate reads a clock, a font, or the filesystem.

pub mod displaylist;
pub mod document;
pub mod ids;
pub mod lint;
pub mod shader;
pub mod timeline;
pub mod tracking;
pub mod validate;

pub use document::Deck;
pub use lint::{Finding, Severity};
pub use tracking::{Snapshot, resolve_states};

/// Format version this crate reads and writes. Bump per SPEC §3.1.
pub const FORMAT_VERSION: &str = "0.1";

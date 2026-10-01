//! # scaena-mcp
//!
//! MCP server over the same operations as the CLI (SPEC §7.2, ADR-0003):
//! `deck_create`, `deck_read`, `deck_patch`, `deck_lint`, `deck_inspect`,
//! `deck_render` (image content), `deck_export`, `deck_diff`, `theme_apply`,
//! `data_attach`, `spine_read`, `spine_update`; resources `scaena://schema/*`,
//! `scaena://lint/catalog`, `scaena://examples/*`.
//!
//! Implementation: `rmcp` over stdio, Phase 1 task 1.17. Tool schemas are
//! generated from the Rust types with `schemars` into `docs/schema/mcp/`.

pub const PHASE: &str = "1.17";

# ADR-0004: Text and rendering stack — parley/swash/fontique, taffy, vello, krilla

**Status:** proposed (becomes accepted at gate 0) · **Date:** 2026-10-01

## Context

Typography is the whole game. We need shaping with full OpenType feature and variable-axis support, our own line breaking (`pretty`/`balance`), font fallback restricted to bundle fonts, a GPU painter that runs on WebGPU and Metal, a CPU painter for headless parity, and a PDF backend with real text.

## Decision (to be confirmed by the Phase 0 spike)

| Concern | Crate | Version at writing |
|---|---|---|
| Rich text layout, line breaking | `parley` | 0.11 |
| Shaping, glyph outlines | `swash` | 0.2 |
| Font enumeration/fallback (bundle-only source) | `fontique` | 0.11 |
| Flex/grid layout | `taffy` | 0.14 |
| Geometry / paint primitives | `kurbo`, `peniko` | 0.13 / 0.6 |
| GPU painter | `vello` (wgpu) | 0.10 |
| CPU painter | `vello_cpu` | 0.2 |
| PDF | `krilla` (over `pdf-writer`) | 0.8 |
| Font subsetting | `subsetter` or `klippa` | tbd in Phase 1 |

Rationale: one ecosystem (Linebender) with aligned primitives; `vello_cpu` gives deterministic headless output with no GPU; `krilla` is the PDF path Typst uses; everything compiles to WASM and to a static library for Swift.

## Alternatives

- **Skia (`skia-safe` / CanvasKit):** most mature, has a PDF backend, but a ~7 MB WASM payload, C++ build, and no ownership of line breaking. Kept as the fallback for *painting* only.
- **`rustybuzz` + custom layout:** pure-Rust HarfBuzz port; full control, more code. The fallback if `parley`'s breaking or metrics cannot meet the bar (PLAN gate 0 no-go path).
- **`cosmic-text`:** solid, but tied to its own font system and less aligned with vello's glyph pipeline.

## Consequences

- We depend on a fast-moving ecosystem; versions are pinned and bumped deliberately with golden-test diffs.
- Line-breaking quality beyond greedy may require our own Knuth–Plass pass over parley's clusters (tracked as a Phase 0 measurement, Phase 1 task).

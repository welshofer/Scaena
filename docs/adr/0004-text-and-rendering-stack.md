# ADR-0004: Text and rendering stack — parley/harfrust/fontique, taffy, vello, krilla

**Status:** proposed (becomes accepted at gate 0) · **Date:** 2026-10-01 · **Amended:** 2026-10-01 (PLAN 0.1: stack pinned; `swash` replaced by `harfrust` + `skrifa`, which is what `parley` actually uses)

## Context

Typography is the whole game. We need shaping with full OpenType feature and variable-axis support, our own line breaking (`pretty`/`balance`), font fallback restricted to bundle fonts, a GPU painter that runs on WebGPU and Metal, a CPU painter for headless parity, and a PDF backend with real text.

## Decision (to be confirmed by the Phase 0 spike)

| Concern | Crate | Resolved (Cargo.lock, PLAN 0.1) |
|---|---|---|
| Rich text layout, line breaking, bidi | `parley` | 0.11.1 |
| Shaping (via `parley`) | `harfrust` — the HarfBuzz project's Rust port | 0.12.0 |
| Font parsing, metrics, glyph outlines (via `parley`, `vello`, `vello_cpu`) | `skrifa` / `read-fonts` | 0.44.0 / 0.41.0 |
| Font collection and fallback (bundle-only source) | `fontique` | 0.11.1 |
| Break opportunities, Unicode properties (via `parley`) | `icu_segmenter` / `icu_properties` | 2.3.0 / 2.3.0 |
| Flex/grid layout | `taffy` | 0.14.0 |
| Geometry / paint primitives | `kurbo`, `peniko` | 0.13.1 / 0.6.1 |
| GPU painter | `vello` on `wgpu` (`vello_shaders`, `naga`) | 0.10.0 on 29.0.4 |
| CPU painter | `vello_cpu` (`vello_common`, `glifo`, `fearless_simd`) | 0.2.0 (0.2.0, 0.3.0, 0.4.1) |
| PDF | `krilla` (over `pdf-writer`) | 0.8.2 — not yet a dependency (PLAN 1.20) |
| Font subsetting | `subsetter` or `klippa` | tbd in Phase 1 |

Every crate in the rows above that can change a glyph position or a pixel resolves to exactly one version in the workspace graph (`cargo tree -d` lists only leaf utilities: `hashbrown`, `foldhash`, `miniz_oxide`, `syn`). In particular there is one `skrifa`, one `peniko`, one `kurbo`, and one `linebender_resource_handle`, so the `FontData` a `parley` glyph run carries is the type `vello` and `vello_cpu` draw with — no copying or conversion between shaping and painting.

**Feature choices** (workspace manifest):
- `parley`, `fontique`: `default-features = false, features = ["std"]`. Their default `system` feature compiles in fontconfig / CoreText / DirectWrite discovery, which SPEC §13.3 forbids in the render path. The engine additionally builds its collection with `system_fonts: false` (`scaena_engine::fonts::bundle_font_context`), so feature unification from some other crate cannot reopen it. Negative control (one-off, Linux dev container with 59 installed font files): with `parley` defaults, `FontContext::new()` sees 33 system families, and `system_fonts: false` alone already sees 0. The committed test `fonts::tests::bundle_context_sees_no_system_fonts` asserts zero families and zero generic-family matches.
- `taffy`: `std`, `taffy_tree`, `flexbox`, `grid` only. Block, float, `calc`, and content-size layout are CSS behaviour we do not use and WASM bytes we do not want.
- `vello_cpu`: defaults (`std`, `png`, `text`, `u8_pipeline`). `multithreading` stays off until PLAN 0.6 shows multi-threaded output is bit-identical to single-threaded.
- `vello`: defaults (`wgpu` with its default backends) until PLAN 0.8 trims for WASM size.
- `skrifa` is declared in the workspace at `parley`'s minor for direct font-table access when code needs it (E120 coverage, a metrics fallback); no crate depends on it directly yet.
- `swash` is not in the graph. `parley` stopped shaping with it; nothing else here needs it.

Rationale: one ecosystem (Linebender + fontations) with aligned primitives and a single font-parsing stack; `vello_cpu` gives deterministic headless output with no GPU; `krilla` is the PDF path Typst uses; everything compiles to WASM (checked in CI for `scaena-engine` and `scaena-paint`, both painters) and to a static library for Swift. `harfrust` returns positions in integer font units and leaves scaling to the caller; no platform text library is involved anywhere between bundle bytes and glyph positions.

## Findings that later tasks must act on

1. **SIMD level is a raster input.** `vello_cpu::RenderSettings::default()` picks the level at runtime (`Level::try_detect()`: AVX2 on x86-64-v3, NEON on Apple silicon); upstream recommends the scalar `Level::fallback()` for reference images because levels can differ slightly. `fallback()` needs `fearless_simd`'s `force_support_fallback` feature. PLAN 0.6 decides the golden-raster level by measurement, not assumption.
2. **u8 vs f32 pipeline.** `vello_cpu` defaults to `RenderMode::OptimizeSpeed` (u8/u16); `OptimizeQuality` (f32) needs the `f32_pipeline` feature. `vello` on the GPU computes in f32. PLAN 0.9 measures which CPU mode sits within the SPEC §13.5 tolerance of the GPU.
3. **Cap height and x-height come from OS/2.** `parley::RunMetrics` exposes `cap_height` / `x_height` as `Option<f32>` (absent below OS/2 v2). The torture-deck fonts (PLAN 0.2) must carry OS/2 v2+ metrics, or PLAN 0.5 needs a table-based fallback; SPEC §3.5 rules out bounding boxes.
4. **`krilla` 0.8.2 declares `rust-version = 1.92`;** the workspace declares 1.90. Bump the workspace when PLAN 1.20 adds it.

## Alternatives

- **Skia (`skia-safe` / CanvasKit):** most mature, has a PDF backend, but a ~7 MB WASM payload, C++ build, and no ownership of line breaking. Kept as the fallback for *painting* only.
- **`harfrust` + our own line breaker:** keep the shaper, own the layout — drive `harfrust` directly over `icu_segmenter` break opportunities with our own Knuth–Plass and metrics. The fallback if `parley`'s breaking or metrics cannot meet the bar (PLAN gate 0 no-go path). (`rustybuzz`, named here before PLAN 0.1, is the crate `harfrust` forked from; `harfrust` tracks HarfBuzz 13.0.0 on `read-fonts`, so it is the shaper to keep.)
- **`cosmic-text`:** solid, but tied to its own font system and less aligned with vello's glyph pipeline.

## Consequences

- We depend on a fast-moving ecosystem. `Cargo.lock` is the pin and CI builds `--locked`; a bump is a deliberate PR that carries its golden-test diffs and updates the table above.
- Line-breaking quality beyond greedy may require our own Knuth–Plass pass over parley's clusters (tracked as a Phase 0 measurement, Phase 1 task).

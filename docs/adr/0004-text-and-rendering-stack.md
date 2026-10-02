# ADR-0004: Text and rendering stack — parley/harfrust/fontique, taffy, vello, krilla

**Status:** proposed (becomes accepted at gate 0) · **Date:** 2026-10-01 · **Amended:** 2026-10-01 (PLAN 0.1: stack pinned; `swash` replaced by `harfrust` + `skrifa`, which is what `parley` actually uses) · 2026-10-01 (PLAN 0.6: CPU painter findings measured) · 2026-10-01 (PLAN 0.7: GPU painter findings) · 2026-10-01 (hanging quotes: finding 8) · 2026-10-02 (PLAN 0.11: finding 9) · 2026-10-02 (PLAN 1.4: subsetting, finding 10)

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
| Font subsetting (bundle save) | `skera`: fontations' subsetter, formerly klippa, a port of hb-subset | 0.7.0 (PLAN 1.4; finding 10) |

Every crate in the rows above that can change a glyph position or a pixel resolves to exactly one version in the workspace graph (`cargo tree -d` lists only leaf utilities: `hashbrown`, `foldhash`, `miniz_oxide`, `syn`). In particular there is one `skrifa`, one `peniko`, one `kurbo`, and one `linebender_resource_handle`, so the `FontData` a `parley` glyph run carries is the type `vello` and `vello_cpu` draw with — no copying or conversion between shaping and painting.

**Feature choices** (workspace manifest):
- `parley`, `fontique`: `default-features = false, features = ["std"]`. Their default `system` feature compiles in fontconfig / CoreText / DirectWrite discovery, which SPEC §13.3 forbids in the render path. The engine additionally builds its collection with `system_fonts: false` (`scaena_engine::fonts::bundle_font_context`), so feature unification from some other crate cannot reopen it. Negative control (one-off, Linux dev container with 59 installed font files): with `parley` defaults, `FontContext::new()` sees 33 system families, and `system_fonts: false` alone already sees 0. The committed test `fonts::tests::bundle_context_sees_no_system_fonts` asserts zero families and zero generic-family matches.
- `taffy`: `std`, `taffy_tree`, `flexbox`, `grid` only. Block, float, `calc`, and content-size layout are CSS behaviour we do not use and WASM bytes we do not want.
- `vello_cpu`: defaults (`std`, `png`, `text`, `u8_pipeline`). `multithreading` stays off: PLAN 0.6 measured it bit-identical to single-threaded (63 of 63 torture paints, 0 pixels differ), but slower on text frames (13.9 vs 11.2 ms per 1080p frame on a 4-core x86-64 container, `num_threads` at its default of cores − 1), so it buys nothing yet. Revisit with the shader-heavy B3 bench. If feature unification turns it on, `RenderSettings::default()` makes `CpuPainter` multi-threaded; the measurement says that is harmless.
- `vello`: defaults (`wgpu` with its default backends) until PLAN 0.8 trims for WASM size.
- `skrifa` is declared in the workspace at `parley`'s minor for direct font-table access when code needs it (E120 coverage, a metrics fallback); no crate depends on it directly yet.
- `swash` is not in the graph. `parley` stopped shaping with it; nothing else here needs it.

Rationale: one ecosystem (Linebender + fontations) with aligned primitives and a single font-parsing stack; `vello_cpu` gives deterministic headless output with no GPU; `krilla` is the PDF path Typst uses; everything compiles to WASM (checked in CI for `scaena-engine` and `scaena-paint`, both painters) and to a static library for Swift. `harfrust` returns positions in integer font units and leaves scaling to the caller; no platform text library is involved anywhere between bundle bytes and glyph positions.

## Findings that later tasks must act on

1. **SIMD level is a raster input, but barely.** `vello_cpu::RenderSettings::default()` picks the level at runtime (`Level::try_detect()`: AVX2 on x86-64-v3, NEON on Apple silicon); upstream recommends the scalar `Level::fallback()` for reference images because levels can differ slightly (`fallback()` needs `fearless_simd`'s `force_support_fallback` feature; on a default x86-64 target `Level::baseline()` is already the scalar fallback). Measured in PLAN 0.6, u8 pipeline, 21 torture frames at 1080p: scalar vs AVX2 differ in 3 pixels in total, by 1/255. *Decision:* golden rasters are painted at `Level::new()` (the host's best) and compared with the SPEC §13.5 metric, never byte for byte, so no reference level and no extra feature. macOS CI (NEON) checks the x86-64-blessed goldens on every push and prints each state's delta; its first run painted all 21 frames byte-identical to them.
2. **u8 vs f32 pipeline.** `vello_cpu` defaults to `RenderMode::OptimizeSpeed` (u8/u16); `OptimizeQuality` (f32) needs the `f32_pipeline` feature. `vello` on the GPU computes in f32. Measured in PLAN 0.6 on the same 21 frames: f32 differs from u8 on about 6,800 anti-aliased edge pixels per frame, by at most 3/255 (largest off-edge ΔE 0.30, nothing over tolerance), and paints 65% slower (18.5 vs 11.2 ms). `CpuPainter` stays u8 and names its `RenderMode` explicitly. With only `u8_pipeline` compiled, vello_cpu ignores `RenderMode`; if feature unification compiles in `f32_pipeline`, the explicit mode keeps u8. PLAN 0.9 measures which CPU mode sits closer to the GPU.
3. **Cap height and x-height come from OS/2.** `parley::RunMetrics` exposes `cap_height` / `x_height` as `Option<f32>` (absent below OS/2 v2). The torture-deck fonts (PLAN 0.2) must carry OS/2 v2+ metrics, or PLAN 0.5 needs a table-based fallback; SPEC §3.5 rules out bounding boxes.
4. **`krilla` 0.8.2 declares `rust-version = 1.92`;** the workspace declares 1.90. Bump the workspace when PLAN 1.20 adds it.
5. **Hinting defaults differ between the painters.** `glifo`'s glyph run builder (what `vello_cpu` draws text with) hints outlines unless told not to; `vello`'s does not. Glyph positions are final in the display list (SPEC §6), and hinting would let one painter move outlines that the other draws as laid out. So `CpuPainter` calls `.hint(false)`, and every painter draws unhinted.
6. **`vello` fills a whole glyph run as one path.** Its encoder resolves a run into a single path holding every glyph's outline (`vello_encoding::resolve`: one `n_paths` per run), so overlapping glyphs share one nonzero winding count. glifo (vello_cpu's text) fills each glyph on its own: its `draw_glyphs` loops over the run. Where overlapping contours run in opposite directions the windings cancel: in the torture deck's `combining` state, Roboto Serif draws the j's dot clockwise and the combining caron counter-clockwise, and the first GPU raster had a hole where they overlap (one pixel 231/255 from the CPU's). *Decision:* `GpuPainter` draws one run per glyph, so each glyph is its own fill, as on the CPU. Overlaps are routine (marks on bases, tight tracking, cursive joins), so this applies to all text, not just this font.
7. **The painters' anti-aliasing differs at edges, not inside.** Across the 21 torture frames (PLAN 0.7, lavapipe), vello's area coverage and vello_cpu's sparse-strip coverage disagree on 5,209–27,265 edge pixels per frame, by at most 63/255. Interiors match exactly, except for COLRv1 gradients in `emoji` (114 pixels, ΔE ≤ 1.49). A vertical edge at 0.75 coverage reads 191 on the CPU and 180 on the GPU. The finding-6 hole sat on a glyph edge, so SPEC §13.5's edge mask hid it, and the torture test passed. §13.5 now also fails any pixel that differs by half the channel range or more: anti-aliasing never comes close, and a hole or a misplaced glyph does.
8. **parley has no hanging punctuation, and the engine leans on its per-line alignment box.** Quotes hang on top of parley's breaker (SPEC §3.5): line *k* is broken at the measure plus its hang (`BreakLines::state_mut().set_line_max_advance`), and the layout's max advance is raised to the widest such line, because parley asserts that no line exceeds it by 1 cu or more. parley then aligns each line inside its own box (`inline_max_coord = line_x + line_max_advance`). That box is what puts a right-to-left line's opening quote past the right edge; the engine shifts left-to-right lines itself. If an upgrade aligned lines to the layout's width instead, right-to-left quotes would move back inside, and `a_right_to_left_line_hangs_its_opening_quote_past_the_right_edge` would fail.
9. **vello draws GPU-computed pixels without a readback.** `Renderer::register_texture` (vello 0.10) takes an `Rgba8Unorm` texture with `COPY_SRC` and copies it into vello's image atlas every frame; a scene draws it as an ordinary image. A shader op's WGSL therefore runs as a compute pass, writes its RGBA bytes as integers into a buffer (sidestepping float-to-unorm rounding, which differs between backends), and is copied into such a texture before vello renders, natively and on WebGPU. An image brush at `ImageQuality::Low`, placed with the brush transform `xf⁻¹ · translate(box)`, lands texel for pixel on vello and on vello_cpu, whose paint transform composes the same way. The atlas tops out at 8,192 px a side (`MAX_ATLAS_SIZE`): a larger shader rect will need tiling.

10. **Bundles subset with `skera`, keeping every glyph id.** Saving subsets each font to what its deck can draw (SPEC §3.1). The subset must still shape: GSUB, GPOS, GDEF, variations, COLR, and hinting all stay. `subsetter` (typst's, which `krilla` uses for PDF) drops layout tables, which suits a PDF and not a bundle that is shaped again. `skera` 0.7, the hb-subset port in fontations (it was klippa), keeps them. It reads fonts with its own `skrifa` 0.47 and `write-fonts` 0.53, a second copy beside the render path's 0.44. That copy never touches a glyph position: it lives in `scaena-store`, which only writes files. *Decisions:*
    - Subsets keep their glyph ids (`RETAIN_GIDS`), so a saved bundle's display lists are the original's.
    - Each subset keeps the deck's characters plus ASCII, Latin-1, Latin Extended-A, and general punctuation, so an edit in a Latin script needs no original font.
    - Legacy `kern` and the AAT tables, which hb-subset drops by default, stay: shaping reads them when a font has no GPOS.

    Measured: the ten torture states that stress shaping most draw their golden display lists from a saved zip, font ids aside: Arabic joining, Hebrew bidi, combining marks, COLRv1 emoji, ligatures, discretionary ligatures, kerning, hanging quotes, variable axes, and accents. B1's states draw the same before and after a save.

## Alternatives

- **Skia (`skia-safe` / CanvasKit):** most mature, has a PDF backend, but a ~7 MB WASM payload, C++ build, and no ownership of line breaking. Kept as the fallback for *painting* only.
- **`harfrust` + our own line breaker:** keep the shaper, own the layout — drive `harfrust` directly over `icu_segmenter` break opportunities with our own Knuth–Plass and metrics. The fallback if `parley`'s breaking or metrics cannot meet the bar (PLAN gate 0 no-go path). (`rustybuzz`, named here before PLAN 0.1, is the crate `harfrust` forked from; `harfrust` tracks HarfBuzz 13.0.0 on `read-fonts`, so it is the shaper to keep.)
- **`cosmic-text`:** solid, but tied to its own font system and less aligned with vello's glyph pipeline.

## Consequences

- We depend on a fast-moving ecosystem. `Cargo.lock` is the pin and CI builds `--locked`; a bump is a deliberate PR that carries its golden-test diffs and updates the table above.
- Line-breaking quality beyond greedy may require our own Knuth–Plass pass over parley's clusters (tracked as a Phase 0 measurement, Phase 1 task).

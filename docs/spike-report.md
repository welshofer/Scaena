# Phase 0 spike report

**Verdict: go.** All seven gate 0 exit criteria are met (table below). Text parity, the question that could have killed the project, held. vello_cpu, native vello, and vello on WebGPU in the browser agree on every kill case, and the display lists are bit-identical on x86-64 and arm64. Phase 1 starts at PLAN 1.1. What did not match, and what Phase 1 inherits from it, follows the table.

**Numbers:** Linux dev container (Intel Xeon @ 2.10 GHz, 4 vCPU), release builds, unless a line says otherwise. SPEC §15's reference machine is an M-series Mac. The closest measured is CI's Apple Silicon runner (`Apple M1 (Virtual)`, 3 cores, macOS 26.6), which is what criterion 6's cold render is judged on. Linux numbers are recorded, not gated.

## Gate 0

| # | Exit criterion | Result | Evidence |
|---|---|---|---|
| 1 | Kill cases: bit-identical display lists on macOS and Linux; rasters within tolerance across vello_cpu, native vello, and WASM/WebGPU; catalogue recorded | met | **Display lists:** `raw_display_lists_are_bit_identical_across_platforms` hashes the unquantized display lists of all 25 torture states and 4 frames inside transitions, and every CI run passes it on x86-64 Linux and arm64 macOS (0.5). **Kill cases:** the 11 `kill_*` tests in `crates/scaena-engine/tests/torture.rs`. **Rasters:** the parity harness passes every state on every pair: vello_cpu, vello on lavapipe and on Metal, and vello on WebGPU (SwiftShader). The NEON rasters are byte-identical to the AVX2 goldens (0.6, 0.9). **Catalogue** (0.4): discretionary ligatures pass, bidi passes with a finding, combining marks are partial, emoji fails. |
| 2 | OpenType features and variable axes demonstrably applied | met | **Ligatures:** `kill_standard_ligatures_form_and_switch_off`, 22 glyphs with `liga` and 28 without. **Axes:** `kill_variable_axes_reach_the_instance` drives wght, wdth, and opsz to their extremes; wdth 50 sets under 0.75× the width of wdth 150, and opsz 8 against 144 changes advances. **Seen:** the reviewed `liga`, `dlig`, and `axes` rasters (0.4, 0.6). |
| 3 | Chart data motion at four sample points, no layout per frame | met | Goldens at t = 0, 0.25, 0.5, and 1 for values animating in and for the next quarter (0.10). `Transition::frame` takes `&self` and holds no fonts or layout engine, so a frame cannot lay out. |
| 4 | Mesh CPU/GPU parity | met | `shader_parity`: 7 cases within one step on lavapipe and on Metal. The torture `mesh` state passes every painter pair (0.11). |
| 5 | WASM engine ≤ 3.0 MB gzip | met | 1.08 MB: 1,083,673 bytes, `gzip -9`, no fonts (0.8, 0.11, re-measured for 0.14). |
| 6 | Per-stage timings for B1 and B4; cold headless 1080p render ≤ 300 ms on an M-series Mac | met | B1 built for this (`tests/bench/b1.scaena`). Every SPEC §15 stage is timed in 0.14 below. Cold `scaena render` of B1 on CI's Apple Silicon runner: 32.6 ms median and 48.7 ms worst over its 40 states, against 300 ms. |
| 7 | Authorability spike completed | met | 0.13: all five edits validate and lint clean, every patch is local, and identity survives (`docs/examples/agent-authorability.md`). |

## What did not match

- **Emoji (catalogue): fail.** Flags fall back to EB Garamond's regional-indicator letters, because parley queries the style's own font stack before the emoji family. Fix proposed for PLAN 1.8 (0.4).
- **Combining marks (catalogue): partial.** `j` plus a caron keeps the j's dot: the font has no precomposed form to substitute. A font limitation, identical in both painters (0.4).
- **Bidi (catalogue): a finding.** parley reports one space of trailing whitespace on an RTL line that has none. Glyphs land correctly; the line's measured width is short by one space. Upstream issue candidate (0.4).
- **CPU paint on the Linux container is over budget on one thread.** B1's median frame paints in 14–20 ms there, varying between runs. The budget is 12 ms with 8 threads, and vello_cpu's multithreading is off. On Apple Silicon the same frames paint in 5.8 ms median and 12.0 ms worst on one thread, so the reference-class machine meets the budget with no threads at all. Threads are the headroom for B2 and B3. Whether threaded rasters stay byte-identical is a question PLAN 1.21's video path has to answer anyway.
- **GPU paint alone is not measured.** Lavapipe emulates a GPU on the CPU, and CI's Metal device is paravirtualized. Both numbers, about 21 ms per B1 frame, include submit, wait, and readback. SPEC §15's 6 ms budget is for paint alone on an M-series Mac, which needs a physical machine and a paint-only timer.
- **No criterion benches and no CI baseline.** SPEC §15 asks for both from Phase 0 onward. `stages` records medians and worst cases on demand; nothing fails on a regression. Proposed as PLAN 1.24.
- **Benchmarks B2 and B3 do not exist.** They need multi-chart decks and the `grain` shader (PLAN 1.9, 1.10). B1 and B4 are the Phase 0 pair criterion 6 names.
- **What lint does not check.** A deck can validate and lint clean and still fail to render: the renderer needs a font file for every theme family, and nothing checks for one. The 0.13 findings list this and the other gaps in "clean"; each points at its Phase 1 task.

## 0.1 Stack

Versions, feature choices, and the reasoning are in ADR-0004. What the spike learned:

- `parley` 0.11 shapes with `harfrust` 0.12 (the HarfBuzz port) and reads fonts with `skrifa` 0.44. `swash` is not in its graph and was dropped; SPEC §3.5/§12/§13.3 corrected.
- System fonts are compiled out (`parley`/`fontique` without `system`) and refused at runtime (`system_fonts: false`). Negative control: with parley's defaults this container exposes 33 system families; the bundle context exposes 0.
- One version of every glyph- or pixel-affecting crate; parley's `FontData` is the type both painters draw with.
- Open for later tasks: SIMD level as a raster input (0.6), u8 vs f32 CPU pipeline (0.9), OS/2 cap/x-height requirement (met by all five torture fonts), `krilla` needing Rust 1.92 (1.20).

## 0.2 Torture deck

22 states in `tests/fixtures/torture.scaena/` (case table in its README): 10 kill, 5 catalogue, 7 recorded; 0.5 added `anchors` (recorded), for 23. Five OFL fonts subset deterministically from a pinned google/fonts commit (emoji 24.7 MB → 28 KB, COLRv1 only). Balance and widow baits were measured with HarfBuzz 14.5 to fail under greedy breaking with ±15 cu of slack.

Found before any rendering existed:
- `scaena inspect` panicked slicing multi-byte text at a byte offset (fixed in 0.2).
- EB Garamond maps the regional indicator letters, so per-codepoint fallback would draw flags as monochrome letters (became the emoji catalogue trap; confirmed in 0.4).

## 0.3 Display list

- One type, JSON (goldens, tooling) and postcard (runtime). A torture state (`pretty`) is 2,526 bytes in postcard and encodes in 3–4 µs.
- 996,136 random finite `f32` bit patterns round-trip bit-exactly through serde_json; a 50k sample runs in CI.
- Fixed `quantize`, which rounded transform linear parts to 1/64 and erased small rotations.

## 0.4 Text layout

`scaena-engine` lays out text with parley over bundle fonts only and emits glyph runs with final positions. 20 of 22 torture states render to golden display lists (`tests/golden/torture/`); `chart` and `mesh` wait for 0.10 and 0.11.

**Kill cases, at the layout level** (raster parity is 0.6–0.9). Each is a test in `crates/scaena-engine/tests/torture.rs`. Four were mutation-checked (each fails when its feature is broken on purpose: node-level features ignored, `balance` a no-op, the widow rule dropped, tracking dropped).

| Case | Result | Evidence |
|---|---|---|
| Variable axes | pass | Normalized coords reach the runs: wght, wdth, opsz each at −16384 / +16384 for their extremes. wdth 50 sets under 0.75× the width of wdth 150; opsz 8 vs 144 changes advances at a fixed 64 cu. |
| Standard ligatures | pass | 22 glyphs with `liga`, 28 without: ffi, ffl, fj, fl formed. |
| Kerning | pass | Same glyph ids; AV ≥ 3 cu tighter and the line 20+ cu shorter with `kern` on. |
| Numeral styles | pass | Tabular: 1111 and 0000 equal width (< 0.001 cu). Proportional: 1111 narrower by > 10 cu. Oldstyle and lining 1 are different glyphs. |
| Accented Latin | pass | All three lines (16 accented words) set in Roboto Serif, no fallback, no `.notdef`. |
| Bundle-internal fallback | pass | One run → Roboto Serif, EB Garamond (α, ω), Noto Sans Hebrew. |
| Mixed weights | pass | Five runs at 28/56/112 cu and three `wght` instances on one baseline. |
| Tracking extremes | pass | The last glyph moves by exactly −7.68 · (n−1) and +22.4 · (n−1) cu; glyph ids unchanged. |
| Balanced headline | pass | Greedy 1641.5 + 521.4 cu → balance 1076.7 + 1086.1 cu. |
| Widow avoidance | pass | Greedy strands "mistake."; pretty ends on "a mistake." Three lines either way. |
| Hanging quotes (added on review after 0.9) | pass | The quotes opening lines 1 and 3 of `hanging` sit at −advance, the next glyph at 0; line 1 fits only because its quote is not measured. See "Hanging quotes". |

**Gap measured (greedy vs ours):** headline line ratio 0.32 → 0.99; paragraph lines 1699.6 / 1610.4 / 159.8 → 1616.8 / 1661.6 / 191.3 cu. parley's widths agree with the HarfBuzz measurements made for 0.2 within 0.5 cu (1641.5 vs 1642; 521.4 vs 521).

**How breaking works.** `balance` bisects (24 steps) for the narrowest width that keeps greedy's line count. `pretty` asks parley's own breaker for every UAX #14 opportunity (breaking at ~0 width puts one segment per line, with exact widths with and without trailing space), plans minimum raggedness over those segments with the last line held to `minLastLineWords`, realizes the plan with one width per line, and checks parley broke where planned (falling back to greedy, and saying so, if not). Both fall back to greedy for right-to-left paragraphs: parley aligns per-line-width breaks inside those widths, which puts RTL lines at the wrong edge.

**Catalogue:**

| Case | Result | Note |
|---|---|---|
| Discretionary ligatures | pass | 35 glyphs with `dlig`, 44 without. |
| Combining marks | partial | NFD sequences shape in Roboto Serif with no `.notdef`. On the rasters (0.6, 0.7), ế ệ q̃ x́ are placed right, but `j` + U+030C keeps the j's dot under the caron: the font has no precomposed ǰ to compose to and never swaps in its dotless j. A font limitation, identical in both painters, pinned by a test. |
| Emoji | **fail** | 🚀 🙂 👩🏽‍💻 ❤️ are one COLRv1 glyph each, but 🇺🇸 is two EB Garamond glyphs. Root cause (parley 0.11 `shape/mod.rs`): an emoji cluster queries the style's own stack first and appends the generic Emoji family last; EB Garamond covers both regional indicators and wins. Fix options: segment default-emoji-presentation sequences in the engine (UTS #51; `icu_properties` is already in the graph) and give them an emoji-first stack, or upstream the same preference into parley. Proposed for PLAN 1.8. |
| Bidi (Hebrew, Arabic) | pass, with a finding | Both paragraphs resolve RTL, right-aligned, with Latin and digits as LTR islands. parley reports 17.28 cu (one space) of `trailing_whitespace` on an RTL line that has none; alignment uses the full advance, so glyphs land correctly, but the line's width reads one space short. Upstream issue candidate. |

**Recorded:** the URL breaks only after `/` and `?`, never inside a path segment. The unbreakable word overflows its 852 cu box by 748 cu, with no emergency break (E100's job, PLAN 1.15). The 268 cu column sets six ragged greedy lines. Optical margins and hanging punctuation other than quotes are not implemented (PLAN 1.8). Quotes did not hang either; that was a failure, fixed after 0.9 ("Hanging quotes").

**Cross-platform (gate 0 criterion 1, display-list half):** CI on arm64 macOS (`aarch64-apple-darwin`, PR #4) reproduced all 20 quantized goldens blessed on x86-64 Linux, byte for byte; the determinism test (fonts registered in reverse order) passed there too.

**Timings** (`cargo run --release -p scaena-engine --example torture_timing`):

| Stage | Measured | SPEC §15 |
|---|---|---|
| Register the 5 bundle fonts | 1.3–1.5 ms | (part of cold start) |
| First frame of a state, cold shaping caches | 0.3–0.8 ms (worst: `axes`, 7 text nodes) | resolve + layout ≤ 15 ms per snapshot (B1) |
| All 20 text states | 6.2–6.6 ms | all snapshots ≤ 400 ms (B1) |
| Display list → postcard | 3–4 µs | — |

Warm frames cost the same as cold ones because `frame()` lays out on every call; there is no per-snapshot layout cache yet. PLAN 0.10 needs one: frames must only sample (SPEC §5).

## 0.5 Cap-height and baseline alignment

Text aligns in its cell by its trimmed box or by a typographic anchor (SPEC §3.4, now precise). `box: cap` trims from the first line's cap height to the last baseline; `y: cap` / `y: x-height` put the first line's cap or x-height on the cell's top edge; `y: baseline` puts the last baseline on the cell's bottom edge. Metrics come from OS/2 via parley's run metrics (cap 710 / 1000 × 96 cu = 68.16 cu, as the table says).

- A new torture state, `anchors`, sets 112, 64, and 28 cu text in three rows aligned by cap, baseline, and x-height; every anchor lands within 0.001 cu of its line (210, 642, 780 cu). Test: `anchors_line_up_cap_heights_baselines_and_x_heights_across_sizes`.
- The golden diff for this task touched only layer transforms: the `case` label moved up 5.09 cu (its cap top now on the slot top) and the `box: cap` headline 12.62 cu. No glyph moved relative to its layer, which is the point of positioning glyphs in layer space.
- Fixed on the way: the 0.4 `LineBox.top` was parley's `block_min_coord`, the top of the *ascent* box, which sits above the line box when leading is tighter than ascent + descent (−8.21 cu for the 96 cu headline). It is now the CSS line-box top.
- Stricter determinism check: `raw_display_lists_are_bit_identical_across_platforms` digests the *unquantized* display lists (`tests/golden/torture/raw.fnv1a`, FNV-1a over postcard bytes) and gates on them. The contract only needs quantized equality; this shows whether quantization is margin or necessity, and catches platform-dependent float math before it crosses a rounding boundary. First arm64 result (PR #5's macOS job, `aarch64-apple-darwin`): it passed, so the unquantized display lists of all 21 states are bit-identical on x86-64 Linux and arm64 macOS. So far quantization is margin, not necessity.
- Not yet: centered and end-aligned text (`NotImplemented`, PLAN 1.8); a font without OS/2 cap height keeps its line edge (none in the torture deck).

## 0.6 CPU painter

`vello_cpu` paints display lists to PNG (`scaena_paint::cpu::CpuPainter`), and `scaena render --painter cpu` runs bundle → fonts → `Engine::frame` → painter → PNG, with `--display-list`, `--size`, and `--json` stage timings. States the engine or painter cannot draw yet exit 3, naming their PLAN task: chart (0.10), shader (0.11), `--painter gpu` (0.7).

- **Golden rasters.** One 1920×1080 PNG per torture state under `tests/golden/torture/` (1.45 MB for 21; 36–131 KB each), painted from the golden display lists so the test covers the painter alone. Compared with the SPEC §13.5 metric (`scaena_paint::diff`: Oklab ΔE × 100 over white; edges, a 4-neighbor step over 8/255 in either raster, masked with a 1-px dilation), never byte for byte. The test runs in 3.2 s in a debug build. The first version took 49 s, nearly all of it in the comparison's per-pixel closures; packed pixels, one pass per neighbor pair (cross-checked against the old code on 1,286 raster pairs), and one worker per core fixed that.
- **Visual review of all 21 rasters.** Every state draws what its notes predict, recorded failures included: the 🇺🇸 flag is two monochrome Garamond letters (0.4), the long compound overflows its 852 cu box by 748 cu silently (E100, PLAN 1.15), and the opening quote of `hanging` sits on the text edge. That last one was wrongly passed as recorded for PLAN 1.8. Jay failed it on review, since quotes always hang outside the margin; fixed after 0.9 ("Hanging quotes"). The review caught a fixture bug that every test missed: in `anchors`, "xheight 112" wrapped in its four columns, and the x-height row's ascenders ran into the baseline row. The math was right; the picture was illegible. The baseline row now sits in rows 4–5 (baselines at 642 cu), and the x-height specimens are `axe 112 / 64 / 28`. The test now also asserts one line per specimen and that the rows' line boxes don't overlap. One flaw got past this review at full size and turned up in 0.7: the ǰ in `combining` (catalogue above).
- **What changes pixels** (21 frames, 1080p, against the AVX2 u8 goldens):

| Variant | Identical frames | Pixels that differ | Max channel step | Max off-edge ΔE | Paint, ms/frame |
|---|---|---|---|---|---|
| AVX2, u8, release build (the goldens were painted in debug) | 21 / 21 | 0 | 0 | 0 | 11.2 |
| Scalar fallback, u8 | 18 / 21 | 3 in total | 1 | 0.00 | 12.6 |
| u8, `multithreading` (3 workers) | 21 / 21 | 0 | 0 | 0 | 13.9 |
| f32 pipeline (AVX2 or scalar) | 0 / 21 | ≈ 6,800 per frame | 3 | 0.30 | 18.5 |
| Unquantized display list (what `render` paints) | 0 / 6 sampled | 4,900–16,000 per frame | 5 | 0.32 | — |

  Nothing comes near the tolerance. The goldens stay at the host's best SIMD level with the u8 pipeline, single-threaded; ADR-0004 findings 1, 2, and 5 have the decisions. SPEC §13.4 says only comparisons round, so `render` paints the unquantized list, and its PNG sits within tolerance of the golden rather than on it. NEON: PR #6's macOS job (`aarch64-apple-darwin`) painted all 21 states byte-identical to these x86-64 AVX2 goldens, 0 pixels different in every state. With #5's unquantized display lists also bit-identical across the two platforms, the CPU path is bit-identical end to end on Linux x86-64 and macOS arm64, for this deck.
- **Hinting.** glifo (vello_cpu's text) hints glyph outlines by default, and vello does not. `CpuPainter` turns hinting off so the two painters can agree (ADR-0004 finding 5).

| Stage, cold `scaena render` (release, 21 states, 1080p) | Min | Median | Max | SPEC §15 |
|---|---|---|---|---|
| Read deck, theme, fonts | 1.4 | 1.7 | 3.4 | — |
| Register fonts | 1.3 | 1.5 | 1.7 | — |
| `Engine::frame` | 0.5 | 0.7 | 1.2 | ≤ 15 ms (B1, warm) |
| Paint (first paint, cold caches) | 15.3 | 18.7 | 21.9 | ≤ 12 ms (B1, warm, 8 threads) |
| PNG encode + write | 15.5 | 21.4 | 34.3 | — |
| **Total** | **34.5** | **43.6** | **58.6** | **≤ 300 ms cold (B1)** |

All in ms. Warm single-threaded paint is 11.2 ms per frame (table above), already inside the 8-thread budget on text frames, which is why `multithreading` stays off. PNG encoding at the default compression costs more than painting; if the video path (PLAN 1.21) needs the time back, that is where to take it. At 3840×2160 the `emoji` state paints in 85 ms and encodes in 70 ms, cold.

## 0.7 GPU painter

`vello` 0.10 on `wgpu` 29 paints the same display lists (`scaena_paint::gpu`). `scene()` builds the vello scene on any target, wasm32 included (for 0.8). `GpuPainter` renders it headless into an `Rgba8Unorm` texture and reads it back; vello unpremultiplies before storing, so the readback is straight RGBA like the CPU's. Both painters take their geometry from one set of conversions (paths, transforms, strokes, blend modes, colors), so they can differ only in rasterization. `scaena render --painter gpu` works in a CLI built with `--features gpu`; without the feature it exits 3 and says so. wgpu's native futures resolve on their first poll, so the painter needs no async runtime. Measured here on Mesa's lavapipe (`llvmpipe`, software Vulkan). CI runs the same tests on Metal (macOS) and lavapipe (Linux), with `SCAENA_REQUIRE_GPU=1`, so a missing adapter fails CI instead of skipping.

- **Parity with the CPU goldens** (21 states, SPEC §13.5): all pass. Interiors match exactly, except `emoji`, where COLRv1 gradients put 114 of 2.03 M compared pixels between ΔE 1 and 1.49 (0.006%, inside the 0.1% allowance). Edges differ on 5,209–27,265 pixels per frame (median 12,741), by at most 63/255 (`dlig`, `numerals`): the two anti-aliasing algorithms disagree at edges by design. A vertical edge at 0.75 coverage reads 191 on the CPU and 180 on the GPU.
- **A hole the tolerance missed.** The first GPU raster of `combining` had a near-white gap, 231/255 from the CPU, where the caron overlaps the j's dot. vello fills a whole glyph run as one path, and Roboto Serif draws the j's dot clockwise and the combining caron counter-clockwise, so their windings cancelled. `GpuPainter` now draws one run per glyph, as vello_cpu does, and `combining`'s worst pixel fell to 33/255 (ADR-0004 finding 6). The torture test passed while the hole was there: it sat on a glyph edge, inside the edge mask. SPEC §13.5 now also fails any pixel that differs by half the channel range or more. The hole does, anti-aliasing (≤ 63) does not, and a mutation that restores one run per op fails the test.
- **And a shaping flaw the 0.6 review missed.** The same crop shows the caron sitting on the j's dot in both painters. Roboto Serif has no precomposed ǰ (U+01F0) to compose to, and nothing in its GSUB swaps in the dotless j it carries. The combining-marks catalogue row above now says *partial*, pinned by `catalogue_j_with_caron_keeps_its_dot_because_the_font_cannot_drop_it`.
- **First Metal run** (macOS CI, "Apple Paravirtual device"): vello left colour in fully transparent pixels beside pixel-aligned edges, `[2, 1, 1, 0]` where the CPU has `[0, 0, 0, 0]`. vello unpremultiplies as rgb / max(a, 1e-6), so coverage below 1/255 survives as colour. That is invisible, but it would trip the new half-range step rule, which compares raw channels. `GpuPainter` now zeroes fully transparent pixels, and `diff` treats any two fully transparent pixels as equal.
- **Timings** are lavapipe's, a CPU emulating a GPU, so they say nothing about the SPEC §15 GPU budget (≤ 6 ms per 1080p frame); that needs a real GPU (gate 0 criterion 6 names an M-series Mac). Cold `scaena render --painter gpu`, release, 21 states: renderer start-up (adapter, device, vello pipelines) 193–263 ms, paint 136–340 ms, total median 430 ms. The very first render in a fresh container took 2.5 s to paint: lavapipe compiles vello's pipelines on first use and caches them on disk (`MESA_SHADER_CACHE_DISABLE=true` brings the 2.5 s back every time).

## 0.8 WebGPU page

`scaena-wasm` exports two things.
- `Player`: deck, theme, and fonts in; `frame(state, t)` returns the postcard display list.
- `Canvas`: `attach(canvas)` sets up the WebGPU device, the vello renderer, and a storage-texture target blitted to the canvas. `player.paint(canvas, state, t)` draws through `scaena_paint::gpu::scene`, the scene the native GPU painter renders.

`crates/scaena-wasm/www/index.html` (43 lines) loads a bundle and offers a state picker. `www/smoke.mjs` checks every torture state in headless Chromium through Playwright; it runs as `just wasm-smoke` and as a CI job.

- **The engine is bit-identical in WASM.** All 21 text states' display lists, built in the browser, hash to the native digests (`raw.fnv1a`, FNV-1a over unquantized postcard bytes). x86-64 Linux, arm64 macOS, and wasm32 produce the same bytes, and CI rechecks on every push.
- **WebGPU paints every state.** Read back from the canvas, every frame is opaque and has ink on it. Against the CPU goldens all 21 pass SPEC §13.5. The worst step is 64/255, and `emoji` has 115 pixels over ΔE 1 (at most 1.49). That is within a pixel of native lavapipe's numbers, here on Chromium's SwiftShader. 0.9 turns this into the parity harness.
- **Size** (wasm-bindgen output, no fonts; the gate 0 budget is ≤ 3.0 MB gzip):

| Module | opt-level | Raw | gzip -9 | brotli 11 |
|---|---|---|---|---|
| engine + vello + wgpu (WebGPU) | 3 (release) | 3.18 MB | 1.04 MB | 0.74 MB |
| same | `s` | 2.99 MB | 0.92 MB | 0.67 MB |
| same | `z` | 2.86 MB | 0.87 MB | 0.64 MB |
| engine alone (`--no-default-features`) | 3 | 1.47 MB | 0.53 MB | 0.40 MB |
| engine alone | `z` | 1.22 MB | 0.43 MB | 0.34 MB |

  The JS glue adds 67 KB raw, 10 KB gzipped. `wasm-opt -Oz` (binaryen 132) cuts the raw size by 8–11% but grows the gzip and brotli payloads by up to 4%: it removes redundancy the compressor was already exploiting. So the build ships without it, and PLAN's risk register no longer counts on it. The levers are opt-level and leaving painters out of a module.
- **Timings** (headless Chromium, this 4-vCPU container, SwiftShader):
  - The first frame is submitted 202 ms after navigation. That covers fetching the module and five fonts, compiling, WebGPU and vello pipeline setup, layout, and paint. SPEC §15's cold-start budget is 500 ms, on B1.
  - `frame()` in WASM takes 0.9–3.6 ms per state (median 1.3), about 2–4× native.
- **A headless WebGPU trap.** Chromium's headless shell grants a WebGPU adapter by default but leaves WebGPU canvases blank. Even a pure-JS clear reads back transparent, with only "A valid external Instance reference no longer exists" in the console. With `--enable-features=Vulkan --use-vulkan=swiftshader --use-angle=swiftshader --enable-unsafe-swiftshader` the canvas presents. The first smoke check passed on blank canvases because it trusted "no exception". It now reads every frame back and requires opaque pixels with ink on them, and the page sends GPU errors and panics to the console instead of dropping them.
- **Not yet:** Firefox and Safari (PLAN 2.1); the `vello_cpu` → `ImageBitmap` fallback where WebGPU is missing (SPEC §9.2); running in a Worker on an `OffscreenCanvas` (PLAN 2.1).

## 0.9 Parity harness

`crates/scaena-paint/tests/parity.rs` compares every torture state across three painters, pairwise, with the SPEC §13.5 metric:
- `cpu`: vello_cpu, the goldens;
- `gpu`: vello on this machine's adapter;
- `web`: vello on WebGPU in Chromium, read back by the 0.8 smoke check.

A failing pair writes a diff image (`scaena_paint::diff::image`) beside the goldens, and CI uploads it as an artifact. The image shows the reference faded to grey, compared pixels over ΔE 1 in red, half-range steps in magenta, and tolerated differences in blue. `just spike` runs all three painters. CI runs `cpu`–`gpu` on Metal (macOS) and lavapipe (Linux) on every push, and all three pairs in the `wasm` job.

Linux, with lavapipe as `gpu` and Chromium's SwiftShader as `web`: all 21 states pass every pair.

| Pair | Max step (state) | Compared px over ΔE 1 | Why |
|---|---|---|---|
| cpu–gpu | 63 (`dlig`, `numerals`) | 114, all in `emoji` (ΔE ≤ 1.49) | different anti-aliasing; COLRv1 gradients |
| cpu–web | 64 (`numerals`) | 115, all in `emoji` | as cpu–gpu |
| gpu–web | 10–16 | 0 | the same vello shaders on two drivers |

Pixels differ along the CPU/GPU line, not across platforms: two GPU drivers running vello agree within 16/255, while either one differs from vello_cpu by up to 64 at glyph edges. Mutation check: swapping one browser readback for another state's fails `liga` (27,056 pixels over ΔE 1, step 231) and writes the diff image.

macOS CI on Metal ("Apple Paravirtual device", PR #9): cpu–gpu passes all 21 states with the same worst step per state as lavapipe (63 in `dlig` and `numerals`; 114 px over ΔE 1, all in `emoji`), and the NEON CPU rasters there are byte-identical to the AVX2 goldens. The CI `wasm` job ran all three pairs green on its first run.

## Hanging quotes (review after 0.9)

Jay's review of the `hanging` raster: the opening “ sat on the text edge. That is a failure, not a PLAN 1.8 item. Quotes always hang outside the margin, in every role. SPEC §3.5 now says so; the 0.6 review had passed it as recorded.

- **What hangs.** A line's hang is the advance of the quotation marks that open it (Unicode `Quotation_Mark`, less the CJK corner brackets and fullwidth forms), set in the paragraph's direction so they sit on its start edge. A left-to-right line moves left by its hang. A right-to-left line is broken in a box its hang wider than the measure, and parley's start alignment puts the quote past the right edge (ADR-0004 finding 8).
- **Breaking measures the line without it** (CSS `hanging-punctuation`). Greedy breaks line *k* at the measure plus its hang. A line's hang depends on where it starts, which depends on the lines above, so each pass reuses the hangs the previous one found. Line *k* breaks right once line *k* − 1 has, so the passes settle within one per line. Text where no quote opens a line breaks in one pass, exactly as before. `pretty` gives each line the measure plus its first segment's hang; `balance` bisects over the hang-aware greedy.
- **The fixture now tests it.** The old `hanging` text never opened a later line with a quote, and it broke the same with or without hanging. The new text opens lines 1 and 3 with “ and ‘, and line 1 fits only because its quote is not measured: 985.3 cu inside a 998 cu measure, 1009.0 with the quote, at least 11 cu of margin either way. Its other lines start with V and W, for optical margins (recorded).
- **Tests.**
  - `kill_quotes_that_open_a_line_hang_outside_the_text_edge`, on the fixture: which lines hang; line 1's fit; the quote at −hang and the next glyph at 0; and `specimen`, a role with no hanging settings, hangs too.
  - `every_breaking_measures_a_line_without_the_quote_it_hangs`: greedy, pretty, and balance each keep "‘quoted’ words" whole at a measure it fits only when hung.
  - `a_right_to_left_line_hangs_its_opening_quote_past_the_right_edge`.
  - Mutation-checked: with no quote recognized, or with breaking ignoring the hang, all three fail.
- **Goldens.** Only `hanging` and `punctuation` moved, the only text nodes a quote opens. The other 19 states' display lists, digests, and rasters are unchanged. WASM display lists hash to the new digests, and all 21 states pass every painter pair (lavapipe, SwiftShader).
- **Not yet (PLAN 1.8):** optical margins (T V W A); hanging stops, commas, hyphens, and brackets (`hangingPunctuation`); closing quotes at an aligned end edge, which arrive with end-aligned and justified text.

## 0.10 Chart data motion

*Revised on review.* The first version morphed bars into a line by key. Jay rejected it: nobody morphs a column chart into a line. Decks move data: values animate in, one period gives way to the next, growth shows. This section is the revision. A change between kinds that draw different marks, bars to a line, now cross-fades (SPEC §3.7).

Charts compile to keyed marks: one-series `bar` charts, from data files the caller hands over (the engine still reads no files). Each snapshot lays out once into a `Scene`. `Engine::transition` lays out a state and the one before it, and `Transition::frame(t)` samples them. It takes `&self` and holds no fonts or layout engine, so a frame cannot lay out: gate 0 criterion 3 holds by construction, not by measurement.

- **Values in.** `chart-intro` shows the case label alone. `chart` adds the chart with a 420 ms transition in the theme's standard easing. Every bar grows from the baseline. Every value label rides its bar's top and counts up from 0 as it fades in. The quarter labels and the baseline fade in.
- **The next quarter.** `chart-next` swaps in the next window of data, 2025-Q2 through 2026-Q3. Marks match by key, so the five quarters that stay slide one band left, and they rescale as the axis grows from 31 to 38. A removed key shrinks onto the baseline and an added key grows from it, each moving with its nearest matched neighbor. So the window scrolls: 2025-Q1 rides out under the chart's left edge counting down, while 2026-Q3 rides in from the right counting up. The chart clips at its cell's sides, and only there. Vertically it draws on the whole canvas, so a figure that overshoots the cap height keeps its top. The first rework left the leaving and arriving bars in place, and the review frames showed them overlapping their sliding neighbors. Riding along fixed that.
- **Counting without shaping.** A counting label is composed each frame from figures shaped once per snapshot, so frames still never shape. That works because tabular figures share one advance and do not kern. Found: Roboto Serif sets a different period between two figures (glyph 402) than alone (glyph 310), so the figures are shaped in context: "0123456789", "0.0", "0,0", and "-0". A test spells 0–200 and eight signed and decimal samples both ways and requires the same glyphs, within 1/1000 cu.
- **The four sample points.** For both transitions, t = 0 is the previous state at rest and t = 420 ms is the state at rest, byte for byte: both ends draw a scene, not an interpolation. t = 0.25 and t = 0.5 have goldens. Tests check each bar against its key's interpolation at the eased progress, and the leaving and arriving bars against their neighbors' displacement. They also check every counting label's glyphs and alpha. Mutation-checked: pairing marks by position instead of by key fails, and so does dropping the ride-along.
- **Determinism.** All 28 golden frames hash the same in native and WASM. New rule, SPEC §13.7: transcendental math in the render path goes through `libm`, whose pure-Rust functions give the same bits everywhere, because `std`'s float methods call the platform math library. Oklab color interpolation uses `libm`'s `cbrt` and `pow`. Rounded corners are arithmetic Béziers, not kurbo's arcs, which call `sin` and `cos`. A counting label's number rounds exactly, through `f64::round` or Rust's fixed-precision formatting, so it reads the same everywhere. Found: `Spring::position` still calls `std`'s `exp`, `sin`, and `cos`, and must move before springs drive frames (PLAN 1.11).
- **Parity.** The seven chart frames pass every pair with 0 compared pixels over ΔE 1. The worst step is 8/255 for cpu–gpu (lavapipe) and 9–13 for the pairs with WebGPU (SwiftShader). These are the first goldens with a clip, and the transition frames also hold isolated layers (labels fading in and out). All three painters agree on both.
- **Timings.** Release build on the x86-64 container: laying out the next-quarter transition takes 1.1 ms. A frame samples in 3.6 µs, the mean of 2,000 frames across the 420 ms, counting included. A mid-transition display list is 2,888 bytes as postcard. In WASM (Chromium, SwiftShader), the first frame inside a transition takes 2.8 ms, because it lays out both states; the next takes 0.2 ms.
- **Decisions.** A change of chart kind morphs only between kinds that draw the same marks, bars that regroup (`bar` ↔ `stackedBar`); bars to a line cross-fades (SPEC §2.3, §3.7). A state without `transition` cuts. The transition into a state starts from the state before it in the cue list, whichever state it tracks `from` (SPEC §3.9). Phase 0 charts are one-series bars with the first categorical color, `labels.show` (`all` | `ends` | `none`), and default axes (SPEC §3.7). Everything else is NotImplemented, pointing at the chart and table sprint (PLAN 1.9), where Jay sets the aesthetic.
- **Found in SPEC.** The chart `axes` object in SPEC §3.7 collides in the schema with text `axes` (variable-font axes, numbers only), so a chart cannot declare axes settings. Proposed fix: type node props per node type in PLAN 1.1.

## 0.11 Mesh shader

A shader op carries parameters, not pixels (SPEC §6). Each kind's CPU reference and its WGSL twin live side by side in `scaena-core::shader`, because both painters run them and painters depend only on core. SPEC §12 had put them in the engine, which `scaena-paint` cannot depend on. That is corrected.

- **The mesh.** `points` palette colors sit at seeded, evenly spread places (the R2 sequence) and drift on slow Lissajous paths. Each pixel blends them in Oklab, weighting a point at distance d by 1/(1 + d²/σ²)². `softness` sets σ, and `grain` adds seeded noise to Oklab lightness per device pixel. The kernel is rational rather than Gaussian, so the per-pixel function needs no `exp`.
- **Parity by construction.** The uniforms (point positions and Oklab colors) are computed once per frame in Rust, with `libm` for `sin`, `cos`, `cbrt`, and `pow`, and both sides read the same `f32` bytes. Per pixel, the CPU and the WGSL do the same `+ − × ÷` in the same order. sRGB encoding was the risky step, since a GPU's `pow` is not correctly rounded. So both sides compare against one table of the 255 linear values halfway between sRGB bytes, eight comparisons per channel. The WGSL writes its RGBA bytes as integers into a buffer, avoiding a float-to-unorm conversion whose rounding differs between backends.
- **The parity test.** `shader_parity` runs seven cases: seeds, times up to an hour, 2–16 points, heavy grain, a translucent palette, and 2× and rotated transforms. On lavapipe, six are bit-identical, and the rotated one differs in 4 pixels by one step. On Metal (PR #12's macOS job, Apple Paravirtual device), every case is within one step: 4–87 pixels per case differ, by at most ΔE 0.32.
- **Drawing it.** The CPU painter computes the reference over the device pixels the rect covers. It fills the rect with that image, nearest-sampled and placed texel for pixel. The GPU painter dispatches the WGSL as a compute pass into a buffer and copies that into a texture. It hands the texture to vello with `register_texture` (vello 0.10), so there is no readback, natively or in the browser. A test paints one op both ways: the CPU raster equals the reference byte for byte, and the GPU raster is within one step of it.
- **Clock.** A shader's `t` is the frame's time on the global timeline: the cue list's transitions laid end to end (SPEC §2.2; Phase 0 has no holds). A background that persists across states drifts on through each transition instead of restarting. A state at rest shows the time it comes to rest; the torture deck's `mesh` rests at 0.84 s, after the two chart transitions before it.
- **The torture state.** `mesh` passes every painter pair (cpu, lavapipe, SwiftShader WebGPU) with 0 pixels over ΔE 1. The worst step is 2–3/255, at the case label's glyph edges. Its WASM display list hashes the same as native. On Metal it is within one step of the CPU, and the NEON CPU raster is byte-identical to the x86-64 golden: the reference is plain `f32` arithmetic and pure-Rust `libm`, so it reads the same on both architectures. Gate 0 criterion 4 holds on Linux and on Metal.
- **Timings.**
  - Release build on the x86-64 container. The CPU reference takes about 115 ms single-threaded for a 1080p mesh, and a cold `scaena render` of `mesh` takes 166–182 ms.
  - Found: grain is noise that deflate cannot shrink. The CLI's balanced PNG compression spent 1,020 ms on that frame, which alone broke the 300 ms cold-render budget.
  - `scaena render` now writes fast-compressed PNGs: 23 ms for the mesh. Text states now render cold in about 33 ms (was 62), and files come out 1.5–2.5× larger.
  - Goldens keep balanced compression. The mesh golden is 2.6 MB, more than the other 28 rasters together. Grain at that amplitude carries about 3 bits per channel, so no encoder makes it much smaller.
  - The WASM module grew from 1.04 to 1.08 MB gzip with charts and shaders.
- **Decisions.**
  - The engine types the params as it resolves the node, so a bad one is the document's error and names the node. The JSON schema types them with the rest of node props in PLAN 1.1.
  - A shader whose kind, seed, palette, or params change between states cross-fades. Uniform interpolation is PLAN 1.12.
  - Grain is per device pixel, so a 2× render has finer grain than a 1× render. The viewport is a render input (SPEC §1, principle 1).
- **Not yet.** The CPU reference is single-threaded; its rows are independent, so threads would divide it. A shader rect wider than 8,192 device pixels overflows vello's image atlas and will need tiling.

## 0.13 Authorability

The second hypothesis held: semantic text documents are naturally agent-authorable. Five fresh agents made PLAN 0.13's five edits to one deck, each knowing the format only from SPEC, the schemas, the examples, the skills, and CLI help. All five validate and lint clean, every patch is local, and identity survives, so gate 0 criterion 7 is met. The record is `docs/examples/agent-authorability.md`. The bundle, the agents' transcript, and the deck after each edit are in `docs/examples/authorability/`.

- **The edits were small because the document is semantic:**
  - The data roll repointed one data source by name.
  - The retheme swapped one file.
  - The headline was one node default plus its beat's claim.
  - The build added a state and trimmed one.

  Tracking did the rest. In every edit, every state outside its reach resolved exactly as before.
- **The agents checked their own work.** After `validate` and `lint`, `inspect` and `diff` were their main tools. `inspect` caught the one real trap: object values merge across states, which SPEC did not mention.
- **Clean is a low bar today.** `validate` and `lint` read nothing in the bundle besides the deck. All of these pass:
  - a missing data file;
  - a chart field that is not in the data;
  - a theme without the slots a deck uses;
  - a theme whose ink is the color of its paper.

  The judge checked theme names and resolved snapshots with `scripts/judge_edit.py`, and recomputed every figure from the data. Nothing checks that a deck renders. This one lists two of its theme's three font families, as two agents decided on purpose, and `render` refuses every state until the third is added.
- **Copy is literal.** Rolling the chart forward to the next quarter was one field. The words took 22 edits, and lint stayed clean over a headline the new data made false. SPEC §16 question 9 proposes figures bound to data.
- **Contrast over a shader, measured.**
  - The agents estimated the Dusk cover's title at 2.3–2.5:1 over the mesh's orange stop.
  - Rendered and compared with the mesh beneath it, glyph pixel by glyph pixel, the minimum is 3.41:1. That passes the 3:1 display bar by 0.4, at rest.
  - E111 should make that same measurement, against the shader's CPU reference (PLAN 1.15).
- **SPEC fixed where it was silent about what the code does:**
  - object values merge one level, and `null` deletes;
  - placement precedence and the built-in slots;
  - a re-entering node starts from its node defaults;
  - how a build names its slide;
  - the scope of id uniqueness;
  - `dataTransform` and `axesSpec`.

  Thirteen open findings point at their Phase 1 tasks, the chart sprint (1.9) and lint (1.15) foremost.

## 0.14 Timings (SPEC §15)

**B1 is new.** `tests/bench/b1.scaena` is SPEC §15's text-heavy benchmark: the manifesto as a talk.
- **Content:** 40 states over 11 text nodes, no charts. Builds add the argument, then a line to remember, so 39 of the 40 transitions move and cross-fade text.
- **Fonts:** Fraunces, Inter, JetBrains Mono, and Source Serif 4, 1.1 MB as subsets. `scripts/build_bundle_fonts.py` builds them from the torture fonts' pinned google/fonts commit.
- **Kept honest:** `crates/scaena-engine/tests/bench.rs` keeps B1 at 40 states in four fonts, with every state drawing.

B4 is the torture deck: 25 states, five fonts, a bar chart, and a mesh.

**How it was measured:**
- `crates/scaena-cli/examples/stages.rs` times every SPEC §15 stage that Phase 0 has, in one process per deck. Each state's figure is the median of several runs; the table gives the median and the worst over states.
- The cold render is the whole `scaena render` process, started once per state.
- `crates/scaena-wasm/www/coldstart.mjs` times WASM to the first frame in fresh headless-Chromium contexts.
- `just bench` runs all of it. The `bench` workflow runs the native stages on CI's Apple Silicon runner, on main and on every change to the bench files.

**B1**

| Stage | Linux container: median · worst | Apple Silicon runner: median · worst | SPEC §15 budget |
|---|---|---|---|
| Load bundle (deck, theme, 4 fonts 1096 KB) | 0.58 ms | 0.22 ms | cold start |
| Register and check fonts | 0.43 ms | 0.05 ms | cold start |
| Resolve + layout one snapshot, first pass (fresh engine) | 0.48 ms · 0.85 ms (`belief-5-line`) | 0.36 ms · 16.56 ms (`cover`) | — |
| Resolve + layout all 40 snapshots, first pass | 19.26 ms | 33.55 ms | ≤ 400 ms (B1) |
| Resolve + layout one snapshot, warm | 0.46 ms · 0.83 ms (`belief-6-line`) | 0.36 ms · 1.04 ms (`belief-1-line`) | ≤ 15 ms (B1) |
| Resolve + layout all 40 snapshots, warm | 18.45 ms | 18.13 ms | ≤ 400 ms (B1) |
| Build a transition (lay out both ends), 39 transitions | 0.71 ms · 1.98 ms (`belief-6-why`) | 0.50 ms · 2.14 ms (`belief-2-line`) | — |
| Sample one frame (interpolate laid-out geometry) | 1.0 µs · 2.2 µs (`belief-6`) | 1.5 µs · 11.4 µs (`last-word`) | ≤ 1 ms (B1, B2) |
| CPU paint, one frame at 1080p | 19.66 ms · 23.50 ms (`belief-8`) (vello_cpu, Avx2, 1 thread) | 5.84 ms · 11.99 ms (`belief-8-line`) (vello_cpu, Neon, 1 thread) | ≤ 12 ms with 8 threads (B1) |
| PNG encode (fast compression) | 3.92 ms · 5.25 ms (`goal-bar`) | 3.56 ms · 7.94 ms (`refuse-2`) | — |
| GPU paint + readback, one frame at 1080p | 21.45 ms · 28.00 ms (`belief-2-line`) (llvmpipe (LLVM 20.1.2, 256 bits), Vulkan) | 21.34 ms · 66.99 ms (`belief-4-why`) (Apple Paravirtual device, Metal) | ≤ 6 ms, paint alone (B1) |
| Validate + document-level lint | 0.50 ms | 0.29 ms | ≤ 100 ms (B1) |
| Headless PNG render, cold (`scaena render`, whole process) | 32.64 ms · 42.16 ms (`refuse-3`) | 32.59 ms · 48.71 ms (`belief-5-why`) | ≤ 300 ms (B1) |

**B4, the torture deck**

| Stage | Linux container: median · worst | Apple Silicon runner: median · worst | SPEC §15 budget |
|---|---|---|---|
| Load bundle (deck, theme, 5 fonts 2347 KB) | 0.62 ms | 0.41 ms | cold start |
| Register and check fonts | 0.20 ms | 0.08 ms | cold start |
| Resolve + layout one snapshot, first pass (fresh engine) | 0.39 ms · 0.84 ms (`axes`) | 0.42 ms · 1.69 ms (`axes`) | — |
| Resolve + layout all 25 snapshots, first pass | 10.67 ms | 12.77 ms | ≤ 400 ms (B1) |
| Resolve + layout one snapshot, warm | 0.34 ms · 0.60 ms (`chart`) | 0.20 ms · 0.37 ms (`chart`) | ≤ 15 ms (B1) |
| Resolve + layout all 25 snapshots, warm | 9.26 ms | 5.64 ms | ≤ 400 ms (B1) |
| Build a transition (lay out both ends), 2 transitions | 0.90 ms · 1.07 ms (`chart-next`) | 0.53 ms · 0.65 ms (`chart-next`) | — |
| Sample one frame (interpolate laid-out geometry) | 3.8 µs · 3.9 µs (`chart`) | 4.9 µs · 4.9 µs (`chart`) | ≤ 1 ms (B1, B2) |
| CPU paint, one frame at 1080p | 19.69 ms · 139.65 ms (`mesh`) (vello_cpu, Avx2, 1 thread) | 4.37 ms · 97.42 ms (`mesh`) (vello_cpu, Neon, 1 thread) | ≤ 12 ms with 8 threads (B1) |
| PNG encode (fast compression) | 3.51 ms · 21.02 ms (`mesh`) | 2.52 ms · 20.91 ms (`mesh`) | — |
| GPU paint + readback, one frame at 1080p | 20.13 ms · 112.56 ms (`mesh`) (llvmpipe (LLVM 20.1.2, 256 bits), Vulkan) | 23.63 ms · 61.58 ms (`bidi-hebrew`) (Apple Paravirtual device, Metal) | ≤ 6 ms, paint alone (B1) |
| Validate + document-level lint | 0.43 ms | 0.38 ms | ≤ 100 ms (B1) |
| Headless PNG render, cold (`scaena render`, whole process) | 33.45 ms · 168.78 ms (`mesh`) | 26.19 ms · 125.76 ms (`mesh`) | ≤ 300 ms (B1) |

**In the browser.** Headless Chromium with SwiftShader WebGPU, on the Linux container; five cold starts per deck, each in a fresh context with nothing cached:

| Stage | B1 | B4 | SPEC §15 budget |
|---|---|---|---|
| WASM load → first frame at 1080p | 162 ms median | 189 ms median | ≤ 500 ms (B1) |
| WASM engine size (gzip, no fonts) | 1.08 MB | — | ≤ 3.0 MB |

Reading them:
- **The gated number has room.** On Apple Silicon, B1's cold render is 32.6 ms median and 48.7 ms worst, against 300 ms.
- **Every other budget Phase 0 can measure is met on the Apple Silicon runner, except GPU paint.** CI cannot measure that one (below).
- **Layout is not the cost.** A B1 snapshot lays out warm in 0.36 ms on Apple Silicon (budget 15 ms), and all 40 in 18 ms (budget 400 ms). A transition frame samples in 1.5 µs (budget 1 ms).
- **The one slow first frame is set-up, not layout.** On Apple Silicon, `cover` takes 16.6 ms as a fresh engine's first frame. On Linux, where the binary was already warm, no first frame exceeds 1 ms.
- **CPU paint on one thread meets the 8-thread budget on Apple Silicon:** 5.8 ms median, 12.0 ms worst. The Linux container is two to three times slower and varies between runs: 14.2 and 19.7 ms medians on two runs.
- **GPU numbers bound nothing yet.** Both include submit, wait, and readback through a device that is emulated (lavapipe) or paravirtualized (CI's Metal). SPEC's 6 ms budget is for paint alone, and measuring it needs a physical M-series Mac and a paint-only timer.
- **B4's worst cases are the mesh.** Its CPU reference paints in 97 ms on Apple Silicon, and its cold render takes 126 ms, inside the 300 ms render budget.
- **Not measured, because the stages do not exist yet:**
  - layout-level lint (PLAN 1.15);
  - the MCP round trip (1.17);
  - video export (1.21);
  - decks B2 and B3 (1.9, 1.10).

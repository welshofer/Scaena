# Phase 0 spike report

**Status:** in progress; PLAN 0.14 completes it with the go/no-go. Each section is written when its task lands.
**Numbers:** Linux dev container (Intel Xeon @ 2.10 GHz, 4 vCPU), release builds. SPEC §15's reference machine is an M-series Mac and is not measured yet; Linux numbers are recorded, not gated.

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

**Gap measured (greedy vs ours):** headline line ratio 0.32 → 0.99; paragraph lines 1699.6 / 1610.4 / 159.8 → 1616.8 / 1661.6 / 191.3 cu. parley's widths agree with the HarfBuzz measurements made for 0.2 within 0.5 cu (1641.5 vs 1642; 521.4 vs 521).

**How breaking works.** `balance` bisects (24 steps) for the narrowest width that keeps greedy's line count. `pretty` asks parley's own breaker for every UAX #14 opportunity (breaking at ~0 width puts one segment per line, with exact widths with and without trailing space), plans minimum raggedness over those segments with the last line held to `minLastLineWords`, realizes the plan with one width per line, and checks parley broke where planned (falling back to greedy, and saying so, if not). Both fall back to greedy for right-to-left paragraphs: parley aligns per-line-width breaks inside those widths, which puts RTL lines at the wrong edge.

**Catalogue:**

| Case | Result | Note |
|---|---|---|
| Discretionary ligatures | pass | 35 glyphs with `dlig`, 44 without. |
| Combining marks | pass (layout) | NFD sequences shape in Roboto Serif with no `.notdef`; mark placement is judged on rasters (0.6). |
| Emoji | **fail** | 🚀 🙂 👩🏽‍💻 ❤️ are one COLRv1 glyph each, but 🇺🇸 is two EB Garamond glyphs. Root cause (parley 0.11 `shape/mod.rs`): an emoji cluster queries the style's own stack first and appends the generic Emoji family last; EB Garamond covers both regional indicators and wins. Fix options: segment default-emoji-presentation sequences in the engine (UTS #51; `icu_properties` is already in the graph) and give them an emoji-first stack, or upstream the same preference into parley. Proposed for PLAN 1.8. |
| Bidi (Hebrew, Arabic) | pass, with a finding | Both paragraphs resolve RTL, right-aligned, with Latin and digits as LTR islands. parley reports 17.28 cu (one space) of `trailing_whitespace` on an RTL line that has none; alignment uses the full advance, so glyphs land correctly, but the line's width reads one space short. Upstream issue candidate. |

**Recorded:** the URL breaks only after `/` and `?`, never inside a path segment. The unbreakable word overflows its 852 cu box by 748 cu, with no emergency break (E100's job, PLAN 1.15). The 268 cu column sets six ragged greedy lines. Hanging punctuation and optical margins are not implemented (PLAN 1.8).

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
- **Visual review of all 21 rasters.** Every state draws what its notes predict, recorded failures included: the 🇺🇸 flag is two monochrome Garamond letters (0.4), the long compound overflows its 852 cu box by 748 cu silently (E100, PLAN 1.15), and nothing hangs yet (PLAN 1.8). The review caught a fixture bug that every test missed: in `anchors`, "xheight 112" wrapped in its four columns, and the x-height row's ascenders ran into the baseline row. The math was right; the picture was illegible. The baseline row now sits in rows 4–5 (baselines at 642 cu), and the x-height specimens are `axe 112 / 64 / 28`. The test now also asserts one line per specimen and that the rows' line boxes don't overlap.
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

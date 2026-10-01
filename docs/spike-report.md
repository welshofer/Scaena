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

22 states in `tests/fixtures/torture.scaena/` (case table in its README): 10 kill, 5 catalogue, 7 recorded. Five OFL fonts subset deterministically from a pinned google/fonts commit (emoji 24.7 MB → 28 KB, COLRv1 only). Balance and widow baits were measured with HarfBuzz 14.5 to fail under greedy breaking with ±15 cu of slack.

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

**Timings** (`cargo run --release -p scaena-engine --example torture_timing`):

| Stage | Measured | SPEC §15 |
|---|---|---|
| Register the 5 bundle fonts | 1.3–1.5 ms | (part of cold start) |
| First frame of a state, cold shaping caches | 0.3–0.8 ms (worst: `axes`, 7 text nodes) | resolve + layout ≤ 15 ms per snapshot (B1) |
| All 20 text states | 6.2–6.6 ms | all snapshots ≤ 400 ms (B1) |
| Display list → postcard | 3–4 µs | — |

Warm frames cost the same as cold ones because `frame()` lays out on every call; there is no per-snapshot layout cache yet. PLAN 0.10 needs one: frames must only sample (SPEC §5).

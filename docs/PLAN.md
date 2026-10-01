# Scaena — Build Plan

**Status:** Draft 0.1 — 2026-10-01
**Companion:** `docs/SPEC.md` (what), `docs/adr/` (why). This file is *when* and *in what order*.

Durations are a solo-builder-with-agents estimate, not a promise. Phases are gated: do not start Phase N+1 work until Phase N's exit criteria are met and recorded in `docs/PLAN.md` under "Gate log". The only exception is research spikes that de-risk a later phase; mark them as such.

Checkboxes are the live task list. Claude Code: when you finish a task, tick it, run the gate check (`/gate`), and commit.

---

## Phase 0 — The spike that can kill the project (≈ 2 weeks)

**Question answered:** Can the Linebender stack deliver pixel-identical typography across the CPU painter and WebGPU, and is the display-list pipeline sound?

**Scope:** one crate chain (`core → engine → paint → cli`), one fixture deck, no DSL, no CRDT, no charts beyond one bar/line morph, no theme cascade beyond what the fixture needs.

### Tasks

- [x] 0.1 Pin the stack: add `parley`, `fontique` (bundle-only font source), `taffy`, `peniko`, `kurbo`, `vello_cpu`, `vello` to `scaena-engine`/`scaena-paint`. Confirm `cargo check` on macOS + Linux. Record versions in ADR-0004. *(Done: CI checks Linux + macOS, the `gpu` feature, and wasm32, all `--locked`. `swash` dropped — `parley` 0.11 shapes with `harfrust` and reads fonts with `skrifa`; versions, feature choices, and four findings for 0.2/0.5/0.6/0.9 are in ADR-0004.)*
- [x] 0.2 **Typography torture deck** `tests/fixtures/torture.scaena/` (also benchmark B4). Phase 0 is adversarial: we want parley/harfrust to fail now, not in month five. Three fonts bundled (one variable with `wght`+`opsz`+`wdth`; one with discretionary ligatures; one as fallback for a script the first two lack). States covering: standard and discretionary ligatures; kerning-sensitive pairs (AV, To, LT, f-ligatures); tabular vs proportional and lining vs oldstyle numerals; accented Latin and combining marks; smart quotes, em/en dashes, ellipsis; mixed weights and sizes within one line; very tight and very loose tracking; hanging punctuation and optical margins; a two-line `balance` headline; a `pretty` paragraph with a bait widow; deliberately ugly wrap cases (one long unbreakable word, a URL, narrow measure); emoji; a Hebrew and an Arabic line with Latin inlined (bidi); mixed-font fallback within one run; a bar chart with 6 bars; a mesh background. Each case is its own state so the parity harness reports per case.
  Kill criteria (must pass for gate 0): variable axes, standard ligatures, kerning, numeral styles, accented Latin, bundle-internal fallback, mixed weights, tracking extremes, balanced headline, widow avoidance, hanging quotes (added on review after 0.9). Catalogue only (recorded in the report, not gating): combining marks, emoji, bidi, discretionary ligatures.
  *(Done: 22 states, case table in the bundle README. Roboto Serif, EB Garamond, Noto Sans Hebrew fill the three roles; Noto Sans Arabic and Noto Color Emoji (COLRv1) exist only for the bidi and emoji catalogue cases. Added an explicit `axes` state for the variable-axes kill criterion. Quotes/dashes, hanging punctuation, and the three wrap cases have no class above, so they are recorded, not gating. Balance and widow baits are verified to fail under greedy breaking with HarfBuzz.)*
- [x] 0.3 `DisplayList` type (SPEC §6) with JSON + postcard encodings and a stable ordering. *(Done: one type, both encodings via `is_human_readable`; stateless ops with layer-scoped transform/clip; a font table and normalized coords exactly as the painters take them; `to_golden_json` puts one op and one glyph per line (layout pinned by a test); `paint_order` is z then scene-graph order; encoders refuse non-finite numbers. `quantize` fixed: it rounded transform linear parts to 1/64, which erased small rotations.)*
- [x] 0.4 Text layout: `parley` → glyph runs with final positions; OpenType features; `wrap: balance` for the headline, `wrap: pretty` for the paragraph (greedy fallback acceptable in spike, but measure the gap). *(Done: 20 torture states render to golden display lists; every kill case passes at the layout level with a test per case. `balance` bisects for the narrowest width keeping greedy's line count; `pretty` plans minimum raggedness over parley's own break opportunities and holds the widow rule, so no greedy fallback was needed (both fall back for RTL). Catalogue: the emoji flag fails (root cause in parley's emoji fallback order); measurements in docs/spike-report.md. *Review after 0.9:* the opening quote in `hanging` sat on the text edge; Jay failed it, since quotes always hang outside the margin. Now every quotation mark that opens a line hangs, in every role, and greedy, balance, and pretty all measure the line without it (SPEC §3.5). `hanging` is a kill case. Spike report, "Hanging quotes".)*
- [x] 0.5 Cap-height/baseline alignment from font metrics; `box: "cap"` trimming. *(Done: `y: cap | x-height | baseline` and `box: cap` defined precisely in SPEC §3.4 and implemented from OS/2 metrics; a new torture state, `anchors`, lines up all three anchors across 112/64/28 cu within 0.001 cu. The golden diff moved only layer transforms.)*
- [x] 0.6 `vello_cpu` painter → PNG. `scaena render --painter cpu`. *(Done: `scaena render` runs bundle fonts → `Engine::frame` → `vello_cpu` → PNG, with `--display-list`, `--size`, and `--json` stage timings; what it cannot draw yet exits 3 naming its PLAN task. 21 golden rasters are checked with the SPEC §13.5 metric; the visual review caught an illegible `anchors` layout that every test passed, now fixed. Measured: scalar vs AVX2 differ in 3 pixels over 21 frames, `multithreading` is bit-identical but slower on text, f32 vs u8 stays within 3/255; a cold 1080p render takes 35–59 ms on the Linux container. Findings and decisions in ADR-0004 and docs/spike-report.md.)*
- [x] 0.7 `vello` painter → PNG via headless `wgpu` (Metal on Mac, Vulkan/GL on Linux CI if available). *(Done: `scaena_paint::gpu`. `scene()` builds the vello scene on every target, wasm32 included; `GpuPainter` renders headless and reads back; `scaena render --painter gpu` sits behind the CLI's `gpu` feature. CI requires an adapter: Metal on macOS, Mesa's lavapipe on Linux. All 21 torture states are within SPEC §13.5 of the CPU goldens. Found: vello fills a glyph run as one path, so overlapping glyphs of opposite winding cancelled to a hole in `combining`; the painter now draws one run per glyph. The hole hid in the edge mask, so §13.5 gained a half-range step limit. ADR-0004 findings 6–7, spike report §0.7.)*
- [x] 0.8 Bare WebGPU page: `scaena-wasm` exposes `frame(state, t)`; a 40-line HTML page paints it with `vello` on WebGPU. Measure WASM size. *(Done: `Player.frame` returns a postcard display list; `Canvas` paints with vello on WebGPU through the same `scene()` as native. `crates/scaena-wasm/www/index.html` is the page (43 lines); `just wasm-smoke` and a CI job run it in headless Chromium. All 21 states' WASM display lists are bit-identical to native, and every state paints within SPEC §13.5 of the CPU goldens. Size: 1.04 MB gzip with vello and WebGPU, 0.53 MB for the engine alone, against the 3.0 MB budget; `wasm-opt` made the gzip size worse. Spike report §0.8.)*
- [x] 0.9 Parity harness: CPU vs GPU vs WASM-GPU PNGs, ΔE tolerance per SPEC §13.5, diff images on failure. *(Done: `crates/scaena-paint/tests/parity.rs` compares vello_cpu goldens, native vello, and vello on WebGPU in the browser, pairwise, on every torture state; failing pairs write diff images (`diff::image`), which CI uploads. `just spike` runs all three. CI runs cpu–gpu on Metal and lavapipe on every push, and all three pairs in the `wasm` job. All 21 states pass every pair; the two GPU paths agree within 16/255, and either differs from the CPU by at most 64 at glyph edges. Spike report §0.9.)*
- [x] 0.10 One morph: `state a` (bar) → `state b` (line), marks matched by key, geometry interpolated post-layout; render at t = 0, 0.25, 0.5, 1.0. *(Done: charts compile to keyed marks (`bar`, `line`, one series) from data files the caller hands over; each snapshot lays out once into a `Scene`, and a `Transition` samples two of them: `frame(t)` takes `&self` and holds no fonts or layout engine, so frames cannot lay out. The torture deck's `chart-line` morphs `chart`'s bars into points by key while the line fades in. t = 0 and t = 420 ms are the two states at rest byte for byte, t = 0.25 and 0.5 have goldens, and all four pass cpu/gpu/web parity and match native in WASM. Laying out the transition takes 0.82 ms; a frame samples in 3.1 µs. A state without `transition` cuts. Spike report §0.10.)*
- [ ] 0.11 Mesh shader: CPU reference in Rust + WGSL; parity test with seed.
- [x] 0.12 Spring solver + cubic-Bézier easing with settle-time computation. *(Done in the scaffold: `scaena-core::timeline`. Keep the tests.)*
- [ ] 0.13 **Authorability spike** (the second hypothesis: semantic text documents are naturally agent-authorable). With only `deck.json`, the schema, and today's CLI (`validate`, `lint`, `inspect`, `diff`), have Claude Code: (a) create a 6-state deck from `docs/examples/data/q3-revenue.csv` and a two-sentence brief; (b) turn its bar chart into a line chart preserving identity; (c) change the headline; (d) add a build; (e) retheme by swapping the theme file. Judge only: does it validate and lint clean, is every patch local (no rewrites of untouched states), did identity survive. Record the transcript and the diffs in `docs/examples/agent-authorability.md`. Do not judge visual quality — nothing renders yet.
- [ ] 0.14 Write `docs/spike-report.md`: what matched, what didn't, measured sizes/timings per SPEC §15 stages, catalogue of torture-deck results, go/no-go.

### Exit criteria (gate 0)

1. Every kill-criterion case in the torture deck renders **bit-identical display lists** on macOS and Linux, and rasters within tolerance across `vello_cpu`, native `vello`, and WASM/WebGPU. Catalogue cases are recorded with pass/fail and a note.
2. OpenType features and variable axes demonstrably applied (ligature visible; `opsz` and `wdth` change glyph shapes).
3. Bar → line morph renders correctly at four sample points with no layout re-run per frame.
4. Mesh background CPU/GPU parity within tolerance.
5. WASM engine ≤ 3.0 MB gzip (or a credible plan to get there recorded in the report).
6. Per-stage timings recorded against SPEC §15 for B1 (text) and B4 (torture); cold headless 1080p render ≤ 300 ms on an M-series Mac.
7. Authorability spike (0.13) completed: all five edits validate and lint clean, patches are local, identity survives.

**No-go path:** if text parity cannot be reached with `parley`/`harfrust`, the fallback is `harfrust` driven directly + our own line breaker (ADR-0004 lists the trade). If GPU painting is the problem, ship CPU + WebGL fallback and revisit. If the whole display-list approach fails, that is a different product; stop and reconsider the Tauri/HTML path (ADR-0001 records why we didn't start there).

---

## Phase 1 — Document, engine, agent surface (≈ 4–5 weeks)

**Goal:** an agent can author, lint, render, patch, and export a real deck from Claude Code with no UI.

### 1A Document model
- [ ] 1.1 `scaena-core` types for the full SPEC §3 document; `serde` + `schemars`; generate `docs/schema/*.json` from Rust and diff against the hand-written schemas (hand-written ones are the spec until the generated ones match; then the generated ones win).
- [ ] 1.2 Validation: schema + semantic (ids, references, type stability E104, duplicates E105).
- [ ] 1.3 Tracking resolution → absolute snapshots; `from` branching; `mode: absolute`.
- [ ] 1.4 Bundle I/O: directory and zip; manifest; content-addressed assets; font subsetting at save (`subsetter` or `klippa`).
- [ ] 1.5 DSL: lexer/parser (`logos` + hand-written, or `chumsky`), compiler to JSON, canonical decompiler; round-trip property tests; error reporting with `miette`.

### 1B Engine
- [ ] 1.6 Theme cascade (SPEC §3.6) with override tracking.
- [ ] 1.7 Layout templates + `taffy` containers; `at` resolution; alignment anchors (cap/baseline/x-height).
- [ ] 1.8 Text: roles, runs, fit policies, widows/orphans, hanging punctuation beyond quotes (quotes hang since Phase 0, SPEC §3.5), optical margins, hyphenation (by `lang`), text splitting units.
- [ ] 1.9 Charts → marks: exactly the v1 kinds in SPEC §3.7 (bar, stackedBar, line, area, scatter, dot, donut); scales; axes/labels/legend from theme; key-based morph; label collision handling. Deferred kinds (slope, waffle, range, heatmap) are out of scope for this phase and the schema rejects them.
- [ ] 1.10 Shaders: `mesh`, `gradient`, `noise`, `grain`, `particles` — CPU ref + WGSL each, parity tests.
- [ ] 1.11 Timeline resolution: transitions, presets, choreography (`with`/`after`, stagger, sequence), global timeline with `hold`.
- [ ] 1.12 Sampling: post-layout interpolation (geometry, glyph runs, marks, uniforms); word-diff text morph.
- [ ] 1.13 Multi-format layout (`formats`) — minimum: 16:9 and 9:16 via template sets.

### 1C Agent surface
- [ ] 1.14 CLI: all commands in SPEC §7.1 with `--json`.
- [ ] 1.15 Lint engine + the initial catalog (SPEC §7.5): all mechanical and design rules, plus the narrative rules (W420–W425) where `semantic` and the spine give them something to check; fixtures per rule; `--fix` for safe fixes.
- [ ] 1.16 Patch: JSON Patch + semantic ops; atomic apply; `--dry-run` lint delta.
- [ ] 1.17 MCP server (`rmcp`): tools + resources per SPEC §7.2; image content for `deck_render`.
- [ ] 1.18 Skills: `skills/author-deck`, `skills/retheme`, `skills/chart-from-data`, `skills/motion-pass`, `skills/tighten-copy`.
- [ ] 1.19 Agent-loop smoke test (SPEC §14, last bullet) in CI.

### 1D Exports
- [ ] 1.20 PDF painter (`krilla`): vector text with subsets, tagged structure from spine, shaders as images.
- [ ] 1.21 PNG/SVG per state; video via frame sequence → `ffmpeg` (mp4/webm/ProRes); `hold` dwell.
- [ ] 1.22 `export --format spine` + per-beat renders; integration note for the existing infographic/motion/podcast pipelines.

### 1E Store
- [ ] 1.23 `scaena-store`: Loro document with the container layout in SPEC §8.1; `deck.json` export/import; `fs`-authored changes; undo manager; fork/merge smoke test. (If Loro's rich-text or tree APIs fight the model, ADR-0002 names Automerge as the fallback — decide by end of week 2 of this phase.)

### Exit criteria (gate 1)
1. From Claude Code, using only MCP: create a 12-state deck from a CSV and a one-paragraph brief; lint to zero errors; render every state; export PDF and a 1080p60 video. Document the transcript in `docs/examples/agent-run.md`.
2. Re-theme the same deck with a second theme via `theme_apply`; lint delta is empty or explained.
3. Golden display-list and raster suites green on macOS + Linux; parity harness green on a GPU machine.
4. All SPEC §15 budgets met for the headless path on B1 and B2; B3 recorded.
5. Narrative lints run on the authorability deck and produce at least one true positive that an agent then repairs.

---

## Phase 2 — Web player and source editor (≈ 6 weeks)

**Goal:** humans can watch, present, and edit what agents make, in a browser, with no backend.

- [ ] 2.1 `web/` Vite + TS; WASM engine in a Worker; `OffscreenCanvas` + WebGPU; `vello_cpu` → `ImageBitmap` fallback.
- [ ] 2.2 Player: navigation (keys, click, touch), scrubber, `hold` auto-advance, presenter view (second window via `BroadcastChannel`), fullscreen.
- [ ] 2.3 Source editor: CodeMirror 6 `.scn` mode, live compile, lint gutter with fixes, live preview; inspector (resolved values, override counts, timeline).
- [ ] 2.4 Storage: OPFS + File System Access; open/save bundle; drag-and-drop assets with hashing; font subsetting in WASM.
- [ ] 2.5 Single-file HTML export (engine + bundle inlined; no network at runtime).
- [ ] 2.6 BYOK assistant in the Worker: Anthropic (direct browser header), OpenAI, Gemini adapters; function-calling onto the same operations; session-only key option; skills loaded from the bundle/repo.
- [ ] 2.7 Static deploy (existing hosting) with a demo deck.
- [ ] 2.8 Accessibility pass: keyboard, reduced-motion preference (collapses transitions to cuts), screen-reader order from the spine.

### Exit criteria (gate 2)
1. A deck authored in Phase 1 plays at 60 fps on WebGPU in Chrome and Safari 26+, and acceptably on the CPU fallback in Firefox.
2. Edit → lint → preview round trip < 200 ms for a 40-state deck.
3. Single-file export opens offline from a USB stick.
4. The assistant, with a user's own key, performs the Phase 1 agent-loop test inside the browser.

---

## Phase 3 — Mac client (≈ 6 weeks)

**Goal:** a native, document-based SwiftUI app on the same engine.

- [ ] 3.1 `scaena-ffi` (C ABI via `cbindgen`): load bundle, states, timeline, `frame`, lint, patch, store ops; Swift package wrapper.
- [ ] 3.2 Metal surface: `wgpu` surface from `CAMetalLayer`; `vello` painter in an `NSView`; 120 Hz on ProMotion.
- [ ] 3.3 Document-based app (`FileDocument`/`ReferenceFileDocument`) over bundles; autosave to CRDT; versions.
- [ ] 3.4 Chrome: state list with thumbnails, timeline scrubber, inspector, source pane (same `.scn`), lint panel with one-click fixes.
- [ ] 3.5 Presentation mode: external display, presenter notes, remote (iOS later).
- [ ] 3.6 Keys in Keychain; BYOK assistant; Apple Foundation Models for on-device tasks.
- [ ] 3.7 Direct manipulation v1: move/resize within template slots emits patches; off-template drags create explicit overrides (visibly flagged).
- [ ] 3.8 Exports wired to Share sheet / Quick Look.

### Exit criteria (gate 3)
1. Same bundle, same frame, same pixels (within tolerance) as the web player.
2. Open → first frame < 300 ms for a 40-state deck on an M-series Mac.
3. SwiftUI code contains zero text layout; all geometry comes from the engine.

---

## Phase 4 — When there is a second user (unscheduled)

- [ ] 4.1 Sync server (Loro sync or `automerge-repo`), identity, presence; conflict UX.
- [ ] 4.2 Shared links with permissions; comments on states/beats.
- [ ] 4.3 Hosted render for link previews (first server-side rendering — reuse the CLI).

---

## Cross-cutting tracks (run alongside phases)

- **Performance:** benches from Phase 0 onward; budgets in SPEC §15; a regression fails CI.
- **Fonts:** bundle-only policy, subsetting, licensing warnings (SPEC §16 Q3); a very large catalog to pick from, kept out of the render path (SPEC §16 Q8).
- **Docs:** `docs/spec/format.md` (number/date formats), `docs/spec/expr.md` (data transform expressions), `docs/spec/dsl.md` (full grammar) — written in Phase 1 when the code forces decisions.
- **Security:** no arbitrary code in documents; single-file export has no network; keys never in bundles.

---

## Working agreements (for humans and agents)

1. **Small, green PRs.** Every PR compiles, passes golden tests, and updates PLAN checkboxes.
2. **Determinism is a test, not a hope.** Any change to layout, text, timeline, or painters updates golden display lists in the same PR with a reviewed diff.
3. **Decisions get an ADR.** Library choices, format changes, anything that would be expensive to reverse: `docs/adr/NNNN-title.md`, status `proposed → accepted`.
4. **Schema is law.** `deck.json` changes require a schema change, example updates, and a format version bump (minor for additive, major for breaking).
5. **Lint rules come with fixtures.** One deck that triggers, one that doesn't.
6. **PPTX/Keynote never touch the model or the engine.** No import filter, no shared abstraction, no "just a quick converter" in `crates/`. A lossy external projection is permitted in principle, like PDF export, and is not planned.
7. **No HTML/CSS layout in the engine, no TextKit in the Mac app.** The engine lays out; clients paint.

---

## Risk register

| Risk | Signal | Mitigation |
|---|---|---|
| Text parity across painters | Phase 0 task 0.9 | glyph positions in the display list; painters never shape; fallback to `harfrust` + own line breaker |
| `parley` line-breaking quality (`pretty`/`balance`) | 0.4 | own Knuth–Plass pass over parley's clusters if needed |
| WebGPU availability/quality | 0.8 | CPU painter fallback; measure Firefox |
| WASM size | 0.8 (measured 1.04 MB gzip with vello; budget 3.0) | feature-gate painters; opt-level `s`/`z` (−12–16%); lazy-load shaders. `wasm-opt -Oz` grew the gzip size, so it is off. |
| Loro API friction with rich text/tree | 1.23 | Automerge fallback (ADR-0002) |
| Chart grammar scope creep | 1.9 | fixed kind list; everything else is annotations or deferred |
| Agent output quality | 1.15–1.19 | lint + render loop; roles not pixels; skills |
| Font licensing | 1.4 | honor embedding bits with warnings; document policy |
| Solo bandwidth | all | phases are sequential by design; Phase 4 is unscheduled |

---

## Gate log

| Gate | Date | Result | Notes |
|---|---|---|---|
| 0 | — | — | — |
| 1 | — | — | — |
| 2 | — | — | — |
| 3 | — | — | — |

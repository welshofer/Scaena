# Scaena — Build Plan

**Status:** Draft 0.1 — 2026-10-01
**Companion:** `docs/SPEC.md` (what), `docs/adr/` (why). This file is *when* and *in what order*.

Durations are a solo-builder-with-agents estimate, not a promise. Phases are gated: do not start Phase N+1 work until Phase N's exit criteria are met and recorded in `docs/PLAN.md` under "Gate log". The only exception is research spikes that de-risk a later phase; mark them as such.

Checkboxes are the live task list. Claude Code: when you finish a task, tick it, run the gate check (`/gate`), and commit.

---

## Phase 0 — The spike that can kill the project (≈ 2 weeks)

**Question answered:** Can the Linebender stack deliver pixel-identical typography across the CPU painter and WebGPU, and is the display-list pipeline sound?

**Scope:** one crate chain (`core → engine → paint → cli`), one fixture deck, no DSL, no CRDT, no charts beyond one bar chart's data motion, no theme cascade beyond what the fixture needs.

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
- [x] 0.10 One morph: data motion on one bar chart (its values animate in, then the next period arrives), marks matched by key, geometry interpolated post-layout; render at t = 0, 0.25, 0.5, 1.0. *(Revised on review: the task first read bar → line, which is not a use case.)* *(Done: one-series bar charts compile to keyed marks from data files the caller hands over. Each snapshot lays out once into a `Scene`, and a `Transition` samples two of them: `frame(t)` takes `&self` and holds no fonts or layout engine, so frames cannot lay out. In the torture deck, `chart` grows its values in from the baseline while the labels count up. Then `chart-next` advances the window a quarter: the five quarters that stay slide by key, 2025-Q1 rides out under the chart's left edge, and 2026-Q3 rides in from the right. Counting labels compose figures shaped once per snapshot, so frames never shape. A kind change between different marks (bars to a line) cross-fades. For both transitions, t = 0 and t = 420 ms are the states at rest byte for byte, and 0.25 and 0.5 have goldens. All 28 frames pass cpu/gpu/web parity and match native in WASM. Laying out a transition takes 1.1 ms; a frame samples in 3.6 µs. A state without `transition` cuts. Spike report §0.10.)*
- [x] 0.11 Mesh shader: CPU reference in Rust + WGSL; parity test with seed. *(Done: `scaena_core::shader::mesh` is the CPU reference and `mesh.wgsl` its twin. Seeded points drift on Lissajous paths and blend in Oklab through a rational kernel, with grain per device pixel. Uniforms are computed once per frame with `libm`; per pixel both sides do the same `+ − × ÷` in the same order and encode sRGB through one shared table of 255 thresholds, so on lavapipe they agree bit for bit in 6 of 7 parity cases and within one step in the seventh, and on Metal within one step in all seven (`shader_parity`: seeds, times up to an hour, 2–16 points, heavy grain, translucent palettes, 2× and rotated transforms). The CPU painter places the reference texel for pixel. The GPU painter runs the WGSL as a compute pass into a texture vello draws (`register_texture`), natively and in the browser. A shader's clock is the global timeline. The torture deck's `mesh` passes cpu/gpu/web parity with 0 pixels over ΔE 1, on lavapipe and on Metal, and its NEON raster is byte-identical to the x86-64 golden. A cold 1080p render takes 166 ms, 115 ms of it the CPU reference, after `scaena render` moved to fast PNG compression: grain made balanced deflate take 1 s. Spike report §0.11.)*
- [x] 0.12 Spring solver + cubic-Bézier easing with settle-time computation. *(Done in the scaffold: `scaena-core::timeline`. Keep the tests.)*
- [x] 0.13 **Authorability spike** (the second hypothesis: semantic text documents are naturally agent-authorable). With only `deck.json`, the schema, and today's CLI (`validate`, `lint`, `inspect`, `diff`), have Claude Code: (a) create a 6-state deck from `docs/examples/data/q3-revenue.csv` and a two-sentence brief; (b) roll its chart forward to the next quarter's data, preserving identity; (c) change the headline; (d) add a build; (e) retheme by swapping the theme file. Judge only: does it validate and lint clean, is every patch local (no rewrites of untouched states), did identity survive. Record the transcript and the diffs in `docs/examples/agent-authorability.md`. Do not judge visual quality — nothing renders yet. *(Done: five fresh agents, one per edit, knew the format only from SPEC, the schemas, the examples, the skills, and CLI help; the deck was the only handoff (`docs/examples/authorability/`, with each agent's transcript and the deck after each edit). All five edits validate and lint clean, every patch is local, and identity survives. Rolling the data forward was one field for the chart, plus 22 literal figures in the copy that nothing checks. The headline was one field and its beat, the build one new state and one trimmed, the retheme one field. Clean is a low bar today: validate and lint read no data, fonts, or theme, so `scripts/judge_edit.py` also checks theme names and resolved snapshots. Nothing checks that a deck renders: this one lists two of its theme's three font families. SPEC now states what the code already did about merging, placement, re-entry, slides, ids, and names; 13 open findings point at Phase 1 tasks. Write-up: `docs/examples/agent-authorability.md`.)*
- [x] 0.14 Write `docs/spike-report.md`: what matched, what didn't, measured sizes/timings per SPEC §15 stages, catalogue of torture-deck results, go/no-go. *(Done: go, and gate 0 is logged below. The report opens with the verdict, each exit criterion with its evidence, and what did not match. Criterion 6 needed B1, so `tests/bench/b1.scaena` (the manifesto as 40 text states in four fonts) now exists. `crates/scaena-cli/examples/stages.rs` times every SPEC §15 stage Phase 0 has, `coldstart.mjs` times WASM to the first frame, `just bench` runs both, and a `bench` workflow records the native stages on CI's Apple Silicon runner. There, B1's cold `scaena render` takes 32.6 ms median and 48.7 ms worst, against 300 ms; layout, sampling, lint, and CPU paint on one thread are inside their budgets. Not measured: GPU paint on physical hardware. Not done: criterion benches with a CI baseline (added as 1.24).)*

### Exit criteria (gate 0)

1. Every kill-criterion case in the torture deck renders **bit-identical display lists** on macOS and Linux, and rasters within tolerance across `vello_cpu`, native `vello`, and WASM/WebGPU. Catalogue cases are recorded with pass/fail and a note.
2. OpenType features and variable axes demonstrably applied (ligature visible; `opsz` and `wdth` change glyph shapes).
3. Chart data motion (values animating in, the next period arriving by key) renders correctly at four sample points with no layout re-run per frame.
4. Mesh background CPU/GPU parity within tolerance.
5. WASM engine ≤ 3.0 MB gzip (or a credible plan to get there recorded in the report).
6. Per-stage timings recorded against SPEC §15 for B1 (text) and B4 (torture); cold headless 1080p render ≤ 300 ms on an M-series Mac.
7. Authorability spike (0.13) completed: all five edits validate and lint clean, patches are local, identity survives.

**No-go path:** if text parity cannot be reached with `parley`/`harfrust`, the fallback is `harfrust` driven directly + our own line breaker (ADR-0004 lists the trade). If GPU painting is the problem, ship CPU + WebGL fallback and revisit. If the whole display-list approach fails, that is a different product; stop and reconsider the Tauri/HTML path (ADR-0001 records why we didn't start there).

---

## Phase 1 — Document, engine, agent surface (≈ 4–5 weeks)

**Goal:** an agent can author, lint, render, patch, and export a real deck from Claude Code with no UI.

### 1A Document model
- [x] 1.1 `scaena-core` types for the full SPEC §3 document; `serde` + `schemars`; generate `docs/schema/*.json` from Rust and diff against the hand-written schemas (hand-written ones are the spec until the generated ones match; then the generated ones win). *(Done: `scaena-core::model` types every node type, the theme, and the values they hold, and generates both schemas, which replace the hand-written ones (ADR-0007). The runtime keeps node props as JSON maps, and `Node::typed()` gives a node its type's view. A test holds the model to every node in every state of every deck in the repository and to every theme; another holds `docs/schema/` to the generator (`just bless` regenerates). The diff found eight places where the hand-written schemas disagreed with SPEC, each resolved for SPEC. Another type's props now fail; `null` and partial objects pass in deltas; charts take `axes` settings (`axesSpec` is gone); mesh params and label policies are typed; an image's `fit` validates; a slot's `align` is checked. `StateDelta` is derived from the node types, so a property a type gains is one a delta may set. Deck format 0.2; the theme format stays 0.1.)*
- [x] 1.2 Validation: schema + semantic (ids, references, type stability E104, duplicates E105). *(Done: `scaena validate` reads the bundle as it is on disk. `scaena-core::model::check` checks the deck and its theme against their generated schemas, with no schema library. Its verdict matched Python's `jsonschema` on 9,528 documents, the corpus and random mutations of it. Its messages are in the author's terms: a node is checked as its own type, so "`role` is not a property of a chart node; text nodes have it", and a near miss gets "did you mean `role`?". Beyond the schema: every reference, to files and to theme names (roles, layouts, slots, presets, durations, easings, springs, palettes, colors), is E102. A theme family the deck's `fonts` omits is E102 too, with the fix, because render needs it. A delta that sets `type` is E104. A key written twice is E105 for an id, else E106. Each resolved state is checked against its nodes' types (E106), reported at the delta that wrote it. E106 is new in SPEC §7.5, and findings in a theme file name it (`file`, SPEC §7.4). Fixtures per code are in `tests/lint/`. The docs examples became complete bundles, with fonts from `scripts/build_bundle_fonts.py`, so they validate and render. Not done: E103, fields and types in data, which goes to 1.9.)*
- [x] 1.3 Tracking resolution → absolute snapshots; `from` branching; `mode: absolute`. *(Done: resolution, `from`, and `mode: absolute` have produced snapshots since Phase 0; PLAN 0.10's transitions are built on them. 1.3 fixed what 1.1 found: an object value with nothing to merge into kept its `null` keys, and now drops them; deletes keep the order of what remains. Tests now pin each rule of SPEC §2.2. A deletion tracks, and an empty state repeats the one before. A node that re-enters starts from its defaults. `from` takes its props and layout from the earlier state, while entered and exited are judged against what was on screen. `absolute` shows exactly its own props, with no layout. Nodes keep scene-graph order.)*
- [x] 1.4 Bundle I/O: directory and zip; manifest; content-addressed assets; font subsetting at save (`subsetter` or `klippa`). *(Done:*
  - *Every command reads a bundle directory, a `.scaena` zip, or a bare deck file. `scaena save` writes a bundle as SPEC §3.1 lays it out: canonical JSON; fonts subset to what the deck can draw; fonts and images named by their content, with references rewritten; other files carried; and `manifest.json`, whose schema is generated like the others.*
  - *The subsetter is `skera` 0.7, fontations' port of hb-subset (it was klippa). `subsetter` drops the layout tables a bundle still shapes with (ADR-0004 finding 10).*
  - *Subsets keep every glyph id, so a save changes no frame: ten torture states that stress shaping (Arabic, Hebrew, marks, COLRv1 emoji, ligatures, kerning, hanging quotes, axes) draw their goldens from a saved zip, and B1 draws the same before and after.*
  - *The same bundle zips to the same bytes. `SOURCE_DATE_EPOCH` fixes the manifest's times.*
  - *Open: whether fonts should follow the theme (SPEC §16 Q10).)*
- [x] 1.5 DSL: lexer/parser (`logos` + hand-written, or `chumsky`), compiler to JSON, canonical decompiler; round-trip property tests; error reporting with `miette`. *(Done:*
  - *`scaena-core::dsl` compiles `.scn` to a deck and decompiles any deck to canonical source. The lexer and parser are hand-written, without `logos` or `chumsky`: the grammar is line-based and small, and the errors need byte spans and JSON pointers, which the parser records anyway.*
  - *The round trip is exact. Every deck in the repository, and random mutations of them, valid or not (4,000 per test run; 80,000 more in deep runs, `SCAENA_FUZZ=seed:count`), decompile to source that compiles back to the same canonical JSON byte for byte. Decompiling again gives the same source.*
  - *The scaffold's sketch, `docs/examples/revenue.deck.scn`, set the style: a node is declared on the state line that first shows it, a text node's bare string is its text wherever it stands, and times take units. The sketch claimed to compile losslessly to `revenue.deck.json`, but under SPEC §4's old rule (a first mention's props go to the state) it could not. Now the file is the deck's canonical source, a test holds the two together, and SPEC §4.1 shows it whole.*
  - *A beat's `duration` is the deck's one field in seconds, so `15s` is `15` there and `15000` everywhere else.*
  - *`scaena compile` validates what it compiles, in the bundle it writes to. miette shows each finding at the source that wrote it, through a source map from JSON pointers to spans, and the command writes only a valid deck. `scaena decompile` reads a bundle, a zip, or a deck file.*
  - *SPEC §4 now gives the grammar as built (§4.2) and the canonical form (§4.3).)*

### 1B Engine
- [x] 1.6 Theme cascade (SPEC §3.6) with override tracking. *(Done:*
  - *The engine reads the theme as the typed model (`scaena-core::model::theme`); the JSON-backed theme is gone, and every golden is unchanged.*
  - *The cascade (`scaena-engine::cascade`):*
    - *A text node is set in its role, or its slot's, refined by its `style`.*
    - *A run in the node's role takes the node's look, then its own `style`; a run with a role of its own starts from that role.*
    - *Tracking merges each state's props as before, and the deck's `overrides` then merge into every node, in every state, as a delta does.*
    - *A test renders the example deck with a style, a state change, and an override, and reads the order back out of the display list.*
  - *Deck format 0.3:*
    - *`style` is typed (`family`, `size`, `weight`, `leading`, `tracking`, `opsz`, `case`, `color`) and exists only on text nodes and runs.*
    - *`overrides` are deltas, validated like a state's: E102 for a node that is not there, E104 for `type`, E106 against the node's type, and their theme names.*
  - *Override tracking: `Deck::overridden` lists what a node's overrides set. I402 names each one, and `scaena inspect --resolved` shows each text node's look and its overrides.*
  - *`scaena theme <bundle> --apply theme.json [--dry-run]` re-themes a directory or a zip and reports the delta in what `validate` and `lint` find.*
    - *Retheming the example deck to Daybreak changes no finding.*
    - *The same holds from a saved zip, where Daybreak's families find the bundle's content-named fonts by family name.*
  - *Colors written as `oklch(…)` or `oklab(…)` render, read as CSS Color 4 reads them, through `libm`, and clipped to sRGB. An oklch palette no longer exits 3.*
  - *Shader palettes are named for their job: `dusk` is now `ambient` in the examples and fixtures (authorability finding 17).*
  - *Not done: W300 (a literal outside `overrides`) is lint work, PLAN 1.15. Whether fonts follow the theme stays SPEC §16 Q10.)*
- [x] 1.7 Layout templates + `taffy` containers; `at` resolution; alignment anchors (cap/baseline/x-height). *(Done:*
  - *Layout templates, `at` resolution on the theme grid, and the cap, baseline, and x-height anchors came with Phase 0 (0.4, 0.5). This task adds the node types a layout places and the containers that place them.*
  - *Shapes: `rect` with `radius`, `ellipse`, `line`, `arrow`, `polygon`, and SVG `path` data fitted to the box. Points are fractions of the box, so a shape morphs when its box changes. Fill and stroke are theme colors.*
  - *Images: PNG only, content-addressed (`sha256:`), with `fit`, `focal`, `crop`, and `radius` (SPEC §3.3). At most 8192 px a side, vello's atlas. Every painter decodes once and filters bilinearly.*
  - *Containers (`scaena-engine::containers`, ADR-0008):*
    - *`stack`, `grid`, and `frame` lay out with `taffy`, once per snapshot. Text is measured with parley, and images by the part of the picture they show. Positions are unrounded and the same bits on every platform.*
    - *A child names its container in `at.parent`, ordered by `at.index`. `children` and the node-level `parent` are gone (deck format 0.4).*
    - *Sizing values (`fit`, `fill`, `fraction(n)`, lengths, `aspect`, bounds), grid areas and lines, frame rects from the padding edge, groups, panels from a container's `fill` and `stroke`, and tree paint order.*
    - *Text in a row stack shares its last baseline through `align: { y: baseline }`.*
    - *Validation reports a missing or hidden container (E102), a node placed in a non-container (E106), a loop (E106), and an unknown area (E102).*
  - *Evidence:*
    - *Torture cases 24 (`shapes`), 25 (`images`, a generated test card seven ways), and 26 (`containers`: cards, a baseline row, a grid with areas, a frame, a group) have display-list and raster goldens.*
    - *All three painters agree on every one within SPEC §13.5.*
    - *An engine test moves a child between containers and finds it halfway at t = 0.5, with the at-rest paint order.*
    - *No earlier golden changed.*
  - *Not done: centered and end-aligned text (1.8); group compositing, which needs nested layers in sampling (1.12); JPEG and per-frame image budgets (SPEC §16 Q11).)*
- [x] 1.8 Text: roles, runs, fit policies, widows/orphans, hanging punctuation beyond quotes (quotes hang since Phase 0, SPEC §3.5), optical margins, hyphenation (by `lang`), text splitting units. *(Done. Roles and runs came with Phase 0 and the cascade (1.6); the rest is `scaena-engine::text`, SPEC §3.4–§3.5:*
  - *Lines: the engine places every line itself from parley's left-aligned lines, `start`, `center`, or `end` in the paragraph's direction, so `pretty`, `balance`, and hung lines align in either direction (right-to-left no longer falls back to greedy). `measure` caps lines in `ch`; `case` sets capitals, lower, title, or the font's `smcp`.*
  - *Hanging and margins: quotes hang only at an aligned edge. `hangingPunctuation` hangs brackets, stops, commas, and hyphens there too; `opticalMargins` moves edge characters part way out (microtype's factors) without changing a break.*
  - *Fit: `shrink` and `grow` find the largest size that fits by a 12-step bisection between the text's bounds; `clip` clips the layer; `error` refuses the frame. Overflow is reported for E100 and W203.*
  - *Hyphenation by `lang` through `hypher` (TeX patterns, 17 languages, ADR-0004 finding 11): soft hyphens, lines planned over measured segments, `-` drawn in the line's look; `pretty` charges a hyphen half a line.*
  - *Widows: every breaking holds each paragraph's last line to `minLastLineWords` (greedy and balance move the break above it back). A word is the text between break opportunities, a hyphenated word once, so no paragraph ends on a word's tail. A last line that cannot be held is reported (`TextLayout::widow`, for W200). No orphans in v1: text does not flow between boxes.*
  - *Split units: `TextLayout::units` cuts the laid-out text into lines, words, or clusters, in reading order, from each glyph's cluster offset, for choreography (1.11).*
  - *Evidence: torture cases 27 `alignment` (kill), 28 `case-measure`, 29 `fit`, 30 `hyphenation`; engine tests `text.rs` (14) and `fit.rs` (5); the `pretty` kill test now also shows greedy holding the widow. Earlier goldens moved only where intended: case 11's V and W protrude (the role sets `opticalMargins`), and the bidi states' raw digests (float operation order; quantized goldens unchanged).)*
- [ ] 1.9 **Chart and table sprint.** Jay sets an opinionated aesthetic for charts and tables, and the theme carries it. Charts → marks: exactly the v1 kinds in SPEC §3.7 (bar, stackedBar, line, area, scatter, dot, donut); scales; axes/labels/legend from theme; number and date formats; data motion by key (values in, the next period, growth) with enter/exit presets and stagger; label collision handling. Tables: `table` becomes a node type (schema, SPEC §3.3), styled by the theme like charts. Deferred kinds (slope, waffle, range, heatmap) are out of scope for this phase and the schema rejects them. E103 (an encoding's field missing from its data, or of the wrong type) arrives here, where data is read for encodings. *(Engine work and the Tufte defaults done; open: Jay's review of the defaults, the last item below. The rest of Phase 1 does not wait on it, since every look is a theme token. In `scaena-engine::charts`, `tables`, and `sample`, and SPEC §3.3, §3.6, §3.7, §3.10:*
  - *Charts: the seven v1 kinds compile once per snapshot to keyed marks, with series, categorical colors kept per series across the cue list, sequential and diverging shades, sizes by area, a legend (top, bottom, right, or none, with an optional title), value and category axes with d3's nice domains and ticks, gridlines on either axis, and titles.*
  - *Data: number and date formats in d3's grammar, per locale (`docs/spec/format.md`); dates read with `parse`; `dataTransform` filters, derives, sorts, limits, aggregates, folds, and pivots, with typed expressions (`docs/spec/expr.md`). `validate` reports what a chart or a table cannot read (E103) and what does not parse (E106).*
  - *Motion: marks match by key: bars grow, stacks open where they stand and never gap, a donut sweeps from twelve, a line's new point slides in off its end, and bars regroup in two stages. Value labels count; axes rescale as d3's do. `enter` and `exit` presets stagger marks within the transition, on springs through `libm`. Other kind changes cross-fade.*
  - *Labels that collide are hidden or nudged (least squares, by column), or reported for W310.*
  - *Annotations: a rule, a band, a callout that clears what is under it, and a highlight that dims the rest. Their values widen the axes. They match from state to state by kind and place, so a target rule rises and a callout slides.*
  - *Tables: the `table` node (deck format 0.5) sets a source's rows in the theme's `tables` styles: numbers at the column's end in tabular figures, the first column taking the slack. Rows move by key; changed cells cross-fade.*
  - *Evidence: torture cases 31–38 (every kind, a table, regrouping, annotations) with goldens at rest and mid-transition. CPU, GPU, and WebGPU agree within ΔE 1 on every one, and WASM display lists match native. Tests: `charts.rs` (23) and `tables.rs` (3) in the engine, plus sampling unit tests; `validate.rs` and the E103 and E106 fixtures in core. No earlier golden changed.*
  - *Aesthetic pass (Jay, 2026-10-02: "any look and feel, but chart and table defaults that look like Edward Tufte designed them"). Every default is the theme's to change; unset, a chart is data ink and little else (SPEC §3.7 Defaults; deck format 0.9, theme format 0.5):*
    - *Values on the marks: `labels.show: auto` (the default) prints every bar's, stack total's, dot's, and slice's value, and a line's first and last; an area and a scatter print none and show their value axes instead. Values nobody asked for that collide hide, rather than tripping W310. `charts.label.show` sets it for a theme.*
    - *Series named where they end, not in a legend: `legend: direct`, the default through `auto` and `charts.legend.place`. A line's names stand past the value over its last point, an area's and a stacked bar's level with their last span; one column, nudged apart, in the series' colors, the plot giving up only what the names need past the last point. Charts without ends (grouped bars, dots, scatters, donuts) keep a legend on top.*
    - *Thinner, quieter marks: bars at half their band with square corners, dots of radius 6, a donut ring of 0.28 of its radius (hole 0.72).*
    - *Tables as wide as their columns, at the cell's start (`tables.stretch` spans the cell).*
    - *Dusk and Daybreak (and the authorability copies) lose their rounded bars and take muted palettes led by a soft ink, the accent kept for emphasis. The torture theme keeps its values, so its goldens moved only where a chart leaned on a default: `chart-kinds`, `chart-kinds-2`, `regroup-stacked`, and `annotations`, and their transitions.*
    - *`docs/examples/charts.deck.json` is the review gallery: every kind and a table, nothing styled, linting clean.*
  - *After the fifteen-slide deck (#36):*
    - *A line's first and last values get room beside a continuous axis, two spaces from the value axis's labels, so neither is pushed back over its line.*
    - *The baseline rules 0 only where the value axis reaches it, so a line that starts at 120 shows no rule that reads as zero.*
  - *After the skill trials (1.18):*
    - *A stack whose members move on different clocks, as under a stagger, is re-stacked each frame. Each member keeps the extent its own clock gives it and starts where the one before it ends, so stacks build member on member and a ring sweeps open slice by slice. A stack's total counts with the stack.*
    - *A line's or an area's unit is its series, so its points move together and no point drops to the baseline while its neighbors are up.*
    - *Counts stop at their ends on a spring (SPEC §3.9 already said so).*
    - *A date `parse` format that reads a month or a day but no year is refused, instead of reading 1900.*
    - *Torture case 44 `stagger` holds the motion, at 0.3 and 0.6 of its span; no other golden moved. The trails deck drops its `stagger: 0` workarounds.*
  - *Second pass, by Jay's chart style guide (2026-10-02, "Let's start with it everywhere"): graphite with one signal color, few reference lines, labels a room can read. Each rule is a default or a theme token (theme format 0.6, SPEC §3.7 Defaults):*
    - *At most five value ticks: `charts.maxTicks` (default 5) caps the value axis, which asks for `tickCount` ticks, then for fewer until they fit, so a chart draws four or five reference lines at most.*
    - *One signal color: a highlight colors what it picks in `charts.signal` (the accent when unset) and dims the rest's marks to `charts.annotation.dimmed`, now 0.5. Their value labels and names dim half as far, to 0.75, so the context stays legible.*
    - *Dusk (and its authorability copy) and Daybreak carry the rest:*
      - *data in graphite and quiet grays, each readable as text on the theme's surfaces (4.5:1), since a direct legend names a series in its color;*
      - *a one-hue sequential scale;*
      - *sentence-case `axis` and `value` roles (12 and 13 pt) for ticks, values, series names, and table headers;*
      - *1.5-pt lines (`regular`).*
    - *The trails deck highlights what each data slide claims (Alpine, Tread, Clay, Grants). The gallery highlights the series or category its headline names (Pro, then Core twice). Both lint clean.*
    - *The torture theme keeps its palette, so its goldens moved only where a chart leaned on a changed default:*
      - *fewer ticks in `chart-kinds`, `chart-kinds-2`, `regroup`, `regroup-stacked`, and `annotations`;*
      - *the highlight's signal color and opacities in `annotations`;*
      - *the transitions between them.*
    - *Engine tests: `charts.rs` (35).*
    - *Not yet in the engine:*
      - *contrast and size checks for chart text (1.27);*
      - *forms the guide asks for, proposed for Jay to schedule (1H): dashed forecasts and estimates (1.28), horizontal bars (1.29), slopegraphs and dumbbells (1.30), small multiples (1.31).*
  - *Open: Jay's review of the gallery and trails renders, then the tick. Next candidates, by the same rule:*
    - *range frames for the value axis;*
    - *white gridlines knocked through bars;*
    - *direct labels for donuts and scatters (a scatter's series in quiet grays are told apart only by a legend);*
    - *value labels that clear other series' marks (the torture dot plot's `$3` touches the `$6` dot);*
    - *a rule over a table's total row.)*

- [x] 1.10 Shaders: `gradient`, `noise`, `grain`, `particles` (`mesh` since 0.11) — CPU ref + WGSL each in `scaena-core::shader`, parity tests; theme shader presets. *(Done. In `scaena-core::shader`, SPEC §3.8:*
  - *Kinds: `gradient` (linear, radial, or conic, turning; the palette as a ramp in Oklab), `noise` (3D simplex in octaves, evolving along time, keyed by the seed), `grain` (per device pixel, two palette tones, changing `fps` times a second), and `particles` (seeded soft discs that drift and wrap, blended in linear light). Each has a CPU reference and a WGSL twin side by side. Per frame, geometry comes from `libm`; per pixel the arithmetic matches line for line.*
  - *Params: a number, or a name a kind lists (a gradient's `shape`); the shader op carries numbers only. The schema types params by kind through `allOf` of `if`/`then`, which the checker now reads. The engine types them again as it resolves the node.*
  - *Presets: a theme's `shaders.presets` give a node of their kind its palette and params, and the node's own win. A preset the theme lacks is E102, and one of another kind is E106. Deck format 0.6, theme format 0.2.*
  - *Gradient paints, which the painters had pointed at this task: a shape's fill or stroke takes `{ gradient: { kind, angle, center, stops } }` across its box. Both painters draw it through peniko in Oklab, with a sweep repeating round the turn (SPEC §3.3, §6). vello's GPU ramp blends stops in sRGB whatever the gradient asks, so the shared conversion adds the in-between stops itself, as vello_cpu does (ADR-0004 finding 12).*
  - *Evidence: `shader_parity` holds every kind's CPU reference to its WGSL. On llvmpipe, all but the conic case are bit-identical; the conic case is 30 pixels one step apart (ΔE 0.32), from the arctangent. Torture case 39 `shaders` (every kind, a preset, gradient fills and a conic stroke) has goldens, and CPU, GPU, and WebGPU agree on it. Engine and core unit tests cover each kind, presets, and the paints. No earlier golden changed.)*

- [x] 1.11 Timeline resolution: transitions, presets, choreography (`with`/`after`, stagger, sequence), global timeline with `hold`. *(Done. `scaena-core::timeline` and `scaena-engine::motion`/`sample`, SPEC §2.4, §3.9:*
  - *The cue: a state's transition and its motions run on one clock from `t = 0`, and its span is when both have ended. `Transition::frame(t)` samples the whole cue; `t` at or past the span is the state at rest, exactly.*
  - *Motions: a node's own `enter` and `exit` presets as it enters and leaves, with the transition; its `emphasis` (after it) and `anim` tracks (with it), which no longer track to the next state (deck format 0.7); and choreography, which a node's own preset gives way to. Settings come from the item, then the preset call, then the theme's preset, then `standard`; a spring wins and lasts its settle time.*
  - *Scheduling: `with` and `after`, delays, staggers across a split's units and across targets, `sequence` and `parallel` groups (`timeline::schedule`).*
  - *Looks: opacity times the node's own; translate, scale, and rotate about an anchor in the unit's box. An entrance holds its unit out of sight until it starts, an exit holds it at rest until it starts and removes it when it ends, an emphasis goes out and back (a spring answers a tap, `Spring::impulse`). An entrance moves only what enters, so a stack that gains a child brings in that child alone.*
  - *Units: text split into lines, words, or glyphs draws each unit in its own layer about its box (glyph runs now carry advances). A container's children move with everything in them, looks composing from the outermost container in. A chart's marks run on the cue's clocks instead of shrinking into the transition, its frame coming in with the first mark.*
  - *Spring transitions follow the spring for its settle time; positions and sizes overshoot, while opacity, colors, and chart data stop at their ends.*
  - *The global timeline: each state's span, then its `hold`. `Engine::timeline` works it out as far as a frame needs, laying a state out only where a cue splits a node into what layout counts. Everything else is timed from the document, so B1 lays out nothing extra. Shaders keep its time through cues and holds. The WASM `Player` reports spans (`duration`) and the timeline as JSON, and `render --json` reports `span_ms`.*
  - *Validation: E106 for a choreography item with no motion or several, and for a split the target's type does not have.*
  - *Fixes on the way:*
    - *vello's GPU ramp blends gradient stops in sRGB, so the painters disagreed on Oklab gradients (ADR-0004 finding 12, in PR #24).*
    - *The example deck's `revenue` chart keyed its bars by product alone, which repeated across quarters; nothing rendered the state until now, so it went unnoticed. The key is gone, so it defaults to quarter and product.*
  - *Evidence:*
    - *Torture case 40 `motion` (words, cards, spring-grown bars, a pulse) has goldens at rest and at 0.15, 0.35, and 0.65 of its span.*
    - *`motion.rs` has 10 engine tests: splits, `with`/`after`, exits held, spring overshoot, emphasis, children in flow order, entrances on what enters, sequences, keyframes, and the global timeline with holds and shader time. There are 10 core tests for clocks, looks, scheduling, and the timeline.*
    - *The example deck's `intro` and `revenue` states render their choreography.*
    - *No earlier golden changed: a deck without motions keeps its spans and its shader times.*
  - *Not done: preset looks beyond opacity and transform (`params`, colors), and interpolating shader uniforms, are PLAN 1.12. Lint for a cue that moves nothing (an entrance on a node that stays) is PLAN 1.15. `validate` did not catch a chart key that repeats; only rendering refused it. *After 1.14:* `validate` reports one as E103, keyed as rendering keys it, in every state that shows the chart, and a table's too; engine tests hold the two to the same rule. It found the example deck's old key in the authorability deck's `rev-chart`, so none of the three states that show it rendered; that key is gone too.)*
- [x] 1.12 Sampling: post-layout interpolation (geometry, glyph runs, marks, uniforms); word-diff text morph. *(Done. `scaena-engine::sample`, `shapes`, `shaders`, `motion`, and `scaena-core::shader`/`timeline`; SPEC §2.3, §3.3, §3.4, §3.8, §3.9, §6:*
  - *Text morphs by word diff (`WordPlan`, a longest common subsequence of the two texts' words, punctuation before and after a word its own word). A shared word with the same drawing moves from where it stood to where it stands, its color mixing in Oklab; one whose size or face changes scales between its two boxes as its two drawings cross-fade. Words that leave fade out over the transition's first half, and words that arrive fade in over its second, so neither shows under the words moving past. Both layouts are the snapshots': nothing is shaped.*
  - *Shapes morph point by point when their outlines line up (the same kind and number of points, or path data of the same commands), a path's points once both are fitted to the box reached; paints mix in Oklab, gradients' stops, angle, and center included, and strokes' widths and dashes move. Anything else cross-fades.*
  - *Shaders morph by their uniforms: with the same kind, seed, and number of palette colors, the rect moves, colors mix in Oklab, and params move, one a side leaves out at its kind's default (`shader::default`). Names, counts, and rates do not (`shader::interpolates`): a rate's phase on the global clock is its value times t, so moving `speed` mid-transition would race the phase through (b − a)·t. Those cross-fade.*
  - *Looks (theme format 0.3, deck format 0.8): `color` mixes every paint a unit draws toward a theme color, keeping its alpha, on nodes and split text alike; `params.progress` draws a shape's outline on by arc length (square roots only, so every target cuts it in the same place), an arrow's head riding the tip and growing in over its own length. A call's `params` win over the preset's, and `anim` takes a `progress` track. The model types the look, a call's `params`, and `anim`'s property names, so a misspelled key is a schema error rather than a render-time one.*
  - *Groups composite (open since PLAN 1.7): a group draws its members into one nested layer at its opacity, its own motions move that layer, and it comes and goes with the transition as one, while a `children` split moves each member inside it. A tint inside a tint composes as one mix in Oklab; progress multiplies.*
  - *Evidence:*
    - *Torture cases 41 `morph-from` → 42 `morph`: goldens at rest and at 0.25, 0.5, and 0.75 of the cue. `just spike`: no pixel over ΔE 1 between any two of the CPU, GPU, and WebGPU painters on these or any other torture frame, and the WASM display lists match native.*
    - *Tests: `morph.rs` (6: words that move and fade, punctuation, a word that changes size, one that changes color, a shape's points and paint, a shader's uniforms and a rate that cross-fades); `motion.rs` gains 8 (color looks on nodes and on split text; draw-on, a call's params, and an `anim` track; typed looks; groups at rest, coming and going, cued by their children, and drawn on); unit tests for path morphs and trimming, look composition, and shader defaults.*
    - *Changed goldens, reviewed: the case label in five chart transitions now morphs by word (`chart-kinds-next@0.5`, `chart-kinds-2-next@0.5`, `regroup-stacked@0.25` and `@0.75`, `annotations-next@0.5`); `containers` draws its group as one layer, and its raster is unchanged (the ring and the dot do not overlap).*
  - *Not done: a chart's marks take a look's opacity, translation, and scale only; color and draw-on reach a chart as a whole. A line chart that reveals left to right waits for Jay's chart pass (1.9). Lint for a draw-on that reaches no shape is PLAN 1.15's.)*
- [x] 1.13 Multi-format layout (`formats`) — minimum: 16:9 and 9:16 via template sets. *(Done. `scaena-core::model::Format`, `scaena_engine::project`, SPEC §3.4, §3.6:*
  - *A deck's `formats` name the other shapes it is laid out in: 16:9, 4:3, 9:16, 1:1, and upright A4 and Letter. A format's canvas keeps the canvas's shorter side, so a 1920 × 1080 deck is 1080 × 1920 in 9:16.*
  - *Template sets (theme format 0.4): a theme's `formats.<format>.grid` is the grid there, and a layout's `formats.<format>.slots` take the place of its slots of the same names. Both are typed now; the layout key was an untyped placeholder since PLAN 1.1.*
  - *`project` puts the deck on the format's canvas and the theme on its template set before layout, so layout knows nothing of formats. Everything downstream is unchanged, and the global timeline is cached per projection (a cue on lines counts them after layout). A format the deck does not list is an error naming those it does.*
  - *Surfaces: `FrameRequest.format`, `scaena render --format 9:16` (exit 2 for an unlisted format), and in WASM `Player.formats()`, `setFormat()`, `canvasSize()`, and `Canvas.resize()`, with a format picker on the bare page.*
  - *Found on the way: in a narrow grid container, an image whose row sets its height asked for the width its picture's shape would take, and pushed its track past the canvas. Grid children now never widen a track unless their `minW`/`minH` asks (CSS's `min-width: 0`); no existing golden moved.*
  - *Evidence:*
    - *Torture case 43 `formats`: a claim and a chart side by side in 16:9, stacked in 9:16 by the specimen layout's 9:16 slots on the theme's 9:16 grid.*
    - *9:16 goldens for it and for `containers`, `chart-next@0.5`, and `morph@0.75`, named `~9x16`. `just spike` holds CPU, GPU, and WebGPU to SPEC §13.5 on them, and WASM matches the native digests.*
    - *Tests: 3 in `formats.rs` (canvas, slots, the format's grid, cells by column and row, the deck's own shape, unlisted and unknown formats, a format the theme says nothing about), a CLI test against the portrait golden, and a unit test for every format's canvas.*
  - *Not done: per-format node props (SPEC §16 Q6), and a lint for nodes placed by `col`/`row` or `rect` in a deck with formats, which do not move with the slots (PLAN 1.15). Exports in a format (PDF, video) follow with PLAN 1.20–1.21.)*

### 1C Agent surface
- [x] 1.14 CLI: all commands in SPEC §7.1 with `--json`. *(Done. `scaena-cli`, SPEC §7.1:*
  - *Under `--json`, stdout holds exactly one JSON value on every path: the command's result, or `{ "error": { "exit", "message", "plan"? } }` when it stops with exit 2 or 3. That includes usage errors and `compile`'s parse errors, which add `line`, `col`, and `path`. `compile`, `decompile`, and `export --format spine` now report `{ out, … }` objects, with the content when it goes to stdout, where `compile` printed a bare array and the others printed raw text. Any error from a path a later task builds exits 3 with that task (`plan`), wherever it comes from. Without `--json`, `diff` reads as a line per node (`+`, `-`, `~ keys`), where it printed JSON either way.*
  - *`inspect --timeline` closes authorability finding 15. Each state shows where it falls on the deck's timeline (start, span, hold), its transition (duration, curve, match), and each motion as placed on its clock: node, kind, split and units, start, stagger, duration, end, curve, and its look as what it changes from rest (`from`, `to`, `peak`, or `anim` tracks). It is worked out by the engine, which reads the bundle's fonts, so a cue on words or marks counts them after layout. `inspect --data` shows the rows each chart and table reads, after its `dataTransform`. Without either, `inspect` prints the snapshot as before.*
  - *`export --states a,b` is parsed for the frame exports (PLAN 1.20–1.21). Each export format now names its own task.*
  - *Evidence: `tests/json.rs` (6). Every command prints one JSON value; 13 stopping paths print the error object with their exit and task. The torture deck's timeline lays states end to end, and each state's span is its last motion or its transition. Case 40's words, children, marks, spring, and emphasis, and case 42's draw-on and tint, read back as placed. A table's rows come back sorted by the state's `dataTransform`, and a chart's from its CSV. `diff` prints a line per change. `dsl.rs` moves to the object shape.*
  - *Not done: `inspect` lays out in the deck's own format only (`render --format` has the others). The MCP server (PLAN 1.17) will want these results as typed outputs with generated schemas; today they are built in the CLI.)*
- [x] 1.15 Lint engine + the initial catalog (SPEC §7.5): all mechanical and design rules, plus the narrative rules (W420–W425) where `semantic` and the spine give them something to check; fixtures per rule; `--fix` for safe fixes. *(Done. `scaena-core::lint` (document rules) and `patch` (RFC 6902), `scaena-engine::lint` (layout rules), `scaena_paint::Backdrop`; SPEC §7.4, §7.5:*
  - *`lint` runs the bundle's validation, then the document rules (with the theme's thresholds), then, once nothing is an error, the layout rules. Those lay out every state in the deck's own format and in each of its `formats`; motion is judged once. Each finding names its format. Lint lays out what a frame refuses (`fit: error` text, a table whose rows do not fit) and reports it. `theme --apply` reports the delta in all of it.*
  - *Mechanical: E100 (text and tables), E101 (content overlapping in one container at one `z`, text by its lines), E110/E111, E120 (`.notdef` in text, tables, and chart labels), and E103 now finds two rows a chart would draw as one mark (authorability finding 7).*
  - *Contrast is judged against what is painted behind the text: the state at rest with its text taken away, painted by the client's painter, which the engine borrows through `Backdrop` because it never paints. A shader is judged at rest and at the end of the hold. A run fails when more than 2% of the pixels under its glyphs fall below the line. Display text is WCAG's large text at the rendered size on a 1080-px screen, so a theme declares nothing (finding 16).*
  - *Design: W200–W203, W210, W220, W300 (colors, text sizes, and lengths where a token goes), W301 (canvas `rect` only; a container's child is placed in it), W310, W320, W321. Two new codes: W302 (a deck with `formats` placing nodes by `rect` or grid cells, open since 1.13) and W322 (a motion that moves nothing yet takes its time, open since 1.11 and 1.12). W221 stays reserved: the engine does not snap text to the baseline grid, which SPEC §3.4 had claimed, so nearly every baseline would be off it. SPEC now says so.*
  - *Narrative, judged as finding 12 proposed: `takeaway` counts as a claim, and W421 and W422 judge each slide at its last state, with stat numerals exempt. W420 skips signpost beats (title, closing). W423 counts an evidence node as showing a data source, since lint cannot trace a literal figure (finding 6). W424 allows one claim at a time. Whether a slide says what its beat claims stays a question for the user's model (finding 11).*
  - *Fixes: E100 and W202 offer `fit: shrink`. Lint keeps a fix only after laying its state out again with the fix applied, which shows that the text fits. `lint --fix` applies them, writes the deck canonically, and lints again (`{ fixed, findings }` under `--json`).*
  - *What lint found in our own decks, and what changed:*
    - *The example deck's chart overlapped its footer (E101), the dusk-theme slot problem: dusk gains a `figure` layout, and Daybreak with it, since theme names are the swap contract. Its source note read 3:1 over the mesh (E110), so the mesh stays on the title slides. Its chart was placed by cells in a deck with formats (W302). Its opening slide showed no claim (W420), and now says its thesis.*
    - *The authorability deck's chart repeated its `key` (E103) and drops it.*
    - *B1, the manifesto deck, is dense by design: its theme allows 110 words a state.*
    - *The torture deck's incidental literals became tokens or overrides (no frame changed), and its tag says it lies over its photo (`z`). The rest of its findings are its cases', pinned in `tests/golden/lint/torture.txt`.*
  - *Evidence:*
    - *`tests/lint/<CODE>/` holds a trigger and a clean deck for each of the 29 codes beyond E102–E106, checked end to end in `tests/lint/bundle/` by `crates/scaena-cli/tests/lint.rs`: each trigger at its paths, each clean deck clean.*
    - *Fixes are kept only where they work, and `lint --fix` applies them and is idempotent.*
    - *The example deck and B1 lint clean; re-theming the example to Daybreak has an empty delta. The torture deck lints to its golden. RFC 6902 has its own tests.*
    - *Full lint takes 0.8 s for B1, 0.5 s for the example (both formats), and 1.8 s for the torture deck (45 states in two formats), in release on the 4-core container; layout dominates.*
  - *Not done:*
    - *Chart labels are not judged for contrast, and W221 waits on baseline snapping.*
    - *Fixes beyond `fit: shrink`: a color role for contrast, or a slot for a `rect`, would change the look, and are left to the author.*
    - *A deck that lists 9:16 for one case is linted in 9:16 throughout (the torture deck's W302s); a per-state format is SPEC §16 Q6.)*
- [x] 1.16 Patch: JSON Patch + semantic ops; atomic apply; `--dry-run` lint delta. *(Done. `scaena-core::patch`, `scaena_core::lint::delta`, `scaena patch`; SPEC §7.1, §7.3:*
  - *A patch is a JSON array of ops, applied in order, all or none: RFC 6902's, and 14 semantic ops that compile to them, each against the deck as the ops before it leave it. SPEC's nine, plus `rename_node` (authorability finding 13), `rename_state`, `remove_state`, and `show_node`/`hide_node`, so a node enters or leaves a state without hand-written JSON Patch.*
  - *Each semantic op checks what it names and refuses what would not do what it says, naming the op and what to do instead: a delta on a node not on screen in that state (it would enter), `type` (E104), a key deeper than a delta merges, a container that still holds nodes, a state that others build on. A misspelled member is an error. RFC 6902 ops ignore members they do not define, as the RFC says.*
  - *`rename_node` keeps the node's place in `nodes`, whose order is paint order at equal `z`: within one object, a `move` renames in place. Every state resolves as before, under the new name.*
  - *The ops are typed (`Op`, `JsonOp`, `SemanticOp`) and generate `docs/schema/patch.schema.json`. It carries the deck schema's node, state, and delta definitions, so it stands alone, as an MCP tool's input schema must (1.17).*
  - *`scaena patch <bundle> --ops ops.json|- [--dry-run]` checks the deck the ops make as `validate` checks a bundle, and refuses a patch that adds a validation finding (exit 1, nothing written). It reports `{ applied, patch, added, removed, errors }`: the patch as RFC 6902, and the lint delta.*
  - *The delta (`lint::delta`, now `theme --apply`'s too) knows a finding by its code, format, state, and node; a state by its id rather than its place; renamed ids by their new names; and its message but for its figures. So a state added early does not move every finding after it.*
  - *Evidence:*
    - *Core `tests/patch.rs` (9): each op against the example deck, to the exact RFC 6902 it applies; a rename resolves every state as before; removals, show and hide, and state ops with their references; presets, data, and themes; 17 refusals, each failing the whole patch at its op; `set_text` and `runs`; JSON Patch and semantic ops that fail together.*
    - *Unit tests: RFC 6902 (a move renames in place; `test` compares numbers by value), choreography edits nested and multi-target, and the delta's keys.*
    - *CLI `tests/patch.rs` (4):*
      - *The example patch (`docs/examples/revenue.patch.json`: a rename, a guarded headline, a build) dry-runs to an empty delta and writes nothing. It then applies canonically and lints clean, and a second run fails at op 0 with nothing written.*
      - *Three patches that would make the deck invalid are refused.*
      - *A headline too long for its slot shows E100 in both formats, and E101 and W202 in 9:16. With lint's own fix in the same patch, it shows nothing.*
      - *A rename keeps a finding the same finding. Ops come from stdin.*
    - *`just schema` validates the example patch. It checks that the patch schema rejects a misspelled member, a prop three levels deep, and a lone op, and accepts an RFC 6902 op with a member it ignores.*
  - *Found on the way: the order of a deck's `nodes` is paint order at equal `z`, and a CRDT map keeps no order of its keys. 1.23 must hold the order apart (noted there).*
  - *Not done: a patch becomes a CRDT change with 1.23. `remove_state` does not fold its delta into the next state; the states after it track from the one before, as SPEC §7.3 says.)*
- [x] 1.17 MCP server (`rmcp`): tools + resources per SPEC §7.2; image content for `deck_render`. *(Done. `scaena-ops`, `scaena-mcp`, `scaena mcp`; SPEC §7.2, §12; ADR-0009:*
  - *The operations moved out of the CLI into `scaena-ops` (ADR-0009). Each takes a bundle and returns a typed result, or an `OpsError { message, plan?, op? }`. The CLI parses, calls, and prints, and every CLI test passed unchanged across the move.*
  - *Five operations are new, for agents:*
    - *`deck_create` makes a bundle from a theme, the fonts its families name, data files, and a deck, and writes it only if it validates. The deck's `fonts` come from the theme. That is where authorability finding 5's deck failed: it listed two of its theme's three families.*
    - *`data_attach` copies a CSV or JSON file in and declares it, each column typed by inference unless `schema` says.*
    - *`deck_read` returns the deck as JSON or `.scn`.*
    - *`spine_read` reads the spine, and `spine_update` replaces it as a patch: checked, with its lint delta.*
  - *`scaena mcp` serves 12 tools over stdio, on `rmcp` 3.5.*
    - *A tool's result is its command's `--json` result, as structured content and as text. Structured content is an object, so a command that prints a list or a map has it named (`findings`, `states`, `changes`).*
    - *A tool that stops returns an error result whose text is `{ message, plan?, op? }`.*
    - *`deck_render` returns the PNG as image content, with the display list's digest (`DisplayList::digest`, which the golden tests now share).*
  - *Resources, compiled in:*
    - *the deck, theme, and patch schemas;*
    - *the lint catalog (SPEC §7.5, cut from SPEC at build time) and SPEC itself;*
    - *the `author-deck` skill;*
    - *the example deck (JSON and `.scn`), its patch, and its theme.*
  - *Each tool's input and output schemas are generated and committed in `docs/schema/mcp/`. A test holds them to what the server lists, and `just bless` regenerates them. `just schema` checks that each is a JSON Schema whose root is an object, as MCP requires.*
  - *A patch's ops and a deck to create are typed loosely, and point at their resources: inlined, the patch schema would add 73 KB to every `tools/list`. The server checks each op as `scaena patch` does.*
  - *Evidence:*
    - *`crates/scaena-mcp/tests/mcp.rs` (3), a client over an in-process pipe:*
      - *The tools and resources are listed, and the resources read.*
      - *A deck is built with the tools alone: created from Dusk, the Q3 CSV attached, then one patch for a title, a state, and a chart, then a spine. It lints with no errors and renders to a PNG twice with one digest. Inspect, diff, and read return what was built.*
      - *Three tools that stop say why: a PDF export names 1.20, a patch names its op, and a missing bundle is reported.*
    - *`crates/scaena-cli/tests/mcp.rs`: `scaena mcp` answers over real stdio, and exits cleanly when its client goes.*
    - *`crates/scaena-ops/tests/create.rs` (3): a bundle from a theme, then data, then a chart; a whole deck with its data in one step; nothing written that does not validate. A unit test types columns by inference.*
    - *SPEC §15's MCP budget, in release on the 4-core container: a `deck_render` round trip from a cold process (spawn, handshake, render, PNG back) takes 36 ms median and 50 ms worst over B1's 40 states, against 1 s.*
  - *Not done:*
    - *The skills still name CLI commands; 1.18 writes them for both.*
    - *`deck_create` and `data_attach` have no CLI command.*
    - *Paths are local to the server, and nothing streams progress for a long lint.)*
- [x] 1.18 Skills: `skills/author-deck`, `skills/retheme`, `skills/chart-from-data`, `skills/motion-pass`, `skills/tighten-copy`. *(Done. `skills/`, `crates/scaena-cli/tests/skills.rs`; SPEC §7.2, §7.6:*
  - *Each skill works through either surface. A table gives each step as a CLI command and as an MCP tool, then come a procedure and rules. MCP serves each skill as `scaena://skills/<name>`, and the server's instructions point to `author-deck` first.*
  - *`skills.rs` holds them to what exists. That covers every command, flag, MCP tool, `scaena://` resource, repository path, lint code, and SPEC section a skill names, and each skill's frontmatter. SPEC §7.6 lists exactly the skills there are, and MCP serves each file as it is. Planting eight bad references, one of each kind, failed it eight times.*
  - *Trials: a fresh agent ran each of `chart-from-data`, `motion-pass`, and `tighten-copy` on the fifteen-slide trails deck, with the skill and the docs alone. Each ended lint-clean, and what each could not find in its skill is now in it:*
    - *Charts:*
      - *periods without a year stay text;*
      - *`y.domain: [0, null]`;*
      - *declaring data from the CLI;*
      - *a chart is a slide of its own, and a delta state keeps the last slide's nodes;*
      - *a peak needs a callout.*
    - *Copy:*
      - *where the claims are without a spine, and which beat rules then find nothing;*
      - *the `set_text` op;*
      - *`errors` and `applied` beside the lint delta;*
      - *notes in the beat;*
      - *overflow before overlap;*
      - *figures checked with `inspect --data` and kept on one line.*
      - *The agent also invented a figure the data does not hold ("about a foot a year"). Review caught it; lint cannot (SPEC §16, question 9).*
    - *Motion: between slides that share nothing, a long transition lays one slide's words under the next's, so the skill now says to transition fast and build after. Also added:*
      - *the forms a transition takes;*
      - *when a node's own `enter` runs, against choreography's;*
      - *`parallel`, `glyphs`, exits, emphasis, and holds and how long to make them;*
      - *JSON Patch for a state's motion, guarded by `test`;*
      - *which frames to render.*
  - *The trails deck is now `docs/examples/trails.deck.json` (`scaena://examples/trails.deck.json`), on the examples' Dusk theme and fonts. It holds a spine, a stat, a photograph, five kinds of chart, a table, cards, and a quote, with the motion trial's choreography and holds. It validates, lints clean, and round-trips through `.scn`. On the way in, its narrative lint found a true positive: W423, a beat citing data that none of its states shows.*
  - *Not done:*
    - *No semantic op sets a state's `transition`, `choreography`, or `hold`; the motion skill writes JSON Patch guarded by `test`.*
    - *W320 counts nodes, not the marks or split units a motion moves.*
    - *The motion trial found engine defects: a staggered chart entrance misordered stacked segments and donut slices and dropped a line's points to zero, and counting labels overshot their values on a spring. 1.9 fixes them, after the skill trials.)*
- [x] 1.19 Agent-loop smoke test (SPEC §14, last bullet) in CI. *(Done. `crates/scaena-cli/tests/mcp.rs`; SPEC §14:*
  - *The test starts `scaena mcp` as a child process and talks to it on stdio, as an agent's client does. Through the tools alone it:*
    - *creates a deck from Dusk (`deck_create`);*
    - *patches in a headline too long for its one-row slot (`deck_patch`);*
    - *finds the E100 on it, which carries a fix (`deck_lint`);*
    - *applies the fix, which lint checks by laying the state out again (`deck_lint` with `fix`);*
    - *lints again and finds no errors;*
    - *renders the state, and gets a PNG back (`deck_render`).*
  - *CI runs it with the workspace's tests on Linux and macOS. It takes under half a second.)*

### 1D Exports
- [x] 1.20 PDF painter (`krilla`): vector text with subsets, tagged structure from spine, shaders as images. *(Done. `scaena-export::pdf`, `scaena export --format pdf --out FILE`, and `deck_export`; SPEC §3.12, §6, §10; ADR-0004 finding 13:*
  - *A page is a slide at its last state, in spine order, or each state `--states` names. The canvas is set at 2 units to the point.*
  - *Paths and gradients stay vectors. Text is text, in subset fonts. Glyph runs now carry the text of their clusters (SPEC §6), so a PDF copies and searches as the deck reads: ligatures, marks, Arabic, emoji, and hyphenated words. Images embed at their own resolution, and shaders as images of their CPU reference at twice the canvas.*
  - *Rasterized by `hayro`, every torture page passes SPEC §13.5 against the CPU painter, with shaders drawn at one pixel to the unit so their grain compares.*
  - *The PDF is tagged by how each node reads (`scaena-export::reading`): headings by role, figures with their alt text, tables by rows of header and data cells, and decoration as artifacts. Sections come from the spine, and so does the outline. A table cell's layer now says its row and column (display list `cell`).*
  - *Not done:*
    - *Shader images embed losslessly and are large: the trails example is 15 MB (ADR-0004 finding 13).*
    - *PDF/UA conformance is not checked. `krilla` can validate it; headings would need titles first.*
    - *A PDF is drawn in the canvas format only, not in another of the deck's `formats`.)*
- [x] 1.21 PNG/SVG per state; video via frame sequence → `ffmpeg` (mp4/webm/ProRes); `hold` dwell. *(Done. `scaena export --format png|svg|mp4|webm|prores`, and `deck_export`; `scaena-export::svg` and `::video`; SPEC §7.1, §7.2, §10; ADR-0004 finding 14:*
  - *`export` writes every format where `--out` says and returns what it wrote. png and svg write an image of each state at rest into a directory, `<state>.png`, at `--size`; a video writes a file. The spine still prints without `--out`.*
  - *A PNG is the CPU painter's frame.*
  - *An SVG is written by hand:*
    - *layers are groups (transform, clip, opacity, `mix-blend-mode`);*
    - *paths and linear and radial gradients stay vectors;*
    - *each glyph is an outline path, unhinted, with each run's text laid transparent over it, so the SVG selects and copies as the deck reads;*
    - *images embed as PNG, indexed where they hold 256 colors or fewer, and are filtered as the painters filter them;*
    - *shaders, COLR and bitmap glyphs, and sweep gradients are the CPU painter's pixels at the SVG's size.*
  - *Rasterized by `resvg`, every torture state's SVG passes SPEC §13.5 against the CPU painter, and so does every trails slide at half size. In headless Chromium the torture SVGs draw within ΔE 5 of it on 99% of pixels, and their text selects and copies (checked by hand).*
  - *A video plays the global timeline, or the states `--states` names in that order: each state's cue, then its `hold`, at `--fps` (60).*
    - *Frames are the CPU painter's, painted one per core, each single-threaded, so their pixels do not depend on the core count. This answers the spike report's question about threaded rasters. A frame that draws what the one before it drew is painted once.*
    - *Frames go to `ffmpeg` as RGB over black, with BT.709 named and tagged: H.264 at CRF 18 in MP4, VP9 in WebM, or ProRes 422 HQ in QuickTime.*
    - *`--audio` lays a sound track under the frames.*
    - *The video is written beside `--out` and renamed when whole, so a failed export leaves nothing behind.*
  - *Frame k shows the moment `render --t` draws (`Reel`). Decoded, each tested frame is within 2/255 of it on average, and nearer it than to the moments 100 ms either side.*
  - *The trails example, 110 s at 1080p60 (6,624 frames), exports to a 7.8 MB MP4 in 2 min 43 s on 4 cores (release build), 1.5× its running time.*
  - *CI installs ffmpeg on Linux and requires it there; elsewhere the video tests skip.*
  - *Not done:*
    - *Exports in another of the deck's formats (`9:16`), as for PDF.*
    - *Per-beat chapters in a video (SPEC §10's motion graphic), with the spine: PLAN 1.22.*
    - *A state with no cue and no hold has no frame, so a deck without holds plays only its transitions. Nothing warns about it.*
    - *The SVG's text layer is set in a system font stretched to each run, so a selection covers runs, not glyphs.)*
- [x] 1.22 `export --format spine` + per-beat renders; integration note for the existing infographic/motion/podcast pipelines. *(Done. `scaena export --format spine --out FILE`, `deck_export`, and `spine_read`; `scaena-core::spine`, `docs/schema/spine.schema.json`; `docs/projections.md`; SPEC §7.1, §7.2, §10:*
  - *The projection is typed in `scaena-core::spine`, and its schema is generated (ADR-0007). It holds the deck's title, language, canvas, and formats; the spine; every state, with its slide, notes, and place on the global timeline; and every beat, with the state that shows it (the last of its states in deck order) and where it starts and ends. `spine_read` returns it untimed, so it runs no layout. The schema is also the MCP resource `scaena://schema/spine`.*
  - *Written with `--out`, each beat is drawn at rest into `renders/` beside the file: a thumbnail 480 px wide (or `--size`), and the beat laid out again in each other format the deck lists, at that format's canvas size. The result lists the files. The projection itself is returned only when it is not written, by the CLI and the MCP tool alike.*
  - *A video carries a chapter per beat, titled by its claim, through ffmpeg's metadata input: a chapter track in MP4 and QuickTime, Matroska chapters in WebM. A state no beat names is a chapter of its slide; a deck without a spine has none. The video tests read the chapters back with ffprobe from all three containers, and the export's result lists them.*
  - *`docs/projections.md` says what each pipeline reads, and how to time a narration to the beats: lengthen a beat by its last state's `hold` with `patch`, then lay the track under the video with `--audio`.*
  - *Found on the way:*
    - *ffmpeg's `-map_metadata -1` strips chapter titles, and in MP4 and QuickTime the chapter track itself. The video now strips only global and stream metadata, and takes no chapters from a sound file.*
    - *SPEC §3.1 reserves a bundle's `spine.json` for an externalized spine, which nothing reads yet; the projection has the same name. The note says to write the projection outside the bundle. Rename the bundle's file when 1.23 lays the bundle out.*
  - *Not done:*
    - *No existing pipeline was run against it; they live outside this repository.*
    - *A video plays only in the deck's own format, so a portrait motion piece waits on the same work as 1.21's other formats.*
    - *Times are the deck's own format's; a cue that counts lines can run differently in another format.)*

### 1E Store
- [x] 1.23 `scaena-store`: Loro document with the container layout in SPEC §8.1; `deck.json` export/import; `fs`-authored changes; undo manager; fork/merge smoke test. (If Loro's rich-text or tree APIs fight the model, ADR-0002 names Automerge as the fallback — decide by end of week 2 of this phase.) The order of `nodes` is paint order at equal `z` and flow order in a container (`displaylist::paint_order`), and a CRDT map keeps no order of its keys: hold it apart, as a movable list of ids, and keep `rename_node` (1.16) a rename, not a remove and an add. *(Done. `scaena-store::crdt` (`DeckDoc`), `history/deck.loro`, `scaena save --history`; SPEC §3.1, §7.1, §7.2, §8; ADR-0002 records the decision: Loro stays:*
  - *The containers are SPEC §8.1's.*
    - *Nodes are keyed by keys of the CRDT's own. Everything that names a node does so by key: state props, `remove`, choreography, overrides, and `at.parent`. So a rename sets one field, one operation.*
    - *`order`, a movable list of the keys, holds paint order.*
  - *Values are JSON text, and each map keeps its key order beside it: a Loro map keeps none, and neither does a map value. Every deck in the repository comes out of the CRDT as it went in, byte for byte, saved and loaded.*
  - *A deck goes in as the smallest change. Text is edited by character. Runs are re-marked only where they changed. States and beats are moved, not made again.*
  - *In a bundle that keeps history, every write through the CLI and MCP records its change: by its author (`$SCAENA_AUTHOR`, or `agent:<client>`), saying what it did. A `deck.json` edited outside Scaena goes in first, as a change by `fs`.*
  - *Undo and redo are per author, and never undo `fs` changes or other peers'.*
  - *Tests:*
    - *every repository deck through the CRDT and back;*
    - *concurrent edits;*
    - *a rename against an edit;*
    - *beats moving between sections;*
    - *states moved and renamed;*
    - *rich-text runs;*
    - *nodes made apart under one id;*
    - *undo;*
    - *through ops and MCP, the history a bundle keeps.*
  - *Found on the way:*
    - *serde_json's `Map::remove` with `preserve_order` swaps the last key into the removed one's place.*
    - *SPEC §3.1's optional `spine.json` is gone. The CRDT holds the spine as a tree and `deck.json` carries it, so a third copy would be a third authority. It also shared its name with the projection (1.22).*
  - *Not done:*
    - *A `history` command (list, undo, branches) for the CLI and MCP. The editors of Phases 2 and 3 use `DeckDoc` directly.*
    - *`deck.scn` is not regenerated from the CRDT.)*

### 1F Benchmarks
- [x] 1.24 Benchmarks (SPEC §15): B2 (chart-heavy, with 1.9) and B3 (shader-heavy, with 1.10) under `tests/bench/`; `criterion` benches for every stage on B1–B4 with a recorded baseline per CI runner, failing CI on a regression beyond that runner's measured noise. Gate 1 criterion 4 is judged on these. *(Added by 0.14: SPEC §15 asks for criterion benches from Phase 0 on; `crates/scaena-cli/examples/stages.rs` is the stopgap that recorded gate 0.)* *(Done:*
  - *The decks. `scripts/build_bench_decks.py` writes `tests/bench/b2.scaena` and `b3.scaena` on B1's theme and fonts, and `just schema` checks the committed ones against it. Both validate and lint clean.*
    - *B2: six charts (bar, stacked bar, line, area, donut, scatter), each shown and then updated by key, for 12 states.*
    - *B3: a full-bleed mesh with grain over it on all 8 states, its seed and palette changing between them.*
  - *The benches. `crates/scaena-cli/benches/stages.rs` (`just bench`) names each `stage/deck` and covers every SPEC §15 stage the code has, on B1–B4:*
    - *load and fonts;*
    - *layout: fresh, warm, and the slowest state;*
    - *cues and sampling;*
    - *CPU paint, every state and the slowest;*
    - *GPU paint with readback;*
    - *both lints;*
    - *a cold `scaena render`, and the MCP `deck_render` round trip;*
    - *video's frame loop over the longest cue;*
    - *a probe of the machine.*

    *A stage that goes over states, cues, or frames counts them.*
  - *The gate. CI's `bench` workflow runs the benches on macOS (Apple Silicon, Metal) and on Linux. Each runner keeps a history of `main`'s runs in the Actions cache, and `scripts/bench_gate.py` judged every run against it:*
    - *a bench regressed when it was slower than the median of `main`'s runs by more than max(10%, 4σ), σ being their spread (1.4826 × MAD);*
    - *one that looked slower was run again, and judged on the faster of its two runs;*
    - *`bench-accept` lets a regression through.*

    *SPEC §15 says how.*
  - *The gate, again (2026-10-03). Judged against `main`'s history, the macOS runner failed three of the four pull requests that gate 1's agent runs opened, each on a bench it did not touch. The history was six runs, each on a different virtual machine. Of their times, 46% were more than 10% from their bench's median and 13% more than 30%, while the MAD of six put most benches' noise at the 10% floor. Linux runs landed on five processor models. Now a pull request is timed beside its base, built in the same job on the same machine:*
    - *a bench regresses when it is slower than the base by more than 10%, and again when the two are timed a second time, the base first;*
    - *`main`'s history is shown beside each bench, and no longer judges;*
    - *`scripts/test_bench_gate.py` tests the judgment, in `just check` and CI.*
    - *A third time: #61, which changed no code, failed on macOS. Two of its 60 benches came out 11–16% slower than the base, twice. Beside an identical base, that runner's benches strayed by 12% (σ, from their median absolute change), and Linux's by 1.6%, so a fixed 10% floor fails a bench or two on most macOS runs by chance. Now each run sets its own floor: 10%, or 2.5σ of its benches' changes if that is more. That is 10% on Linux and about 30% on macOS, where only a larger regression shows. The report says which. And a bench timed again is judged turn by turn, three turns of the two side by side: it regresses when it is slower than the floor in every turn. Each side at its fastest of two let one slow run decide, as it did for #62's `layout_one/b3` (+34%, then +11.8%).*
    - *On its own pull request, which changed no Rust, the macOS run still failed seven benches. Timed after its build, the pull request ran 20–80% slower than the base on most benches; timed again, the base first, the gap closed on all but the benches that paint on every core, which a spell of load on three cores slows by half. So both are built before either is timed, the two take turns a group of benches at a time, a slower bench is timed twice more beside its base bench by bench, each side counting at its fastest, and Spotlight is off on macOS.*
  - *The `wasm` job fails the engine over 3.0 MB gzipped, and records B1's cold start in headless Chromium.*
  - *On this Linux container (Xeon @ 2.8 GHz, 4 vCPU, AVX2; frames 1080 high, a frame painted on one thread):*
    - *B1:*
      - *the slowest state lays out in 0.73 ms, and all 40 in 20 ms;*
      - *a frame samples in 6 µs;*
      - *the slowest frame paints in 23 ms;*
      - *`scaena render` takes 38 ms cold, and the MCP round trip 49 ms;*
      - *document lint takes 0.5 ms, and layout lint 0.97 s;*
      - *video plays its longest cue at 56 frames a second.*
    - *Over budget there: CPU paint and video.*
      - *B1's and B2's slowest frames paint in 21–23 ms against 12 ms. Gate 0 measured Apple Silicon at about 3.4× this container, on one thread.*
      - *B1's video reaches 0.94× realtime at 60 fps, and B2's 1.1×. The frame loop paints a batch on every core, then writes it, so painting and writing never overlap.*
      - *B3 paints a frame in 208 ms against 25 ms, and its video reaches 0.16× realtime against 0.5×. The mesh's CPU reference renders one pixel at a time on one thread. Each pixel is its own, so rows can go to threads with the same result.*

    *The runners' numbers are in each run's summary.*
  - *On CI's macOS runner (Apple M1, virtual, 3 cores), its first run:*
    - *B1 and B2 are within budget for CPU paint and video.*
      - *Their slowest frames paint in 4.6 and 4.4 ms.*
      - *Their video takes 4.4 and 4.6 ms a frame, past 3× realtime.*
      - *So the container's overruns are the container's.*
    - *B3 stays over: 119 ms to paint a frame, and 70 ms a video frame.*
    - *GPU paint with its readback takes 16–26 ms a frame on B1–B3. SPEC's 6 ms budget is for the paint alone, so the report records this stage without judging it.*
  - *Not measured:*
    - *GPU paint alone: the GPU stage reads its frame back.*
    - *The GPU video path, which does not exist yet.)*

### 1G Design calls (Jay, 2026-10-02)
- [x] 1.25 Baseline grid: snapping is opt-in per text role, on for `body` and `caption` in the shipped themes (Dusk, Daybreak), and display text aligns by its cap height. W221 stops being reserved: it flags a snapping role whose leading is not a whole number of grid lines. SPEC §3.4, §3.6, §7.5; goldens for the torture deck's text cases. *(Done:*
  - *A text role's `snap` (theme format 0.7) sets it on the baseline grid, whose lines run every `grid.baseline` cu from the grid's top margin:*
    - *`baseline`: every baseline on a grid line. Layout rounds each gap between lines up to whole grid lines, so `fit` and containers measure the text as set.*
    - *`cap`: the first line's cap height on a grid line, the lines below at the role's leading.*
  - *After its alignment, a snapping text moves down to the next grid line, or, aligned to its box's foot (`end`, `baseline`), up to the line above. Texts aligned to one line move together. Charts and tables do not snap.*
  - *W221 is a document rule over the theme's roles. It flags a `snap: baseline` role whose `size × leading` is not a whole number of grid lines (the theme's grid, and each listed format's that has a baseline of its own), and a grid with no baseline at all. It points into the theme file. Its fixtures carry inline themes.*
  - *Dusk and Daybreak:*
    - *body (32 cu) and caption (22 cu) snap their baselines, at leadings of 40 and 32 cu, five and four grid lines;*
    - *display, headline, title, and numeral snap their cap heights.*

    *For Jay: body is set tighter than before (1.35 → 1.25) and caption looser (1.3 → 1.4545). The example decks still lint clean.*
  - *The torture deck's case 45, `baseline-grid`, has goldens, and its lint golden gains W221 for the case's off-grid role. No other golden moved.*
  - *Not done: a node cannot turn snapping on or off; `snap` belongs to the role.)*
- [x] 1.26 Data stays legible: the shipped themes keep the mesh off data slides, and lint warns about a shader painted behind a chart or a table, a new code beside W310. SPEC §3.8, §7.5; fixtures. *(Done:*
  - *W311, a layout rule, flags a shader painted behind a chart or a table where they overlap, by paint order. It reports once per shader and chart or table, at the shader, naming every state it happens in. Fixtures are in `tests/lint/W311`.*
  - *Dusk and Daybreak say where the mesh goes: their `title` layout is the place for a shader backdrop, and `figure` and `split` keep the plain surface. The author-deck skill says so too.*
  - *No deck in the repository has W311: the examples already keep the mesh on title slides.*
  - *Not done: a shader painted over a chart, such as a grain overlay, is not judged. W311 reads paint order and flags only what lies under the data.)*
- [x] 1.27 Chart text is judged.
  - E110 and E111 cover a chart's labels, ticks, and direct names: in their colors, at their dimmed opacities, over what is painted behind them. `contrast.rs` skips them today.
  - A new code beside W310 flags chart text under 12 pt at presentation size (24 units on a 1920-unit canvas, scaling with the canvas).
  - SPEC §7.5; fixtures. *(Jay's chart style guide: "presentation labels at least 12 pt"; PLAN 1.9, second pass.)* *(Done:*
    - *E110 and E111 judge every text a chart sets (category and value labels, axis labels, titles, series names, annotations) in its color, at the opacity a highlight dims it to, over the chart's own marks, rules, and bands. The backdrop drops a chart's glyphs and keeps the rest. A chart is reported once for each kind of its text that fails, naming the text.*
    - *Contrast now reads the ink. Each pixel counts by how much of it the glyphs cover, painted on their own, so a rule under a baseline no longer fails the text over it. Text at no opacity is not judged, and a run is bold by its role's weight, so a bold table header counts as WCAG's large text.*
    - *W312 flags chart text under 12 pt at presentation size: 24 cu where the canvas's shorter side is 1080 cu, in proportion on others. One finding per chart names each kind of text and its size. Fixtures are in `tests/lint/W312`.*
    - *Defects the rules found, fixed in the engine:*
      - *a grouped bar's value label wider than its bar lay over the taller bar beside it (the revenue deck in 9:16); it now starts or ends with its bar, whichever clears;*
      - *a donut's values at twelve and six o'clock touched the ring; they now stand half their cap height farther out;*
      - *a rule's text and a callout's text crossed their own line with their descenders; they now clear it.*
    - *The torture theme, B1's (whose charts are B2's), and the lint bundle's set chart text in a new `chart` role at 24 cu, the guide's floor, so the torture deck's chart goldens moved with the larger text.*
    - *Not done: six findings in the torture deck's lint golden are value labels and callouts the layout lets marks and rules cross (PLAN 1.32; PR #41 holds most of the fix).)*
- [ ] 1.32 Chart text clears what it would cross. 1.27's contrast check found text the layout lets a mark or a line cross, now E111s in the torture deck's lint golden:
  - a dot's value over another series' dot (`k-dot`, both formats);
  - a steep line through its first value in 9:16 (`st-line`, `n-lines`);
  - a `y` rule across a bar's value (`n-bars`);
  - a callout's text on another annotation's rule (`n-bars` in `annotations-next`).

  PR #41 (chart pass 3, open for Jay's review) already moves a dot's value under its dot, breaks a rule where it would cross text, and raises a callout's text past a rule. It leans a bar's value off a taller neighbor as 1.27 does. Landing it on 1.27 should clear all but the steep lines, which need their first value moved off the line where the plot's side clamps it. Then the six findings leave the golden. *(Found by 1.27.)*

### 1H Chart forms the style guide asks for (proposed 2026-10-02; Jay schedules)
*Not gate-1 work until Jay schedules them. 1.30 would lift PLAN 1.9's deferral of `slope` and `range`.*
- [ ] 1.28 Forecasts and estimates read as such. A line's or an area's rows can be marked projected (a field the encoding names). The line runs dashed from the last actual point, and its end value says it is an estimate. SPEC §3.7; schema; a torture case. *(The guide: "distinguish actual and forecast".)*
- [ ] 1.29 Horizontal bars: `bar` and `stackedBar` take `orient: horizontal`, with categories down the side and their names read across, for rankings and long names. SPEC §3.7; schema; a torture case.
- [ ] 1.30 `slope` (two states, both ends labeled, the change said) and `range` (a dumbbell between two values, or a point with its interval) join the v1 kinds in SPEC §3.7 and the schema enum, with data motion by key; torture cases.
- [ ] 1.31 Small multiples: a chart facets by a field into a grid of panels on one shared scale, each named directly, with no frames. SPEC §3.7; schema; a torture case.

### 1I What gate 1's agent runs found (2026-10-03)
*From `docs/examples/agent-run.md`, Findings. The runs found eight problems that are fixed (#53–#58); these are the rest.*
- [x] 1.33 Resources an MCP-only agent can read whole.
  - **The problem.** Claude Code saves a resource over its output limit to a file, and an agent with no file tools cannot open it. The deck schema (74 KB) and SPEC (157 KB) never reached gate 1's agents, which learned `dataTransform`'s syntax from E106 messages.
  - **The task.** Serve SPEC by section, and `docs/spec/format.md`, `docs/spec/expr.md`, and the example themes on their own, each small enough to arrive whole. The server's tests hold every resource under that size.
  - *(Done: every resource weighs under 40 KB as `resources/read` returns it, its text escaped (`scaena_mcp::LIMIT`). Gate 1's agent got a 45 KB result whole and a 74 KB one as a file; Claude Code's documented limit is 25,000 tokens.*
    - *SPEC: `scaena://spec` is an index; each `##` section is `scaena://spec/N`, and each numbered subsection `scaena://spec/N.M`. §3, too large to arrive whole, holds its text up to §3.1 and the uris of its subsections, which are listed. The other subsections read by uri. Their texts in order are SPEC, which a test checks.*
    - *The schemas: the deck's in four parts (`scaena://schema/deck`, its root and states; `/nodes`; `/deltas`, what a state sets; `/values`, what both share), the theme's in four (its root; `/charts`, `/shaders`, `/motion`), and the patch's ops by reference into the deck's parts. Each part is a JSON Schema whose `$id` is its uri. A test resolves every `$ref` and checks that the parts are the files in `docs/schema/`.*
    - *`scaena://spec/format`, `scaena://spec/expr`, `scaena://examples/charts.deck.json`, and the three themes, Dusk, Daybreak, and Ember.*
    - *JSON resources come without their whitespace, which takes the trails deck from 45 KB to 26 KB as read. The largest resource is §7, at 32 KB. The author-deck skill names the sections to read; SPEC §7.2 says how resources are served.)*
- [x] 1.34 One vocabulary for the shipped themes.
  - **The problem.** The retheme skill calls names the swap contract, but the shipped themes break it, so a deck cannot move between them without edits.
  - **The task.** Dusk, Daybreak, and Ember name their layouts and slots by job, the same names for the same jobs:
    - Dusk's `figure` layout, with its slots `main` and `footer`, is Ember's `chart`, with `chart` and `side`;
    - Dusk's `stat` slots `meaning` and `aside` are Ember's `claim` and `detail`.
  - **Shader presets** are named by job, too, and every theme has the ones the example decks use.
  - **The test.** It re-themes each example deck across the three and expects no E102.
  - *(Done: Dusk, Daybreak, and Ember define the same names for the same jobs, and SPEC §3.6 lists them.*
    - *Layouts and slots. Ember's `chart` becomes `figure`, its slots `main` and `note` (`chart` and `side`), and `narrow-chart` becomes `narrow-figure`. A stat's `meaning` and `aside` become `claim` and `detail`. A source line or a column of facts is a `note` (Dusk's `footer`, Ember's `side`), and `split`'s `left` and `right` are `body` and `main`. Each theme gains the layouts the others had, in its own geometry: Dusk and Daybreak `statement`, `poster`, `narrow-figure`, `art-left`, and `art-right`, and Ember `split` and `quote`. Dusk's kicker runs in the bottom corner, its top row being the header's.*
    - *Grids. Dusk and Daybreak lay out on 12 rows, as Ember does. Each slot covers the two rows its one did, so nothing moves, and an `at.row` names the same cell in each theme. Before, higher-ed's rows past 6 stopped lint on Dusk.*
    - *Roles: Dusk and Daybreak gain `lede`, `kicker`, `figure`, and `quote`, and Ember `code`, in JetBrains Mono. Ember gains the color `accent-2` and the spring `heavy`. The shader presets are `backdrop` and `texture` (Dusk's `mesh-soft` and `noise-fine`), and the palettes `ambient` and `texture` (Dusk's `ember`, named for a theme), in each theme.*
    - *`theme_apply` lists a theme's families in the deck's `fonts` when the bundle holds their files (`listed`), which E102 asked for by hand.*
    - *The example decks use the new names. A test re-themes each of the six onto each theme: none is refused, and none has an E102. Another checks that the three themes name the same things. What remains in those deltas is the new look's to report: Dusk's larger headlines in higher-ed's header slots (E100), text over trails' storm photo on Daybreak (E110), and a few more.)*
- [x] 1.35 A re-theme is all or nothing. `theme_apply` refuses to leave a deck with new errors, as `deck_patch` does, unless asked to (`force`). The retheme skill shows the way that keeps the deck valid throughout: one `deck_patch` with the `retheme` op and the fixes the dry run's E102s name.
  *(Done: `theme_apply` validates the deck with the new theme before it writes. A theme that adds a validation error is refused: `refused` in its result, exit 1. The deck keeps its theme, and the theme is copied into `themes/` all the same, for one `patch` with the `retheme` op and the fixes. `--force` (`force`) applies it anyway. The retheme skill shows the patch, and SPEC §7.1–§7.2 say so. A CLI test is refused, finds the `retheme` op alone refused too, and forces.)*
- [x] 1.36 Lint sees what gate 1's renders showed.
  - E101 judges a container's children against the nodes around it (in attempt 3, a source note over the cost cards).
  - A narrative rule reports a spine whose beats run in another order than their states, which leaves the PDF and the video telling the story in different orders.
  - A chart squashed by a theme's grid below a legible plot is flagged.
  - `at.align` on a grid placement either places the grid in its cells or is rejected. In attempt 3 it did nothing.
  *(Done:*
    - *E101 compares two nodes where their paint paths part: the nodes themselves in one container, else the containers they are in. So a card in a stack collides with a note beside the stack when the stack and the note share a `z`, and the finding names the container whose `z` ties.*
    - *W426 reports a beat that comes after another in the spine while its states play first. The ridgeline example now plays `lesson` after `sixfold`, where its beat stands.*
    - *W313 flags a chart whose plot, the room its marks have once its labels, axes, and legend have theirs, is under 120 cu across or down at presentation size.*
    - *A root container whose own `align` or `at.align` names an axis takes its content's size on that axis and aligns in its box, as SPEC §3.4 now says. Before, it filled the box, and the alignment had nothing to move. A slot's `align` is for text, and shrinks no container.*
    - *Fixtures for W313 and W426, E101's trigger widened to a stack and a note, and a container test. The example decks still lint clean, and the torture deck's lint golden is unchanged.)*
- [x] 1.37 A placement outside the theme's grid is a finding, not a stop. An `at.col` or `at.row` past the grid's tracks makes layout fail, and `lint` exits 2 with no findings. It should be E102 at the node's `at`, naming the grid's size, with the rest of the deck linted. (Found in 1.34: a 12-row deck on a 6-row theme.)
  *(Done: validation reports each cell past the theme's grid as E102, so `lint` exits 1 with it, beside whatever else validation and the document rules find.*
    - *A node's `at.col` or `at.row` past the grid is reported at the node's or the state's `at`: "`late` is placed in rows 6–7, past the theme's grid, which has 6 rows".*
    - *A slot a node stands in that runs past the grid is reported in the theme. A slot no node uses places nothing, and is not judged.*
    - *Each format the deck lists is judged on its own grid, and with its own slots.*
    - *A range that runs backward is E106.*
    - *A re-theme onto a smaller grid is refused, as for a missing name: a test re-themes higher-ed onto a 6-row Ember. SPEC §3.4 and §7.5 and the retheme skill say so.)*

### Exit criteria (gate 1)
1. From Claude Code, using only MCP: create a 12-state deck from a CSV and a one-paragraph brief; lint to zero errors; render every state; export PDF and a 1080p60 video. Document the transcript in `docs/examples/agent-run.md`.
2. Re-theme the same deck with a second theme via `theme_apply`; lint delta is empty or explained.
3. Golden display-list and raster suites green on macOS + Linux; parity harness green on a GPU machine.
4. All SPEC §15 budgets met for the headless path on B1 and B2; B3 recorded.
5. Narrative lints run on the authorability deck and produce at least one true positive that an agent then repairs.

---

## Phase 2 — Web player and source editor (≈ 6 weeks)

**Goal:** humans can watch, present, and edit what agents make, in a browser, with no backend.

- [x] 2.1 `web/` Vite + TS; WASM engine in a Worker; `OffscreenCanvas` + WebGPU; `vello_cpu` → `ImageBitmap` fallback.
  *(Done: `web/` is the player page and the engine's worker, on Vite 8 and TypeScript 7, pinned by `web/package-lock.json`. `just web` builds it, and `just web-dev` serves it with the repository's bundles.*
    - *The WASM module gains `Canvas.attachOffscreen` (WebGPU in a worker) and `Player.pixels` (`vello_cpu` RGBA, which a native test holds to the goldens). The CPU painter adds 186 KB gzipped: the module is 2.14 MB, against SPEC §15's 3.0.*
    - *The worker asks for a WebGPU adapter before WebGPU takes the canvas. Where there is none, or `?painter=cpu`, `vello_cpu` paints into `ImageBitmap`s; if WebGPU fails anyway, the page starts over on a new canvas with the CPU painter.*
    - *`web/smoke.mjs` opens the built player in headless Chromium with each painter and screenshots every torture frame: 68, at rest, into a cue, and in 9:16. The parity harness now takes several browser directories, and holds both painters' frames to the goldens within SPEC §13.5. The CPU painter's are within a channel step of 7 of them, none over ΔE 1. WebGPU's are within a step of 1 of the WASM page's. `just web-smoke` runs both; CI's wasm job runs them with the rest.*
    - *`scaena-paint` builds at `opt-level = 2` in dev, so the raster diff holds two painters' 68 frames to the goldens in 11 s, not 88. `Raster::from_png` reads a browser's RGB screenshot as opaque.)*
- [x] 2.2 Player: navigation (keys, click, touch), scrubber, `hold` auto-advance, presenter view (second window via `BroadcastChannel`), fullscreen.
  *(Done: SPEC §9.2 says how the player behaves. `web/player.mjs` checks it in headless Chromium, on the revenue example, whose states hold, and on the torture deck, whose states do not.*
    - *The worker keeps the clock (`run`, `seek`, `pause`) and says where the deck is with each frame: a state and a time into its cue. The torture deck is why it is not a place on the global timeline: 23 of its states stand at 0 ms.*
    - *The page's `Stage` is a canvas and its worker. The player has one, and the presenter view two: the slide as the room sees it, and the next state at rest.*
    - *The test drives the keys, the scrubber, a click, a swipe, and fullscreen. It checks a hold going on by itself, the last state resting, the scrubber's frame matching `show`'s for the same state and time, and the presenter view following the player and steering it. `just web-smoke` runs it after 2.1's checks, and so does CI's wasm job.*
    - *It runs on the CPU painter. Headless Chromium composites WebGPU on SwiftShader at about a frame a second, and the clock keeps the display's frames.)*
- [x] 2.3 Source editor: CodeMirror 6 `.scn` mode, live compile, lint gutter with fixes, live preview; inspector (resolved values, override counts, timeline).
  *(Done: SPEC §9.2 says how the editor behaves. `web/editor.mjs` checks it in headless Chromium, on the revenue example and on B1.*
    - *The operations are `scaena-ops`' (ADR-0009). Compile with its source map, lint with an engine the caller keeps, and inspect from a deck already in memory moved there from the CLI. `scaena-wasm`'s `editor` feature binds them for the worker.*
    - *Each finding stands where the source wrote what it is about. A finding about a whole node in a state stands at the node's line in that state.*
    - *An edit lints the state shown, and every state is linted once typing stops. A lint of every state takes 1.2 s on B1 in the browser (SPEC §15 budgets 1 s natively), so it cannot be gate 2's round trip. The engine's lint lays out one state on request. `crates/scaena-ops/tests/lint.rs` holds it to the whole lint on every torture state.*
    - *Gate 2's round trip on B1 has a median of 114 ms (compile 16, the frame 66, lint 32) on the CPU painter in headless Chromium.*
    - *The WASM engine with the editor's operations is 2.76 MB gzipped (SPEC §15: 3.0). `just web-smoke` and CI's wasm job run the test after 2.2's.)*
- [x] 2.4 Storage: OPFS + File System Access; open/save bundle; drag-and-drop assets with hashing; font subsetting in WASM.
  *(Done: SPEC §9.2 says how a page keeps a bundle. `web/storage.mjs` checks it in headless Chromium.*
    - *The WASM session holds every file of the bundle by its path. The engine is built from the fonts and images the deck names, and again once it names others, so a file can join after the first frame. `scaena-store` opens a bundle in memory, from its files or a zip's bytes, and saves it to files in memory (`Bundle::saving`), which the page writes where the bundle is kept, or zips.*
    - *A bundle opens from a URL (with every file its manifest lists), a folder (File System Access), a `.scaena` file, or the browser's storage (OPFS, `?bundle=opfs:NAME`, which the player opens too). Save writes as `scaena save` does, fonts whole, where the bundle is kept, and the source takes the save's renames and nothing else. Download subsets the fonts with the subsetter's own module (`scaena-subset`, 244 KB gzipped), which the worker loads the first time, into the bytes `scaena save` writes. A file dropped on the source joins the bundle where its kind goes, an image named by its SHA-256, and its path goes where it was dropped.*
    - *The engine's module stays at 2.75 MB gzipped (SPEC §15: 3.0): the subsetter (0.24 MB) and the CRDT (0.95 MB) are kept out of it, and validation reads the committed schemas rather than generating them, which takes `schemars` out (0.15 MB).*
    - *The page keeps no CRDT: a bundle's history is carried as it is, and the page's edits go in as `fs`'s when a client that records next writes the deck (2.9).*
    - *`just web-smoke` and CI's wasm job run the test after 2.3's.)*
- [x] 2.5 Single-file HTML export (engine + bundle inlined; no network at runtime).
  *(Done: SPEC §9.2 and §10 say what the file holds, and §3.12 how it reads. `web/standalone.mjs` checks it in headless Chromium. ADR-0010 records how `scaena` comes to carry the page.*
    - *`scaena export --format html --out FILE` writes one file. It holds the player's page, its code, and the engine, which `just web` builds from `web/standalone.html`, and which `scaena` carries when it is built after it. The export fills it in with the bundle as `scaena save` writes it (fonts subset, no manifest or history), each file gzipped in base64, and how each state reads. `--states` picks the states it plays, in order. The same bundle gives the same bytes. The revenue example is 3.4 MB, and the torture deck 4.8 MB. A `scaena` built before the page exits 3 and says to run `just web`.*
    - *The engine in the file is the player's module alone, without the editor's operations: 2.14 MB gzipped, against 2.75 MB. `just wasm` builds both.*
    - *A page opened from a file starts no module worker, and no worker from its own address. So the worker is `src/worker.ts` built as a classic script into the page's code, and the page hands it the engine compiled. The browser's storage refuses a file's page, and the file keeps nothing there. Its content security policy lets nothing load.*
    - *How a state reads is HTML made from the data a tagged PDF is built from: headings, paragraphs, figures named by their alt text, and tables by rows. The page keeps the state shown in a live region, out of sight. What reads as it did stays put, so a screen reader says what each state changed.*
    - *Opened from disk with the network off, the torture deck's file shows its first frame about 0.55 s after navigation, and all 68 golden frames within SPEC §13.5 of the goldens. The engine it carries paints each frame byte for byte as the web player's engine does. Its screenshots can differ from the web player's by a step in a few pixels, between a page from a server and one from a file: that is the browser's compositing, not the engine. WebGPU paints a file, and the presenter view opens from one. Nothing asks for more than the file and its worker's blob: gate 2's third criterion, in headless Chromium. `just web-smoke` and CI's wasm job run it.)*
- [x] 2.6 BYOK assistant in the Worker: Anthropic (direct browser header), OpenAI, Gemini adapters; function-calling onto the same operations; session-only key option; skills loaded from the bundle/repo.
  *(Done: SPEC §9.2 and §11 say how it works, and ADR-0011 why it is built so. `web/assistant.mjs` checks it in headless Chromium.*
    - *Its tools are the MCP server's, on the bundle the page holds, by the same names and arguments less `bundle`, `out`, and `painter`, with their schemas from `docs/schema/mcp/`. `resource_read` reads the server's resources and the skills a bundle carries. The worker runs each in the engine's module (`Player.tool`) by `scaena-ops`' operations. Each operation that writes has a twin that computes what to write and writes nothing (`patching`, `fixing`, `attaching`, `spine_updating`), which is what the page calls: the CRDT, which writing reaches, stays out of the module. A native test runs PLAN 1.19's loop through the tools, and holds each one's arguments to its MCP schema.*
    - *What an agent reads moved out of `scaena-mcp` into `scaena-resources`, which the MCP server serves and the page loads as a WASM module of its own (0.21 MB gzipped) with the assistant's code (61 KB) the first time the user asks something.*
    - *The editor's module shrank while it took the tools on: 2.73 MB gzipped, from 2.75. Each crate that parsed a theme had compiled its own copy of the theme model's parser, and two paths parsed the deck from a JSON value. Each now goes through one function in `scaena-core`, which took out more code than the tools added.*
    - *Providers: Anthropic's Messages API, with its header for direct browser calls, its system prompt cached; OpenAI's Chat Completions, or any server that speaks it at an address the user gives; Gemini's generateContent, its thought signatures sent back as they came. Each is a `fetch`, with no SDK. The page lists the models the key can use, and names none itself.*
    - *Keys: for the tab by default; on the device when asked, encrypted (AES-GCM) with a key the browser keeps in IndexedDB and never hands out; forgotten on request; never in a bundle. The panel says where the key goes, and what keeping it risks.*
    - *The source is read-only while the assistant works. Each edit it makes is compiled, shown, and linted in the worker before its next call, and comes into the source as an edit, which undoes as one. Stop ends it after the call it is in. The test caught a first version that let the editor compile each source it took: one could arrive after a later edit and briefly undo it.*
    - *The test runs PLAN 1.19's agent loop through the editor against a scripted server for each provider's wire format. It checks what the browser sends and what the editor ends with, then Stop and the key's storage. `just web-smoke` and CI's wasm job run it.*
    - *On B1, in headless Chromium, a patch takes 2.5 s (it lints before and after), a lint 1.2 s, and a render 1.3 s after an edit.*
    - *Gate 2's fourth criterion asks for the user's own key: a run with a real one closes it, in the gate log.)*
- [ ] 2.7 Static deploy (existing hosting) with a demo deck.
  *(Built and tested; the deploy itself is open. SPEC §9.2 says what the site is and what a host must do. `web/site.mjs` checks it in headless Chromium.*
    - *`just site` builds the player and the editor into `target/site`, with a demo deck in `decks/NAME/`: the trails example by default, saved as `scaena save` saves it, fonts whole. The pages open it when the address names no bundle, as the build says (`VITE_BUNDLE`). The player's Edit opens the editor on its bundle, and the editor's Play opens the player on the bundle as last saved.*
    - *Its paths are relative, so any static host serves it, at its root or under any path. The host must serve it over HTTPS, with `.wasm` as `application/wasm`; it needs no code on the server, no rewrites, and no other headers. The site is 13 MB: the engine is 7.7 MB of it (2.7 MB gzipped), the history's module, since 2.9, 2.9 MB (1.0 MB gzipped, loaded only to save), and the trails example 0.85 MB.*
    - *The `site` workflow builds it by hand from the branch chosen, keeps it as the run's artifact, and publishes it to GitHub Pages only when asked.*
    - *`web/site.mjs` serves it from a path under a plain static server, with each file's media type, as a host would:*
      - *The player plays the deck's 15 states, each painting a frame.*
      - *Edit opens the editor, which lints the deck clean, and Play opens the player again.*
      - *Download loads the subsetter from the site. The assistant, asked a question of a scripted server, loads its code and what it reads from the site.*
      - *Nothing is asked of another origin or outside the site's path, and no file is missing.*

      *`just web-smoke` and CI's wasm job run it.*
    - *Open: putting it on the host Jay picks. The repository is private, and a site on Pages is public. Either run the `site` workflow with "Publish to GitHub Pages" ticked, once the repository's Pages source is GitHub Actions, or copy `target/site` (or the run's artifact) to the existing host. Then tick, with the address.)*
- [x] 2.8 Accessibility pass: keyboard, reduced-motion preference (collapses transitions to cuts), screen-reader order from the spine.
  *(Done: SPEC §3.12 and §9.2 say what a reader gets. `web/a11y.mjs` checks it in headless Chromium.*
    - *How a state reads moved from `scaena-export` into `scaena-core::reading`, where the PDF tags, the single-file export, and the engine's module all read it. The module writes it with `Player.reading`, in the format shown: 6 KB more gzipped on the editor's module (2.74 MB) and 7 KB on the player's (2.15 MB), against SPEC §15's 3.0 MB. A native test holds each of the revenue example's states to what `export --format html` writes.*
    - *Screen reader: the web player hides its canvas and keeps the state's reading in a polite live region, as a single file does, from one shared reader (`web/src/reading.ts`). What reads as it did stays put. The state picker is the spine's outline: a group for each section, each state named with its beat's claim, then the states no beat names.*
    - *Less motion: `prefers-reduced-motion`, or `?motion=reduce`, makes each cue a cut. The worker paints the state at rest once, says so once, and waits out its cue and hold, so the deck keeps its pace; `?motion=full` plays the cues anyway. On the revenue example, `revenue` stayed 7.52 s against its cue and hold's 7.44 s, with no frame inside a cue.*
    - *Keys: a focused button or link keeps Enter and Space, and the deck's keys work everywhere else, so → goes on after a click on ▶ (before, a clicked button swallowed them). The scrubber says which state it is at. In the editor, a finding is a button, F8 and Mod-Shift-M are CodeMirror's lint keys, the source is named, and the arrow keys move between the tabs.*
    - *axe-core 4.13 finds nothing against WCAG 2.1 A and AA on the player, the presenter view, the editor with each tab, and a single file. Its first run found the source without a name and its scroller without focusable content; both are fixed. `just web-smoke` and CI's wasm job run the check.)*
- [x] 2.9 The page records into a bundle's history: the CRDT as a WASM module of its own, loaded to save a bundle that keeps one, as the subsetter is, so the page's edits go in as `user`'s (SPEC §8, §9.2). *(Found by 2.4: in the engine's module, the CRDT takes it to 3.7 MB gzipped.)*
  *(Done: SPEC §8.2 and §9.2 say what a save records. `web/history.mjs` checks it in headless Chromium.*
    - *`scaena-history` is the CRDT as a WASM module of its own, 1.01 MB gzipped. It records changes in a history's bytes (`record`) and lists them (`changes`). The worker loads it the first time it saves, or downloads, a bundle that keeps a history, and hands it to `Player.save`. The engine's save calls it where `scaena save` records, before the manifest names the history's hash. The changes go over as JSON, each the deck it leaves as deck.json's text, so the engine's module compiles in no second serializer of the model. The editor's module stays at 2.74 MB gzipped, and the player's at 2.15 MB.*
    - *What a save records, after what the history held (`DeckDoc::record`, in order; one that changes nothing is no change):*
      - *`deck.json` as the bundle held it, by `fs`, if it was edited outside Scaena since;*
      - *each edit the assistant made since the bundle was opened or saved, by `agent:` and the model's name, with its tool's message and renames, after the user's edits until then, by `user` (`edit`);*
      - *then the deck as saved, by `user` (`save`).*

      *Each is stamped when it was made. Loro never stamps a change before the one it follows, so the change by `fs` takes the earliest time.*
    - *The session keeps each edit an operation makes (`Session::keep`) only for a bundle that keeps a history. The page passes `Player.tool` who calls it and when, and the assistant calls each tool as `agent:` and its model's name.*
    - *Native tests (`scaena-wasm`'s `store`, `scaena-history`, `scaena-store`'s `crdt`) check what a save records and by whom. A rename stays the node it was (one operation, its id). The history holds the deck as saved, so the next command that records takes in nothing by `fs`. A deck edited outside goes in first by `fs`, a download records too, and a bundle without a history records nothing. The history module's record matches `Bundle::record`'s for the same edit.*
    - *`web/history.mjs` drives the editor on the revenue example saved by `scaena save --history`: the user types, a scripted assistant renames the title, and the user types again. The save loads the module, which nothing before it did, and records the edit, the patch by `agent:scripted`, and the save. Copied to disk, `scaena save` takes in nothing by `fs`, and a download's history is the one kept plus the save that subset its fonts. A bundle that keeps no history saves without the module. `just web-smoke` and CI's wasm job run it.)*

- [x] 2.10 A guide for people who write decks, `docs/authoring.md`: how to edit, save, and see a deck, in the editor and on the command line; what checks one (the grammar, the schemas, lint); the language, in one deck; the theme's names; recipes. *(Jay, 2026-10-04: "Is there a verifiable grammar? A DTD? Lint? How do I edit, save, and see changes?")*
  *(Done:*
    - *`crates/scaena-cli/tests/skills.rs` holds the guide to what exists, as it holds the skills: each command and flag it shows, inline or on a command line, each lint code, file, and SPEC section.*
    - *In a bundle of `docs/examples/`'s files, its deck compiles and lints with no findings. Each recipe compiles on top of it, with no finding but W401, a state no beat names.*
    - *The two errors it shows are what `compile` prints, line for line, with their exit codes. Its tables of names are the three shipped themes'.*
    - *Ten planted defects each failed it: a flag in the prose, a flag and a path on a command line, a lint code, a SPEC section, a deck that does not compile, a recipe that lints with an error, an error's words, a slot, and a spring.*
    - *`web/serve.py` serves a site on this machine only, and `just site` copies it in beside the pages.*
    - *What the guide found missing is 2.11 and 2.12.)*
- [x] 2.11 `scaena serve <bundle>` (SPEC §7.1): the player and the editor on a bundle on disk, on this machine. The bundle is watched: a `deck.scn` saved in any text editor compiles, and the pages show the new deck, so a deck written by hand plays as it is saved. *(Found by 2.10: until then a deck written on the command line was seen by `render` one state at a time, or by exporting one file.)*
  *(Done: ADR-0012; SPEC §7.1, §9.2.*
    - *`crates/scaena-serve` serves the pages and the bundle's folder: HTTP/1.1 by `hyper` on the MCP server's tokio, on 127.0.0.1 alone.*
      - *It answers only a request that names `localhost` or 127.0.0.1 at its port, and takes a write (`PUT`, `DELETE`) only from its own pages.*
      - *It writes only inside the folder, each file whole.*
      - *`npm run build` copies the pages into the crate, which gzips them into the binary, as the single-file page is carried (ADR-0010). A `scaena` built before them exits 3.*
    - *The folder is scanned every 150 ms. A change counts once two scans agree, and goes out as a server-sent event. A saved `deck.scn` compiles into `deck.json` first, as `scaena compile` does; one that does not compile is said at its line, in the terminal as `compile` says it and in the pages, and the deck is kept. A page's own writes carry its id, so they come back to it once, as its own.*
    - *The player (`?serve`) has the worker read the bundle again and shows the state it was on; the live region reads it anew. The editor opens the folder's `deck.scn` as its source and saves the bundle back to the folder, in any browser, then `deck.scn`. A change on disk comes into an editor with nothing of its own not saved, and is offered over changes that are.*
    - *Tests: `scaena-serve`'s, over a socket (the folder's files and nothing outside it, refusals for another host or origin, a page's write heard once as its own, a source compiled or said broken at its line); the CLI's, for both builds; and `web/live.mjs`, the whole loop in headless Chromium. `just web-smoke` and CI's wasm job run it.)*
- [x] 2.12 The editor starts a deck: **New**, a bundle from a theme and its fonts (as `deck_create` makes one), and **Save as**, to a folder on disk or under another name in the browser. *(Found by 2.10: until then a deck started as a copy of another bundle.)*
  *(Done: SPEC §9.2.*
    - *`scaena-ops::create::creating` is `create` with nothing written. The page calls it through `Player.create` with the theme, the fonts it names, and a title.*
    - *New's dialog takes a title and one of the three themes that ship.*
      - *The build carries them and their fonts beside the pages (`web/src/themes.ts`, 373 KB gzipped). The worker fetches them only to make a deck, and a single file carries neither.*
      - *The deck is kept nowhere, named for its title, until its first save puts it in the browser's storage.*
    - *Save as saves where the page is told and keeps the bundle there: another name in the browser's storage, or a folder on disk, kept by name as an opened one is. A served bundle saves to its folder alone.*
    - *Tests: a new deck from each shipped theme lints with no error (`scaena-wasm`), and `web/new.mjs` runs New and Save as in headless Chromium.)*

- [x] 2.13 A deck starts anywhere: `scaena new <dir> [--theme NAME|FILE] [--title T]`, and `deck_create` takes a theme that ships by its name. The CLI and the MCP server carry Dusk, Daybreak, and Ember and their fonts, so neither a person nor an agent needs a theme file of the repository's. *(Found by 2.12: the editor could start a deck where the command line and an MCP-only agent could not.)*
  *(Done: SPEC §7.1, §7.2.*
    - *`scaena-ops`' `shipped` feature carries the three themes and their fonts (`shipped.rs`). The CLI and the MCP server turn it on; the browser's module does not, as the web build carries its own copy beside its pages (2.12).*
    - *`create` takes the theme that ships by its name, in any case, where no file of that name is. A name that is neither says which themes ship.*
    - *Each font carries its copyright and its license (OFL) in its name table, and covers Latin (U+0020–017F, U+2010–203A, and €).*
    - *Tests: `scaena new` with each theme by name, then validated and linted with no error; a theme file with the fonts beside it; a place that is not empty, and a theme that is neither, exit 2 (`crates/scaena-cli/tests/new.rs`). MCP's agent test makes its deck from `dusk` by name.)*
- [x] 2.14 Font licensing (SPEC §16 Q3, the fonts track): lint warns, as W230, where a font's OS/2 embedding bits (`fsType`) do not allow what a bundle does with it, a restricted license, preview and print only, no subsetting, or bitmaps only, and blocks nothing. *(Done: the engine reads each font's bits with skrifa as it registers it, and lint reports each font the deck lists once, at `/fonts/i/file`. The trigger's font is made from nothing by `scripts/build_lint_fonts.py`, plain boxes, restricted and not to be subset, so no one's font is bent to say it. Every font the repository ships is installable, so no deck's findings change.)*
- [x] 2.15 Shaders on the CPU, faster and to the same bytes: `mesh` and `grain` render a block of a row at a time, as SIMD, with sRGB encoding by table. *(Found by SPEC §15's B3, a full-canvas mesh and grain on every state: its frames painted in 209 ms on the Linux container and 129 ms on CI's M1, against 25 ms, and its video ran at 0.17× realtime. The shipped themes' backdrop is a mesh, so a deck's title slide is the slowest thing the browser's CPU fallback plays.)*
  *(Done: SPEC §3.8.*
    - *`Frame::render` works through 128 pixels of a row one step at a time, each pixel doing `Frame::pixel`'s arithmetic in its order. `pixel` stays the reference, `mesh.wgsl`'s twin line for line. The table encoder holds, for each 4096th of the unit, the thresholds at or below its start and the one inside it.*
    - *The same bytes: `render_is_pixel_for_pixel` holds each kind's render to its `pixel` in boxes scaled, offset, and turned, whose rows are not whole blocks. The encoder is the threshold search at and beside every threshold and every 4096th, and for all 2³² floats in a test run by hand (24 s optimized). No golden moves.*
    - *Time, by criterion on the Linux container, one thread. Alternating the two builds six times, B3's slowest frame takes 89 ms against 208 (medians), and B1's 40 states 539 ms in both: B1 and B2 draw no shader. One run each, the base first: B3's eight states 1.70 → 0.69 s, its video cue 10 → 23 frames a second (0.39× realtime, against 0.5×), and B4's slowest frame 152 → 117 ms. Alone at 1080p, a mesh renders in 37 ms against 132, and grain in 10 against 31. Runs of one build an hour apart differed by up to 13% on this container, so a comparison is made beside its base.*
    - *Left: B3 is still over its frame budget. On the container a frame is now the mesh (37 ms), vello compositing a full-canvas image and another in an overlay layer (about 17 ms), and the page faults of the 8 MB buffers each frame allocates afresh. The budget allows 8 threads, and a shader's rows are independent; the GPU path does the rest.*
    - *Tried and left out, as nothing measurable on B3: premultiplying a shader's pixels two channels to a word (exact, and twice as fast alone), marking an opaque shader's image opaque for vello, and keeping the render context between frames.)*
- [x] 2.16 `noise` on the CPU, faster and to the same bytes, as 2.15 made `mesh` and `grain`. *(Found beside 2.15: the shipped themes' `texture` preset, a full-canvas noise of four octaves, took 577 ms a 1080p frame on the Linux container, the slowest shader the themes draw. The trails example's closing slide is one.)*
  *(Done: SPEC §3.8.*
    - *`Frame::render` takes each octave across 128 pixels of a row in three passes. First, each pixel's cell, corners, and simplex, picked by masks rather than branches, as SIMD. Then the corners' hashes, which `Lattice` works out once for each cell, since neighboring pixels mostly share one. Last, the corners' shares, as SIMD. Then come the ramp, Oklab to linear light as SIMD, and the bytes by 2.15's table. `pixel` and `simplex` stay the reference, `noise.wgsl`'s twin line for line.*
    - *`floor` is written so the compiler can run it as SIMD, and it gives the floor as an `i32` from the same conversion. It is `f32::floor` for all 2³² floats, in a test run by hand (22 s optimized).*
    - *The same bytes: `render_is_pixel_for_pixel` at each count of octaves; with cells far wider than a pixel and far narrower; in a box turned and offset whose rows are not whole blocks; with grain and without; with colors opaque and not; and on a field moved to where its cells are negative. The 1080p `texture` frame matches byte for byte. No golden moves.*
    - *Time: alone at 1080p, alternating the two in one process, 185 ms against 591.*
    - *Left: Rust's float-to-integer casts saturate. x86-64 at its baseline (SSE2) has no instruction that does, so the compiler converts one lane at a time, about a tenth of the frame here. WebAssembly's SIMD converts four lanes in one instruction, which the module uses, and so does AArch64. A shader's rows are independent, so threads are next. `gradient` (a radial one takes 122 ms at 1080p) is in no shipped theme and keeps its reference.)*
- [x] 2.17 The CPU painter makes no copy of a frame: a shader's bytes become its image where they lie, and a frame's pixels become its raster. *(Found beside 2.15: the painter copied each shader's pixels and each frame's, 8 MB apiece at 1080p. On the Linux container glibc gave that memory back between frames, so each frame faulted it in again. B1's and B2's slowest frames painted in 13.5 and 13.9 ms, against SPEC §15's 12.)*
  *(Done: ADR-0004 finding 16. No byte moves. By criterion on the Linux container, alternating the two builds: B1's 40 states 546 → 141 ms; the slowest frames of B1 13.5 → 2.7 ms, B2 13.9 → 4.1, B3 91 → 81, and B4 88 → 75; B3's video cue 2.25 → 2.08 s.)*
- [x] 2.18 A shader's rows on every core: off the web, the CPU painter, PDF, and SVG compute each shader's box in bands of rows, one thread each, up to 8 (SPEC §15 allows 8). *(Found beside 2.15: B3's frame was over its 25 ms with the mesh worked out on one core of the eight its budget allows.)*
  *(Done: SPEC §3.8, §15.*
    - *`Job::render_rows` works out the box's rows from any row into a caller's buffer, and `Job::render_on(threads)` splits the box into bands of at least 64 rows on scoped threads. No row reads another, so the bytes are `render`'s for any count: `rows_in_bands_are_the_render` holds every kind to it on 1 to 9 threads, in bands that do not split the box evenly, and `shader_threads_change_no_pixel` holds the painter. WebAssembly, which has no threads, takes one.*
    - *`CpuPainter::threads` sets the count, the host's cores up to 8 by default. Video's painters take 1: they already paint a frame on every core.*
    - *By criterion on the 4-core Linux container, alternating the two builds: the slowest frames of B3 79 → 44 ms and B4 79 → 29 ms (its `shaders` state). B1, which draws no shader, and B3's video, whose frames already used every core, did not move.)*
- [x] 2.19 Lint's contrast check reads each pixel's luminance from tables. *(Found by profiling B1's layout lint, at 999 ms against SPEC §15's 1 s on the Linux container: 84% of it was `pow` in the contrast check, called for each channel of each pixel under text, twice.)*
  *(Done: `Ratios` holds, for a run of text, each background byte's linear light and the text's over it, each worked out once by the arithmetic `judge` did for each pixel, so every ratio is the same `f64`. `ratios_by_table_are_ratio` holds it to `ratio` bit for bit over every byte of each channel, and the lint of B1, the torture deck, and the trails example prints the same findings, byte for byte. Each deck's whole lint, by the release CLI: B1 996 → 72 ms, the torture deck 1908 → 606 ms, and the trails example 415 → 212 ms.)*

### Exit criteria (gate 2)
*(Evidence so far, and the runs that close it: `docs/gate-2.md`.)*
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
- **Fonts:** bundle-only policy, subsetting, licensing warnings (SPEC §16 Q3; W230, PLAN 2.14); a very large catalog to pick from, kept out of the render path (SPEC §16 Q8).
- **Docs:** `docs/spec/format.md` (number/date formats) and `docs/spec/expr.md` (data transform expressions), written in Phase 1 when the code forced the decisions. The DSL's full grammar is SPEC §4's, rewritten to the implemented language in PLAN 1.5, so it has no file of its own.
- **Security:** no arbitrary code in documents; single-file export has no network; keys never in bundles.

---

## Working agreements (for humans and agents)

1. **Small, green PRs.** Every PR compiles, passes golden tests, and updates PLAN checkboxes.
2. **Determinism is a test, not a hope.** Any change to layout, text, timeline, or painters updates golden display lists in the same PR with a reviewed diff.
3. **Decisions get an ADR.** Library choices, format changes, anything that would be expensive to reverse: `docs/adr/NNNN-title.md`, status `proposed → accepted`.
4. **Schema is law.** `deck.json` changes are made in the typed model (`scaena-core::model`), which regenerates the schema (`just bless`), with example updates and a format version bump (minor for additive, major for breaking; before 1.0 a breaking change takes the minor, ADR-0007).
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
| WASM size | 0.8 (measured 1.04 MB gzip with vello; 1.66 MB at 1.8, 0.36 MB of it hyphenation patterns; budget 3.0) | feature-gate painters; opt-level `s`/`z` (−12–16%); lazy-load shaders; load hyphenation patterns on demand, on every target alike (ADR-0004 finding 11). `wasm-opt -Oz` grew the gzip size, so it is off. |
| Loro API friction with rich text/tree | 1.23 | Automerge fallback (ADR-0002) |
| Chart grammar scope creep | 1.9 | fixed kind list; everything else is annotations or deferred |
| Agent output quality | 1.15–1.19 | lint + render loop; roles not pixels; skills |
| Font licensing | 1.4 | honor embedding bits with warnings; document policy |
| Solo bandwidth | all | phases are sequential by design; Phase 4 is unscheduled |

---

## Gate log

| Gate | Date | Result | Notes |
|---|---|---|---|
| 0 | 2026-10-02 | met: go | All seven exit criteria met; evidence, timings, and what did not match in `docs/spike-report.md`. Phase 1 starts at 1.1. |
| 1 | 2026-10-03 | met | All five exit criteria met. The evidence per criterion is in `docs/gate-1.md`, and the agent runs are in `docs/examples/agent-run.md`. Phase 2 may start at 2.1. Of Phase 1's open tasks, 1.9 and 1.32 wait on Jay's review, 1.28–1.31 on his scheduling, and 1.33–1.36 come from the runs. |
| 2 | — | open | Criteria 2 and 3 are met in headless Chromium. Criterion 1 needs a real machine's browsers, read with the player's frame meter (`?fps`). Criterion 4 needs Jay's own key. The evidence and the steps are in `docs/gate-2.md`. |
| 3 | — | — | — |

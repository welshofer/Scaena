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
  - *Open: Jay's review of the gallery renders, then the tick. Next candidates, by the same rule: range frames for the value axis, white gridlines knocked through bars, and direct labels for donuts and scatters.)*

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
- [ ] 1.19 Agent-loop smoke test (SPEC §14, last bullet) in CI.

### 1D Exports
- [ ] 1.20 PDF painter (`krilla`): vector text with subsets, tagged structure from spine, shaders as images.
- [ ] 1.21 PNG/SVG per state; video via frame sequence → `ffmpeg` (mp4/webm/ProRes); `hold` dwell.
- [ ] 1.22 `export --format spine` + per-beat renders; integration note for the existing infographic/motion/podcast pipelines.

### 1E Store
- [ ] 1.23 `scaena-store`: Loro document with the container layout in SPEC §8.1; `deck.json` export/import; `fs`-authored changes; undo manager; fork/merge smoke test. (If Loro's rich-text or tree APIs fight the model, ADR-0002 names Automerge as the fallback — decide by end of week 2 of this phase.) The order of `nodes` is paint order at equal `z` and flow order in a container (`displaylist::paint_order`), and a CRDT map keeps no order of its keys: hold it apart, as a movable list of ids, and keep `rename_node` (1.16) a rename, not a remove and an add.

### 1F Benchmarks
- [ ] 1.24 Benchmarks (SPEC §15): B2 (chart-heavy, with 1.9) and B3 (shader-heavy, with 1.10) under `tests/bench/`; `criterion` benches for every stage on B1–B4 with a recorded baseline per CI runner, failing CI on a regression beyond that runner's measured noise. Gate 1 criterion 4 is judged on these. *(Added by 0.14: SPEC §15 asks for criterion benches from Phase 0 on; `crates/scaena-cli/examples/stages.rs` is the stopgap that recorded gate 0.)*

### 1G Design calls (Jay, 2026-10-02)
- [ ] 1.25 Baseline grid: snapping is opt-in per text role, on for `body` and `caption` in the shipped themes (Dusk, Daybreak), and display text aligns by its cap height. W221 stops being reserved: it flags a snapping role whose leading is not a whole number of grid lines. SPEC §3.4, §3.6, §7.5; goldens for the torture deck's text cases.
- [ ] 1.26 Data stays legible: the shipped themes keep the mesh off data slides, and lint warns about a shader painted behind a chart or a table, a new code beside W310. SPEC §3.8, §7.5; fixtures.

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
| 1 | — | — | — |
| 2 | — | — | — |
| 3 | — | — | — |

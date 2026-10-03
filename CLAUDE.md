# CLAUDE.md — Scaena

Scaena is a presentation engine: **a timeline of states over one persistent scene graph**, rendered deterministically by one Rust core to the browser, the Mac, PDF, and video. Read `docs/MANIFESTO.md` for why, `docs/SPEC.md` for what, `docs/PLAN.md` for when. Decisions live in `docs/adr/`.

## Start here

1. `docs/PLAN.md` is the live task list. Work the lowest unchecked task in the current phase. Do not start the next phase before the current gate's exit criteria are met and logged in the gate log.
2. `just check` must be green before any commit (fmt, clippy with `-D warnings`, tests, schema validation).
3. Keep `docs/SPEC.md`, the JSON schemas, and the examples consistent with the code. If a code change forces a format change, the same PR updates the schema, the examples, the spec section, and bumps the format version.

## Repo map

```
crates/scaena-core     document model, tracking, timeline math, display list, shader kinds (CPU ref + WGSL), document-level lints, spine projection   (no fonts, no fs, no clock)
crates/scaena-engine   theme cascade, layout, text, charts, shader nodes → ops, sampling, frame()      (Phase 0/1)
crates/scaena-paint    painters: vello_cpu (default), vello/wgpu (feature "gpu"); both run shader ops (Phase 0)
crates/scaena-export   pdf, png, svg, video, html                                                     (Phase 1)
crates/scaena-store    bundle I/O, and the loro CRDT document that keeps a bundle's history       (PLAN 1.23)
crates/scaena-ops      the operations every client exposes, over a bundle, with typed results         (ADR-0009)
crates/scaena-cli      `scaena` binary — the first client
crates/scaena-mcp      MCP server (rmcp): the operations as tools, the format as resources             (PLAN 1.17)
crates/scaena-wasm     wasm-bindgen bindings                                                           (PLAN 0.8)
crates/scaena-ffi      C ABI for Swift                                                                 (PLAN 3.1)
docs/                  SPEC, PLAN, MANIFESTO, adr/, schema/, examples/
skills/                agent skills (SKILL.md) that drive the CLI/MCP
tests/                 golden display lists, golden rasters, lint fixtures, parity harness, bench decks B1–B3
web/                   Vite + TS player/editor                                                         (Phase 2)
apps/mac/              SwiftUI client                                                                   (Phase 3)
```

## Invariants (violations are bugs, not style)

1. **Determinism.** `render(document, theme, state, t, viewport)` is pure. No wall clock, no unseeded randomness, no system fonts in the render path. (SPEC §13)
2. **Painters never shape text or lay out.** Glyph positions are final in the display list. (SPEC §6)
3. **Layout is per snapshot; frames only sample.** Never re-run layout inside the frame loop. (SPEC §5)
4. **Semantics over pixels.** Documents reference roles, slots, tokens, presets. Raw literals live only in `overrides` and are lint W300 elsewhere. (ADR-0005)
5. **Tracking semantics are in `scaena-core::tracking`** and nowhere else. `anim` never tracks. `layout` does.
6. **Nothing render-derived enters the CRDT.** Snapshots, layouts, display lists, lint results are recomputable. (SPEC §8.3)
7. **No server assumptions in v1.** Local-first; BYOK; keys never in bundles. (ADR-0006)
8. **PPTX/Keynote never touch the model or the engine. No HTML/CSS layout in the engine. No TextKit in the Mac app.** (MANIFESTO)
9. **Shader nodes have a CPU reference implementation first**, WGSL second, parity test third. No arbitrary shader source. (SPEC §3.8)
10. **Authority:** the logical document is the truth; `deck.json` is its canonical interchange form; the CRDT holds persistence authority while editing; `.scn` is an authoring projection. DSL round-trip is semantic (`compile(decompile(doc)) == doc`), never source-preserving. (SPEC §3.1, §4)

## Conventions

- Rust 2024 edition, stable toolchain. `cargo clippy --all-targets -- -D warnings` is clean. `cargo fmt` (rustfmt.toml: 120 cols).
- Crates depend downward only: `cli`/`mcp → ops → export/paint/engine/store → core`. `core` has no heavy dependencies. An operation lives in `scaena-ops` and returns a typed result; the CLI and the MCP server only parse, call, and print or return it (ADR-0009).
- Errors: `thiserror` in libraries, `anyhow` only in `scaena-cli`. Unimplemented paths return `NotImplemented("… — PLAN x.y")`, never `todo!()`, so the CLI exits 3 with a pointer instead of panicking.
- Every lint rule: a struct implementing `Rule` with a stable code from SPEC §7.5, plus fixtures under `tests/lint/<CODE>/trigger.deck.json` and `tests/lint/<CODE>/clean.deck.json`.
- Every change to layout/text/timeline/painters updates golden display lists in the same PR; the diff is reviewed, not regenerated blindly.
- New node types, properties, or state fields: change the typed model (`scaena-core::model`; the deck's skeleton is `scaena-core::document`) → `just bless` regenerates `docs/schema/*.json`, which nobody edits by hand (ADR-0007) → `docs/examples/*` → SPEC §3. Run `just schema` to validate.
- Commit messages: imperative, one line, optional body. Reference PLAN task ids (`PLAN 0.4`) and ADRs.

## Commands

```
just check          # fmt + clippy -D warnings (all features) + test + test-gpu + schema validation + the scripts' tests + wasm32 clippy; mirrors CI
just test           # cargo test --workspace
just schema         # validate docs/examples + tests/fixtures against docs/schema; torture-deck font coverage (python jsonschema, fonttools)
just cli ARGS       # cargo run -p scaena-cli -- ARGS
just example        # validate/lint/inspect the example deck
just test-gpu       # painter + CLI tests with vello on the GPU; skip without an adapter unless SCAENA_REQUIRE_GPU=1
just bless          # regenerate docs/schema from the model (and the MCP tools' in docs/schema/mcp), then re-bless golden display lists and rasters; only after reviewing the diffs
just wasm           # WASM engine + JS glue into crates/scaena-wasm/www/pkg (needs wasm-bindgen-cli 0.2.129)
just wasm-smoke     # the WebGPU page in headless Chromium: WASM display lists match native, every state paints
just spike          # parity harness: vello_cpu goldens vs vello on this GPU vs vello on WebGPU in Chromium (PLAN 0.9)
just bench [FILTER] # SPEC §15's stages on B1–B4, timed by criterion (PLAN 1.24); CI times each pull request ready for review beside its base on one machine (scripts/bench_gate.py)
just stages BUNDLE  # per-state medians and worst cases on one bundle (PLAN 0.14's tables); `just coldstart`: B1's WASM cold start
```

The CLI today: `scaena validate | lint | inspect | diff | save | compile | decompile | theme --apply | patch | export --format spine|pdf|png|svg|mp4|webm|prores` work, on a bundle directory or a `.scaena` zip. `validate` checks a bundle as it is on disk: both schemas, every reference to a file or theme name, and each resolved state against its nodes' types (PLAN 1.2). `save` subsets fonts and names files by their content without changing a frame (PLAN 1.4). `compile` turns `.scn` into `deck.json`, validating it and showing each finding at its source line, and `decompile` writes any deck as canonical `.scn`; the round trip is exact (PLAN 1.5, SPEC §4). Every node passes through the theme cascade (role → `style` → state → `overrides`, SPEC §3.6): `theme --apply` re-themes a bundle and reports the lint delta, and `inspect --resolved` shows each text node's look and its overrides (PLAN 1.6). A text role with `snap` sets its baselines, or its first cap height, on the theme's baseline grid, and lint W221 flags a snapping leading that is off it (PLAN 1.25). `inspect --timeline` shows each state's cue (where it falls on the deck's timeline, its transition, each motion as placed), and `--data` the rows each chart and table reads (PLAN 1.14). Under `--json` every command prints exactly one JSON value on stdout, its result or `{ "error": { "exit", "message", "plan"? } }` (SPEC §7.1). `scaena render` renders states built from text, chart, table, shape (filled and stroked in colors or gradients), image (PNG, SPEC §3.3), shader (every kind, from the node or a theme preset, SPEC §3.8), and container nodes (`stack`, `grid`, `frame`, `group`, laid out by `taffy`; a child names its container in `at.parent`, ADR-0008) to PNG, at rest or `--t` ms into the state's cue (its transition, eased or sprung, then its motions: presets, `anim` tracks, and choreography on one clock, SPEC §3.9), on the deck's canvas or, with `--format 9:16`, laid out again in one of its `formats` with the theme's template set for it (SPEC §3.4), with `--painter cpu` (the default) or, in a CLI built with `--features gpu`, `--painter gpu`. Between states, changed text morphs word by word, shapes point by point, shaders by their uniforms, and a group composites its members as one layer (PLAN 1.12); looks also tint paints toward a theme color and draw a shape's outline on. `lint` runs validation, the document rules, and the layout rules (overflow, collisions, contrast against what is painted behind the text, chart text included, chart text under 12 pt, shaders behind data, missing glyphs, lines, motion, narrative) in every format the deck lists. `lint --fix` applies the fixes lint has checked by laying the state out again (PLAN 1.15, SPEC §7.4–§7.5). `patch` applies JSON Patch and semantic ops (`add_node`, `rename_node`, `set_prop` in a state, `add_state`, …), all or none. It refuses a patch that would make the deck invalid and reports the lint delta (PLAN 1.16, SPEC §7.3; `docs/schema/patch.schema.json`). `scaena mcp` serves the same operations over stdio as 12 MCP tools (`deck_create`, `data_attach`, `deck_patch`, `deck_lint`, `deck_render`, which returns the PNG as an image, …), with the schemas, lint catalog, SPEC, and examples as resources (PLAN 1.17, SPEC §7.2; tool schemas in `docs/schema/mcp/`). `export --format pdf --out FILE` draws each slide at its last state, in spine order, as a PDF page with krilla: vector paths, text in subset fonts that copies as the deck reads, shaders as images at twice the canvas, tagged by how each node reads and outlined from the spine (PLAN 1.20, SPEC §3.12, §10). `export --format png|svg --out DIR` writes each state at rest, at `--size`: PNGs by the CPU painter, and SVGs with paths and gradients as vectors, glyphs as outlines with their text over them for a reader, and what SVG cannot draw (shaders, color glyphs, sweeps) as the CPU painter's pixels. `export --format mp4|webm|prores --out FILE` plays the global timeline, each state's cue then its hold, at `--fps`, painted by the CPU painter and piped to `ffmpeg`, with an optional `--audio` track and a chapter per beat, titled by its claim (PLAN 1.21–1.22, SPEC §10). `export --format spine --out DIR/spine.json` writes the spine projection (`scaena-core::spine`, `docs/schema/spine.schema.json`): every state and beat placed on the global timeline, and each beat drawn at rest into `renders/` beside it, as a thumbnail and in each other format the deck lists. The infographic, motion, and podcast pipelines read it and the CLI, never `deck.json` (`docs/projections.md`, PLAN 1.22). `save --history` starts a bundle's history in `history/deck.loro`, a Loro document laid out as SPEC §8.1 says. From then on, every command that writes the deck records the change by its author (`$SCAENA_AUTHOR`, or `agent:<client>` over MCP). A `deck.json` edited by hand goes in as a change by `fs`, and a renamed node stays the node it was (PLAN 1.23). `export --format html` and `serve` exit 3 with their PLAN task.

## Phase 0 in one paragraph

Prove text parity, adversarially. The stack is pinned (PLAN 0.1): `parley` (shaping by `harfrust`, fonts read by `skrifa`), `fontique`, `taffy`, `peniko`, `kurbo`, `vello_cpu`, `vello`; versions, feature choices, and open findings are in ADR-0004. System fonts are compiled out: get a font context only from `scaena_engine::fonts::bundle_font_context`. Build the typography torture deck under `tests/fixtures/torture.scaena/` (PLAN 0.2 lists every case and which ones are kill criteria). `scaena render --state … --painter cpu` emits a PNG and a display list (PLAN 0.6; golden rasters in `tests/golden/torture/`), and `--painter gpu` paints the same display list with vello (PLAN 0.7; draw glyphs one run per glyph, ADR-0004 finding 6). The engine runs as WASM, bit-identical to native, and paints through WebGPU in `crates/scaena-wasm/www/` (PLAN 0.8; `just wasm-smoke`). The parity harness compares all three painters on every torture state (PLAN 0.9; `just spike`). Chart data motion (values animating in, the next quarter by key) samples two laid-out snapshots through `Engine::transition` (PLAN 0.10); frames never lay out or shape. Charts stay minimal until the chart and table sprint (PLAN 1.9), where Jay sets the aesthetic. The mesh shader's CPU reference and WGSL twin sit side by side in `scaena-core::shader`, and `shader_parity` holds them together (PLAN 0.11); a shader's clock is the global timeline. In the authorability spike (PLAN 0.13), fresh agents wrote and edited a deck from the docs alone; `docs/examples/agent-authorability.md` records it and its findings. Gate 0 is met and logged (PLAN 0.14): `docs/spike-report.md` holds the verdict, the evidence per criterion, the SPEC §15 timings on B1 (`tests/bench/b1.scaena`) and B4, and what Phase 1 inherits. Phase 1 starts at PLAN 1.1. If parity cannot be reached, follow the no-go path in PLAN §0 — do not quietly lower the bar.

## Gate 1

Gate 1 is met and logged (2026-10-03). `docs/gate-1.md` holds the evidence per exit criterion. `docs/examples/agent-run.md` records three runs:

- **Making a deck.** An agent with MCP alone made `docs/examples/ridgeline.deck.json` from a CSV and a brief, exported it, and re-themed it.
- **A narrative repair.** A fresh agent repaired a narrative lint finding on the authorability deck.
- **What the runs found.** Eight problems are fixed, and the rest are PLAN 1.33–1.36.

Phase 2 may start at PLAN 2.1. Phase 1's open tasks continue: 1.33–1.36, and 1.9 and 1.28–1.32 as Jay reviews and schedules them.

## Working with Jay

Direct, rigorous, no filler. State the decision, the evidence, and the trade. If something in SPEC is wrong, say so and propose the ADR. Don't pad commits or reports; the task list and the gate log are the status report.

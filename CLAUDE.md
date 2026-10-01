# CLAUDE.md — Scaena

Scaena is a presentation engine: **a timeline of states over one persistent scene graph**, rendered deterministically by one Rust core to the browser, the Mac, PDF, and video. Read `docs/MANIFESTO.md` for why, `docs/SPEC.md` for what, `docs/PLAN.md` for when. Decisions live in `docs/adr/`.

## Start here

1. `docs/PLAN.md` is the live task list. Work the lowest unchecked task in the current phase. Do not start the next phase before the current gate's exit criteria are met and logged in the gate log.
2. `just check` must be green before any commit (fmt, clippy with `-D warnings`, tests, schema validation).
3. Keep `docs/SPEC.md`, the JSON schemas, and the examples consistent with the code. If a code change forces a format change, the same PR updates the schema, the examples, the spec section, and bumps the format version.

## Repo map

```
crates/scaena-core     document model, tracking, timeline math, display list, document-level lints   (no fonts, no fs, no clock)
crates/scaena-engine   theme cascade, layout, text, charts, shaders, sampling, frame()                 (Phase 0/1)
crates/scaena-paint    painters: vello_cpu (default), vello/wgpu (feature "gpu")                      (Phase 0)
crates/scaena-export   pdf, png, svg, video, html, spine                                              (Phase 1)
crates/scaena-store    bundle I/O now; loro CRDT document in PLAN 1.23
crates/scaena-cli      `scaena` binary — the first client
crates/scaena-mcp      MCP server (rmcp)                                                               (PLAN 1.17)
crates/scaena-wasm     wasm-bindgen bindings                                                           (PLAN 0.8)
crates/scaena-ffi      C ABI for Swift                                                                 (PLAN 3.1)
docs/                  SPEC, PLAN, MANIFESTO, adr/, schema/, examples/
skills/                agent skills (SKILL.md) that drive the CLI/MCP
tests/                 golden display lists, golden rasters, lint fixtures, parity harness
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
- Crates depend downward only: `cli → export/paint/engine/store → core`. `core` has no heavy dependencies.
- Errors: `thiserror` in libraries, `anyhow` only in `scaena-cli`. Unimplemented paths return `NotImplemented("… — PLAN x.y")`, never `todo!()`, so the CLI exits 3 with a pointer instead of panicking.
- Every lint rule: a struct implementing `Rule` with a stable code from SPEC §7.5, plus fixtures under `tests/lint/<CODE>/trigger.deck.json` and `tests/lint/<CODE>/clean.deck.json`.
- Every change to layout/text/timeline/painters updates golden display lists in the same PR; the diff is reviewed, not regenerated blindly.
- New node types, properties, or state fields: update `docs/schema/deck.schema.json` → `docs/examples/*` → `scaena-core::document` → SPEC §3. Run `just schema` to validate.
- Commit messages: imperative, one line, optional body. Reference PLAN task ids (`PLAN 0.4`) and ADRs.

## Commands

```
just check          # fmt + clippy -D warnings (all features) + test + schema validation + wasm32 clippy; mirrors CI
just test           # cargo test --workspace
just schema         # validate docs/examples + tests/fixtures against docs/schema; torture-deck font coverage (python jsonschema, fonttools)
just cli ARGS       # cargo run -p scaena-cli -- ARGS
just example        # validate/lint/inspect the example deck
just bless          # re-bless golden display lists, then rasters; only after reviewing tests/golden/**/actual/
just spike          # (Phase 0) run the parity harness once it exists
```

The CLI today: `scaena validate | lint | inspect | diff | export --format spine` work, and `scaena render --painter cpu` renders states built from text nodes to PNG (other node types and `--painter gpu` exit 3 with their PLAN task). `export` (other formats), `compile`, `patch`, `theme`, `serve`, `mcp` exit 3 with their PLAN task.

## Phase 0 in one paragraph

Prove text parity, adversarially. The stack is pinned (PLAN 0.1): `parley` (shaping by `harfrust`, fonts read by `skrifa`), `fontique`, `taffy`, `peniko`, `kurbo`, `vello_cpu`, `vello`; versions, feature choices, and open findings are in ADR-0004. System fonts are compiled out: get a font context only from `scaena_engine::fonts::bundle_font_context`. Build the typography torture deck under `tests/fixtures/torture.scaena/` (PLAN 0.2 lists every case and which ones are kill criteria). `scaena render --state … --painter cpu` emits a PNG and a display list (PLAN 0.6; golden rasters in `tests/golden/torture/`). Next GPU, then WASM/WebGPU, then the parity harness, then one bar→line morph, then one mesh shader, then the authorability spike (PLAN 0.13 — it needs only today's CLI). Write `docs/spike-report.md` and log gate 0 in PLAN. If parity cannot be reached, follow the no-go path in PLAN §0 — do not quietly lower the bar.

## Working with Jay

Direct, rigorous, no filler. State the decision, the evidence, and the trade. If something in SPEC is wrong, say so and propose the ADR. Don't pad commits or reports; the task list and the gate log are the status report.

# Gate 1

Phase 1's exit criteria (PLAN, "Exit criteria (gate 1)"), each with its evidence. **All five are met.** Phase 2 may start. Phase 1's open tasks stay open:

- 1.9 and 1.32 wait on Jay's review of the chart passes.
- 1.28–1.31 wait on his scheduling.
- 1.33–1.36 are what the gate's agent runs found.

## 1. An agent makes a deck with MCP alone: met

`docs/examples/agent-run.md`, attempt 3. Using only the scaena MCP server, a Claude Code agent made a 12-state deck from `docs/examples/data/ridgeline-rides.csv` and a one-paragraph brief, in 8.7 minutes and 80 turns:

- **Lint:** 0 errors.
- **Renders:** every state, at least once.
- **PDF:** 12 pages, tagged, 1.6 MB.
- **Video:** 1080p at 60 fps, 87 seconds, a chapter per beat.

The deck it left is `docs/examples/ridgeline.deck.json`. Two earlier attempts found eight problems in Scaena, fixed in #53–#58 before the third.

## 2. Re-theme with `theme_apply`, the lint delta explained: met

Same file, "Re-theme". `theme_apply` to Ember added 14 errors, each an E102 naming what Ember lacks and listing what it has:

- two shader presets;
- the layout `figure`, in six states;
- two `stat` slots, on three slides.

Once those names resolved, the states laid out under Ember for the first time. That showed 19 layout errors, from Ember's larger numerals in cards: 3 E100s and 16 E101s. The agent fixed both sets, and lint ended at 0 findings.

## 3. Goldens green on macOS and Linux; parity green on a GPU: met

At main's f348af6, the merge of #54 (ci run 118, `37110338312`), every job passed:

- **`rust (ubuntu-latest)` and `rust (macos-latest)`.**
  - The workspace's tests pass, including the golden display lists and the golden rasters. On macOS every torture raster is 0 px over ΔE 1 against the x86 goldens.
  - The `gpu` tests ran with `SCAENA_REQUIRE_GPU=1`. On macOS the parity harness compares the CPU goldens with vello on "Apple Paravirtual device (Metal, IntegratedGpu)", the runner's GPU, and every state passes. So does `shader_parity`, the CPU reference against WGSL for every shader kind. On Linux the same suites run on Mesa's lavapipe.
- **`wasm`.**
  - All 68 torture states' WASM display lists are identical to native.
  - WebGPU in headless Chromium paints each of them.
  - The parity harness compares all three painters (vello_cpu, vello on lavapipe, and vello on WebGPU) on every state, and all pass.

## 4. SPEC §15 budgets met on B1 and B2; B3 recorded: met

The `bench` workflow's macOS runner (Apple M1, virtual, 3 cores) at f348af6, run `37110338297`:

| Stage | Budget | B1 | B2 | B3 (recorded) |
|---|---|---|---|---|
| Resolve + layout, one snapshot (the slowest) | ≤ 15 ms | 411 µs | 923 µs | 152 µs |
| Resolve + layout, all snapshots | ≤ 400 ms | 15 ms | 7.06 ms | 1.54 ms |
| Sample one frame | ≤ 1 ms | 4.85 µs | 23.4 µs | 4.15 µs |
| CPU paint, one frame at 1080p (one thread) | ≤ 12 ms (B3 ≤ 25 ms) | 4.0 ms | 4.81 ms | 129 ms ✗ |
| Headless PNG render, cold | ≤ 300 ms | 19 ms | 20.6 ms | 161 ms |
| Lint, document-level | ≤ 100 ms | 443 µs | 274 µs | 170 µs |
| Lint, layout-level, all states | ≤ 1 s | 654 ms | 70.1 ms | 542 ms |
| MCP `deck_render`, cold process | ≤ 1 s | 22.8 ms | 21.3 ms | 207 ms |
| Video, CPU path, a frame (1× realtime at 60 fps; B3 0.5×) | ≤ 16.7 ms (B3 ≤ 33.3 ms) | 7.04 ms | 7.02 ms | 98.3 ms ✗ |

- **The `wasm` job:**
  - the engine is 1.95 MB gzip, against 3.0 MB;
  - B1's cold start to the first 1080p frame takes 188 ms (the median of 5), against 500 ms.
- **Not on the headless path, and so not judged:** GPU paint, recorded with its readback (22.7 ms on B1), and the GPU video path, which does not exist yet (SPEC §15).
- **B3.** Its shader-heavy frames miss both of their budgets: a frame paints in 129 ms and exports at 0.17× realtime. Each frame draws a full-bleed mesh and grain through the CPU reference. The SPEC's way out is shader tiles cached while uniforms do not change, but a mesh's uniforms change every frame. B3 is recorded here, as the criterion asks. It is the case for the GPU video path, and SPEC §15 has that path take over if the CPU path falls short.
- **Video B1.** On this run of main, B1's video bench was 52% slower than its five-run baseline (7.04 ms a frame against 4.65), on both of its runs, so its history started over. The bench on #54 itself had passed. Either way it is 2.4× realtime, inside its budget.

## 5. A narrative lint true positive, repaired by an agent: met

`docs/examples/agent-run.md`, "Narrative lint, repaired":

- **The finding.** The authorability deck's W423 is real: the opening beat cites `@revenue`, and its one state shows a title and nothing from the data.
- **The repair.** A fresh MCP-only agent, told only to judge and repair the narrative findings, dropped the citation in one op and left the deck's other finding alone.
- **The record.** The bundle's history records the change as `agent:claude-code`.

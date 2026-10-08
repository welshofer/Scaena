# Gate 3

Phase 3's exit criteria (PLAN, "Exit criteria (gate 3)"), each with its evidence so far. Every task of Phase 3 has landed (3.1–3.8). What the gate still needs is a real Mac: CI's macOS runner is a virtual machine with a paravirtual GPU, which compiles and tests the Swift but cannot read the gate's bars.

**Where it stands:**
- **Criterion 1** (same pixels as the web player) is met on CI's Mac, through the goldens both clients are held to. A real M-series Mac's GPU reads it again in the run below.
- **Criterion 2** (first frame under 300 ms) waits on that run. On CI's virtual Mac, the release build opens B1 to its first frame in 32–35 ms warm and 161 ms cold, once the GPU the app makes as it starts is made. That is not the gate's reading.
- **Criterion 3** (no text layout in SwiftUI) is met for everything that draws a deck, and held there by a test. The source pane is TextKit in chrome, which is Jay's call (below).

## 1. Same bundle, same frame, same pixels (within tolerance) as the web player: met on CI's Mac

- **Same frame.** The Mac edits and draws through the session the browser does (`scaena-session`, ADR-0021).
  - `scaena-ffi`'s `a_bundle_opened_from_its_files_draws_its_goldens` opens the torture deck from its files, as the app does. Every state's display list at rest digests to the goldens (`tests/golden/torture/raw.fnv1a`).
  - The WASM engine matches the same digests (PLAN 0.8, `just wasm-smoke`).
- **Same pixels.** The Mac paints with vello on Metal: `LayerPainter` builds the scene `GpuPainter` does, then blits it onto the layer.
  - On CI's macOS runner, vello on Metal paints every torture state within SPEC §13.5 of the CPU goldens (`cargo test -p scaena-paint --features gpu`, with `SCAENA_REQUIRE_GPU=1`).
  - ScaenaKit's `aSurfacePaintsOnMetalWhatTheCPUPainterPaints` holds a frame presented on a `CAMetalLayer` (B1's cover, read back from the layer) to the CPU painter's pixels.
  - The web player's frames, by WebGPU and by the CPU painter, are held to the same goldens (`just web-smoke`). The two clients meet there.
- **Still to do.** The run on a real Mac (below) shows the torture deck in the app beside the web player on the same Mac, by eye, and reads B1's cover through the layer on that Mac's GPU.

## 2. Open → first frame < 300 ms for a 40-state deck on an M-series Mac: needs a real Mac

`GateTests`' `aFortyStateDeckShowsItsFirstFrameSoonAfterOpening` times B1 (40 states, SPEC §15) as the app opens a deck, once the app has started:
- the bundle's files read;
- the session opened, and its timeline;
- the Metal surface made;
- the first state's first frame presented on a layer.

It prints the time, and where it went.

- **Where the time went.** The `mac-app` workflow first read the release build on CI's virtual Mac at 0.62 s warm, and 2.2 s cold:
  - files read and session opened, 2 ms; the timeline, 1 ms; the first frame, 31–50 ms;
  - the surface, 0.56–0.61 s: the first device and vello's pipelines. A second surface, made after it, took 65–90 ms.
- **So every layer paints on one GPU** (PLAN 3.2). The GPU is one device and one set of vello's pipelines for the process, which the app makes as it starts, off its main thread (`ScaenaSurface.warm`). A surface on it only configures its layer.
  - The test makes the GPU first, as the app does.
  - It prints the time that took beside the reading, and the reading from the app's start, both together.
- **The same run, with one GPU:**

  | | open → first frame | files and session | timeline | surface | first frame | the GPU, at the app's start | from the app's start |
  |---|---|---|---|---|---|---|---|
  | warm | 32–35 ms | 2–3 ms | 2 ms | 2 ms | 26–28 ms | 0.66–0.68 s | 0.70–0.71 s |
  | cold | 161 ms | 39 ms | 3 ms | 24 ms | 97 ms | 2.5 s | 2.7 s |

  A second surface, made right after the first frame, took 72–121 ms. That is most likely the first frame's work still finishing on the virtual GPU: wgpu waits for all of the device's work before it configures a layer. A real Mac reads it again.
- **The reading is open → first frame with the app running**, as SPEC §15 reads its budgets: warm unless noted. The cold reading, an app launched by opening the deck, is printed beside it. Whether the gate means that one is Jay's call.
- **CI's readings are not the gate's.** CI's macOS job runs the test on every pull request under three conditions that are not the gate's:
  - the engine linked unoptimized (the debug library);
  - every test at once;
  - a virtual machine with a paravirtual GPU.

  It holds the test to five seconds, so that a gross regression shows.
- **The gate's reading** is the release build, on an M-series Mac. From the repository's root:

  ```
  cargo build --release -p scaena-ffi
  libs=$(cargo rustc -q --color never --release -p scaena-ffi --lib --crate-type staticlib -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
  cd apps/mac/ScaenaKit
  swift test -c release --filter aFortyStateDeck -Xlinker -L"$PWD/../../../target/release" $(for l in $libs; do printf -- '-Xlinker %s ' "$l"; done)
  ```

  The line `gate 3, criterion 2: B1 (40 states), open to first frame: …` is the reading, and the line after it says where the time went. Run it three times, and log the median.

  CI's `mac-app` workflow runs the same command three times on its virtual Mac, beside the app it builds, and puts the readings in the run's summary. They show the command works and how near the release build comes, and are not the gate's.

## 3. SwiftUI code contains zero text layout; all geometry comes from the engine: met for what draws a deck

- **What draws a deck is the engine's.** Every frame, glyph, and box comes from the engine:
  - the canvas, the stage, and the presenter's view: vello on Metal;
  - the states drawn small and the next state: the CPU painter's pixels;
  - each outline, handle, and landing a drag shows: the engine's boxes (`boxes`, `hit`, `targets`, `snap`).

  SwiftUI draws chrome: lists, the inspector, the findings, the assistant's conversation.
- **Held by a test.** `crates/scaena-ffi/tests/chrome.rs` reads every Swift file of the app, and fails on any API that lays text out, measures it, or draws it outside the engine. It names Core Text, TextKit's layout managers and containers, `NSTextField`, string measuring, and string drawing.

**Jay's call: the source pane.** The source pane is an `NSTextView` of the deck's `.scn`: TextKit, in chrome, as the browser's source pane is CodeMirror. The two documents disagree about it:
- SPEC §9.3 bars TextKit and Core Text from the render path.
- CLAUDE.md's invariant 8 says "No TextKit in the Mac app."

The recommendation is to read invariant 8 as SPEC does: the deck is never laid out by TextKit; its source is text in an editor. If invariant 8 holds as written, the source pane needs an editor that is not TextKit, and the options are:
- the engine draws the `.scn` itself, with its own carets: a text editor in the engine, weeks of work;
- the browser's CodeMirror in a web view: HTML in chrome.

The test allows `NSTextView` in `SourcePane.swift` alone, so either decision is one line.

## The run on a real Mac

About fifteen minutes on an M-series Mac with macOS 15 or later, Rust, and Xcode's Swift; or with the `mac-app` workflow's `Scaena.app` and a release checkout for criterion 2.
1. `just mac`, then open `tests/fixtures/torture.scaena`, and the same deck in the web player (`just web-dev`). Look at a few states side by side: one with text, `hanging`; one with a chart, `chart`; one with a shader, `mesh`.
2. Open `tests/bench/b1.scaena`. Play it (⌥⌘P), with a second display if there is one.
3. Run criterion 2's command three times. Log the median.
4. Drag the cover's title off its slot with Shift, then undo. Ask the assistant something with a key from Settings. Export the PDF, and share it.

What the run finds is logged here and in PLAN's gate log.

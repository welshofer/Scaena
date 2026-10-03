# web/ — player and source editor (Phase 2)

Vite + TypeScript, no backend (SPEC §9.2). Today it is the player (PLAN 2.1–2.2) and the source editor (PLAN 2.3). The player is the engine as WASM in a Web Worker, painting into the page's canvas, handed over as an `OffscreenCanvas`, with its controls and a presenter view. The editor is the deck as `.scn`, compiled, shown, and linted as it is typed. Storage and the assistant come in PLAN 2.4–2.8.

## Run it

```
just web        # the WASM engine (`just wasm`), then the player into web/dist
just web-dev    # Vite's dev server, on the WASM engine `just wasm` last built
just web-smoke  # in headless Chromium: every torture frame by each painter, then the parity harness, the controls, and the editor
```

`just web` builds two static pages: serve the repository's root and open `/web/dist/` for the player, or `/web/dist/editor.html` for the editor. `just web-dev` serves the repository's bundles at their paths in it, as that root does. Needs Node 22; the versions are pinned in `package-lock.json`.

The page reads these parameters:

| parameter | |
|---|---|
| `bundle` | a bundle's directory or its deck file, by URL: `/tests/fixtures/torture.scaena` (the default) or `/docs/examples/revenue.deck.json`. Its files are where the deck names them, from the deck's directory. |
| `painter` | `gpu` or `cpu`. By default, WebGPU where the browser has an adapter, and the CPU painter elsewhere. |
| `state` | the state to open on; the first by default. |
| `view` | `presenter` for the presenter view. The player opens it with P. |

## Present with it

| | |
|---|---|
| → ↓ PageDown Space Enter, a click or a tap, a swipe left | go on: play the next state's cue, or finish the one playing |
| ← ↑ PageUp Backspace, a swipe right | go back: the state before, at rest |
| Home, End | the first state, the last |
| F | fullscreen, the slide alone |
| P | the presenter view, in a second window |

A state with a `hold` goes on to the next by itself once its cue and hold are over; one without, and the last, waits (SPEC §2.4). The state scrubber gives each state an equal step and runs through its cue within it. Picking a state plays its cue; picking a format lays the deck out again in it.

The presenter view follows the player frame by frame. It shows the state's notes (its beat's, where it has none), the next state at rest, and a clock, and its ◀ ▶ and keys steer the player. The two talk on a `BroadcastChannel` named for the deck.

## Edit with it

`/web/dist/editor.html?bundle=…` opens a deck as its canonical `.scn` (SPEC §4), the revenue example by default; `painter` works as in the player.

- **Each edit** compiles in the worker as you type. A source that does not compile says where, and the preview keeps the deck it had.
- **The preview** shows the state the cursor is in, at rest. The state picker moves the cursor to a state's line.
- **Lint.** An edit lints the state shown at once, and every state once typing stops for half a second. The status line says how long each step took.
- **Findings** stand in the gutter and in the list under the source, where the source sets what each is about. A click on one goes there, and a fix is one click: the fix comes back as source, and only the lines it changes change.
- **The inspector** shows the state's cue (where it starts, its span, its hold, its transition, and each motion), each node, each text node's look, and how many props its overrides set.

## How it fits

- `src/main.ts` is the player's page: the player, or the presenter view. `src/editor.ts` is the editor's: CodeMirror 6, with the `.scn` mode in `src/scn.ts`, and a stage for the preview.
- `src/stage.ts` is a canvas and the worker that paints it. The player has one; the presenter view has two.
- `src/worker.ts` holds the bundle, the engine (`crates/scaena-wasm`), the canvas, and the deck's clock. It paints with `vello` on WebGPU (`Canvas.attachOffscreen`, `Player.paint`) or with `vello_cpu`, whose pixels (`Player.pixels`) reach the canvas as an `ImageBitmap`.
  - The worker asks for a WebGPU adapter before WebGPU takes the canvas: a canvas WebGPU holds takes no other painter.
  - If WebGPU fails anyway, the page starts over on a new canvas with the CPU painter.
- `src/protocol.ts` is what the page and the worker say to each other:
  - `show`: a frame, a state at rest or a time into its cue.
  - `run`, `seek`, `pause`: the clock. A run plays from a state and a time in it, cue by cue and hold by hold, and the worker says where the deck is (`at`) with each frame.
  - `timeline`: the deck's slots.
  - `source`, `edit`, `lint`, `fix`, `inspect`: the editor's (`Player.source`, `compile`, `lint`, `fix`, `inspect`, from `scaena-wasm`'s `editor` feature). An edit compiles, repaints the state shown, and lints it; `lint` lints every state. Places in the source are UTF-16 offsets, as JavaScript counts them, with a line and a column.

  A place in the deck is a state and a time into its cue, never a place on the global timeline: states with no transition and no hold all stand at one instant, as 23 of the torture deck's do at 0 ms. Each request names a format, one of the deck's `formats`, or none for the deck's own canvas.
- `serve.mjs` serves the repository and launches headless Chromium for the two checks:
  - `smoke.mjs` opens the built player with each painter, shows every frame the golden rasters hold, and saves its screenshots under `target/web-smoke/`. The parity harness (`crates/scaena-paint/tests/parity.rs`) holds them to the goldens within SPEC §13.5.
  - `player.mjs` drives the controls and the presenter view on the CPU painter. Headless Chromium composites WebGPU on SwiftShader at about a frame a second, and the clock keeps the display's frames.
  - `editor.mjs` types into the editor on the revenue example: an overflow, its fix, a source that does not compile, the cursor leading the preview and the inspector. Then it times an edit's round trip on B1, which gate 2 holds under 200 ms, and the lint of every state that follows.

For tests and the console, the page sets `window.scaena`. It holds the open bundle's states, formats, notes, and painter, and these calls: `show(state, t?, format?)`, `timeline(format?)`, `seek(index, t?)`, `run(index, t?)`, `at()`, `on()`, and `back()`. The editor's has `source()`, `type(text)`, `cursor(offset)`, `fix(code)`, `last()`, `trips()`, `wholes()`, `shown()`, `inspector()`, and `at()`.

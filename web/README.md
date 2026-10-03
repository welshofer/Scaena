# web/ — player and source editor (Phase 2)

Vite + TypeScript, no backend (SPEC §9.2). Today it is the player's core (PLAN 2.1): the engine as WASM in a Web Worker, painting into the page's canvas, handed over as an `OffscreenCanvas`. Navigation, presenting, the editor, storage, and the assistant come in PLAN 2.2–2.8.

## Run it

```
just web        # the WASM engine (`just wasm`), then the player into web/dist
just web-dev    # Vite's dev server, on the WASM engine `just wasm` last built
just web-smoke  # every torture frame in headless Chromium, by each painter, then the parity harness
```

`just web` builds a static page: serve the repository's root and open `/web/dist/`. `just web-dev` serves the repository's bundles at their paths in it, as that root does. Needs Node 22; the versions are pinned in `package-lock.json`.

The page reads three parameters:

| parameter | |
|---|---|
| `bundle` | a bundle's directory or its deck file, by URL: `/tests/fixtures/torture.scaena` (the default) or `/docs/examples/ridgeline.deck.json`. Its files are where the deck names them, from the deck's directory. |
| `painter` | `gpu` or `cpu`. By default, WebGPU where the browser has an adapter, and the CPU painter elsewhere. |
| `state` | the state to open on; the first by default. |

Picking a state plays the cue into it, then rests. Picking a format lays the deck out again in it.

## How it fits

- `src/main.ts` is the page. It hands its canvas to the worker and asks for frames.
- `src/worker.ts` holds the bundle, the engine (`crates/scaena-wasm`), and the canvas. It paints with `vello` on WebGPU (`Canvas.attachOffscreen`, `Player.paint`) or with `vello_cpu`, whose pixels (`Player.pixels`) reach the canvas as an `ImageBitmap`.
  - The worker asks for a WebGPU adapter before WebGPU takes the canvas: a canvas WebGPU holds takes no other painter.
  - If WebGPU fails anyway, the page starts over on a new canvas with the CPU painter.
- `src/protocol.ts` is what the two say to each other:
  - `show`: a frame, a state at rest or a time into its cue.
  - `play`: a cue on the display's clock.
  - `timeline`: the deck's timeline.

  Each request names a format, one of the deck's `formats`, or none for the deck's own canvas. The worker answers a `show` once its frame is on the canvas, and a `timeline` with the deck's slots.
- `smoke.mjs` opens the built player in headless Chromium with each painter, shows every frame the golden rasters hold, and saves its screenshots under `target/web-smoke/`. The parity harness (`crates/scaena-paint/tests/parity.rs`) holds them to the goldens within SPEC §13.5.

For tests and the console, the page sets `window.scaena`: the open bundle's states, formats, and painter, `show(state, t?, format?)`, and `timeline(format?)`.

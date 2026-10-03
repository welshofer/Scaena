# web/ — player and source editor (Phase 2)

Vite + TypeScript, no backend (SPEC §9.2). Today it is the player (PLAN 2.1–2.2), the source editor (PLAN 2.3) with its storage (PLAN 2.4) and its assistant (PLAN 2.6), the page a single-file export fills in (PLAN 2.5), and the two pages as a static site with a demo deck (PLAN 2.7). The player is the engine as WASM in a Web Worker, painting into the page's canvas, handed over as an `OffscreenCanvas`, with its controls and a presenter view. The editor is the deck as `.scn`, compiled, shown, and linted as it is typed, and saved where the bundle is kept. The assistant works on it with the user's own key.

## Run it

```
just web        # the WASM engine (`just wasm`), then the player and editor into web/dist, and the single-file page
just web-dev    # Vite's dev server, on the WASM engine `just wasm` last built
just web-smoke  # in headless Chromium: every torture frame by each painter and from a single file, the parity harness, the controls, the editor, the assistant, the site
just site       # the player and editor as a static site with a demo deck, into target/site (see "Put it online")
```

`just web` builds two static pages: serve the repository's root and open `/web/dist/` for the player, or `/web/dist/editor.html` for the editor. It also builds the page a single-file export fills in, into `crates/scaena-export/player/`, which `scaena` carries when it is built after it (see "Export one file"). `just web-dev` serves the repository's bundles at their paths in it, as that root does. Needs Node 22; the versions are pinned in `package-lock.json`.

The page reads these parameters:

| parameter | |
|---|---|
| `bundle` | a bundle's directory or its deck file, by URL: `/tests/fixtures/torture.scaena` (the default, but on the site, whose default is its demo deck) or `/docs/examples/revenue.deck.json`. Its files are where the deck names them, from the deck's directory, and a saved bundle's manifest lists the rest. `opfs:NAME` is a bundle the browser keeps; in the editor, `folder:NAME` is a folder opened before. |
| `painter` | `gpu` or `cpu`. By default, WebGPU where the browser has an adapter, and the CPU painter elsewhere. |
| `state` | the state to open on; the first by default. |
| `view` | `presenter` for the presenter view. The player opens it with P. |
| `motion` | `reduce` for cuts in place of cues, `full` for the cues whatever the system asks. By default, the system's `prefers-reduced-motion`. |

## Present with it

| | |
|---|---|
| → ↓ PageDown Space Enter, a click or a tap, a swipe left | go on: play the next state's cue, or finish the one playing |
| ← ↑ PageUp Backspace, a swipe right | go back: the state before, at rest |
| Home, End | the first state, the last |
| F | fullscreen, the slide alone |
| P | the presenter view, in a second window |

A state with a `hold` goes on to the next by itself once its cue and hold are over; one without, and the last, waits (SPEC §2.4). The state scrubber gives each state an equal step and runs through its cue within it. Picking a state plays its cue; picking a format lays the deck out again in it.

Edit opens the editor on the bundle.

For a screen reader (PLAN 2.8), the canvas is hidden, and the page keeps how the state shown reads in a polite live region, out of sight: each heading, paragraph, figure (by its alt text), and table, in paint order, as a single file reads it (SPEC §3.12). What reads as it did stays put, so a screen reader says what each state changed. The state picker is the spine's outline: a group for each section, each state named with its beat's claim.

With less motion asked for (`prefers-reduced-motion`, or `?motion=reduce`), each cue is a cut to its state at rest, and the deck keeps its pace: a state that holds goes on when its cue and hold are over.

A focused button or link keeps Enter and Space, and the scrubber and the pickers keep every key; the deck's keys work everywhere else, so → goes on after a click on ▶. The scrubber says which state it is at.

`?fps` shows a frame meter beside the status, for gate 2's first criterion: while the deck plays, its frames a second over the run so far, its worst frame, how many frames came more than 25 ms after the one before (late for a 60 Hz display), and the mean paint. The worker times each frame of a run by the display's clock. `docs/gate-2.md` says how to read it on a real machine.

The presenter view follows the player frame by frame. It shows the state's notes (its beat's, where it has none), the next state at rest, and a clock, and its ◀ ▶ and keys steer the player. The two talk on a `BroadcastChannel` named for the deck.

## Edit with it

`/web/dist/editor.html?bundle=…` opens a deck as its canonical `.scn` (SPEC §4), the revenue example by default (on the site, its demo deck); `painter` works as in the player. Play opens the player on the bundle as last saved, in a new tab.

- **Each edit** compiles in the worker as you type. A source that does not compile says where, and the preview keeps the deck it had.
- **The preview** shows the state the cursor is in, at rest. The state picker moves the cursor to a state's line.
- **Lint.** An edit lints the state shown at once, and every state once typing stops for half a second. The status line says how long each step took.
- **Findings** stand in the gutter and in the list under the source, where the source sets what each is about. A click on one goes there, and a fix is one click: the fix comes back as source, and only the lines it changes change.
- **The inspector** shows the state's cue (where it starts, its span, its hold, its transition, and each motion), each node, each text node's look, and how many props its overrides set.
- **Keys.** A finding under the source is a button that goes to where it stands. F8 and Shift-F8 move between findings, and Mod-Shift-M lists them. Tab indents, so Escape then Tab leaves the source. The arrow keys move between the tabs.

## Keep it

The editor's bar opens and keeps bundles (PLAN 2.4, SPEC §9.2):

| | |
|---|---|
| Open folder… | a bundle's folder on disk, where File System Access is (Chromium). The page writes there when it saves, and keeps the folder by name across reloads (`?bundle=folder:NAME`), asking again for leave to write. |
| Open .scaena… | a `.scaena` file, copied into the browser's storage and kept there. |
| Kept in this browser… | the bundles the browser keeps, in its origin-private file system under `bundles/`. |
| Save, ⌘S | the bundle as `scaena save` writes it, fonts whole, where it is kept. A bundle from a URL goes into the browser's storage under its name, and the address says `?bundle=opfs:NAME` from then on. The source takes the save's renames, and nothing else in it changes. A source that does not compile is not saved. |
| Download .scaena | the bundle as a zip, its fonts subset by the subsetter's own module, which loads the first time. |

A file dropped on the source joins the bundle: an image under `assets/`, named by its SHA-256, a font under `fonts/`, a data file under `data/`. Its path, quoted, goes where it was dropped, as an image node's source: `photo image "assets/…png" at:in(figure)`. A `.scaena` file dropped opens instead. The page warns before it closes or opens over changes not saved.

A bundle that keeps a history (`history/deck.loro`, from `scaena save --history` on) has each save, and each download, recorded in it, and the status says so (PLAN 2.9, SPEC §8). The CRDT is a WASM module of its own (`scaena-history`, 1.00 MB gzipped), which the worker loads the first time it saves such a bundle. Your edits go in as `user`'s, and each edit the assistant made as `agent:` and the model's name, with what its tool did. A `deck.json` edited outside Scaena since goes in first, as `fs`'s. A bundle that keeps no history saves without the module, and starts none.

## Ask the assistant

The editor's Assistant tab (PLAN 2.6, SPEC §9.2, §11) is a model of the user's choosing at work on the open deck, with their own key.

- **Who answers.** Anthropic, OpenAI, or Gemini, at its own address or one the user gives: an OpenAI-compatible server works too. With a key typed in, the model picker lists the models the key can use. The page names none itself.
- **The key** goes from the page to that provider and nowhere else, and never into a bundle. It is kept for the tab alone, unless "keep on this device" is ticked: then it is encrypted (AES-GCM) with a key the browser keeps in IndexedDB and never hands out. The panel says that a script the page runs could still use it. Forget key forgets it everywhere.
- **Its tools** are the MCP server's (SPEC §7.2), on the open bundle, less `bundle`, `out`, and `painter`: `deck_read`, `deck_patch`, `deck_lint`, `deck_inspect`, `deck_diff`, `deck_render`, `spine_read`, `spine_update`, `data_attach`, and `resource_read` for the resources and a bundle's own skills. Their schemas are `docs/schema/mcp/`, read at build time.
- **While it works** the source is read-only. Each edit comes into the source as it is made, compiled, shown, and linted, and undoes as one. The conversation shows each call, what it came to, and each frame the model saw. Stop ends it after the call it is in. New conversation starts over.

## Export one file

```
just web                                                       # the page, then:
just cli export docs/examples/revenue.deck.json --format html --out revenue.html
```

`revenue.html` is the deck as one file that plays in a browser with no network: from a disk, a USB stick, or an email (PLAN 2.5, SPEC §9.2, §10). Open it from where it is. It is the player, with the same keys, holds, presenter view, and fullscreen; `?painter=` and `?state=` work as above. `--states mix,intro` makes a file that plays those, in that order. A `scaena` built before `just web` cannot export one, and says so (exit 3).

- **What is in it.** The player's page, code, and styles, and the engine, built by `vite.standalone.config.ts` from `standalone.html` and `src/standalone.ts`. The export fills it in with the deck's title and language, the bundle's files as `scaena save` writes them (fonts subset; each gzipped, in base64, by its path), and how each state reads.
- **The engine** is the player's module alone (`crates/scaena-wasm/player`, built by `just wasm` without the editor's operations): 2.12 MB gzipped against the editor's 2.70. The page compiles it and hands it to the worker.
- **The worker** is the same `src/worker.ts`, built as a classic script into the page's code, since a page opened from a file starts no module worker and no worker from its own address. It opens the bundle from the files it is handed (`open` with `files`) and keeps nothing in the browser's storage, which refuses a file's page.
- **No network.** The page's content security policy lets nothing load.
- **Read aloud.** Each state reads in a live region the page keeps out of sight, as the export wrote it: headings, paragraphs, figures by their alt text, and tables (SPEC §3.12). What reads as it did stays put, so a screen reader says what a state changed. The canvas is hidden from screen readers.

## Put it online

```
just site                        # the pages into target/site, with the trails example as the demo deck
just site tests/bench/b1.scaena  # another demo deck: a bundle's directory or a deck file
```

`target/site` is a static site (PLAN 2.7, SPEC §9.2). Copy it to any static host, at its root or under any path: its paths are relative. `index.html` is the player and `editor.html` the editor, and both open the demo deck in `decks/NAME/` when the address names no bundle. The deck is saved as `scaena save` saves it, with its fonts whole, so the editor sets any character they carry. Its files are named by their content, but for `deck.json` and `manifest.json`. `?bundle=` opens any other bundle the browser can fetch: one on the same host, or one whose host lets the site's origin read it (CORS).

- **What a host must do.**
  - Serve the files over HTTPS. WebGPU, the browser's storage, the folder picker, and the assistant's kept keys need a secure context (`localhost` is one).
  - Serve `.wasm` as `application/wasm`. Otherwise the engine compiles only once all of it has arrived.
  - Compress, where the host can: the engine is 7.7 MB, 2.7 MB gzipped.
  - Nothing else: no code on the server, no rewrites, no other headers.
- **What the site holds**: the pages, their code, the engine, and the deck, 13 MB in all, the trails example 0.85 MB of it. It also holds the subsetter, the history (2.9 MB of the 13), and what the assistant reads, which the pages load only when asked.
- **GitHub Pages.** The `site` workflow builds the same directory from the branch chosen, by hand only (Actions → site → Run workflow), and keeps it as the run's artifact. With "Publish to GitHub Pages" ticked, it publishes the site there. That needs the repository's Pages source set to GitHub Actions, and a site on Pages is public.

## How it fits

- `src/reading.ts` keeps how the state shown reads in the live region, from the engine (`Player.reading`) or, in a single file, from what the export wrote.
- `src/player.ts` is the player, or the presenter view, on any page that plays a bundle: `src/main.ts`, the web player's page, which names the bundle by its address, and `src/standalone.ts`, a single file's, which carries it. `src/player.css` is their styles. `src/editor.ts` is the editor's page: CodeMirror 6, with the `.scn` mode in `src/scn.ts`, and a stage for the preview.
- `src/stage.ts` is a canvas and the worker that paints it. The player has one; the presenter view has two. A page says how its worker starts: `src/spawn.ts`, a module of its own beside the engine's module, for the player and the editor; the inline worker and the compiled module it carries, for a single file.
- `src/assistant/` is the assistant (PLAN 2.6): `providers.ts`, one conversation in each provider's wire format; `converse.ts`, the loop of answers and calls; `tools.ts`, the MCP tools' schemas less what names a place on disk; `prompt.ts`, the system prompt; `index.ts`, what the worker loads the first time the user asks, with the resources' own WASM module (`crates/scaena-resources`); and, on the page, `panel.ts` and `keys.ts`. The worker runs each call in the engine's module (`Player.tool`), and compiles, shows, and lints each edit before the next call.
- `src/worker.ts` holds the bundle, the engine (`crates/scaena-wasm`), the canvas, and the deck's clock. It paints with `vello` on WebGPU (`Canvas.attachOffscreen`, `Player.paint`) or with `vello_cpu`, whose pixels (`Player.pixels`) go onto the canvas by its 2D context (`putImageData`).
  - The worker asks for a WebGPU adapter before WebGPU takes the canvas: a canvas WebGPU holds takes no other painter.
  - If WebGPU fails anyway, the page starts over on a new canvas with the CPU painter.
- `src/protocol.ts` is what the page and the worker say to each other:
  - `show`: a frame, a state at rest or a time into its cue.
  - `run`, `seek`, `pause`: the clock. A run plays from a state and a time in it, cue by cue and hold by hold, and the worker says where the deck is (`at`) with each frame. A `still` run cuts to each state at rest, says so once, and waits out its cue and hold without frames.
  - `timeline`: the deck's slots.
  - `read`: how a state reads at rest, as HTML (PLAN 2.8).
  - `source`, `edit`, `lint`, `fix`, `inspect`: the editor's (`Player.source`, `compile`, `lint`, `fix`, `inspect`, from `scaena-wasm`'s `editor` feature). An edit compiles, repaints the state shown, and lints it; `lint` lints every state. Places in the source are UTF-16 offsets, as JavaScript counts them, with a line and a column.
  - `ask`, `stop`, `forget`, `models`: the assistant's. An `ask` comes back as `assistant` events, one for each thing it says, each call, each result, and each edit (its source, with what the edit came to), until it is `done` or `failed`.
  - `open` names its source: a URL, a bundle the browser keeps, a folder's handle, a zip's bytes, or a single file's files and the states it plays; and hands over the engine's module when the page carries it. `save`, `zip`, `drop`: the bundle's (`Player.save`, `adopt`, `subsetting`, `addSubset`, `addFile`, `place`). The worker writes a save with `src/folders.ts`, the same calls for a folder on disk and the browser's storage, and goes on from it. It hands `Player.save` the history's module when the bundle keeps a history: the engine calls its `record` with the history and the changes to record (JSON), and writes what it returns. The assistant calls each tool as `agent:` and the model's name, with the time (`Player.tool`), and the session keeps each edit for that save.

  A place in the deck is a state and a time into its cue, never a place on the global timeline: states with no transition and no hold all stand at one instant, as 23 of the torture deck's do at 0 ms. Each request names a format, one of the deck's `formats`, or none for the deck's own canvas.
- `serve.mjs` serves the repository (or a directory, at a path), launches headless Chromium, and shows and screenshots the golden frames (`shoot`) for the checks:
  - `smoke.mjs` opens the built player with each painter, shows every frame the golden rasters hold, and saves its screenshots under `target/web-smoke/`. The parity harness (`crates/scaena-paint/tests/parity.rs`) holds them to the goldens within SPEC §13.5.
  - `player.mjs` drives the controls and the presenter view on the CPU painter. Headless Chromium composites WebGPU on SwiftShader at about a frame a second, and the clock keeps the display's frames. With `?fps`, the meter reads a played cue's frames; without it, there is none.
  - `editor.mjs` types into the editor on the revenue example: an overflow, its fix, a source that does not compile, the cursor leading the preview and the inspector. Then it times an edit's round trip on B1, which gate 2 holds under 200 ms, and the lint of every state that follows.
  - `storage.mjs` saves the revenue example into the browser's storage and reloads it, plays it from there, drops an image into it, downloads it with its fonts subset and opens the download, and saves a folder in place.
  - `history.mjs` opens the revenue example saved by `scaena save --history`. The user types, the assistant (a scripted Anthropic server) renames the title, and the user types again. The save into the browser's storage loads the history's module, as nothing before it did. After the history's first change, it records the user's edit, the assistant's patch by `agent:scripted`, and the save, each stamped when it was made. Copied out to disk, `scaena save` finds nothing to take in by `fs`, and a download's history is the one kept with the save that subset its fonts. A bundle that keeps no history saves without the module.
  - `assistant.mjs` runs PLAN 1.19's agent loop through the editor's assistant against a scripted server for each provider: a headline too long for its slot, the E100, its fix, a clean lint, and a render. It checks what the browser sends (the key in its header, Anthropic's opt-in header, the prompt, the tools, each result, the frame as an image, Gemini's thought signatures), that the source takes each edit and lints clean, that Stop stops, and how the key is kept.
  - `a11y.mjs` checks what a reader needs. axe-core finds nothing against WCAG 2.1 A and AA on the player, the presenter view, the editor with each tab, and a single file. The live region reads each state as the single file does, and keeps what reads as it did. The state picker is the spine's outline. Less motion cuts to each state at rest and keeps the deck's pace, and `?motion=` overrides the system. The keys work after a click on ▶, Enter on it goes on once, and the scrubber says where it is. In the editor, a finding is a button that goes to its line, F8 goes to the next, and the arrow keys move between the tabs.
  - `site.mjs` serves `target/site` from a path under a plain static server, with each file's media type, as a host would. The player plays the demo deck, each state painting a frame. Edit opens it in the editor, which lints it clean, and Play opens the player again. The subsetter and the assistant load from the site, and nothing is asked of anywhere else (but the assistant's scripted provider) or found missing.
  - `standalone.mjs` exports the torture deck and the revenue example as single files (with `cargo run`, so `scaena` is built after the page) and opens them from their addresses on disk with the network off. The engine a file carries paints every golden frame byte for byte as the web player's does (run in Node), and the torture deck's file shows every golden frame by the CPU painter for the parity harness. The revenue example's plays, reads as it plays, and opens the presenter view; WebGPU paints it; and nothing asks for more than the file and its worker's blob.

For tests and the console, the page sets `window.scaena`. It holds the open bundle's states, formats, notes, outline, and painter, and these calls: `show(state, t?, format?)`, `timeline(format?)`, `seek(index, t?)`, `run(index, t?, format?, still?)`, `at()`, `on()`, and `back()`. The editor's has `source()`, `type(text)`, `cursor(offset)`, `fix(code)`, `last()`, `trips()`, `wholes()`, `shown()`, `inspector()`, and `at()`; for its storage, `open(source)`, `save()`, `download()`, `drop(name, bytes, at?)`, and `where()`; and `assistant`, with `ask(text)`, `transcript()`, and `usage()`.

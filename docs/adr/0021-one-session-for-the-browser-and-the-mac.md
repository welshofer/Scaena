# ADR-0021: One session, for the browser and the Mac

**Status:** accepted · **Date:** 2026-10-08

## Context

The browser's editor keeps a bundle in memory, in `scaena-wasm`'s `Session`. The session holds:
- the deck, its theme, and every file handed over;
- the engine built from the deck's fonts and images;
- the state laid out at rest;
- the source compiled last, and what lint found in it;
- the edits a save will record in the history.

Its methods are plain Rust with typed results, and the native tests drive it. `Player` wraps each method for JavaScript: it turns arguments and results into JSON and errors into `JsError`. A few of its methods did more than that: they built the timeline's slides, the theme's text, and the layout suggestions' pictures themselves.

PLAN 3.1 asks for a C ABI the Mac client calls: load a bundle, states, timeline, frames, lint, patch, store ops. ADR-0009 made `scaena-ops` the operations every client calls. Each of those works over a bundle whole: it opens the bundle, works, and writes it back. An editor calls far more often than that. A drag asks where a node may go at every move, and a keystroke compiles and lints. The session is what makes those calls cheap: it keeps what was laid out, and asks only for what changed.

A Mac session written apart from the browser's would be a second implementation of everything the editor does:
- each gesture's patch, written where the value lives;
- lint in two steps;
- what a save records.

The two would drift.

## Decision

1. **`Session` moves to a crate of its own, `scaena-session`, unchanged.** The modules it is built from move with it (`editor`, `assistant`, `store`, `data`, `formats`, `theme`, `versions`), and so do its tests, the random-edits fuzz among them. Crates depend `wasm`/`ffi → session → ops → …`. Its features are the session's: `cpu` (the CPU painter) and `editor`.
2. **`scaena-wasm` keeps what only a page needs**: `Player`, the WebGPU `Canvas`, the hyphenation loader, and the helpers' entry point, all `#[wasm_bindgen]`. The logic `Player` held moves into the session (`slots`, `theme_text`, `layouts_painted`, `keep_history`), so a wrapper only converts.
3. **`scaena-ffi` (PLAN 3.1) wraps the same session for C**: a handle, the files handed over, and each call's result as the JSON `Player` gives. The Mac and the browser read the same results.
4. **The Mac app hands over files as a page does.** Swift reads the bundle (sandboxed, by security-scoped URLs) and hands each file over by its path; a save gives the files back to write. The session reads no filesystem.
5. **The `wasm` profile builds `scaena-session` for size**, as it built that code inside `scaena-wasm`.

## Consequences

- **+** One session. A fix reaches the browser and the Mac, and its tests guard both.
- **+** The FFI is thin, like the CLI and the MCP server: it parses, calls, and returns.
- **−** One more crate. The move costs the editor's module 5 KB gzipped (2,600,445 bytes, from 2,595,394), and the player's 1 KB: the boundary changes what the optimizer sees.
- **−** Results cross the C boundary as JSON, which Swift decodes. A frame's pixels and display list cross as bytes.

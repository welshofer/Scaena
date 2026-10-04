# ADR-0010: The single-file export's page is a web build that `scaena` carries

**Status:** proposed · **Date:** 2026-10-03

## Context

PLAN 2.5 asks for a deck as one HTML file that plays with no network: the player, the engine, and the bundle inlined. Gate 2's third criterion is that such a file opens offline from a USB stick.

The file needs three things that come from different builds:
- **The engine as WASM.** It comes from `cargo build --target wasm32-unknown-unknown` and `wasm-bindgen`. A native `cargo build` of `scaena` cannot make it.
- **The player's code.** This is TypeScript, bundled by Vite (`web/`).
- **The bundle.** Only this part differs per export.

Two browser rules shape the page:
- **No module worker.** A page opened from its address on disk (`file://`) has an opaque origin. Chromium starts no worker from such a page's address, and no module worker from a blob it makes. It does start a classic worker from a blob. A probe in headless Chromium confirmed this.
- **No storage.** The origin-private file system refuses such a page (`SecurityError`).

So the export must carry a page that was built ahead of time. The question is how `scaena` gets that page.

## Decision

1. **`just web` builds the page.** It runs `web/vite.standalone.config.ts`, which writes `crates/scaena-export/player/standalone.html`. That file is not committed. It holds:
   - the player's page, code, and styles in one file;
   - the worker, built as a classic script into the page's code (`?worker&inline`);
   - the engine, gzipped, in base64.

   The engine in the page is **the player's module alone**: `scaena-wasm` without its `editor` feature. That is 2.14 MB gzipped against the editor's 2.75 MB. `just wasm` builds both, and Cargo keeps each feature set's build.
2. **`scaena-export` carries the page when it is there as the crate builds.**
   - A build script watches `player/` and sets `cfg(player)` when the page exists.
   - `scaena_export::html::player()` then returns it, through `include_str!`.
   - A `scaena` built before the page exits 3 on `export --format html`, with PLAN 2.5 and the instruction to run `just web` and build `scaena` again. This is the same contract as any capability not built in (SPEC §7.1).
3. **The export fills the page in.** It replaces three markers:
   - the title and language;
   - the bundle's files, what `scaena save` writes (fonts subset, no manifest, no history), each gzipped in base64 by its path;
   - the states the file plays, and how each reads (SPEC §3.12).

   The same bundle gives the same bytes.
4. **The page compiles the engine and hands it to the worker.** The worker opens the bundle from the files it is handed and keeps nothing in the browser's storage. The page's content security policy lets nothing load.

## Consequences

- **+** One binary exports HTML with no files beside it, from any directory, as it exports a PDF.
- **+** The page is the player's own code, and its engine paints the 68 golden frames byte for byte as the web player's engine does (`web/standalone.mjs`). Every fix to the player reaches the export at the next `just web`.
- **+** Without `just web`, `cargo build` and `just check` still build and test everything. The tests take whichever branch the build has: the not-built error, or the file.
- **−** The order of builds matters. The CLI must be built after `just web`, or rebuilt after it. CI's `wasm` job does this, and its `test` job tests the not-built branch.
- **−** The binary grows by the page, about 3 MB.
- **−** A page and a `scaena` from different commits could disagree on the markers. The export refuses a page without exactly one of each, and says to build both again.

## Alternatives

- **Commit the built page.** Every build would always have it. But a 3 MB generated blob would be committed again on every engine change, and it would drift from the code beside it. Rejected.
- **Find the page at run time**, beside the binary or under `$SCAENA_PLAYER`. The binary would stay small, but an export would then depend on where `scaena` runs from. That is a second way to fail, for little gain. Rejected for now; an override is easy to add if packaging wants one.
- **Build the WASM from `build.rs`.** This would mean a nested Cargo build for another target inside a native build: slow, and fragile under `--locked` and in CI's caches. Rejected.
- **Run the engine on the page's main thread, with no worker.** The player's clock and painter would then exist twice. It would also lose the worker that keeps frames off the page's thread. Rejected: a classic worker from a blob works.

# ADR-0015: The editor's module is handed a language's hyphenation patterns

**Status:** proposed · **Date:** 2026-10-05

## Context

SPEC §15 holds the engine's WASM module to 3.0 MB gzipped. At PLAN 2.55 the editor's module was 2,989,754 bytes: 10 KB of room, with the rest of the editor's road (ADR-0013) still to come.

A build that keeps its names (`strip = false`), read with `twiggy`, says where the bytes are:
- **Data.** The module's data is 1.66 MB before gzip:
  - the 17 languages' hyphenation tries, 466 KB, 350 KB of the gzipped module;
  - the WGSL that vello's GPU painter is compiled from, about 425 KB;
  - Unicode tables, which parley, `icu_segmenter`, and harfrust read;
  - the JSON schemas the editor validates by, about 190 KB.
- **Code.** `scaena-core` is about 1.1 MB, `read-fonts` 605 KB, `skrifa` 484 KB, `serde_json` and harfrust 299 KB each. Copies of generic code are a small part: drop glue about 106 KB, sorts about 175 KB.

Of these, only the tries are data that most sessions never read:
- A text hyphenates only where its role or the node sets `hyphenate`, off unless one does (SPEC §3.5).
- The shipped themes set it on no role.
- A page that opens such a deck walks no trie.

Three facts frame the choice:
- **`hypher` takes no trie at run time.** It compiles each language's trie in (`include_bytes!`, a Cargo feature per language), and its walker reads only those. The walker is about 200 lines, MIT or Apache-2.0. The tries are files.
- **A frame is the same on every target** (SPEC §13). A browser build that broke German lines differently from the Mac would be a bug, so no target can drop a language (ADR-0004 finding 11).
- **The engine asks for hyphenation in the middle of a layout.** Which text hyphenates is what the cascade decides: role, node, state, and `lang`, the node's or the deck's.

## Decision

1. **The engine walks the tries itself.**
   - `scaena_engine::hyphen` is `hypher` 0.1.8's walker, cut from it.
   - The 17 tries are that crate's files, byte for byte, in `crates/scaena-engine/hyphenation/`, each with its SHA-256 (`hyphen::LANGUAGES`, and the directory's README).
   - `every_language_breaks_words_where_hypher_does` holds the walk to `hypher`'s. Every language breaks every word of a list where `hypher` does, with the trie compiled in and handed over alike. The list spans every script the languages are written in, and the empty word. `hypher` stays, as a dev-dependency, for that test.
2. **Native builds compile the tries in** (`scaena-engine`'s `hyphenation` feature).
   - The CLI turns it on, and with it the MCP server it runs (`scaena mcp`). Every crate's tests turn it on too.
   - So does the player's module, which a single-file export carries offline (PLAN 2.5).
   - The workspace names the engine without it, so a crate that lays out text asks for it.
3. **The editor's module leaves them out.** It is handed a language's trie the first time a text hyphenates in that language.
   - The page sets a loader (`setHyphenation`), which is given the language's code (`de`) and returns its file's bytes.
   - The engine asks in the middle of a layout, so the worker fetches the file at once. A worker may make a request that waits.
   - The files are beside the pages: the build copies them (`web/src/hyphenation.ts`).
4. **A trie handed over is held to its SHA-256**, and kept for the session. A frame is then the same bytes wherever its patterns came from. A damaged or wrong file is refused, never walked.
5. **A language whose trie cannot be had is an error that says so** (`EngineError::Hyphenation`). The text is never set without hyphens: the frame would differ from every other target's.

## Consequences

- **+** The editor's module is 2,646,494 bytes gzipped, 343 KB less: 353 KB of room under §15. The player's module keeps every trie, at 2,154,467 bytes.
- **+** No frame changes. The walk is `hypher`'s, over the same bytes. The torture deck's hyphenation cases paint the goldens:
  - natively;
  - in the WASM smoke page, which hands the tries over;
  - in the web player on the editor's module.
- **+** A language added is its file, its digest and bounds, and a line in the test.
- **−** The first layout that hyphenates in a language waits for one request to the page's own origin. German, the largest, is 206 KB.
- **−** A page cut off from where it was served cannot hyphenate in a language it has not fetched yet. Its frame is an error that says so. A single-file export carries every language, and needs no network.
- **−** The walker is ours to keep. Moving to a newer `hypher` means copying its walker and its tries again, and the parity test is the check.

## Alternatives

- **Drop languages from the browser build.** Rejected by ADR-0004 finding 11: German would break differently in the browser.
- **Fetch the patterns before the layout.** The page would read the deck for the languages it hyphenates in, and fetch them before it opens. Rejected:
  - Which text hyphenates is what the cascade decides, so the page would compute the engine's answer again.
  - An edit that turns hyphenation on would need a second round trip before its frame.
  - The engine asking when it needs a trie is exact, and a worker may wait.
- **Fork `hypher` to take tries at run time.** The same code, in a second crate to keep in step. The engine's copy is 200 lines.
- **Cut elsewhere.**
  - The WGSL is what WebGPU compiles the painter from.
  - The schemas are what the editor validates every edit by.
  - The code is what runs.
  - Each of those is read in every session that paints or edits. The tries are read only by a deck that asks for them.

# ADR-0012: `scaena serve` is the web pages on a folder, on this machine only

**Status:** proposed · **Date:** 2026-10-04

## Context

The authoring guide (PLAN 2.10) found a gap. A person who writes `deck.scn` in their own text editor could see a state at rest with `scaena render`, or the whole deck by exporting one file, but nothing showed each save as it was made. The web editor shows each keystroke, but it saves into the browser's storage. Only Chrome and Edge can open a folder on disk (File System Access), so in Safari and Firefox a deck stayed in the browser until it was downloaded.

SPEC §7.1 has always listed `scaena serve` ("dev server: live preview + watch + HTTP API"), with the CLI exiting 3. PLAN 2.11 asks for it: the player and the editor on a bundle on disk, a saved `deck.scn` compiled, and the pages showing each change.

The pages exist (PLAN 2.1–2.9), and they already read a bundle from a URL. What is missing is:
- something on this machine that serves the folder;
- a way for the pages to hear of a change;
- a way for a save to reach the disk.

## Decision

1. **A crate of its own, `scaena-serve`, beside `scaena-mcp`.** It is a client of the operations, as the MCP server is (ADR-0009). It compiles `deck.scn` with `scaena_ops::compile`, as `scaena compile` does, and nothing else.
2. **HTTP/1.1 by `hyper` on the tokio the MCP server already runs.** It needs no other async runtime, and the HTTP surface is small:

   | Path | What |
   |---|---|
   | `/` | the player |
   | `/edit` | the editor |
   | `/index.html`, `/editor.html`, `/assets/…` | the pages |
   | `/bundle/…` | the folder's files: `GET`, `PUT`, `DELETE` |
   | `/scaena/events` | server-sent events |
   | `/scaena/state` | where the bundle stands, as JSON |

   The player and the editor open the bundle at `/bundle/` with `?serve`.
3. **The pages are carried, as the single-file page is (ADR-0010).** `npm run build` copies `web/dist` to `crates/scaena-serve/pages/dist`, which is not committed. The crate's build script gzips each file into the binary and sets `cfg(pages)`. A `scaena` built before the pages exits 3 on `serve`, naming PLAN 2.11 and `just web`. The pages go out gzipped, as every browser takes them.
4. **This machine only.**
   - It listens on 127.0.0.1, and no flag changes that.
   - It answers only a `Host` of `localhost` or `127.0.0.1` at its port. A page from another site cannot reach it by pointing a name of its own at this machine (DNS rebinding).
   - It writes only for its own pages: a `PUT` or `DELETE` with any other `Origin` is refused. It sends no CORS headers, so another site's page cannot even ask.
   - It writes only inside the folder. A path with `..`, an empty name, or a name starting with a dot is refused. A link that leads out of the folder is not the bundle's: nothing is read, written, or made through it, and removing a link removes the link.
   - It writes each file whole: to a new hidden file beside it, then renamed into place.
5. **The folder is watched by scanning, every 150 ms.** No file-watching dependency, and the same behavior on every system. A change counts once two scans agree, so a file a text editor writes in steps is read once, whole.
   - A changed `deck.scn` is compiled into `deck.json` and announced. If it does not compile, its problems are announced at their lines, and `deck.json` stays as it was.
   - At start, a `deck.scn` newer than `deck.json` is compiled, as `make` would.
6. **A page's own saves are announced once, as its own.** Each write carries the page's id (`X-Scaena-Client`). The server takes in what it wrote at once, so the scan never sees it as a change from disk. A burst of writes is announced as one `changed`, by that page, once they stop. The page passes over its own; every other page reads the bundle again.
7. **The editor writes where the folder writes.** Where the folder keeps `deck.scn`, the editor opens that text as its source, rather than the canonical decompile. Its save writes the bundle as `scaena save` does, then `deck.scn` as the editor shows it. A change on disk comes into an editor that has nothing of its own not saved; over such changes, it is offered.

## Consequences

- **+** Write `deck.scn` in any text editor, and the player shows each save at the state it was on. Problems are said at their lines, in the terminal as `compile` says them and in the page.
- **+** The editor saves to a folder on disk in any browser, Safari and Firefox included.
- **+** The pages are the same code, with one more home for a bundle: the worker's save writes to a folder handle, the browser's storage, or the server, by the same two calls.
- **−** The binary grows by the pages, 4.3 MB gzipped. The order of builds matters, as for ADR-0010.
- **−** The scan wakes the processor every 150 ms while it serves, and holds each file's size and time. A bundle is tens of files, so this costs nothing that matters.
- **−** Two writers at once, a text editor and the web editor each with changes not saved, resolve by the person: the editor offers what changed on disk and keeps its own until asked. There is no merge. The CRDT's history (PLAN 2.9) is still where concurrent edits would merge, later.

## Alternatives

- **A file-watching dependency (`notify`).** It would be quicker to notice, and it has a different backend on each system. A scan of a small folder is simpler, and it behaves the same everywhere. Rejected for now.
- **Watch from the page with File System Access.** Chrome's editor could poll a folder handle and compile `deck.scn` itself, with no server. That would leave Safari and Firefox out. Rejected as the answer; it may still come as a convenience.
- **Find the pages at run time** in the repository's `web/dist`. The binary would stay small, but `serve` would depend on where `scaena` was built and run from, as ADR-0010 found for the single file. Rejected.
- **The operations over HTTP**, as SPEC once sketched ("HTTP API"). MCP is already the surface for programs, over stdio (ADR-0003, ADR-0009). The server's HTTP is only what the pages need. Rejected.

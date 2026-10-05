# ADR-0014: A data file's edits enter the history as versions of the file

**Status:** proposed · **Date:** 2026-10-05

## Context

PLAN 2.55 has the editor edit a data source in place:
- its cells, edited where the table shows them;
- rows added and removed;
- each change one write of the bundle's data file;
- every chart and table that reads it shown changed.

A data source is a file the bundle holds (`data/q3.csv`, `data/q3.json`), or rows written inline in `deck.json` (SPEC §3.10).

Three facts frame the choice:

- **The deck's history is a CRDT, and data files are outside it** (SPEC §8.1, ADR-0002).
  - `history/deck.loro` holds the deck's containers: `deck`, `meta`, `data`, `nodes`, `order`, `states`, `overrides`, `spine`.
  - The `data` map holds each source's declaration, not its rows.
  - Nothing records a data file's bytes. A change, the commit's first line naming its author, is made only when the deck's containers change.
  - Only `deck.json` is checked for an edit made outside Scaena (`Bundle::history`).
- **The file is the truth of its data.**
  - An author exports it from a spreadsheet, keeps it in git, and replaces it with next quarter's.
  - A chart reads it as bytes, by the engine's own CSV and JSON readers (SPEC §3.10). The bytes a deck renders from are the bytes in the bundle.
- **Undo is per author** (SPEC §8.2).
  - An edit the user made undoes as the user's, never an agent's or a file's.
  - Loro's undo manager undoes this session's own changes. It skips those whose origin is `fs`.

Multi-user editing is deferred (SPEC §8.4). Concurrent edits to one data file come today only from an editor and an agent on one machine.

## Decision

1. **A data file's history is its versions.**
   - The CRDT gains a `files` map: each data file a source names, by its path, and its bytes now, as a binary value. A version is never edited in place, so it is replaced whole.
   - An edit to the file sets its bytes, in the same change as the deck's edit when there is one, by its author with its message (`data_edit q3: revenue of row 3`). Bytes as the map holds them are no change.
   - The history keeps every version, as it keeps every value a map held. Undo and redo set the bytes back, and the file is written from them.
   - The deck's containers are read as before: `deck.json` comes back byte for byte, and the files are read beside it.
   - A history begins with the data files the deck names (`save --history`).
2. **The file stays the truth, byte for byte.**
   - The operation writes the file, and the history records what it wrote. Nothing re-serializes a file the user did not edit.
   - An edit to one cell rewrites only that field's text, and keeps the quoting, line endings, and every other byte as the file had them. That holds in a CSV and in a JSON array of objects alike.
   - A row added copies the line endings, and in JSON the layout, of the row before it. A row taken away takes its line, or its element and one comma, with it.
3. **A file that says otherwise than the history goes in as a change by `fs`**, before anything else is recorded, as `deck.json` does (SPEC §3.1). Examples: a file replaced on disk, or one a spreadsheet wrote.
4. **Merges keep one version per file.** Concurrent writes to one file keep one side's, the same on every side (the map's last writer). Writes to different files both stand.
5. **Inline rows are the deck's.** A source written inline is part of `deck.json`. Its edits are patches of `/data/<name>/source/inline`, as any other edit of the deck is.
6. **Edits are operations, as every edit is** (ADR-0009).
   - An operation in `scaena-ops`, `data_edit`, edits a source's table: a cell set, a row added, a row taken away.
   - It checks each value against its column's type (SPEC §3.10), the rules a chart reads it by, and refuses one the type refuses, saying why.
   - It refuses an edit that would leave the deck invalid, as `patch` does. Otherwise it writes the file and reports the lint delta.
   - With no edits it reads the source: each row's cells as written, and each cell its column does not read, with why. Such a cell is no reason to refuse an edit elsewhere, and the `set` that makes it read is what fixes it.
   - The editor, the CLI (`scaena data`), and the MCP server (`data_edit`) call it alike.

## Consequences

- **+** A data edit is one change by its author, beside the deck's. A history holds the whole state: a deck and the data it was drawn from.
- **+** The file is never rewritten behind the author's back. A CSV from a spreadsheet keeps its quoting and its line endings, and git shows the one field that changed.
- **+** No new format: the bundle, `deck.json`, and the data files are as they were. A bundle saved without a history (`save` without `--history`) carries no versions. A history from before this ADR reads as one that holds no file yet. A file it then meets goes in as a change by `fs`, with its bytes.
- **−** History grows by a copy of the file per edit, a version the file came back to included.
  - A chart's data is kilobytes. A session of a hundred edits to a 20 KB file adds about 2 MB, before Loro compresses it, and Loro drops none.
  - Trigger to revisit: a file over 1 MB, or a history over 50 MB.
- **−** Two concurrent edits to different cells of one file do not merge: one side's version wins.
  - That waits for collaboration (SPEC §8.4). A table container, cells as values, would merge them.
  - The bytes the operation writes would not change, since the file stays an export of what is recorded.

## Alternatives

- **Each version once, by its SHA-256, in a map of its own (`blobs`), and the path naming its hash.** It keeps a version the file comes back to once. Rejected:
  - An undo of the change that added a version takes the version out of the map.
  - A later change, by `fs` or another peer, may name that version too, and the file it names would be gone.

- **The rows as a table container in the CRDT, the file its export.** It merges by cell, and its history is small. Rejected for now:
  - The file would be re-serialized, and a CSV's quoting, number text, and line endings are not the CRDT's to keep.
  - It is the larger build, for a merge no one needs until there is a second user. It stays the path when there is.
- **Data edits make the source inline.** It needs no new container. Rejected: it moves the author's data out of the file they keep and update, and a large table bloats `deck.json`.
- **Data files outside the history, as now.** Rejected: an undo of a cell edit would have nothing to restore, and a saved history would not hold what the deck was drawn from.

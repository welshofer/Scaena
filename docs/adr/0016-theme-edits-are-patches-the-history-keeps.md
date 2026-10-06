# ADR-0016: A theme's edits are patches of the theme the bundle holds, and its history keeps it

**Status:** proposed · **Date:** 2026-10-06

## Context

PLAN 2.61 has the editor edit the deck's theme in a panel:
- its colors;
- its type roles;
- its spacing.

Each edit should be one change, with the deck laid out again. An edit the deck would not validate in should be refused, with why.

Four facts frame the choice:

- **A deck names its theme** (SPEC §3.6).
  - `deck.theme` is a path to a theme file the bundle holds (`themes/dusk.theme.json`), or a theme written inline in `deck.json`.
  - Validation checks the theme against `theme.schema.json` and the deck against the theme: a role, color, layout, slot, preset, or palette the deck names and the theme lacks is E102.
- **A theme that ships enters a bundle as a copy.** `scaena new`, `deck_create`, `theme --apply`, and the editor's theme picker copy Dusk, Daybreak, or Ember into `themes/`, with the fonts it names that the bundle lacks. The deck never names a theme it does not hold, and nothing reads the shipped theme after the copy.
- **Names are the swap contract.** Two themes swap cleanly when they define the same names (SPEC §3.6). A theme's edits that keep its names keep every deck that uses it valid.
- **The history keeps the deck and its data files** (SPEC §8.1, ADR-0014), and not the theme.
  - A theme edited outside Scaena changes how the deck draws with no change in its history.
  - A version restored (PLAN 2.60) is drawn in the theme the bundle holds now, not the one it was made in.

## Decision

1. **A theme edit is a patch of the theme.**
   - It is RFC 6902 operations on the theme the deck names, its paths pointers into that theme's JSON (`/tokens/color/accent`, `/type/roles/body/size`, `/grid/gutter`), applied in order, all or none.
   - An inline theme is part of the deck. Its edits are the same operations under `/theme`, written into `deck.json`.
2. **It is checked as a re-theme is** (`theme --apply`, PLAN 1.35).
   - The theme it leaves must read against `theme.schema.json`. The deck is validated in it, against the bundle as it is, and an edit that adds a validation error is refused, with the finding. A name the deck uses taken out of the theme is E102, so a rename is a patch of the deck and the theme together, not of the theme alone.
   - Otherwise the theme file is written, and the edit reports the lint delta: what `validate` and `lint` find in it that they did not before, and what they no longer find. The deck is laid out again in it: every state, every format.
   - A dry run writes nothing.
3. **The theme is written in canonical form**, as `save` writes it (SPEC §3.1).
   - A theme `save` or an edit has written changes only where the edit changes it: git shows the token that changed.
   - A theme written by hand, as one that ships is and as `scaena new` copies it, is laid out canonically by its first edit, as by its first save.
4. **A theme that ships is edited in the bundle's copy.**
   - The copy is the bundle's own from the moment it is made. An edit writes the copy; the theme that ships, in the CLI, the MCP server, and the page, never changes.
   - The copy keeps its path and its `name`. Renaming it is an edit like any other (`replace /name`).
   - Applying the theme that ships again by `theme --apply` replaces the copy, edits and all: one change, which undoes as any other, and the history keeps the edited copy (5). The editor's picker never writes over a theme file the bundle holds with other bytes (PLAN 2.39): it copies the theme that ships beside the edited one (`dusk-2.theme.json`) and names that, and the Files tab takes out the copy nothing names.
5. **The history keeps the theme, as it keeps the data files.**
   - The CRDT's `files` map holds the theme file the deck names beside each data file a source names, by its path, its bytes now (ADR-0014). An inline theme is in `deck.json` already.
   - A theme edit sets the theme's bytes in its change, by its author, with its message (`theme_edit: tokens/color/accent`). So does a re-theme that writes a theme file, and a save, which writes the theme as it is saved (3).
   - A theme file that says otherwise than the history goes in as a change by `fs`, before anything else is recorded, as a data file does. A history from before this ADR meets the theme so, once.
   - A version is the deck, its data files, and its theme as they were (PLAN 2.60). Comparing two says when the theme's bytes changed, and restoring one restores its theme.
6. **Edits are operations, as every edit is** (ADR-0009).
   - An operation in `scaena-ops`, `theme_edit`, with a twin that writes nothing (`theme_editing`) for the page.
   - The CLI calls it as `scaena theme BUNDLE --edit ops.json` (or `-`), and the MCP server as the tool `theme_edit`.
   - The editor's Theme tab makes each edit one `theme_edit` by `user`. The source does not change, so the editor takes the edit as a change to its source's history that carries the theme's bytes before and after: ⌘Z writes the theme back, and ⇧⌘Z writes it again, as with a restored version's data files (PLAN 2.60).
   - The editor's assistant has the tool too (ADR-0011). Its edit is its agent's, and the editor takes it as it takes the Theme tab's, so ⌘Z undoes it. A look asked of the whole deck ("larger headings", "a warmer accent") is then an edit of the theme, not a literal written into every node (ADR-0005).

## Consequences

- **+** A theme edit is a reviewable change to a file the author keeps: git shows the one token that changed, and the history has it by its author.
- **+** The deck stays semantic. An edit of the theme changes every node that names what it edited, and no literal enters the deck (ADR-0005).
- **+** A shipped theme stays a known quantity. Every bundle's copy diverges on its own, and a deck made in Dusk still opens in Dusk as it shipped.
- **+** A version is the whole of what draws a deck but its fonts and images, which are named by their content.
- **−** Each edit keeps a copy of the theme file in the history: a theme is 10–25 KB, and a session of a hundred edits adds 1–2.5 MB before Loro compresses it. The trigger to revisit is ADR-0014's: a history over 50 MB.
- **−** A theme written by hand loses its layout on its first edit, as on its first save (3).
- **−** Two concurrent edits to one theme do not merge, as with a data file (ADR-0014). That waits for collaboration (SPEC §8.4).
- **−** A token renamed in the theme is not renamed in the deck. Until an operation renames both, the editor offers no rename, and the deck's uses stand in the way of a raw patch that tries (E102).

## Alternatives

- **Theme edits as deck overrides.** Rejected: overrides are per node literals (ADR-0005); a color changed for the deck would be written into every node that shows it, and a theme swap would keep them.
- **A theme layer in the deck (`themeOverrides`)** that merges over the theme file. Rejected for now: it is a second place where a theme lives, which every reader of a theme would need to merge, and the bundle already holds a theme of its own to edit.
- **Edits of a shipped theme saved as a new file** (`themes/dusk-edited.theme.json`) on the first edit, the deck re-pointed at it. Rejected: the copy is already the bundle's own file. A second file leaves an unused one behind, and the path changing on the first edit surprises a reader of `deck.json`.
- **Edits spliced into the theme's text**, every byte kept that the edit did not change, as a data file's are (ADR-0014). Rejected: `save` writes the theme canonically (SPEC §3.1), so the layout an edit kept would last only until the next save, and the canonical form already gives git a change of one line.
- **The theme outside the history, as now.** Rejected: an undo of a theme edit in the CLI would have nothing to restore, and a version would draw in a theme it was never made in.

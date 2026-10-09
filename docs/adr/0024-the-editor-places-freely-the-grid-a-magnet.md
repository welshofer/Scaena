# ADR-0024: The Mac and iPad editors place freely; the grid is a magnet

**Status:** proposed · **Date:** 2026-10-09

## Context

Jay, on 2026-10-09: the Mac app "needs the ability to move shit anywhere, select things, directly edit them, adjust properties", and should "feel lightweight and easy, not hardcore and academic", like Keynote.

A drag on the canvas today (ADR-0013, PLAN 3.7) lands where the theme says. On a 12 × 12 grid over a 1920-unit canvas, that is 146 units a step.
- **A node placed by cells** moves by whole tracks.
- **A node in a slot** jumps into the slot it covers most, and cannot be resized.
- **Shift** takes a node off the grid as a `rect`. The canvas outlines that node in orange, and lint flags it (W301, and W302 in a deck with formats).

A person moving a title an inch to the left cannot. That is invariant 4, semantics over pixels, applied to the hand rather than to the file. It is right for an agent, which writes decks the theme re-lays out in every format. It is wrong for a person at a canvas, who expects an object to stay where they let go of it.

The engine already places freely. A `rect` is a placement the theme's grid takes, as an override, and `free` lands a box where it was dropped. Since PLAN 2.57, `free` also aligns the box, within a reach, to other boxes and to the canvas.

## Decision

1. **In the Mac and iPad editors, a drag lands where it is let go** (PLAN 3.19). A move or a resize of a node the theme's grid holds is `free`, and these draw it in within 8 points on screen, with guides drawn where they meet:
   - the grid's columns and rows (the tracks, their edges and middles);
   - the canvas's edges and middle;
   - every other object's edges and middle.

   ⌘ held turns the magnets off. The session's `guided_freely` and the C ABI's `snap` with `grid: true` do it.
2. **On the grid's tracks all round, a dropped box takes their cells**, not a `rect`. A person who lines an object up with the grid gets a placement the theme moves in every format. A person who does not gets their object where they put it.
3. **Shift snaps as before**: into a slot, or onto the grid's cells, and a `rect` back onto them (`Targets.dragged`). The arrow keys step by tracks, as before (PLAN 3.17).
4. **The editors do not mark free placement.** There is no orange outline. W301 and W302 are not among the issues the Mac and iPad show. They stay lint rules, which the CLI, the MCP server, the assistant's tools, and the browser report, for decks that place by the theme.
5. **What a container holds goes as its container places it**: a stack's child by its order, a grid container's by its cells or areas, and a frame's by a `rect`.

## Consequences

- Invariant 4 holds for the file, whose `rect` is still an override, and is relaxed for the hand. A deck a person edits may carry `rect`s. Re-theming keeps them where they are, and in another format they stay where they are in canvas units (SPEC §3.4). That is what W302 warns of. The editor's other sizes view (PLAN 3.16) shows the result, and Lay Out on Its Own in This Size (PLAN 2.85) answers it.
- The browser's canvas is unchanged. A browser drag still snaps to the theme's grid, with Shift for `free`. Whether it follows is Jay's call.
- `grid: true` is a new argument the C ABI's `snap` takes. Without it, `snap` is what it was, and the browser's `Player.snap` does not see it.
- If Jay rejects this ADR, the Mac goes back to `Targets.snap` for drags. The engine's `guided_freely` can stay, unused.

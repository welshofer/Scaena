# ADR-0013: Direct manipulation asks the engine where, and edits by patches

**Status:** proposed · **Date:** 2026-10-04

## Context

Jay wants a WYSIWYG editor with a full set of options: select a node on the slide, drag it, resize it, type into it, and pick its look. The manifesto refuses WYSIWYG *first*. Direct manipulation is a late feature that emits patches, not the foundation (MANIFESTO, "What we refuse"; SPEC §1.3). PLAN 3.7 already says what v1 is for the Mac:
- a move or a resize within the template's slots emits patches;
- a drag off the template makes explicit overrides, visibly flagged.

The pieces are in place. The web editor (PLAN 2.3) compiles `.scn` and paints the preview from the display list. The operations patch the deck and say what lint makes of it (SPEC §7.3, ADR-0009). The CRDT records each change by its author, with undo per author (PLAN 1.23, 2.9).

What is missing is every answer a pointer needs:
- what is under it;
- where each node's box is;
- where a node may go;
- where a caret stands in a text, and which glyphs a selection covers.

Three invariants decide who answers. Clients paint; they do not lay out (invariant 2, SPEC §1.2). Layout runs per snapshot, and frames only sample it (invariant 3). The document references roles, slots, and tokens, not pixels (invariant 4). The web page cannot answer these questions, and neither can the Mac app (gate 3: zero text layout in SwiftUI). Only the engine knows the layout.

## Decision

1. **The engine answers geometry, from the layout a state already has.** On a state laid out in a format, at rest or at a time into its cue:
   - **Hit:** the nodes under a point, topmost first. Each comes with the chain of containers it sits in (`at.parent`), its box, and, for a text, the character under the point (parley's `Cursor::from_point`).
   - **Boxes:** each node's laid-out box, and a container's tracks and cells.
   - **Targets:** where a node may go. These are the theme's grid cells in that format, the template's slots, and its container's positions, each with the patch that puts the node there.
   - **Text:** a caret's rectangle at a character, and a selection's rectangles over a range (parley's `Cursor::geometry`, `Selection::geometry`).

   These come from the snapshot's layout, the one frames sample, kept per state and format. Nothing is read back from pixels, and no client lays anything out.
2. **Every edit is a patch.** The operations are the only way a gesture changes the deck (ADR-0009):

   | Gesture | Patch |
   |---|---|
   | Move within the grid, into a slot, or among a container's children | `place`: cells, a slot (`in`), an `area`, or an `index` |
   | Resize within the grid | `place`: spans |
   | Typing | `set_text` |
   | Choosing a look | a role, a style, or a preset from the theme's vocabulary |

   A drag off the grid or out of the template places the node by `rect`, in canvas units: an override in SPEC §3.4's sense, allowed, and flagged by lint (W301) and in the inspector. That is what PLAN 3.7 means by visibly flagged. Every patch is validated and linted as an agent's is, and the editor shows the lint delta.
3. **An edit changes the value where it lives.** If the deck's `overrides` set the property, the edit changes it there, since they win in every state. If the state shown sets it in its props, or a state it tracks from does, the edit changes it in the latest of those. Otherwise it changes the node, and the editor says how many states show that node ("in 3 states"). One key moves the edit into this state alone: the property goes into this state's props. Nothing forks a value without being asked.
4. **A gesture previews by painting, and commits once.**
   - While a node is dragged, the client moves the node's layer in the display list and draws snap guides from its targets. That is paint only: no layout runs while the pointer moves.
   - On drop, one patch; the engine lays the state out once, and lint runs. A resize reflows its text when the gesture pauses, as the source editor compiles while one types: by the no-write twin of the patch (`patching`), never once a frame.
   - Each committed patch is one undo step, by `user`, in the CRDT.
5. **One surface for every client.** The queries are the engine's, called by an operation in `scaena-ops`, and so reachable from:
   - the CLI;
   - the MCP server, where an agent looking at a frame asks what stands at a point;
   - the WASM session, for the web editor;
   - the C ABI, for the Mac app (PLAN 3.1).

   The web editor comes first; the Mac app's direct manipulation (PLAN 3.7) is the same calls.

## Consequences

- **+** The source editor and the canvas are two views of one deck. A drag is a patch an agent could have sent, and an agent's patch shows on the canvas as the drag would have. Both round-trip through `.scn` (SPEC §4).
- **+** Placement stays semantic by default. A drag snaps to the theme's grid and slots, so a deck moved by hand still re-themes (`theme --apply`) and still lays out in its other formats.
- **+** Text is hit and selected by the shaper that set it, in any script and direction the engine sets, with no text layout in the page or in SwiftUI.
- **−** Every query needs the state laid out in the client's format. The engine already keeps a state's layout and a cue's transition; queries add nothing to lay out, but a client asking about many states keeps many layouts.
- **−** While a resize is dragged, its text shows its old wrapping until the gesture pauses. That is the cost of frames that never lay out.
- **−** Overrides made by dragging are raw values. Lint and the inspector show them, and a re-theme leaves them where they were put.

## Alternatives

- **Hit-test the display list in the client.** Walk the ops and test each path and glyph box. Rejected: the display list holds no characters, lines, containers, or targets. The Mac app would need the same walk again in Swift, and a client that knows a layout's structure is laying out.
- **Hit-test pixels:** paint an ID per node into a second buffer. Rejected: it costs a painter pass, it is ambiguous under translucency, and it reads back from the GPU on every pointer move.
- **Absolute positions as the document's truth, WYSIWYG first.** Rejected by the manifesto and by invariant 4. A deck placed in pixels cannot be re-themed or laid out in another format.
- **The editor patches source text, not the deck.** Rejected: `.scn` is an authoring projection (invariant 10). The deck is the truth, and the editor already decompiles after each patch.

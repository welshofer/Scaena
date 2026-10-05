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
   - **Hit:** the nodes under a point, topmost first. Each comes with the chain of containers it sits in (`at.parent`), its box, and, for a text, where a caret put at the point stands.
   - **Boxes:** each node's laid-out box, and a container's tracks and cells. A node its `transform` moves carries the map that draws it, and a point is read back through it, so a turned node is found where it is drawn (PLAN 2.51).
   - **Targets:** where a node may go. These are the theme's grid cells in that format, the template's slots, and its container's positions, each with the patch that puts the node there.
   - **Text:** each character of the text as written, as a reader counts it (a grapheme cluster), on its line, with where a caret before it and after it stands (`Scene::carets`). A client draws the caret and the selection from these, puts a caret nearest a point, and moves it up and down the lines. As built, they are read from the glyphs the engine set (each run's clusters, advances, and direction, and the line's box), since a state's layout keeps those and not parley's own: parley's `Cursor` and `Selection` would need the paragraph laid out again. Case that sets a character longer (ß as SS), and the soft hyphens hyphenation inserts, map back to the text as written.

   These come from the snapshot's layout, the one frames sample, kept per state and format. Nothing is read back from pixels, and no client lays anything out.
2. **Every edit is a patch.** The operations are the only way a gesture changes the deck (ADR-0009):

   | Gesture | Patch |
   |---|---|
   | Move within the grid, into a slot, or among a container's children | `place`: cells, a slot (`in`), an `area`, or an `index` |
   | Resize within the grid | `place`: spans |
   | Several moved together, by a drag or the arrow keys | `place` for each: the first as it would snap alone, the rest as far as it went, each its own way; all stopped where the grid's edge stops one (PLAN 2.42) |
   | Several aligned or spread | `place` for each that moves: each box to the edge all of them reach farthest, or the middle of them all; or, the first and the last staying, equal gaps between them; snapped as a move snaps |
   | A motion's bar dragged, or its end; the transition's end (PLAN 2.44) | `time_motion`: the motion's `delay` or `duration`, written where the motion is: its choreography item, else the node's own preset or `anim` where it lives; `set_state` of `transition/duration` |
   | A motion added from the cue | `apply_preset` with `motion`: as the node enters or for emphasis in the state shown, or as it leaves, in the state before |
   | Group, ungroup (⌘G, ⌘⇧G) | `group`: a new group holding the nodes selected where they stand, shown wherever one of them is in it; `ungroup`: its children out to its container and the group gone, the deck as it was before `group` (PLAN 2.43) |
   | Forward, backward, to the front, to the back (⌘], ⌘[, with Shift) | `choose` of `z`: in front of the next it overlaps, or behind the one before; or past all its container holds; the one `z` that does it where there is one, else the fewest |
   | Typing | `replace_text`: the characters typed over, where the text lives; runs keep their looks |
   | ⌘B, or a role or color chosen, with characters selected | `style_text`: those characters' look, as runs split at the selection's ends and joined where alike, where the text lives; ⌘B bold, or not, by the weight the engine sets each in |
   | ⌘I, with characters selected | `style_text` with `style/italic`: italic, or, all italic already, not, by what each asks; the family's italic face, never a slanted roman (PLAN 2.40) |
   | Choosing a look | `choose`: a role, a key of a style, a preset, or a prop, from the theme's names for it (`inspect --choices`); a value written out goes in the deck's `overrides`; with several selected, one for each, of what all of them have alike |
   | An image's focal point picked on it; an image file dropped on an image (PLAN 2.45) | `choose` of `focal`: the point of the image drawn under the pointer, in fractions of the part its crop keeps (`Scene::image_point`); `choose` of `src`: the file's path, once it joins the bundle under `assets/`, named by its SHA-256 |
   | The rotate handle dragged round the node selected; an angle typed in the inspector (PLAN 2.51) | `choose` of `transform/rotate`: the angle the drag reached, in whole degrees, or by 15° with Shift, written where the node's transform lives |
   | Choosing what a chart reads | `choose`: its source from the deck's, and a channel's field from the columns of its data that the channel can read; another source points again what it cannot serve, in the same patch (PLAN 2.41) |
   | Insert | `add_node`, entering in the state shown, then `place`: what the theme and the bundle offer (`inspect --inserts`), a chart and a table of each data source among them, about the pointer, on the grid, or a text or an image in the empty slot under it |
   | Drawing (PLAN 2.48) | `add_node`, entering in the state shown, then `place`: over the grid's cells a drag covers, each edge on the nearest track's, or, with Shift, by `rect` where it went; a line's or an arrow's `points` the way the drag went |
   | A finding's fix, taken from its mark on the canvas (PLAN 2.49) | The finding's `fix`, as lint offers it: a patch lint keeps only once laying the state out again with it took the finding away (SPEC §7.4) |
   | Duplicate | `add_node` with the props the state shows, for the node and what it holds, then `place` one span beside the node, clear of the rest where there is room |
   | Delete | `hide_node` in the state shown, for the node and what it holds; `remove_node` for one no state shows after |
   | Delete from the deck | `remove_node`, for the node and what it holds |
   | Copy | Nothing: a clip of the node and what it holds, as the state shows them, with their overrides, the data sources they read, and the files those read (`application/x-scaena+json`, and as text); of several, each with its box |
   | Cut | A copy, then Delete's patch |
   | Paste | The sources a clip brings, `add_node` for each node it holds under an id new to the deck, entering in the state shown, their overrides, then `place` as Insert places a node, several where they stood about each other; what the theme lacks taken out of the copy, as findings, a text's role given way to its stand-in |
   | A node shown or hidden from the layers (PLAN 2.50) | `show_node` or `hide_node` in the state shown, for it and what it holds |
   | A node renamed in the layers | `rename_node`: the id everywhere the deck names it |
   | A node dragged among the layers of its container, or moved with Alt and an arrow | Just over or under the one it drops beside: the `z` that does it, as an order does; in a stack, `place` of each `at.index` that changes |
   | A node dragged into another container in the layers, or out onto the canvas, or moved with Alt and ← or → | `place` with `parent`: placed as that container places what it holds, where the node stands where it can (the cells its box stands in, a `rect` inside a frame's padding, its place in a stack's order, after a grid's flow), written where the node's placement lives; then the `z` that lists it where it dropped |
   | A state added in the strip | `add_state`: a step of the shown state's slide, tracking from it, or an empty slide (`mode: absolute`) after its slide |
   | A state dragged in the strip, renamed, or deleted | `move_state`, `rename_state`, `remove_state` |
   | Choosing a theme | `retheme`, as `scaena theme --apply` re-themes a bundle: a theme that ships written into the bundle with the fonts it names that the bundle lacks, or the bundle's own; refused, with why, where the deck would not validate in it |
   | Choosing a state's look, with no node selected | `set_state`: its layout, from the theme's layouts with a slot for each node placed in one, written where it lives; its transition's keys, its hold, and its notes, its own (`inspect --state-choices`) |

   A drag off the grid or out of the template places the node by `rect`, in canvas units: an override in SPEC §3.4's sense, allowed, and flagged by lint (W301) and in the inspector. That is what PLAN 3.7 means by visibly flagged. Every patch is validated and linted as an agent's is, and the editor shows the lint delta.
3. **An edit changes the value where it lives.** If the deck's `overrides` set the property, the edit changes it there, since they win in every state. If the state shown sets it in its props, or a state it tracks from does, the edit changes it in the latest of those. Otherwise it changes the node. Before it is made, the editor says how many states it changes ("in 3 states"), and a patch made says which (`states`). One key keeps the edit to this state (Alt in the web editor; `place`'s `fork`): the property goes into this state's props. The states that track this one take it, as they take the rest of its props, so a move in a build carries on through it. Nothing forks a value without being asked.
4. **A gesture previews by painting, and commits once.**
   - While a node is dragged, its layers, and those of what it holds, are drawn moved and over the rest, from the state as laid out at rest, which the session keeps (`Scene::moved`). The client draws snap guides from its targets. That is paint only: no layout runs while the pointer moves.
   - On drop, one patch; the engine lays the state out once, and lint runs. A resize reflows its text when the gesture pauses, as the source editor compiles while one types: the patch compiled and laid out once, made nowhere (`Player.preview`), never once a frame.
   - Each committed patch is one undo step, by `user`, in the CRDT.
   - Typing is the exception that proves it: each change to the text is a patch, laid out once, so the text reflows as it is typed. A burst of typing is one undo step, and a run of it one change in the CRDT (`type`). A keystroke's patch is validated but not linted; the editor lints the state it shows after, as after a keystroke in the source.
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

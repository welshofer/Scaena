# ADR-0008: A child names its container; containers lay out with `taffy`

**Status:** proposed · **Date:** 2026-10-02 · **Amended:** 2026-10-02 (PLAN 1.9: a table takes its rows in a stack, as text takes its lines)

## Context

PLAN 1.7 brings containers: `stack`, `grid`, `frame`, and `group`. Format 0.3 had three ways to say that a node is in one, and none was implemented:

- a container's `children`, a list of ids;
- a node's own `parent`, among the properties every type shares;
- the node's `at.parent`, with `at.index` for order.

Three spellings of one fact can disagree, and an agent has to keep them in step. The authorability spike (PLAN 0.13) measured what that costs. In finding 5, every deck its agents wrote restated the theme's font files in `fonts`, and none of them rendered, because the two lists drifted apart. A container's `children` list is the same trap: a node added to the deck but left off its container's list still renders, at the theme grid's margin box, on top of something else, and nothing says why.

## Decision

1. **One way in: `at.parent`.** A node is in a container when its `at` names one. `at` already says where a node goes, so the container is one more answer to the same question: the slide's grid, a slot, a rect, or this container.
   - Adding a node to a container is one edit, to the node.
   - Moving it to another container in a later state is a one-node delta, `at: { "parent": "right" }`, which tracks and morphs like any placement.
   - Children flow in `at.index` order (default 0), ties in `nodes` order, as CSS `order` does. A node an agent appends to `nodes` therefore goes last, and `index` reorders without renumbering everything else.
2. **`children` and the node-level `parent` are removed.** That is deck format 0.4, and every document in the repository moves with it. The engine never read either, so no frame changes.
3. **Within a container, `at` places in the container's terms.** In a grid container, `at.area` or `at.col`/`at.row` are the container's own tracks. In a frame, `at.rect` is measured from its padding edge. A stack places its children itself. A group lays nothing out: its children are placed on the slide by the rest of their `at`, then drawn together.
4. **`taffy` lays containers out, once per snapshot** (SPEC §5).
   - The engine builds one tree per root container. It measures text with parley, images by the part of the picture they show, and tables by their rows and columns as set.
   - Positions are not rounded to whole canvas units, as the theme grid's are not.
   - taffy's layout uses only IEEE add, multiply, divide, and min/max, so the boxes are the same bits on every platform. The display-list goldens and their digests hold this, on macOS and on Linux.
5. **Defaults follow content, not CSS's `auto` alone.**
   - In a stack, text, tables, and images take the room their content needs, and containers wrap theirs. Shapes, charts, and shaders have no size of their own, so they share the room that is left. Everything stretches across the stack.
   - A table asks for no more room than it is offered and needs none of it, where CSS's automatic minimum would keep its rows. Short of room, it takes what is left and says what to cut (E100 for rows), instead of pushing what follows past the stack's end.
   - A root fills the box `at` gives it unless its `size` says otherwise, which is what every node did before containers.
   - Text in a row stack stays the row's height. So `align: { y: "baseline" }` puts a row's last baselines on one line: a big figure and its label line up typographically, which CSS's `align-items: baseline` does only for first baselines.
6. **Panels and paint order.**
   - A container with `fill` or `stroke` draws a rounded panel under its children.
   - Paint order is the container tree: siblings by `z`, then `nodes` order, and each container under its children. Every layer carries that path as its paint key, so a transition merges two scenes in the same order its frames at rest use.
   - A group's `opacity` multiplies into its children's. Real group compositing needs nested layers in sampling, which is PLAN 1.12.

## Consequences

- **+** One place to say it: forgetting to list a child is no longer possible, and validation names the node whose container is missing (E102), not a container (E106), or part of a loop (E106).
- **+** Cards, stat rows, photo grids, and framed callouts need no `rect` arithmetic. Torture case 26 draws all of them from containers alone, the same on all three painters.
- **+** A child that changes container between states morphs, with no special case: its box moves.
- **−** A container does not list its contents, so reading one means finding the nodes that name it. `scaena inspect` prints each node's `at`, and an editor shows the tree anyway.
- **−** Order by `index` means reordering several children touches several nodes. Ties fall back to `nodes` order, so a deck that never sets `index` reads top to bottom.
- **−** Format 0.4 breaks any 0.3 deck that used `children` or `parent`. No deck in the repository did, and the engine had ignored both.
- **−** Overlapping children of a translucent group darken where they overlap, until sampling learns nested layers (PLAN 1.12).

# ADR-0020: A node may lay out anew in each of the deck's formats

**Status:** proposed · **Date:** 2026-10-07

## Context

A deck lists the other shapes it is laid out in (`formats`, SPEC §3.4), and the theme gives each its grid and its layouts' slots. A node in a slot moves with the slot. A node placed by `col`/`row` takes the same cells of the other format's grid, and lint W302 says so. Everything else about a node is the same in every format.

That is not enough for a real deck. The site's demo deck (trails) was to gain a 9:16 format (PLAN 2.84), and laid out there:
- Its `process` slide is a grid of four steps across. In 9:16 each step is 74 cu wide, and every heading and body in them overflows (E100). The steps have to run down the slide there, but a grid's `cols` and its children's `col`s are the same in every format.
- Its budget table is placed in 8 of 12 columns. In 9:16 that is 552 cu, and the table needs 833. A slot could give it the full width in 9:16, but in 16:9 the slot would have to be the same 8 columns, and no layout of the theme's has one.
- Fifteen of its nodes are placed by cells, and three of its slides lay a comparison out side by side.

A theme can move a slot per format, but it cannot change a container's axis or tracks, nor turn a decoration to suit one format. Re-placing everything by slot would change the 16:9 deck, which Jay approved as it is, and would still leave the `process` and `change` slides wrong in 9:16.

## Decision

1. **A node may say how it lays out in each format** (deck format 0.13): `formats`, a map from a format the deck lists to that node's layout there. A property there takes the place of the node's own of the same name when the deck lays out in that format.
2. **Only what lays out.** The properties a format may set are `at`, `size`, `align`, and `transform`; a container's `axis`, `gap`, `distribute`, `padding`, `cols`, `rows`, and `areas`; and a text's `maxLines`. Words, looks, data, and motion are the deck's in every format, as SPEC §3.4 has it.
3. **The engine applies it where it projects the deck** (`scaena_engine::project`): the deck laid out in a format is the deck with that format's canvas, each node's layout there in place of its own, and the theme's grid and slots for it. Everything that lays a format out goes through that one place (frames, lint, the editor's boxes and guides, inspect, the formats strip).
4. **It is the node's own.** A state's delta still sets what it sets, in every format. A delta or the deck's overrides that set `formats` is E106: per-format state deltas would multiply every question tracking answers (`scaena_core::tracking`, SPEC §2.2) by the number of formats, and no deck yet needs them.
5. **Validation says what is wrong where it is written.** A format the deck does not lay out in, or its own canvas's shape (which the node's own props lay out), is E102. A cell past that format's grid is E102. A property the node's type does not have is E106, checked as the node's own props are. A node placed anew in each format the deck lays out but its own is placed for each, and is not W302.

## Consequences

- **+** One deck lays out well in several shapes, with each slide's 16:9 layout as it was. The trails deck gains 9:16 without a change to how it looks in 16:9.
- **+** No change to a deck without `formats` on its nodes: its layouts, its goldens, and its timeline are as they were.
- **+** The format stays semantic: a format's layout still names slots, cells of that format's grid, and theme tokens.
- **−** A node's placement now lives in up to two places, and the editor writes only its own. A drag on the canvas in 9:16 still writes the node's own `at`, which moves it in 16:9 too. The editor should write a node's layout in the format shown where the node has one; that is a follow-up, not this decision.
- **−** Deck format 0.13: every deck's `scaena` moves from 0.12.

# ADR-0018: A list is a text's paragraphs, marked by the theme

**Status:** proposed · **Date:** 2026-10-06

## Context

A slide's points are a list more often than not, and a text has no way to be one (PLAN 2.69). People type `•` and a space, or a number and a stop, and set the lines by hand. The marker then wraps with the words under it, a screen reader hears "bullet" as a word, a PDF tags it as a paragraph, and a theme cannot change the marker.

Five facts frame the choice:
- **A text's paragraphs are already there.** A hard line break (`\n`) ends one (SPEC §3.5). Breaking, widow control, and hyphenation work paragraph by paragraph inside one layout, and the editor types `\n` as Enter does.
- **The engine places every line itself** (`read_layout`), from parley's lines, and can hand parley a width for each line, as `pretty` and hyphenation do. parley says where each hard break falls (`BreakReason::Explicit`).
- **A text has two writings, `text` and `runs`, which are one property** (SPEC §2.2). Anything per paragraph that lived in a run would have to be repeated or kept in step across both.
- **What a marker looks like belongs to the theme** (ADR-0005): a deck says "bullets", never "•".
- **Exports read the display list.** A tagged PDF takes a table's cells from the layers its ops are drawn in (`Op::Layer::cell`, SPEC §6).

## Decision

1. **A text's `list` marks its paragraphs.** It is an array with one entry for each paragraph, in order: `null` for a paragraph that is no item, or `{ "kind": "bullet" | "number", "level"? }`, `level` from 0 (the default) to 8. Paragraphs past its end are no items, and entries past the last paragraph mean nothing. `list` lives where any property does (a node, a delta, the overrides) and is one value: an edit writes it whole.
2. **The theme marks and indents them** (`type.lists`, theme format 0.10): `indent`, how far each level's words start from the one above, in ems of the text's size; `gap`, the least room between a marker and its words, in ems; `bullets`, a marker for each level; and `numbers`, a pattern for each level, whose first `1`, `a`, `A`, `i`, or `I` is the item's number in digits, letters, or roman numerals (`1.`, `a)`, `(i)`). A level past the lists' end takes their last. A theme without `type.lists` takes 1.2 em, 0.4 em, `• – ·`, and `1. a. i.`.
3. **An item hangs.** Each of its lines breaks at the box's width less its indent, `(level + 1) × indent`, and starts there (at the end edge in right-to-left text). Its marker is set on its first line's baseline, in the look of its first character, its end `gap` short of where its words start. Numbers count among the items of one kind and level that follow each other; an item at a shallower level, or a paragraph that is no item, starts the count again.
4. **Editing keeps `list` in step with the text.** `replace_text` writes `list` beside the text wherever the paragraphs it changes are items: a paragraph a line break splits is two items alike, and paragraphs joined keep the first's mark. A new op, `list`, marks the paragraphs a range of characters touches: a kind, or none; a level; or a level deeper or shallower by `by`. It is written where `list` lives, as `choose` writes a property.
5. **A list reads as a list.** How a state reads (`reading::html`) gives a text with items as `ul` and `ol` elements, nested by level, each item an `li`. The engine draws each item's marker and words in layers of their own, `cell: [item, 0]` and `[item, 1]`, as a table's cells are drawn, and the PDF tags them `L`, `LI`, `Lbl`, and `LBody`.

## Consequences

- **+** Lists are semantic: a theme changes every deck's markers and indents at once, and a reader and a PDF hear a list.
- **+** No change to how text is laid out for a text that is no list. Its lines, its goldens, and its morphs are as they were.
- **+** A list item is typed as a word processor's is: Enter adds one, Enter in an empty one ends the list, and Tab and Shift+Tab move one a level.
- **−** A `list` written by hand, or by a JSON Patch that changes `text`, can fall out of step with the paragraphs. It never fails: the paragraphs past it are no items. `replace_text`, which the editor and `find --replace` use, keeps it in step.
- **−** A marker is a string set in the item's look; a picture as a marker is not offered.
- **−** `balance` and `pretty` still fall back to `greedy` for text with hard line breaks, so a list of several items breaks greedily.

## Alternatives

- **Markers in the text** (`• `, `1. `, as Markdown writes them). The marker would wrap with the words, a reader would say it, and a theme could not change it.
- **The mark on each paragraph's first run.** Plain `text` would have to become `runs` to be a list, and the mark would split and join with the runs' looks.
- **One text per item, in a stack.** Each item a node of its own: a list would no longer be one text to type in, find in, or move, and numbering would cross nodes.

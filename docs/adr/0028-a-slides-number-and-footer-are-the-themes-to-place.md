# ADR-0028: A slide's number and footer are the theme's to place and the deck's to say

**Status:** proposed · **Date:** 2026-10-13

## Context

A deck for a board, a class, or a client carries its slides' numbers, so a reader can say "go back to 14", and often a line at the foot of each slide: a company, a date, "Confidential". Each presentation app does this:

- **PowerPoint** has Header & Footer: the slide number, footer words, a date, and "Don't show on title slide". The slide master places each.
- **Keynote** gives each slide a Slide Number checkbox. Its master slide places the number.
- **Google Slides** has Insert › Slide numbers, with "Skip title slides".

Scaena has neither. A person can type "3" on slide 3, and again on every slide, and number them all again after each move. An agent can do the same, with a node per slide or a delta per state.

Five facts frame the choice:

- **Determinism** (invariant 1). A number comes from the deck, never from a clock, so a date is words the deck holds.
- **The theme owns placement and type** (ADR-0005). Where the number stands, in what role, and on which layouts it is left off is the designer's call, as a master slide's is.
- **A text's characters are the deck's** (invariant 10, ADR-0019). Every offset counts the characters written: carets, `replace_text`, find, and lists. A run drawn otherwise than as written would put each of them off.
- **A slide is a run of states** (`Deck::slide_of`). Its number is its place among them, the same in each of its steps.
- **Nodes belong to the scene graph.** A state shows a node by tracking. A node on every slide would be in every state's props, and a slide that starts absolute (a copy's first state) would drop it.

## Decision

1. **The theme places the furniture** (theme format 0.14): `furniture: { number?, footer?, except? }`.
   - `number` and `footer` are each `{ col: [a, b], edge?: "foot" | "head", role, align?: "start" | "center" | "end", color?, pattern? }`. The text is set in `role`, across the grid's columns `a` to `b`. It stands in the margin at the canvas's foot (the default) or head, its lines centered in the margin's height, where no slot reaches. `color` is a color token or role, the role's color by default.
   - The number's `pattern` writes it: `{n}` is the slide's number, and `{total}` the deck's count of slides (`"{n} / {total}"`). It is `{n}` by default.
   - `except` lists the layouts a slide shows none on: title slides, dividers, and pictures that bleed to the foot.
   - Columns count in each format's own grid, so one placement serves every format.
2. **The deck says what shows** (deck format 0.19): `furniture: { number?: bool, footer?: string }`.
   - `number` shows each slide's number. `footer` is the words at each slide's foot, a date among them if the deck wants one.
   - A slide's first state may say `furniture: false` to show neither on that slide, as Keynote's checkbox does, or `furniture: true` to show them on a layout the theme leaves them off. A later step that sets it is **E106**: a slide shows its furniture whole or not at all.
3. **The number counts slides.** Slides are counted from 1 in the deck's order, each slide counted whether it shows its number or not, as PowerPoint counts a title slide. Every step of a slide shows the same number. Moving, copying, or removing a slide changes no byte of the deck, and each number is right after it.
4. **The engine draws it; it is no node.** A slide that shows furniture lays its number and footer out in their boxes, in the theme's role, as it lays out words, once per snapshot (invariant 3). They are drawn over the slide's nodes, so a picture behind the slide never hides them.
   - Between two steps of one slide, they stay as they are.
   - Between slides, the number of the slide left fades out and the next one's fades in, over the cue's transition. Words alike on both stay still.
   - It has no id. Nothing selects it, hits it, drags it, or lists it as a layer. Reading and the player's live region pass over it.
   - The PDF tags it as a pagination artifact, as PDF/UA marks page numbers and running footers.
5. **Lint judges it as text.**
   - E110 and E111 judge its contrast with what lies behind it, and E120 finds a character its family lacks.
   - **W303**, new, flags a node that draws content where the slide's number or footer stands. The fix is to move the node, or to set `furniture: false` on the slide.
6. **The editors set it in a person's words.**
   - The deck's Slide Numbers and Footer are the `furnish` op, which writes `/furniture` and takes it away when it shows nothing.
   - A slide's On This Slide is `set_state`'s `furniture`, written on the slide's first state.
   - Each editor shows them where the state's own choices are: the browser's inspector, and the Mac's and the iPad's Format tab, with nothing selected (`StateChoices.furniture`).
   - In `.scn`, the deck's header takes `furniture:{number: true, footer: "…"}`, and a state takes `furniture:false`.

## Consequences

- **+** A slide's number is right through every move, copy, and removal, with no edit, in every client and every export: PDF, PNG, video, and the single file.
- **+** The designer decides where it stands and in what type, per theme. A re-theme moves it with the rest.
- **+** It costs a deck one key. Every node, state, and offset stays as it was.
- **−** A number typed into a text is words, not the slide's number. "As shown on slide 12" in a sentence does not follow a move.
- **−** The furniture stands only in a margin. A theme that wants a number inside its grid places a slot there, and the deck writes the number by hand.
- **−** A date is words the deck holds. "Today" would break determinism, so nothing updates it.

## Alternatives

- **A field in any text** (PowerPoint's ‹#›, a run with `field: "slide"`). The text would draw characters it does not hold, which ADR-0019 refused. A node that shows on every slide also fights tracking: a slide that starts absolute drops it.
- **The number written into each slide by every write** (ADR-0019's way for quotes). Every move of a slide would rewrite every later slide's number, and each slide would need a node of its own.
- **Furniture as nodes a layout adds.** A theme would add nodes to the deck, which only `add_node` does, and a re-theme would leave the old theme's nodes behind.

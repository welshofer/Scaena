# ADR-0019: A quote is a run's pointer into its data, set again by every write

**Status:** proposed · **Date:** 2026-10-06

## Context

Every figure in a deck's words is a literal (SPEC §16 Q9). When the data moves on, a chart follows in one field, and the words need a hand edit for each figure. In the authorability spike that was 22 edits (PLAN 0.13). And nothing notices a claim that the new data makes false. PLAN 2.72 asks for a run that quotes a value of a data source, set as the column's format writes it and set again when the data changes. Lint should flag a claim whose figure the data no longer gives.

Five facts frame the choice:
- **The logical document is the truth** (SPEC §3.1, invariant 10). `deck.json` says what a deck shows. Reading, the PDF's text, find and replace, typing, lists, and morphs all read a text's characters as the deck writes them.
- **Offsets count characters of the text as written.** `replace_text`, `style_text`, `list`, carets, and the editor's selection all do (ADR-0013). If a figure drawn differed from the figure written, every one of them would be off by the difference.
- **Data already reads one way everywhere.** `scaena_core::data` loads a source for validation, charts, and the Data tab. `transform` runs `dataTransform`. `format` writes numbers and dates as d3 does.
- **Every write goes through `scaena-ops`.** Patches, data edits, theme edits, and restores all do (ADR-0009), and each is one change in the history, by its author.
- **A data file can change behind the deck's back.** It can be replaced on disk, or arrive with a pull. The history records that change as `fs`'s (ADR-0014).

## Decision

1. **A run's `quote` names the value it shows:** `{ "data": "@name", "dataTransform"?, "row"?, "column", "format"? }`.
   - The source is read as a chart reads it, through its `dataTransform` when it has one, so `aggregate` gives a sum or a mean.
   - `row` picks one row: an object of columns and values that exactly one row has (`{ "quarter": "Q3" }`), or an index from 0, negative from the end (`-1`, the last row, rolls forward with the data). Without `row`, the table must have exactly one row.
   - The cell is written by `format`: a number format for a number column, a date format for a date column. Without one it is written as a chart writes a category.
2. **The run's `text` is the figure as last set.** The engine draws `text`; nothing in a frame reads data for a quote. A quote is a pointer, and the deck still says what it shows.
3. **Every write sets the figures again.** After a patch or a data edit, `scaena-ops` works out each quote's value from the bundle as the write leaves it, and writes each figure that differs into the same change. The patch the operation reports includes these edits.
   - A beat's `claim` is written in the same change, where it holds the old figure as a word and its states show the text. The old figure is one the same quote gave: words a patch first makes a quote were no figure, and a claim that says them keeps them (PLAN 2.77).
   - A quote set by `style_text` is filled in this way too: the op names the quote, and the write sets its figure.
4. **Lint W427 flags a figure the data no longer gives.** That happens when a file changed on disk, or `deck.json` was edited by hand. It flags the quoted run, and each claim of a beat that shows it and holds the old figure; since PLAN 2.90, the beat's notes, the state's notes, and the descriptions of the nodes shown with it too, which a patch and a data edit set again likewise. Each finding comes with the patch that sets the figure, which `lint --fix` applies.
5. **Bad references are validation errors:**
   - a source the deck lacks is E102;
   - a column, a row that is not there, a row that matches many, or an empty cell is E103;
   - a format or a transform that does not parse is E106.
6. **Editing keeps a figure whole:**
   - `style_text` with `quote` makes the characters selected one run that quotes, and `null` takes the quote away (the words stay).
   - A `style_text` that cuts into a quoted run takes the whole figure.
   - Typing inside a figure makes it words: its run loses the quote. Typing beside one goes into a run of its own, with the figure's look but not its quote.

## Consequences

- **+** Every reader of a text's characters works unchanged: reading, PDF, find, carets, lists, morphs, and goldens. A deck's JSON says the figure it shows.
- **+** Rolling a deck forward is one data edit. The figures and the claims that quote them change in the same change, by the same author, and one undo takes them all back.
- **+** A stale figure is never silent: W427 finds it, with its fix.
- **−** A data file replaced on disk leaves stale figures until the next write or `lint --fix`. A render in between draws the old figure, and lint says so.
- **−** A claim is matched by its words, so a figure written another way ("4.2 million" for `$4.2M`) is not found. Figures in `alt` and in notes are not set again.
- **−** A quote is one value. The expression language that Q9 proposed is left to `dataTransform`'s steps, and a claim about a trend ("growth accelerated") is not checked.

## Alternatives

- **The engine sets the value in place of `text` at resolve time.** Rendering would always be fresh. But every offset in a text would count characters the deck does not hold, and an agent reading `deck.json` would see a figure the slide does not show.
- **A node-level binding (`text: "@revenue.total"`).** A sentence with one figure in it would have to be split across nodes.
- **An expression per run** (Q9's `"sum(revenue) where quarter = last(quarter)"`). That is a second query language beside `dataTransform`. Here `row` and the transform's steps cover the same cases with what the deck already has.

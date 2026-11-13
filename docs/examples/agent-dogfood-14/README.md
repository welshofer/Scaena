# Agent dogfood 14: an Italian capital plan from an annual report and a workbook, over MCP

This run was made on main at b80b75f0, with #1344, #1347, and #1348 merged over it locally. The
deck is Acque del Lario's ask to its board: approve a 64-million plan that takes the network's
losses from 24,1 % to 18 % by 2030. It is 12 slides for a 15-minute slot, in Italian, and was made
with MCP alone; `deck.json` was never edited by hand. No model was called: the slides are written
in Narrato's JSON, as an agent writes them.

The calls, in order:
1. `deck_generate`, with the PDF and the workbook as `sources`, 15 minutes, and an outline of
   five beats whose big idea runs 134 characters;
2. `deck_health`, then its fix by `deck_patch`;
3. `theme_apply` to Salon;
4. `deck_lint`;
5. `deck_export` to a PDF with notes.

Kept beside this report:
- `relazione_annuale.pdf`: the annual report, a page of Italian prose;
- `dati_gestione.xlsx`: two sheets, *Mensile* (twelve months) and *Impianti* (four plants);
- `lario.generate.json`: the call;
- `PLAN.md`: what each figure comes from, the answer key;
- `lario.deck.json`: the deck as made, fixed, and re-themed;
- `calls.jsonl`: every call and its result.

**Score:** 83 as made (pacing 90, balance 81, fidelity 81). Coherence, intensity, and tone go
unscored in an Italian deck, by design. It is 84 after the fix and after the re-theme.

## The planted figures

| Planted | Slide | As found | Right? |
|---|---|---|---|
| A: wrong source, 18,4 mln € cited to the workbook, held by the report | 5 | Nothing | **No** (finding 1) |
| B: no source holds it, 37 % cited to the report | 7 | Nothing | **No** (for R2: the `!english` gate) |
| C: a sheet column's total, 324 addetti | 8 | Nothing | Yes |
| D: a rate column's total, 98 % | 9 | Nothing | **No** (for R2: the rate-header rule, not on main) |
| E: a plan, 90 % *entro il 2028* | 10 | Nothing | Yes, but by the gate |
| F: the ask's own figure, 64 milioni | 11 | "cites neither …" | **No**: found wrongly (for R2) |
| (unplanted) the cover's 2026–2030 | 1 | Found as unsourced | **No**: a year range (for R2) |

With the deck's `lang` set to `es` on a scratch copy, A is found: "Slide 5 writes 18,4 milioni
and cites dati_gestione.xlsx, which does not hold it; relazione_annuale.pdf does." So the check
works, and Italian's marks are what it lacks.

## What works

- **Two figures (#1344).** Slide 4 takes `two-figures`: 410.000 abitanti and 96 comuni, its claim
  kept as its headline.
- **A long big idea on the close (#1347).** The 134-character big idea runs past Closing's
  heading, so the close is set in Key Point, the big idea word for word, "Grazie · Domande?"
  under it.
- **A health fix through `deck_patch` (#1348).** Slide 11, its plan taken away, is asked for
  intensity 0.96 with a fix. Sent to `deck_patch` as its `ops`, as the skill now says, the fix
  answers the finding and the score goes 83 → 84. Sent as `patch` it is refused: "unknown field
  `patch`, expected one of `bundle`, `ops`, `dry_run`, `verbose`".
- **The workbook as a source.** Both sheets are kept as data sources, and slide 8's 324, the
  *Addetti* column summed, is held.
- **`theme_apply` to Salon.** Applied with no error; lint is 0 before and after.
- **The PDF with notes.** 12 pages, each slide's Italian notes under it.

## Findings, ranked

### 1. Scaena has no Italian locale, so it reads and writes an Italian deck's figures in English marks

`scaena_core::format` knows en-US, en-GB, de-DE, fr-FR, es-ES, and nl-NL. An Italian deck falls
back to English marks, and three things break:
- **Charts.** Slide 3's line labels its values "3,120" and "3,270" where the deck writes
  "4.380".
- **Months.** The slide's months, *Gen, Mag, Giu, Lug, Set, Ott*, meet none of the sheet's, so
  the line stays unbound and draws its own rows. #1337 fixed how that is said, not why.
- **Sourcing.** "18,4" is read as 184, so the report's "18,4 milioni" never meets it, and the
  wrong source on slide 5 (plant A) goes unfound.

Proposed: an `it-IT` locale (decimal comma, a point for thousands, a space before %, Italian
month names and short forms), and `pt-BR` and `pt-PT` beside it, since a Portuguese deck falls
back the same way. That is mine to build; the health words it unblocks are R2's.

### 2. A line's callout lands on its last point, not the peak its headline names

Slide 3's headline names July's 4.380 peak, but the callout marks December's 3.270. Landing reads
"to" and the latest point; a headline that names a month and a peak ("picco a luglio") is not
read. Proposed: land on the category the headline names, as a bar's highlight already does.

### 3. A Numbers item whose figure ends it shows its figure twice, and a bound is drawn as a value

Slide 9's items are written "Varese perde il 31,5 %": the figure is taken out and set over the
words, which still end in it. And "restano sotto il 22 %" is drawn as the value "22 %". Proposed:
take the figure out of its words where it ends them, as where it leads them, and set an item
whose figure follows a bound word (*sotto*, *oltre*, *entro*; *under*, *over*) as words.

### 4. A 15-minute brief allows 5–10 slides, so 12 is found too long

`length_for_minutes` maps 0–15 minutes to "5-10 slides", so pacing finds this deck two slides
past its range, and the score loses it (pacing 90). Twelve slides in fifteen minutes is 75
seconds each, a common board pace. This moves the score, so it is the coordinator's call: either
16–20 minutes is where "10-20" starts, or 15 goes into the next band.

## For R2

The deck and this key, for the Italian words:
- **F:** the ask's own figure, found as unsourced. The ask words are *Chiediamo … di approvare*.
- **The cover's 2026–2030:** a year range, found as unsourced.
- **B and D:** blocked by the `!english` gate and the rate-header rule on R2's branch.
- **The plan words:** *entro il 2028*, *prevediamo*, which E needs once the gate lifts.

## Minor

- poppler's `pdfinfo` says "Suspects object is wrong type (boolean)" of this PDF and of B1's.
  `/Suspects false` is valid PDF, so this is poppler's, not ours.
- Slide 6 has no plan and is not asked for one, since its place calls for the intensity it
  would get. This is by design.

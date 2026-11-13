# Agent dogfood 13: a German board deck from a workbook and an annual report, over MCP

This run was made on main at b6e244eb, after #1304 landed. The deck is Nordwerk Logistik's
report to its Aufsichtsrat on 2025, closing with an ask: release 4,5 Mio. € for 25 more electric
trucks. It was made with MCP alone, and `deck.json` was never edited by hand. No model was
called: the slides are written in Narrato's JSON, as an agent writes them.

The calls, in order:
1. `deck_create`;
2. `data_attach` of the workbook;
3. `deck_generate` into that bundle, the composed path #1323 opened, with the PDF and the
   workbook as `sources` and an outline;
4. `deck_health`, then its fix by `deck_patch`;
5. `theme_apply` to Salon;
6. `deck_lint`;
7. `deck_export` to a PDF with notes.

Kept beside this report:
- `geschaeftsbericht.pdf`: the annual report, a page of German prose;
- `kennzahlen.xlsx`: two sheets, *Umsatz* (twelve months) and *Standorte* (four sites);
- `nordwerk.generate.json`: the call;
- `PLAN.md`: what each figure comes from, the answer key;
- `nordwerk.deck.json`: the deck as made, fixed, and re-themed;
- `calls.jsonl`: every call and its result.

**Score:** 86 as made (pacing 100, balance 81, fidelity 79). Coherence, intensity, and tone go
unscored in a German deck, by design. It is 86 after the fix and after the re-theme. No finding
below moves it.

## The three planted figures

| Planted | Slide | As found | Right? |
|---|---|---|---|
| A figure cited to the wrong source: 6,2 % EBIT-Marge, cited to the workbook, held by the report | 4 | "Slide 4 writes 6,2 % and cites kennzahlen.xlsx, which does not hold it; geschaeftsbericht.pdf does." Its fix cites the report and clears it. | Yes |
| A figure no source holds: 87 % zufrieden, cited to the report | 5 | Nothing | **No** (finding 1) |
| A sheet column's total: 412 Mitarbeitende, the Standorte sheet's column summed | 7 | Nothing | Right, but by luck (finding 1) |

## What works

- **The composed path.** `deck_create`, then `deck_generate` into the bundle it made, fills it
  in its own theme. The history holds both calls.
- **The workbook as a source.** Its two sheets are kept as `kennzahlen-umsatz` and
  `kennzahlen-standorte`.
  - Slide 6's bars bind to Standorte, and the source line reads "Quelle: Standorte ·
    kennzahlen.xlsx".
  - Two of the Umsatz sheet's cells are quoted into the words.
- **The wrong-source check, in German marks.** It reads "6,2 %" in the deck's marks and the
  report's "6,2 %" in its own. Its fix, cited by `deck_patch`, clears it.
- **`theme_apply` to Salon.** It is applied with no error, the new family listed, and it takes
  away a W203.
- **The PDF with notes.** Each slide's German notes stand under it, as written.

## Findings, ranked

### 1. In a deck not in English, a figure no source holds is never found

`health.rs`, in `misheld`, takes a figure none of the cited sources holds as the talk's own when
the deck is not in English: `own = !english || forward(line) || …`. That gate keeps plan words
(`forward`), which are English, from passing a German plan off as unsourced. But it passes
every unheld figure.

With the gate lifted, on a scratch copy of health, the deck raises three findings:
- **Slide 5's 87 %.** Right: it is planted and no source holds it.
- **Slide 7's 412.** Wrong: it is the Standorte sheet's `Mitarbeitende` column summed.
- **Slide 9's 25.** Wrong: it is the ask's own figure, "Wir bitten um Freigabe … für 25
  weitere E-Lkw".

So lifting the gate needs two things:
- **Plan words in the deck's language.** German first ("bitten um", "Freigabe", "Ziel",
  "planen", "bis"), as the turn's connectives already come in each language.
- **A sheet column's total counted as held.** That is, a figure that sums every cell of one
  column of a sheet the slide cites.

Source findings feed no measure, so no score moves.

### 2. A bare close headed by the big idea runs past its lines

Since #1312, a bare close whose claim an earlier slide states is headed by the big idea. Here
slide 9 states the Antrag claim, so the close takes "Nordwerk wächst profitabel und sauberer; 25
weitere E-Lkw halten Kurs auf 52 Mio. € Umsatz in 2026." (101 characters).
- In Dusk it is W202 (5 lines, `maxLines` 3) and W203 (shrunk to its `minSize`, 56 %, and still
  over).
- In Salon it is W202 (4 lines).

A big idea runs longer than a beat's claim, and the closing's `statement` holds three lines at
display size.

### 3. The close's thanks are said twice

The support line reads "Vielen Dank · Fragen? · Vielen Dank · Fragen?". The cause is
`Slots::thanks`, which joins the title, the subtitle, and each item with no repeat dropped. The
slide's title is "Vielen Dank", its subtitle "Fragen?", and its items the same two lines, which
is how a model writes a closing.

### 4. "Mär" is read as no month, so the monthly chart draws its own rows

The slide's categories are German short months: Jan, Feb, Mär, … Dez. The sheet's `Monat` is
`2025-03-01` and so on. Every month binds but "Mär", so the chart is left unbound. Two problems:
- **The month.** "Mär" is how German shortens März in a chart's axis, beside "Mrz".
- **The message.** `unbound`'s `why` reads "`kennzahlen-umsatz`'s `Monat` holds the slide's
  categories (Jan, …, Dez) but Mär", where "but no row is Mär" says what is meant.

### 5. Two figure-led points asked for as "Two Numbers" are set as a list

Slide 4 asks for `Two Numbers`, the shipped layout's own name, with "3,0 Mio. € EBIT" and
"6,2 % EBIT-Marge". `fell` says no layout is called that, and the slide is set as bullets.
- **The name.** `Two Numbers` is not among the names the generator takes.
- **The fallback.** It sets two figure-led points as bullets, where #1308 sets them as Two
  Numbers.

Two Numbers also has no place for a headline: the Spanish deck's re-measure found its claim
drawn nowhere ("This part of the story shows no claim"). So the fix waits on the coordinator's
call there.

### 6. `data_attach` of a workbook says it is "not UTF-8"

`data_attach` with `kennzahlen.xlsx` answers "`data/kennzahlen.xlsx` is not UTF-8". That names
a bundle path the call never wrote, and it doesn't say what does read a workbook. It should say
that `data_attach` takes CSV or JSON, and that `deck_generate`'s `sources` keep each sheet of a
workbook.

### 7. A health fix's `patch` is `deck_patch`'s `ops`

A finding's fix is `{ "says", "patch" }`. An agent that passes it on as
`deck_patch { patch: … }` is refused: "unknown field `patch`, expected one of `bundle`, `ops`,
…". The message points the way, and the retry with `ops` applies it. Still, either `deck_patch`
should take `patch`, or the skill should say that a fix's `patch` goes in as `ops`.

## Not findings

- **Slide 3's chart labels 4.450 at the line's end and again as its callout.** That is the line
  chart's design: the latest figure stands beside the line.
- **Pacing scores 100 though the notes run about 4 of 15 minutes.** This is as designed: no beat
  is timed but by Narrato's estimate, which is a floor.

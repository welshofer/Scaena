# Editor dogfood 8: a deck made from a PDF and a CSV, worked end to end

This dogfood made a deck with From a prompt…, grounded in a PDF and a CSV, against a scripted Anthropic. That is the path where #1266's PDF reader and #1283's kept figures run in the generator's module. The walk then:

- worked the Story down by its own buttons and fields, and by the assistant;
- saved the deck, typed over a slide, and used Restore This Slide;
- rehearsed against the brief's ten-minute slot;
- exported a PDF with notes.

It was walked in headless Chromium, the CPU painting, at main `0fc1c7c3` (#1283 landed), built with `just web`.

## How it was run

```
python3 docs/examples/editor-dogfood-8/report.py      # writes report.pdf
node docs/examples/editor-dogfood-8/walk.mjs [OUTDIR] [STAGES]   # make,story,restore,rehearse,export
```

**The sources.**
- `report.pdf` (reportlab) is the Clearwater lead-line inventory:
  - 31% of the lines remaining are in Eastgate;
  - $9,800 a line;
  - $42M of grants, which run to 2030;
  - finished by 2034 at today's pace, against a plan of 2032;
  - 3,700 lines a year to meet the plan.
- `lines.csv` is the lines replaced and remaining for each year, 2021–2025. In 2025, 2,100 lines were replaced and 18,400 remain.

**The brief.** A 10-minute talk to persuade the Clearwater Board of Water Commissioners. Its big idea is "Clearwater can replace every lead line by 2032, Eastgate first, and the grants pay for most of it."

**The scripted provider** answers each call:
- the arc: Problem–Solution;
- the outline: six beats;
- the slides, the look, and the notes, as drafted in `walk.mjs`;
- the assistant's turns.

The drafts plant two sourcing cases:
- **Slide 2:** a 40% figure that no source holds, cited to `report.pdf`.
- **Slide 4:** 2,100 and 18,400 from `lines.csv`, cited to `report.pdf`. This is #1283's case.

**As made:** 8 slides, story **85** (pacing 100, balance 88, arc 70, intensity 97, tone 97), with 6 findings. The deck keeps each source's figures (deck format 0.41):
- `report.pdf`: 31, 42, 2026, 2030, 2032, 2034, 2037, 3700, 9800;
- `lines.csv`: 15 figures, 2100 and 18400 among them.

## Findings, worst first

1. **#1283's finding does not fire on the deck it was built for.** Slide 4's beat cites `source:report.pdf`. Its words are "2,100 lines replaced in 2025 / 18,400 still to replace / Finished in 2034, past the plan". `report.pdf` holds neither 2,100 nor 18,400; `lines.csv` holds both. Health finds nothing, in the browser and in `scaena health` on the same deck.
   - **The cause** is in `crates/scaena-ops/src/health.rs`. `miscited` counts a source as cited wherever the slide names it (`names_source`). `source_words("lines.csv")` is `["lines"]`, and the slide's own words "2,100 *lines* replaced" contain it. So `lines.csv` is taken as cited, it holds 2,100, and the finding is never made.
   - **Shown by one word.** With "lines replaced" changed to "pipes replaced", `scaena health` says: "Slide 4 writes 2,100 and cites report.pdf, which does not hold it; lines.csv does." Its fix, Cite lines.csv, applies and clears the finding, and the score stays at 85.
   - **The same match decides what counts as cited for an unsourced figure** (`names_source(&shown, &sources)`, W1's dogfood 5). Any source whose name is one ordinary word (`lines`, `report`, `budget`, `survey`) is "named" by prose that uses the word.
   - **Routed to the coordinator** with a proposal: a one-word stem names its source only as its whole file name ("lines.csv"), or right after "source"/"sources" ("Source: lines"). A stem of two words or more is still matched as it is now ("season report"). `source` findings feed no measure, so no score moves; the counts in `health-calibration.md`'s tables may.

2. **A figure no source holds, on a slide that cites a source, draws no finding.** Slide 2 says "Children in homes with lead lines show 40% higher blood lead" and cites `report.pdf`. No source holds 40. This is #1283's rule as written: a figure no kept source holds may be worked out from the sources, or be the talk's own (a target, an ask), so it is no finding. And the unsourced finding (W1's dogfood 5) only looks at slides that cite nothing. So a factual figure, cited to a source that does not hold it, passes silently. **Noted for the coordinator.** It is by design today, and the fix is a judgment about which figures are claims.

3. **Cite a source… was not exercised.** It is offered on the unsourced finding, and no slide in this deck cites nothing while writing a figure. Finding 1 would have reached the sourcing row's own fix, Cite lines.csv. That fix was exercised in the CLI (above), not through the Story. **The walk should re-run once finding 1 is fixed.**

4. **A made deck's Versions tab, before its first save, is empty.** It shows no summary line and no Keep. After the first save it offers Keep a history, where `deck_generate` would have begun one. **Fixed in #1295** (open): a deck the editor makes begins its history at its first save, and the tab says so before it.

5. **The fix Cite lines.csv replaced `report.pdf` on slide 4, where adding it beside might be wanted.** The slide also says "Finished in 2034", which `report.pdf` holds. But 2034 reads as a year, not a figure, so the fix finds the report spare. This is minor, and the choice is defensible: a year is not checked against sources.

## What held

- **Made from the PDF and the CSV in the browser.** Both were read and said ("Read report.pdf (pdf)", "Grounded in lines.csv (csv, 1 KB)"). The outline was settled (8 slides in all against 5–10), and the slides, the look, and the notes were written. The deck keeps each source's figures (finding above), and `lines.csv`'s 2,100 is quoted as a run on its cell (`quote: {data: @lines, row: 4, column: replaced}`).
- **What was made is said.** "Slide 8 could not be set as “Closing” asks: `closing` has no place for 2 items, so it was set as a list of them all". The link on slide 8 is said too, and health's `link` finding asks after it.
- **The Story's own buttons and fields worked.** The headline field cleared the `label` finding on slide 3; the "Update it to …" offer for the beat's claim followed and was taken. Plan it at 0.72 on slides 5 and 7 raised the arc 70 → 77 and the score 85 → 88. The assistant answered the turn and big-idea findings on slide 8 in one answer ("Answers 2 of the Story's findings").
- **Restore This Slide.** After a save and Keep a history, slide 5 was typed over, saved, compared ("Slide 5: … its words now “Replace the lines faster”"), and put back ("Slide 5: … put back from version 1 · ⌘Z undoes it"). Then ⌘Z gave the typing back.
- **Rehearse.** "40.1 s taken against a 10:37 plan, for a 10-minute slot: −9:56", with each slide against its plan, and Keep these timings offered.
- **Export… a PDF with notes.** It says what it makes before it is made, and gives 8 pages, each slide drawn smaller with the notes read for it under it. The notes repeat within a page because the walk's scripted notes repeat a paragraph to fill each slide's budget. The engine did not do that.

# Dogfood 14: the browser's From a prompt… end to end

This dogfood ran an English 15-minute Inform talk for a hospital board, made in the browser's From a prompt… and grounded in a PDF and a CSV. Pictures were off, and the look and the notes were on. The outline was settled by hand, the run was cancelled mid-slides, and the deck was then made again. The deck was judged against the CLI path dogfood 13 checked. Built at main a3dc732d: the WASM modules and `web/dist`, as `just web` builds them.

## How it was run

`walk.mjs` drives the editor in headless Chromium, the way `web/generate.mjs` does, using `web/serve.mjs`. It also stands in for Anthropic on this machine, answering each step from a script:

- **The arc:** Data Storytelling.
- **The outline:** the big idea, and a claim for each of the arc's five beats.
- **The slides:** written to the settled outline's beats as the slides' prompt lists them. They include:
  - a claim set as a Statement on each beat's first slide;
  - a line chart of `sepsis.csv`'s compliance, with a second bullet its layout has no place for;
  - a "Venn Diagram", which no layout is called;
  - a beat whose slides are about the cafeteria, off its claim;
  - a close that is "Thank you" over "Questions?" and an email.
- **The look:** two changes:
  - a title that sums the slide's figures ("$330,000");
  - a settled claim's title shortened to "Compliance is up".
- **The notes:** written to each slide's budget.

```
node docs/examples/dogfood-14/walk.mjs OUTDIR     # from the repository's root, after `just web`
```

The talk is "Sepsis care at Lakeside General: where we stand after a year of the bundle". It is for the hospital board, its goal Inform and its length 15 minutes. It is grounded in `report.pdf`, a quality report, and `sepsis.csv` (`quarter,bundle_compliance_pct,sepsis_mortality_pct`).

In the dialog:

1. **Settled the outline by hand:**
   - retyped the Data beat's claim;
   - gave the Data beat two slides;
   - moved Implication before Insight;
   - added a beat, "Night shifts", before the call to action.
2. **Wrote the slides, then pressed Cancel while they were being written.**
3. **Opened From a prompt… again and made the deck,** then let it land on its Story tab.
4. **Reloaded the page,** to see whether the deck, kept nowhere, was offered back.

## Findings, worst first

Each finding has its own pull request off main.

1. **A cancelled run lost what it had paid for (#1183).** Cancel said "What has been made so far is lost", and it was. The dialog opened again with its brief and files, but no Go on and no outline. Generate asked for the arc and the outline again, two calls paid for twice, and the outline had to be settled by hand a second time. The worker had already reported the arc and the settled outline as paid. Only the dialog's opener, which reset `paid`, threw them away.
   - Now: a run stopped before its deck is made keeps what it paid for until a deck is made or Generate starts afresh. Cancel says so, and the dialog opened again offers Go on, which asks for the slides alone, written to the outline as settled.
2. **A beat added showed 0 slides while the total counted it as one (#1185).** The total read "9 slides in all", and the prompt gave the beat a slide. Its field said 0, so the person could not tell what it would get.
   - Now: a beat added starts at one slide.

3. **After a fold, the Story said a finding was answered that had only moved (#1191).** Pressed on the slide that said again what the one before it said, Fold into slide 3 folded it, one change that ⌘Z took back. But the status then said "Answers 1 of the Story's findings: slide 9's plan", a plan the fold had left as it was. The Story matched a finding from one read to the next by its slide's number, and a fold renumbers every slide after it. A finding that moved up a number looked gone, and another took its old number.
   - Now: a finding is matched by its slide's state, which renumbering leaves as it was. On `web/story.mjs`'s higher-ed fold, main says "slide 10's turn" was answered when nothing went; the fix says nothing.

## What held

- **The settled claims.** The claim retyped by hand stands on its slide as typed, its figures quoting `sepsis.csv`'s cells. It is set `fit: shrink` and never rewritten by the fit (#1155). The look's shorter "Compliance is up" was refused for dropping its figures, and the beats follow the outline as settled.
- **Go on.** After #1183, going on after Cancel asks for the slides alone, as going on after a failure did.
- **The progress strip and its words.** They say each `fell` ("Slide 8 could not be set as 'Venn Diagram' asks…"), each `unset` ("Slide 4 shows its chart without 'Night shifts are not broken out here'…"), and each change the look let go, with why. The chart was bound to `@sepsis`, so no `unbound` line was needed. On main, the over-long settled claims carry lint marks on their thumbnails, but the progress does not say them. Walked on #1178's branch, the progress says each by its slide: "Not rewritten to fit: slide 3's settled claim stands as written, in 6 lines where its place takes 3; slide 5's …; slide 7's …; slide 10's …: shorten them in the outline to fit".
- **The Story.** The deck lands on its Story tab, at 85 of 100, which the status line says. It leads with:
  - a slide that says again what the one before it said, with Fold into slide 3;
  - the email no source names;
  - a close that never lands the big idea;
  - the drifting "Night shifts" beat: "Says nothing of the claim settled for the Night shifts part of the talk".

  The rest fold under "6 more". Its first fix works: Fold into slide 3 takes the deck from 14 states to 13, and ⌘Z gives back 14.
- **Kept nowhere until saved.** The line under the slide says "Not saved yet". Reloaded, the editor's start lists "Sepsis care at Lakeside General, changed …" with Recover and Discard.
- **Pacing.** About 15 minutes of notes against the 15 asked.

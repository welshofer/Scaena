# Dogfood 17: the narrative loop, end to end, in the browser

This dogfood made a deck with From a prompt…, grounded in a PDF and a CSV, with a brief that asks 12 minutes. Its Story findings were then worked down as a person would. It was rehearsed against the brief, presented with the presenter view, and exported as a PDF with notes.

It was walked in headless Chromium against a scripted Anthropic, the CPU painting, at main cef71d62, built with `just web`.

## How it was run

`walk.mjs` drives the editor and the player by their buttons, fields, and keys. `window.scaena` is only read, to wait and to say what the page shows. It answers as the provider on this machine:

- **The arc:** Problem–Solution.
- **The outline:** a big idea and six claims.
- **The slides:** eight, drafted as a model drafts a council talk from a forestry report.
- **The look:** nothing changed.
- **The notes:** sentences about the slide, to the words asked.
- **The assistant:** a model that answers each question with real calls. It calls `deck_read`, then `deck_patch` with the edit the question asks for, then answers.

```
node docs/examples/dogfood-17/walk.mjs [OUTDIR] [make,story,rehearse,present,export]
```

The talk is "Riverton's street trees: we are losing shade faster than we plant it", for the Riverton City Council. Its goal is Persuade and it runs 12 minutes. It is grounded in `report.pdf`, an urban forest assessment, and `canopy.csv` (`year,canopy_pct,trees_planted,trees_removed`, 2016–2025).

1. **Made:** From a prompt…, the outline as asked, then Write the slides.
   - Health: 84, its pacing 100 ("About 13 minutes of talk, where its brief asks 12 minutes").
   - Eight findings.
2. **The Story worked down:** each finding in the order it stands.
   - Its fix button where it offers one.
   - Else its Edit its headline or Edit its notes field, typed by hand.
   - Else Ask the assistant.

   Before and after each, the score, what was found, and what the editor offered were read. Where Update was offered, it was pressed.
3. **Rehearsed:** from slide 1, four seconds a state.
4. **Presented:** Play, then the presenter view from the player, on through five states.
5. **Exported:** Export…, PDF, Include the presenter's notes.

## Findings, worst first

1. **Working the Story barely moves the score.** Nine findings were worked, and the score went from 84 to 86.
   - **What moved it:** only the two "Plan it at 0.7x" fixes, which raised fidelity.
   - **What cleared for nothing:**
     - two topic headlines rewritten as their claims (`label`);
     - two slides made to carry their beat's claim (`claim`);
     - the close made to land the big idea (`bigIdea`);
     - a turn bridged in the next slide's notes (`turn`).
   - **For Jay:** these findings are not measures of the score, so the panel's number does not follow the work it asks for. A change that moves the score waits on him.
2. **A made deck's beat claims are its slides' first titles, and they go stale.**
   - **What the spine holds:** the beats read "Canopy cover, 2016–2025", "Current planting efforts", and "Thank you". The claims the outline settled are kept only in `meta.narrative.outline`, which health reads.
   - **No Update offered:** typing a new headline through the Story's Edit its headline field offered no Update to restate the beat's claim (`#offering` stayed empty, twice).
   - **The presenter view then misleads:** it said "Existing Approaches · Current planting efforts" over a slide headed "About 450 trees planted a year".
   - **A model misreads it too:** a model reading the deck by `deck_read` takes the beat's claim as the claim. The scripted assistant, asked to carry the settled claim, wrote the beat's topic back over a headline just fixed, until it was pointed at the claim the question quotes.
3. **The close ping-pongs between two findings.** `bigIdea` asks the close to land the big idea. `claim` asks it to carry "Fund the plan in the 2027 budget".
   - **The conflict:** the close has one statement. Landing one raised the other, four rounds in a row.
   - **Neither ask mentions the other.** The generator heads a bare close with its beat's claim, which `bigIdea` then asks to replace.
4. **The bar chart never binds to `canopy.csv`.** Slide 6 (Planted and Removed, 2021–2025) kept its own rows.
   - **The message:** "no column of `canopy` holds the slide's categories (2021, 2022, 2023, 2024, 2025)". But `year` holds each one.
   - **The series:** Planted and Removed are `trees_planted` and `trees_removed`.
   - **Why it matters:** a data edit will not redraw the chart, and the message names the wrong cause.
5. **The label ask leaves words twice on the slide.** "Make slide 4's claim its headline: say 'About 450 trees planted a year'". Done as asked, the headline and the list's first point say the same line. The ask does not say to take it from the list.
6. **One figure in a slide's draft notes drops all of them.**
   - **The first run:** every slide's draft carried one sentence with "2041" in it. The notes pass refused all eight ("Slides 1–8's notes with a figure the slides do not write, each left without notes"), and the deck landed with no notes.
   - **Afterwards:** the Story asked for notes on all eight slides.
   - **The run written up here:** its drafts carry no figure, and every slide kept them.
7. **The rehearsal never mentions the brief's 12 minutes.**
   - **What it says:** "40.1 s taken against a 12:53 plan", the beats' durations.
   - **What health says:** "About 13 minutes of talk, where its brief asks 12".
   - **The presenter view:** its pace line ("5:06 ahead") is against the beats too.
8. **Health quotes a headline the slide does not show.** Slide 3's `label` finding quotes it as "25.8: Canopy cover, 2016–2025", the line's callout figure joined to its headline. The ask then says "in place of" those words.
9. **A row's Ask the assistant joins every ask on it.** Slide 8's row put the link, the big idea, and the claim into one question. The answer made one edit, and the row stayed.
10. **Coherence has no score in an English deck:** "Read by their words: 1 slide where a new part of the talk begins". This may be by design.

## What held

- **From a prompt…:**
  - the PDF and the CSV read;
  - the outline: "8 slides in all · the length asked: 5–10";
  - the look and the notes;
  - "Made: 8 slides on the arc Problem–Solution".
- **The Story's fields:**
  - Edit its headline typed in the slide's headline, its words selected.
  - Edit its notes put the caret in the notes. A bridge typed there answered the `turn` finding: "Answers 1 of the Story's findings: slide 6's turn".
- **The fixes:** each "Plan it at 0.7x" cleared its finding, and the score rose by one each time.
- **The assistant's edits:** each was one change. "Undo this answer" was offered.
- **The rehearsal:** each state against its beat's plan, the whole against the plan, a build's steps each its share, and Keep or Discard.
- **Play on a deck kept nowhere:** it kept the deck in the browser first ("Kept in this browser to play it"), then played it.
- **The presenter view:**
  - the slide and its step ("Slide 4 of 8, step 2"), the section, and the next state;
  - the notes, and the clock waiting at 0:00 until the show starts;
  - the pace against the plan.
- **The PDF with notes:** eight pages (`rivertons-street-trees - Notes.pdf`), each the slide with its notes under it. Its note said what it holds before it was made.

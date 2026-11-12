# Dogfood 16: a hand-built deck, revised in the browser

This dogfood built a deck by hand in the browser's editor, then revised it as a person revises one after a review. It was walked in headless Chromium, the CPU painting, against main 133e70d7, built with `just web`. Nothing was generated.

## How it was run

`walk.mjs` drives the editor by its buttons, menus, keys, and drops, as `web/first-deck.mjs` does. `window.scaena` is only read, to wait and to say what the page shows.

```
node docs/examples/dogfood-16/walk.mjs [OUTDIR]    # from the repository's root, after `just web`
```

The deck is "Our library at night", made with New in Dusk:

1. **The title slide:** its title and the line under it typed over.
2. **The hours slide:** + Slide, Blank, laid out in the theme's `bullets`. A headline and a body are inserted and typed ("We now open late four nights a week" over three lines). Build In is By Bullet.
3. **Notes on the hours slide:** on its first state, "We stayed open late because students asked us to." On its last step, notes of its own: "Then the study rooms: booked out every exam week."
4. **The chart slide:** `visits.csv` (`month,visits`) dropped on a blank slide.
5. **A repeat:** a slide that says the hours again ("Late opening four nights a week"), as a draft often does.
6. **Saved:** in the browser, with Keep a history. Then the title was typed again and saved.

Then it was revised, in order:

1. **The Story:** its findings, and its Fold.
2. **Versions:** a restore.
3. **The Data tab:** a row added at the top.
4. **Export…:** the PDF with notes.

## Findings, worst first

1. **The PDF with notes leaves out notes written on a slide's first state.** Slide 2's page prints only its last step's own notes, "Then the study rooms: booked out every exam week." The notes the slide opens on, "We stayed open late because students asked us to.", are on no page.
   - **Why:** the PDF prints the notes of the state each page draws, its last step. That step has notes of its own, so nothing earlier is read.
   - **Who loses:** a presenter reads both, on step 1 and on step 3. A handout should carry what the presenter says over the whole slide.
   - **Fix:** print each step's notes in turn, said once.
   - **Timing:** #1211 changes the same function, so this fix waits on it.
2. **The Story offers a fold, and the fold refuses.** Slide 4 says the hours again, and the Story says so and offers "Fold into slide 2". Pressed, it is refused: "Slide 4's words would run past slide 2's place for them: shorten them first, or cut slide 4."
   - **Why:** the offer is decided without laying the fold out. The refusal is the fold's design, held by a test (`a_fold_whose_words_would_run_past_their_place_is_refused`).
   - **For the coordinator to decide:** a person is offered a button that fails. Either the offer checks the fold first and says "Fold into slide 2's notes" where the words won't fit, or the fold puts such words in the notes instead of refusing.
3. **A build step's notes are blank.** On main, slide 2's By Bullet steps 2 and 3 show no notes until one is typed on them. A presenter view is blank there.
   - **Fix:** #1211, a step reading on from its slide's notes.
   - **Also checked:** a step's own notes, written in the inspector, landed on that step and left the slide's first state's as they were. That is what #1211's `fork` fix keeps.
4. **The title slide's "Your name · the date" ships.** The text New starts the title slide with under its subtitle is printed in the PDF as written, and presented. Nothing says it was never typed over: no lint, no Story finding.
   - **Proposal:** a finding for words a layout started a slide with, left as they were.
5. **A chart slide is said to say nothing past a headline it does not have.** The CSV dropped on a blank slide drew a chart with no headline. The Story said both "Slide 3 has no headline" and "Slide 3 says nothing past its headline, and has no notes."
   - **Fix:** #1224 asks a slide that draws a chart, a picture, or a table for its notes alone.
6. **The Data tab cannot add a row at the top.** `+ Row` adds after the row focused, so on the first row it added at row 2.
   - **Fix:** #1226 adds `+ Row Above`.
7. **Versions restores only a whole version.** Restoring version 1 brought the first title back, as asked, and would take every later edit with it. The browser offers no way to put back one slide, though the CLI and MCP do (`history --restore V --slide S`, #1172).
   - **Proposal:** Restore This Slide in Versions, by `restore_slide`.

## What held

- **New, Add Slide, and typing:** the title typed over; Blank laid out in `bullets`, its headline and body inserted and typed where they stand.
- **By Bullet:** two steps added, "Build In: By Bullet · on 3 steps of Slide 2".
- **The CSV dropped on a blank slide:** the deck's data `@visits`, with a chart of it.
- **Saved in the browser and a history kept:** "This deck keeps no history. Keep a history saves it now…". Then version 1, "The history begins", and version 2, "Typing".
- **The Story read the repeat:** "Slide 4 says again what slide 2 said: its headline shares "four", "late", "nights" and "week" with "We now open late four nights a week"". It asked for the fold and offered it.
- **A whole-version restore:** "Version 1 restored · ⌘Z undoes it", the first title back.
- **The PDF with notes:** downloaded as `library-report - Notes.pdf`, its note saying what it holds beforehand.

# Editor dogfood 7: a made deck's Story, worked end to end

This dogfood opened a generated deck in the editor as a person would and worked its Story findings down: Show, Ask, the fix rows, the headline and notes fields, Restore This Slide, and a rehearsal against a brief that asks ten minutes. No assistant key was used, so the findings were worked by their own buttons and fields alone, the way a person without a key sees them.

It was walked in headless Chromium, the CPU painting, at main `371e39a8`, built with `just web`.

## How it was run

`walk.mjs` drives the editor and the player by their buttons, fields, and keys. `window.scaena` is only read, to wait and to say what the page shows.

```
node docs/examples/editor-dogfood-7/walk.mjs [OUTDIR]
```

The deck is `larder`, a seed pitch (SaaS that plans a family's week from what is already in the kitchen). It is made from `docs/examples/health-calibration-outlined/larder.{slides,outline}.json` by `scaena generate --slides --from-outline`, with a brief that asks ten minutes, `--no-look --no-notes --pictures none`; its history is begun by `scaena save --history`. The outline is six beats; the deck is ten slides.

1. **The Story read:** its score, its findings, and what each row offers.
2. **Show and Ask tried** on the rows that offer them.
3. **The findings worked down** by their own buttons and fields, no assistant: a fix where one is offered, a headline or notes field typed in, else left where only Ask is offered.
4. **A save, then Restore This Slide** on a slide from the first version, and ⌘Z.
5. **A rehearsal** from the first slide against the brief's ten minutes.

Health as made: **87** — pacing 100, balance 90, arc 74 — with **14 findings**.

## Findings, worst first

1. **Restore This Slide on a slide the deck already draws as the version drew it says "put back" and makes an empty undo step.** The walk's Restore This Slide landed on Slide 2 ("$1,500: A year per family of four"), a slide nothing had edited. The restore wrote the deck all the same (its states written another way), said "Slide 2 … put back from version 1 · ⌘Z undoes it", and `⌘Z` took the deck straight back to itself ("Undone: Slide 2 … put back"). A person is told something came back and given a step to undo that changes nothing.
   - **Fixed in #1276.** `slide_restored` now writes nothing when no state changes what it draws, the spine is unchanged, and no file differs; the editors say "Nothing put back: Slide N is as version V drew it", keep any kept note, and make no step to undo. The same PR fixes the "shown on another slide" test to count a node by whether a state there *draws* it, not merely names it to remove it, so a slide's own object — named by the following slide only to take it away — comes back with its words.

2. **A slide's notes typed in the Story live on its beat, and the version compare then names no slide.** Three turn findings were answered by typing a bridge in the Story's Edit its notes field (slides 4, 8, 10). The save's history listed each as "A slide changed", but comparing version 1 with the deck now said only **"The deck's story changed"**, naming no slide. Restore This Slide on one of those slides cannot bring its notes back either: a beat the deck still has keeps its own notes (#1172's rule).
   - A person who changed one slide's notes and wants to see or undo just that is shown a deck-wide "story changed" with no slide to point at. **Routed to the coordinator** (wording in the version compare, and whether a slide's own notes should be restorable with its slide).

3. **The rehearsal shows a plan a quarter of the slot it names.** From slide 1 the rehearsal said: **"45.2 s taken against a 3:00 plan, for a 10-minute slot: −2:14."** The "3:00 plan" is the beats' estimated durations; the "10-minute slot" is the brief. Health's pacing ignores a made deck's estimated beat durations by design (so the deck scores pacing 100), but the rehearsal puts both numbers in one line, where the plan reads as a quarter of the slot it is measured against. **Routed** as wording.

4. **Without a key, the claim, big-idea, and dense findings are read-only.** Their rows offer only Ask the assistant (and, for the claim kinds, Check the claim), both model-backed; Ask with no key focuses the key field with "Paste a key to ask this". So of the 14 findings, the ones a person without a key could act on were the three with a notes or intensity field and the arc fixes; the claim findings (slides 3, 5, 7), the big idea (slide 9), and the dense slide (slide 6) could only be read. This follows the design — a claim is coached, not a single field to edit (`fieldOf` in `web/src/story.ts` offers a field only for intensity, notes, and headline; `Field::Claim` is for agents) — and matches dogfood 17's finding 10. **Noted, not fixed here** (the claim rows fall under another lane's work).

5. **The score does not follow the work the panel asks for.** Answering slide 8's notes-read-aloud finding dropped the count from 14 to 13 ("Answers 1 of the Story's findings: slide 8's notes") but left the score at **87**. The findings that move the score are the arc "Plan it at 0.8x" fixes; the turn, claim, dense, and notes findings clear without moving it. This is dogfood 17's finding 1 again: a change that moves the score waits on Jay.

## What held

- **Show** on a finding's row went to its slide and opened the Document tab ("No issues"); a finding over two slides listed both.
- **The fields worked:** Edit its notes put the caret in the slide's notes and typed a bridge; a notes edit that answered a turn said "Answers 1 of the Story's findings". Set its Intensity and the "Plan it at 0.8x" fixes stood on the rows health gives a `field` or a fix.
- **Ask with no key** said what to do rather than failing: the key field focused, "Paste a key to ask this", the question kept in the box.
- **The save** wrote ten files and began recording ("Saved, 10 files, and the changes kept in its history"); the versions listed each edit as "A slide changed · you".
- **The rehearsal** paced from the slide shown and named the brief's ten-minute slot, each state 2.5 s, the plan and the slot both shown.

## Disposition

- **#1276** — Restore This Slide / version restore that changes nothing (finding 1), browser and Mac, with tests.
- **Routed to the coordinator** — the version compare naming no slide for a notes-on-beat change and the un-restorable slide notes (finding 2); the rehearsal's plan-vs-slot wording (finding 3).
- **Noted, another lane** — the read-only claim rows without a key (finding 4).
- **For Jay** — the score not following the panel's asks (finding 5).

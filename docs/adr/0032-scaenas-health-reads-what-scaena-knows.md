# ADR-0032: Scaena's health reads what Scaena knows

**Status:** accepted · **Date:** 2026-10-24 · **Decided:** 2026-10-25, Jay

## Context

A deck's narrative health is Vzestup's, "its figures Vzestup's to the last bit" (PLAN G.9, ADR-0026; `scaena_narrato::health`). That was Jay's call. Any change that moves a figure for some deck departs from it, so each one is put to Jay here.

On 2026-10-23 a dogfood pass made five decks from Narrato slides at main e4a32428: a pitch, a QBR, a lecture, a keynote, and a launch. Their inputs are on `wip/dogfood-decks`, #526. Health scored them 85–93, and lint found no errors, yet about one slide in five misstates a figure or looks broken. The pass found five things wrong with what health says.

#532 fixes what moves no figure:
- **No fix rewrites the plan.** The intensity and tone fixes declared what the words read. On the pitch, applying all nine made intensity and tone read clean with no word changed, while fidelity fell from 86 to 82 and the score from 89 to 88. The findings now ask for the words to be rewritten instead.
- **A slide lint finds an error on comes first.** `health_in(deck, lint)` makes it a `layout` finding of its own, ahead of every other.
- **An unread language says so.** A deck whose words the lexicons cannot read now gets no number for intensity and tone, where it scored 100 before.
- **The CLI prints each finding's question.**

This ADR proposes the departures that would move a figure. Each was measured on the five dogfood decks and on `probe.json`, twelve slides of the layouts the five did not reach. "Before" is #532 (bd136ea), whose figures are Vzestup's. "After" is the same code with each departure added (`wip/health-departures`, bcfa1e4). Every deck was made with `scaena generate … --slides … --no-look`, as `dogfood/README.md` says, so no key was needed.

## The departures, measured

### 1. Cap the score where lint finds a slide broken

**Proposal:** score ≤ 100 × (clean slides ÷ slides) − 10 per slide with an error, floored at 0.

**Measured:** this changes nothing on any of the six decks. Lint finds only warnings on them:

| deck | lint |
|---|---|
| pitch | W210 × 3 |
| QBR | W210 × 4 |
| lecture | W210 × 5, W220 × 1 |
| keynote | W210 × 3 |
| launch | W210 × 3 |
| probe | W210 × 1, W220 × 1 |

The faults the pass saw are not lint errors: dropped figures and units, text over text, and a 356 px void on the team slide. A cap driven by lint cannot see them.

**Recommendation:** adopt the cap, since it is right where lint does find an error and costs nothing where it doesn't. But don't expect it to fix the 85–93. That needs lint to find these faults, as its own work: overlap judged against what is drawn, a slot left empty, a figure that lost its unit.

### 2. Read tone from the words the room sees, not the notes

**Proposal:** tone is read from the slide's headline and body, without its notes.

**Vzestup's reading.** Vzestup reads tone from `slideTextForDetection`: a slide's title, body, and notes, a line apart. The Rust port is `SlideForHealth::text`, `crates/scaena-narrato/src/health/mod.rs:82–91`. `estimateDetectedValence` (`valence.rs:203–218`) then spreads the lexicon's weight over every token: `let net_per_tok = (positive + negative) / tokens.len() as f64`. Long, plain notes thin it out. That is Vzestup's formula, so this departure changes what Scaena feeds it, not the formula.

**Measured.** Tone read on each slide that declares −0.3 or graver:

| deck · slide | declared | read before | read after |
|---|---|---|---|
| pitch 3, "errors drain $9 billion" | −0.70 | −0.01 | −0.18 |
| pitch 2, 4, 5 | −0.40, −0.50, −0.50 | 0.08, 0, 0 | 0, 0, 0 |
| QBR 7, "SMB churn climbed" | −0.60 | −0.57 | −0.49 |
| QBR 8 | −0.50 | 0.08 | 0 |
| keynote 2, "went dark" | −0.80 | −0.20 | 0 |
| keynote 3 | −0.50 | −0.08 | −0.20 |
| keynote 6, "nearly died" | −0.50 | −0.07 | 0 |
| launch 3 | −0.60 | −0.10 | 0 |
| launch 5 | −0.30 | 0 | −0.20 |
| probe 7 | −0.40 | −0.19 | −0.26 |

Tone score and tone findings:

| deck | tone before | tone after | tone findings before → after |
|---|---|---|---|
| pitch | 72 | 74 | 7 → 5 |
| QBR | 86 | 84 | 0 → 1 |
| lecture | 86 | 84 | 1 → 1 |
| keynote | 74 | 72 | 4 → 5 |
| launch | 71 | 72 | 6 → 6 |
| probe | 86 | 87 | 0 → 0 |

Leaving the notes out moves a grave slide only where its visible words are in the lexicon ("errors drain $9 billion"). Most grave slides read about 0 either way: "went dark", "nearly died" and "retyping the same answers" carry no lexicon word. The lexicon is what fails, not dilution by the notes.

**Recommendation:** don't adopt this alone; the gain is too small. Since tone is never in the score, a better change is to show the tone finding only where the words read something, not where they read 0 against a declared tone. "Reads neutral, where it was written to sound grave" is mostly the lexicon's silence.

### 3. Don't count one-idea layouts as thin

**Proposal:** big-number, number-photo, stat, quote, photo-quote, quote-portrait, statement, thesis, and big-question make one point by design. Leave them out of the points made on average. This is a `one_idea` flag on `SlideForHealth` that is false by default, so a deck without these layouts reads exactly as Vzestup reads it.

**Measured:**

| deck | balance before → after | score before → after |
|---|---|---|
| pitch | 84 → 87 | 89 → 90 |
| QBR | 97 → 97 | 93 → 93 |
| lecture | 100 → 100 | 93 → 93 |
| keynote | 83 → 97 | 89 → 94 |
| launch | 77 → 91 | 85 → 90 |
| probe | 96 → 96 | 93 → 93 |

It removes a contradiction with lint: balance marked a one-idea slide down while W210 warns past 40 words. But it raises every score it touches, by up to five points, and the pass's complaint is that the scores are already too high.

**Recommendation:** adopt it only together with a change that brings scores down where slides are broken (1, once lint sees those faults). Alone, it makes the headline number worse at its job.

### 4. One source of truth per finding: bend against gap

**Problem.** The arc bend judges the intensity a slide declares against the arc's curve, but tells the user to change the words ("calm its words, or move it nearer the climax"; `crates/scaena-ops/src/health.rs`, the `Kind::Arc` finding). The intensity gap judges the words against the declared intensity, and also tells the user to change the words. Where the two point opposite ways, one slide is told both "calm its words" and "sharper verbs".

**Measured:** slides that got both, pointing opposite ways:
- keynote slides 2, 3, 6 and 7;
- launch slides 7 and 8;
- none on the other four decks.

**Proposal:** the bend judges the plan, so it asks for the plan or the slide's place to change, never the words. The gap asks for the words. Only the findings' words change; no figure moves.

**Recommendation:** adopt it. It moves no figure, so it could go in without this ADR. It is here because it changes what Vzestup's findings say.

### 5. Weigh a slide's words by its script

**Problem.** Balance counts a slide as one that shows something only past 30 UTF-16 units of body (`mod.rs:428`). Japanese says as much in fewer characters. The pass measured the keynote's balance at 70 in Japanese against 83 in English.

**Not measured here:** `wip/dogfood-decks` has no Japanese deck.

**Proposal:** count a CJK character as two units, or measure words by `text::cuts` rather than code units.

**Recommendation:** adopt it with a Japanese deck in the tests, the keynote translated, measured before and after.

### 7. Leave the framing out of balance

**Problem.** Balance averages the points a slide makes over every slide whose layout makes several. The cover, a divider, and a closing slide list none, yet each counts, so a deck's average sits lower than its content slides make it.

On W1's dogfood-4 fleet pitch (`tests/fixtures/dogfood/fleet.slides.json`, 2026-11-10), the slides balance counts list 2.0 points on average, right at the floor. So health's own dense ask, "tighten slide 9 to five points", costs the deck 4 points of score: it takes the average to 1.8, under the floor of 2. A deck is marked down for following its health's advice. #935 names that cost in the ask, but cannot remove it.

**Proposal:** leave the cover, every divider, and the deck's last run of closing slides out of the points made on average, as item 3 leaves out one-idea layouts. These are the cover (the deck's first slide in `title`, `title-center` or `title-split`), the dividers (`section`, `section-photo`, `section-number`), and the closing run (the closing run the calm close of item 6 reads). They are marked `one_idea`, so a deck without them reads as before.

**Measured:** Scaena's score as `scaena_ops::health` gives it (brief pacing, Vzestup's balance, arc fidelity, no lint cap), on the dogfood decks built in Dusk as the health tests build them. "(c)" applies the floor per slide instead: each slide's points are scored by Vzestup's formula, then averaged.

Fleet pitch, score / balance / points on average:

| | Vzestup | (b) framing out | (c) floor per slide |
|---|---|---|---|
| as written | 90 / 87 / 2.0 | 90 / 87 / 4.0 | 84 / 71 / 2.0 |
| slide 9 cut to five | 86 / 75 / 1.8 | 90 / 87 / 3.5 | 86 / 75 / 1.8 |

(b) on the nine dogfood decks, score / balance:

| deck | before | after | left out |
|---|---|---|---|
| deepdive | 93 / 100 | 93 / 100 | 1, 13 |
| board-de | 90 / 93 | 90 / 93 | 1, 10–11 |
| appeal | 93 / 91 | 93 / 91 | 1, 11 |
| training | 93 / 100 | 93 / 100 | 1, 9, 12 |
| pitch | 90 / 87 | 94 / 98 | 1, 7, 15 |
| QBR | 95 / 97 | 95 / 97 | 1, 12–13 |
| lecture | 94 / 100 | 94 / 100 | 1, 11, 15 |
| keynote | 94 / 97 | 94 / 97 | 1, 13 |
| launch | 91 / 91 | 91 / 91 | 1, 13–15 |

Only the pitch moves. Its average sat under the floor (1.8) because of its framing, and rises to 2.4. The climate deck the tests generate moves from 84 to 89, its balance from 71 to 85.

(c) lowers every one of the nine by 2–6 points, since each one-line slide is scored thin on its own.

**Also measured, for Jay beside (b)** (W1's dogfood 5, the grant renewal `tests/fixtures/dogfood/grant.*` on branch `dogfood-5/grant-inputs`, a deck of big numbers and statements). Score / balance:

- (b′) leaves a slide that draws a chart or a table out of the points average too, since its points are its chart.
- (e) gives full marks for points when no counted slide is left, instead of the 30 an average of 0 scores.

| deck | Vzestup | (b) | (b′) alone | (b) + (b′) | (b) + (b′) + (e) |
|---|---|---|---|---|---|
| grant | 80 / 64 | 80 / 64 | 80 / 64 | 80 / 64 | 89 / 88 |
| fleet | 90 / 87 | 90 / 87 | 90 / 87 | 90 / 87 | 90 / 87 |
| fleet, slide 9 cut | 86 / 75 | 90 / 87 | 86 / 75 | 90 / 87 | 90 / 87 |
| pitch | 90 / 87 | 94 / 98 | 94 / 98 | 94 / 98 | 94 / 98 |
| QBR | 95 / 97 | 95 / 97 | 95 / 97 | 89 / 80 | 89 / 80 |
| climate (generated) | 84 / 71 | 89 / 85 | 84 / 73 | 89 / 85 | 89 / 85 |
| deepdive, board-de, appeal, training, lecture, keynote, launch | as before | as before | as before | as before | as before |

The grant reads "Slides make 0 points each" under every rule but (e).

- **The grant:** every content slide is a one-idea layout. The only slides counted are its cover, its chart, and its close, none of which lists points. Their average is 0, and an average of 0 scores 30. Leaving those slides out empties the set but changes nothing, until (e) scores an empty set in full.
- **The QBR:** (b′) on top of (b) leaves only its dense lists, which average 6.2 points, past five. It loses 6.

**Recommendation on these:** leave (b′) aside. It moves only decks whose framing (b) already covers, and it marks the QBR down. (e) is the one that reaches a deck made of one-idea slides, and it moves no other deck measured. It could be adopted with (b), or put to Jay as its own item.

**Recommendation:** adopt (b). It makes following the dense ask cost nothing where the content holds its points, it moves no deck whose content slides already list enough, and it lifts the one deck its framing held under the floor. Leave (c) aside. #935's dense ask still names the cost where the content slides themselves are thin, at the floor.

## Decision

Jay, 2026-10-25: "your recommendation". It is the recommendation above, with what dogfood pass 7 added (`wip/dogfood-decks` 93ce8d04, `dogfood7/findings.md`, item 8):

- **1, adopted.** Lint now sees more of what the first pass saw: text over text, judged by its glyphs' ink (E101); a place a layout leaves empty (W304); a big number's tail over the words under it (I405). The builder also no longer draws a figure its writer didn't write (DF7-1).
- **2, adopted in its narrow form, for intensity as for tone.** A gap finding is raised only where the slide's words read something in the lexicon. Where they read nothing, health says nothing of them. No figure moves. Pass 7 found plainly hopeful English read as neutral (appeal 8 and 9), and German read as neutral where the lexicons read no German (board-de 6). A language the lexicons don't read has no intensity or tone score, as #532 says.
- **3, adopted with 1.** A one-idea layout's points are left out of the average. A slide that draws a picture, a chart, or a table counts as showing something. With the cap and the lint above, scores fall where slides are broken and stop falling where a slide is one idea by design.
- **4, adopted** (#609).
- **5, adopted.** It is measured on the Japanese keynote (`keynote-ja.json`) before and after.
- **6, added: a calm close is no bend.** In pass 7, all four decks got "planned cooler than its place in the talk" on their last two or three slides, the closing at 0.3 against a curve at 0.90–0.96. A calm "Questions?" or "Vielen Dank" is right. The bend finding is not raised on the deck's closing slides, its last run of slides in a closing layout. No figure moves.
- **7, pending Jay.** Held in its own pull request until he decides; nothing lands before.

`a_generated_talks_figures_are_vzestups` is re-pinned in the commit that moves the figures, naming this ADR.

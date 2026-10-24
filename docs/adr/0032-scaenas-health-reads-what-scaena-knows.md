# ADR-0032: Scaena's health reads what Scaena knows

**Status:** proposed · **Date:** 2026-10-24 · **Decides:** Jay

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

## Decision

Jay's to make:
- **Adopt** 4 (moves no figure).
- **Adopt** 1 (inert until lint sees more).
- **Adopt** 5, with its test deck.
- **Hold** 2 and 3. Tone from the visible words gains little over the lexicon's silence. One-idea balance raises scores that are already too high; it should come only together with a cap that lint can feed.

Each adopted item is a small follow-up behind #532's tests. The pinned climate figures (`a_generated_talks_figures_are_vzestups`) are re-pinned in the same commit that moves them, with this ADR named in it.

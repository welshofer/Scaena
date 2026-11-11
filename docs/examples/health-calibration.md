# Health against a reader: a calibration

This checks whether narrative health (SPEC §7.7) judges whole decks the way a careful reader does. Health has been tuned one finding at a time; this report looks at whole decks instead. It is the decision input for the next round of health work, and it changes no scoring.

## Method

- **The decks.** The eleven dogfood decks (`tests/fixtures/dogfood/*.slides.json`), plus two made decks from Narrato's recorded answers: `climate` (25 slides, `crates/scaena-narrato/tests/answers/climate.json`) and `cargo`, a five-slide Series A pitch (`anthropic-slides.json`).
- **The reading.** I read each deck from its slides and notes alone, as a talk coach would, and ranked them. The ranking asks whether the story lands, whether the arc builds, whether the turns carry the room, and whether the close lands.
- **The comparison.** Only then did I run `health_in` on each deck, built as the health tests build them: Dusk, and the dogfood helper's brief.
- **Caveats.**
  - The reading wasn't fully blind. I had seen some of health's findings on these decks while working on them; I kept the ranking to what the slides and notes say.
  - The dogfood helper gives every deck the same frame: its brief asks 11–15 slides, its outline has empty claims, it names no language, and its arc falls back to Hero's Journey. Findings that come from that frame, not from the deck, are marked *(fixture)* below. They are not counted against health.

## The reader's ranking

| # | deck | strongest | weakest |
| --- | --- | --- | --- |
| 1 | keynote | One town's story, told in order, with a real low point: the ice storm and the jar on the counter. It ends on a thesis, four steps and a call the room can act on. | Slide 8's "31 hours of outages a year" sits beside slide 9's "286 to 2". One is likely a usual year and the other the storm year, but nothing on the slides reconciles them. |
| 2 | appeal | One child carries a county-wide number. The ask is concrete at every size ($85 to $180,000), and the close returns to Maya. | Slide 9's three testimonials come after the resolution and slow the run into the ask. |
| 3 | pitch | Every claim has its figure and its source, and it moves through problem, cost, failed fixes, product, proof, team and ask. | The market funnel (slide 12) comes after the proof and delays the ask, and slide 7 is a header with nothing on it. |
| 4 | deepdive | A clean engineering story: symptom, cause found in a trace, a one-loop fix, results and lessons. | The decision brief and the on-call quote interrupt the results, then the deck ends on a bare "Questions?". |
| 5 | qbr | Leads with the answer, shows it, explains the miss, and ends in three owned, dated decisions. | Little narrative pull; slides 4–7 are four metrics in a row, which the agenda frames well enough. |
| 6 | launch | A strong problem → reveal → demo → proof arc. | After the proof, four slides of logistics (pricing, FAQ, steps, close) let the talk run down. |
| 7 | lecture | Opens on a puzzle (21% of doctors) and builds the tool to solve it, with practice and transfer. | It never returns to the doctors' question to close the loop it opened. |
| 8 | board-de | A tidy board update: the numbers, the margin story told in the CFO's words, the risk, the decisions. | It states the decision it needs only at slide 10, and closes on a bare "Vielen Dank". |
| 9 | climate | Broad and well sourced, and the turn to "what is working" is earned. | Twenty-five slides of facts with no person and no story. Five section headers carry nothing, and Questions comes before the close. |
| 10 | training | Practical and concrete: the vague-versus-owned action item lands. | Slides 4 and 5 say the same thing twice. "Three traps" (slide 8) lists two. No real incident anchors it. |
| 11 | grant | The ask is clear and the numbers are honest, cost overrun included. | It repeats itself: slides 2, 3 and 4 all say 40 or 52 miles. The notes read every slide aloud. |
| 12 | fleet | The before and after of planning time is a real result. | The notes read every slide aloud, slides 6 and 7 say the same thing, there is no customer, and it closes on "Let's talk about next steps". |
| 13 | cargo | The ask is clear. | Five slides with notes that are stage directions ("We frame the problem"): a skeleton, not a talk. |

## What health says

| deck | reader's rank | health score | health's rank |
| --- | --- | --- | --- |
| qbr | 5 | 95 | 1 |
| keynote | 1 | 94 | 2–3 |
| lecture | 7 | 94 | 2–3 |
| appeal | 2 | 93 | 4–6 |
| deepdive | 4 | 93 | 4–6 |
| training | 10 | 93 | 4–6 |
| launch | 6 | 91 | 7 |
| board-de | 8 | 90 | 8–10 |
| fleet | 12 | 90 | 8–10 |
| pitch | 3 | 90 | 8–10 |
| climate | 9 | 84 | 11 |
| grant | 11 | 78 | 12 |
| cargo | 13 | 68 | 13 |

**The scores are compressed.** Nine of the thirteen decks score between 90 and 95. That band holds the deck I ranked best (keynote, 94), one of the two weakest dogfood decks (fleet, 90), and the third best (pitch, 90). Only the bottom of the order, cargo and grant, agrees with a reader.

### Where the ranks disagree by two places or more

- **pitch (3 → 8–10).** Health docks it on two things:
  - balance at 87: "1.8 points each", over the slides balance counts as lists;
  - tone at 72: slides 8, 10 and 11 read "graver than written", because the words of a fix are words of trouble ("error", "dispute", "overbilled").

  Neither touches the story. The real weakness, the market slide arriving after the proof, has no finding.
- **lecture (7 → 2–3) and training (10 → 4–6).** Both have clean balance and few gaps, and health has no way to see what holds them down:
  - the lecture's hook is never resolved;
  - training says the same thing on two slides, and "three traps" lists two.
- **qbr (5 → 1).** Health rewards its tidy measures. That's defensible, but a reader ranks the keynote, appeal and pitch above it.
- **fleet (12 → 8–10).** The worst tell, notes that read every slide aloud, is only an info, and health's score doesn't read it. Fleet ties with pitch at 90.
- **appeal (2 → 4–6).** Four intensity warnings. The lexicon reads the big-number and statement slides ("212 families", "1 in 7") as flat, because their words are few and plain. In an appeal, those slides hit hardest.
- **climate (9 → 11).** Ten turn warnings among data slides that share no words, though each section header frames its run.

### What a reader found that health has no finding for

1. **Figures that disagree across slides.** Keynote: 31 hours a year (slide 8) against 286 to 2 (slide 9).
2. **A hook never resolved.** The lecture opens on the doctors' 21% and never comes back to it.
3. **A count the slide doesn't keep.** Training's "Three traps" lists two.
4. **A long tail after the peak.** Launch has four logistics slides after its proof. Health reads the arc from the plan, not the slides.
5. **Two slides that say one thing in different words.** Training slides 4 and 5 share no headline words, so `story` misses them.

### Findings I'd call false or unhelpful

- **Pacing "the talk runs about N minutes, short of …" on twelve of thirteen decks.** These decks give their beats no time, so the talk is timed from terse notes alone. That reads every deck as a third of its length. A reader would not say this. *(Partly fixture: no `duration`; but a made deck's notes are as terse.)*
- **Labels that are claims.** Grant's "We rebuilt 52 miles, not 40" and "6,820 volunteer hours rebuilt it". `labels()` misses irregular pasts ("rebuilt", "grew", "fell", "won", "cut") unless they're in `VERBS`.
- **Board-de read as English.** The fixture names no language, so health reads the German deck with English lexicons. "Die Marge ist kein Zufall" is a label, and slide 3 "reads hotter than written". *(fixture)* The same thing would happen to a real deck whose brief and `lang` are both empty.
- **Turn warnings among a beat's metric slides.** qbr has 4 and climate 10. Each slide is a different figure under a header or agenda that names them, and a reader feels no lost turn.
- **Intensity on title slides.** Appeal, keynote and pitch slide 1 "read hotter than written". The few words of a cover weigh "you" and a question mark heavily, as the thank-you close did (#965).
- **Tone on proof slides.** Pitch's and launch's proof slides "read graver": the words of the problem a product solves are grave words.
- **Arc bends judged against Hero's Journey on a QBR, a lecture and a pitch.** *(fixture: the arc's fallback)*
- **Link warnings on the deck's own domain.** relay.app, manifest.dev and portalderpower.org are flagged. *(fixture: the brief's topic is the fixture's name, not the product's)*

## The five changes that would move health closest to the reader

Ranked by how much of the gap each closes.

1. **Make the notes that read the slide aloud a warning, and let it open in the Story.**
   - *Coaching only* (severity and triage, no figure moves; the coordinator's to approve).
   - It is the clearest tell of the two weakest dogfood decks (fleet, grant), and today it is a folded info.
   - Counting it in balance's notes score would be *score-moving* (ADR-0032, Jay's). I'd propose that separately, with the figures before and after.
2. **Time a talk from its notes only where its beats are given time, or say the reading is a floor.**
   - *Coaching only.*
   - Twelve of thirteen decks are "short of their slot" because the talk is timed from terse notes. Two options:
     - find a short talk only where some beat has a `duration` or a hold;
     - word it "the notes alone run about N minutes", with no short-of-slot warning.
   - #997's brief minutes stay as they are: a brief that asks minutes is a plan.
3. **Read a beat's run of figure slides as one move, not as turns.**
   - *Coaching only.*
   - Within one beat, a pair of slides that are both a figure or chart layout (big number, scorecard, line, bar) and share no word becomes an info, not a warning.
   - That clears qbr's four and most of climate's ten without hiding a real turn between beats.
4. **Finish `labels()`'s verbs and read a deck's language when nothing names it.**
   - Two parts:
     - (a) *Coaching only:* add the common irregular pasts ("rebuilt", "grew", "fell", "won", "cut", "led", "built", "beat") so a claim in the past is not a label.
     - (b) *Score-moving* (ADR-0032, Jay's): where neither the brief nor `lang` names a language, guess it from the deck's words, by a stop-word share, before intensity and tone read English lexicons. That would give a German deck no intensity or tone number, rather than an English reading.
5. **Find what a reader finds first: numbers that disagree, and a count a slide doesn't keep.**
   - *Coaching only*, two new warnings:
     - a slide whose headline names a count ("Three traps", "Four steps") with a different number of items;
     - the same measure ("outage hours", "a year") given two figures on two slides.
   - Both are mechanical, both move no figure, and both hit the decks health ranks too high (training, keynote).

### Change 5's second warning, tried and dropped

The warning for one measure given two figures was built as a prototype and run on the eleven dogfood decks before any code landed. It matched a unit and the words after it, leaving out a chart's own series and figures beside "from", "to", "fell", "before" or "after". Every pair it raised was false:

- grant: the 40-mile goal against the 52 miles rebuilt, a goal and an outcome;
- deepdive: a rollout's 1%, 10% and 50% steps;
- lecture: two worked scenarios, a 9% and a 1% false-positive rate;
- fleet: 6.5 hours before against 0.8 hours after, with no change word on the slide;
- qbr: retention 114% against churn 6%, where "%" is the only unit;
- board-de: +9% against +18% on different lines.

The one case a reader found, keynote's 31 outage hours a year against 286, is ruled out by those same exclusions: it sits on a Before/After slide, beside "fell from 286 to 2". Matching words can't tell a measure said twice from a goal, a step, a scenario or a before and after. That takes reading the sentence. A warning made only of false findings would teach people to ignore health, so it is dropped, and no figure moves.

Figures that disagree belong with the model. The assistant's "Ask about this deck" is the place for "do any of this deck's figures disagree?", which a model reads in context. That touches the assistant's prompt and its fence, so it will be routed as an item of its own.

Behind these five, the **score compression** (9 of 13 decks within 90–95) is the larger question. It is *score-moving* and Jay's: health's score weighs pacing, balance and fidelity, and none of the three reads repetition, notes read aloud or an unresolved hook. Changes 1 and 5 would give it findings to weigh, should Jay want it to.

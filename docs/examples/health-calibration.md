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

## Score compression: candidate fixes, measured

This section is the evidence for Jay's decision on the compression above. Nothing in it is on main. Each candidate was prototyped against the same 13 decks, built as in the method above, on a scratch branch that is not pushed. All figures come from health as it stands on main after #1022: pacing finds no short talk where no beat is timed. The coaching changes still in review, #1027 (a part's run of figures) and #1028 (no turn out of a divider), move turn findings, so the two candidates that read turns are given both ways.

A correction to the count above: on these 13 decks, ten score 90–95 inclusive, not nine. Board-de, fleet and pitch all sit at 90.

**What the score is today.** In a deck read by its words, health's score is Vzestup's fallback composite of three measures:
- pacing (weight 0.20);
- balance (0.25): bullets, notes length, and slides that show something, a third each;
- arc fidelity (0.25).

Coherence carries no score when read by words. Intensity and tone are measured but not weighed. The cap applies only where lint finds a slide that does not stand. None of the 13 decks has one, so the cap's slope moves nothing here.

That is why the score is compressed. Twelve of the 13 decks pace at 90–100, ten balance at 87–100, and all 13 show fidelity at 79–90. What a reader marks down sits in measures and findings the score does not weigh: notes read aloud, a slide said twice, an unresolved hook, turns that lose the room.

### The candidates

| | candidate | what it changes | ADR-0032 |
|---|---|---|---|
| C1 | coherence from turns | words-read coherence scored 100 − 15 per turn warning − 5 per turn info, weighed 0.30 as Vzestup weighs embedding coherence | a new item: Vzestup gives coherence by words no score |
| C2 | pacing and balance by their warnings | pacing −10 per pacing warning; balance −5 per warning of its own kinds (`dense`, `thin`, `notes`, `story`) | amends item 3 (what balance counts) |
| C3 | the cap's slope | ten points a broken slide, made steeper or shallower | amends item 1; **no effect here**, no deck capped |
| C4 | notes read aloud counted in balance | a slide whose notes read it aloud counts as having none in balance's notes third | amends item 3 |
| C5 | each warning costs points *(mine)* | the score less 1.5 per warning of any kind | a new item |
| C6 | intensity and tone weighed in *(mine)* | each at 0.15 beside pacing, balance and fidelity | a new item, departing from Vzestup's composite |
| C7 | C4 and C5 together *(mine)* | | items 3 and new |
| C8 | C2 and C4 together | | item 3 |

### The 13 scores, before and after

Decks are listed in the reader's order, best first.

| deck | base | C1 | C2 | C3 | C4 | C5 | C6 | C7 | C8 |
|---|---|---|---|---|---|---|---|---|---|
| keynote | 94 | 94 | 94 | 94 | 94 | 85 | 88 | 85 | 94 |
| appeal | 93 | 88 | 93 | 93 | 93 | 86 | 87 | 86 | 93 |
| pitch | 90 | 81 | 88 | 90 | 90 | 75 | 86 | 75 | 88 |
| deepdive | 93 | 88 | 93 | 93 | 93 | 88 | 90 | 89 | 93 |
| qbr | 95 | 77 | 95 | 95 | 95 | 88 | 92 | 87 | 95 |
| launch | 91 | 88 | 91 | 91 | 91 | 80 | 87 | 81 | 91 |
| lecture | 94 | 88 | 94 | 94 | 94 | 91 | 91 | 91 | 94 |
| board-de | 90 | 87 | 90 | 90 | 90 | 87 | 87 | 87 | 90 |
| climate | 84 | 58 | 84 | 84 | 84 | 63 | 83 | 63 | 84 |
| training | 93 | 95 | 91 | 93 | 93 | 90 | 91 | 90 | 91 |
| grant | 78 | 77 | 72 | 78 | 70 | 72 | 78 | 64 | 65 |
| fleet | 90 | 81 | 85 | 90 | 80 | 84 | 86 | 74 | 75 |
| cargo | 68 | 73 | 68 | 68 | 68 | 65 | 71 | 65 | 68 |

| | ρ against the reader | decks at 90–95 | spread | largest single-deck move |
|---|---|---|---|---|
| base | +0.65 | 10 | 27 | none |
| C1 | +0.47 | 2 | 37 | climate 84 → 58 |
| C1, after #1027 and #1028 | +0.55 | 4 | 37 | climate 84 → 58 |
| C2 | **+0.72** | 8 | 27 | grant 78 → 72 |
| C3 | +0.65 | 10 | 27 | none |
| C4 | **+0.70** | 9 | 27 | fleet 90 → 80 |
| C5 | +0.30 | 2 | 28 | climate 84 → 63 |
| C5, after #1027 and #1028 | +0.39 | 4 | 27 | climate 84 → 68 |
| C6 | +0.48 | 4 | 21 | keynote 94 → 88 |
| C7 | +0.37 | 2 | 28 | climate 84 → 63 |
| C8 | **+0.72** | 8 | 29 | fleet 90 → 75 |

ρ is Spearman's rank correlation of health's order with the reader's, tied scores taking their average rank.

### What the numbers say

- **Spreading the band and following the reader are different things.** C1, C5, C6 and C7 empty the 90–95 band, leaving two to four decks there, and each makes the ranking worse than today's +0.65.
  - They punish turns and intensity and tone warnings. Those are most numerous on long data decks (climate, qbr) and on decks whose subject is trouble (pitch, keynote). A reader does not mark those down.
  - C6 drops keynote, the reader's best, six points for tone it reads "graver than written" on a story about an ice storm.
- **Only the balance candidates follow the reader.** C2 and C4 move only the decks with the tells a reader names first:
  - fleet and grant, whose notes read every slide aloud;
  - training, whose headline counts three and lists two;
  - pitch, whose notes read one slide aloud, which costs it two points.

  No other deck in the reader's top eight moves. C8, the two together, moves fleet from 90 to 75 and grant from 78 to 65, below climate and next to cargo, where the reader puts them.
- **C2's pacing half does nothing now.** #1022 left no pacing warning on these decks, so C2 is its balance half alone.
- **No candidate lifts pitch** (reader 3, health 8–10). What holds it down is tone reading a fix's words as trouble, and balance's 1.8 points a slide. Neither is a story fault, and no change to the composite reaches that.
- **The cap is not where the compression is.** No deck here has a slide lint finds broken.

### Recommendation

Take **C4**, and **C2's balance half** with it (C8): count a slide whose notes read it aloud as having no notes in balance, and take five points off balance for each warning of balance's own kinds (dense, thin, notes, and a slide said twice or a count not kept).

- It raises ρ from +0.65 to +0.72.
- In the reader's top eight it moves only pitch, two points, for its one read-aloud slide.
- It drops fleet and grant fifteen and thirteen points, to about where a reader puts them.
- It amends one ADR-0032 item, item 3 (what balance counts). It keeps Vzestup's weights and composite, and moves no deck without a warning that names why.

It does not end the compression at the top. Eight decks still score 90–95. That top band is right to be close, because those decks are close. What separates them for a reader (an unresolved hook, a slide in the wrong place, a long tail after the peak) is not something health reads, and no weighting of what it does read will find it.

If Jay wants a wider top band too, the evidence here argues against getting it from turn or tone counts. The way to get it is a measure of its own: the open-loop and long-tail readings. That would be new work, not a change of weights.

## What separates the top band: three detectors, tried

Those three things are what a reader separated the top decks by: an unresolved hook, a slide in the wrong place, and a long tail after the peak. No change of weights reaches them, so I tried each as a detector on the thirteen decks. A detector of this kind would give a coaching finding with an ask, and no figure would move.

I wrote the detectors as a script over the decks' slides and notes, outside the engine. Each was scored against the reader's notes above, which flag:
- **open loops:** lecture's hook never resolved, and training's "three traps" listing two;
- **slides out of place:** pitch's market funnel after the proof (slide 12), deepdive's decision brief and quote inside the results (slides 10–11), and climate's Questions before the close (slide 24);
- **long tails:** launch's pricing, FAQ and steps after its proof, and appeal's testimonials slowing the run into the ask (slide 9).

| detector | hits | misses | false alarms | verdict |
| --- | --- | --- | --- | --- |
| long tail | 2 | 0 | 0 | build, as an info |
| out of place, by words | – | – | 55 of 154 slides | no |
| out of place, Q&A before the close | 1 | 2 | 0 | no: it coaches against good practice |
| open loop, a hook's question that never returns | 1 | 0 | 1 | no |

### Long tail

**The rule.** Count the slides that stand between two points:
- the last slide planned within 0.1 of the deck's peak intensity;
- the closing run: Closing, The Ask, Next Steps, Takeaways, Poster, a Q&A, or a thanks.

The finding fires where those slides number three or more and are a fifth of the deck or more, and no section divider stands among them.

**What it finds.**
- Appeal: slides 7–9, 24% of its notes' words.
- Launch: slides 11–13, 21%.

**The rule's parts.**
- At two slides or more, it would also fire on grant's slides 7–8, which the reader does not flag.
- Without the divider rule, it would fire on climate's Response section (slides 16–22), which the reader called earned. A divider opens a new part of the talk, so what follows it is not a run-down.
- It reads the plan's intensity, which every made deck carries. A deck with no plan gets no finding.

The three thresholds (0.1, three slides, a fifth) and the divider rule were set on these thirteen decks, so the finding is an info, not a warning.

### Out of place

**By words.** A slide's words were compared with its own beat's slides and with other beats'. This does not work:
- it fires on 55 of 154 slides at a ratio of 1.5, and on 2 at 3.0, both fleet's and neither a reader's flag;
- the fixtures' beats are the writers' own labels, and what the reader flagged is the order of the argument (a market slide after the proof), which no beat's words show;
- health's drift already finds deepdive's slide 11, which stands in a beat after its own.

**A Questions slide that is not the last.** It finds climate's slide 24 and nothing else. It is not built, because standard speaking advice is the reverse: take questions, then close again yourself, so the talk ends on its own words and not on the last question. Climate's slide 25 is that close, so a finding there would coach against the advice. What the reader's climate note means is that twenty-five slides of facts end with no person and no story, and the Questions slide is only where that shows.

### Open loop

**A hook's question whose distinctive words never come back in the close.** At best this gives one hit and one false alarm, and only by asking for two words to come back:
- lecture's slide 3 fires, rightly: "gynecologists", "mammogram", "21%" and "breast cancer" never return, only "answer";
- launch's "What if your team never wrote a first reply again?" fires too, though its reveal at slide 7 answers it. The words it answers with ("reply", "first") run through the whole deck, so they are not distinctive.

Words cannot tell an unanswered puzzle from an answered one, so it is not built.

**Promises in the notes ("by the end", "at the end").** These are in keynote's, lecture's and training's notes, and all three are kept, so there is nothing to find.

**The count case.** Training's "three traps" listing two is found already (#1008).

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

## Dogfood 17: why the Story's fixes barely move the score

W1's dogfood 17 made a deck with From a prompt… and worked its Story down as a person would (`docs/examples/dogfood-17/README.md`). Nine fixes took the score from 84 to 86. This section says why, and measures the one score change the reading suggested against the reader's ranking above. The prototype is not on main.

### How the score is made

In a deck read by its words, as this one is, the score is Vzestup's fallback composite: (0.20 · pacing + 0.25 · balance + 0.25 · fidelity) / 0.70. Coherence, intensity and tone are measured but not weighed, and no coaching finding is.

| | pacing | balance | fidelity | score |
|---|---|---|---|---|
| as made | 100 | 84 | 70 | 83.6, shown as 84 |
| as worked | 100 | 84 | 77 | 86.1, shown as 86 |

### The nine fixes, by measure

| finding (slide) | measure | score | why |
|---|---|---|---|
| `label` (3), `label` (4) | none | 0 | a headline's wording is coaching; balance counts points, not headlines |
| `claim` (3), `claim` (8) | none | 0 | the settled outline is Scaena's; Vzestup's measures never read it |
| `bigIdea` (8) | none | 0 | as `claim` |
| `turn` (6) | coherence | 0 | coherence read by words has no score |
| `link` (8) | none | 0 | a security warning, kept |
| `arc` (5), Plan it at 0.73 | fidelity | +1 | fidelity 70 → 73 |
| `arc` (7), Plan it at 0.75 | fidelity | +1 | fidelity 73 → 77 |

Seven of the nine are findings no measure reads, by design. The two that could move the number moved it little. A DTW written beside health's, which reproduces 70 and 77 exactly, says why:

| slide | curve | planned, as worked | gap |
|---|---|---|---|
| 1 | 0.25 | 0.50 | +0.25 |
| 4 | 0.69 | 0.50 | −0.19 |
| 5 | 0.85 | 0.73 | −0.12 |
| 6 | 0.70 | 0.50 | −0.20 |
| 7 | 0.95 | 0.75 | −0.20 |
| 8, the close | 0.98 | 0.50 | −0.48 |

- **The fixes stop short of the curve.** A plan is held within 0.25 of what the slide's words read (ADR-0032 item 4), and these words read about 0.49.
- **The close is the largest cost left, and nothing asks for it.** Item 6 finds no bend in a calm close, but the score's DTW still charges one. Planned on its curve, slide 8 alone would take fidelity from 77 to 88, and the score from 86 to 90.
- **Gaps under a finding's threshold cost too.** Slides 1, 4 and 6 stand 0.19–0.25 from the curve, and no finding names them.
- **A fix could lower the score.** The warp aligns the slides afresh with each plan.
  - On B1, taking each fix in turn read 72 → 74 → 73 → 74 → 74 → 74. Slide 12's plan, 0.70, stood under the tent's 0.73 at its place.
  - On this deck, slide 7's fix taken first and slide 5's after took the score from 87 to 86.
  - #1252 offers a plan only where it raises fidelity as the deck stands. No figure moves.
- **Balance, 84, has no finding naming it.** `thin` passes over this deck's chart and one-idea slides.

### A: the score reads a calm close as item 6 does

The candidate makes item 6 one rule for the finding and the score: a slide in the closing run planned cooler than the curve costs nothing in the DTW, and one planned hotter still costs.

| deck, in the reader's order | base | A |
|---|---|---|
| keynote | 94 | 94 |
| appeal | 93 | 93 |
| pitch | 90 | 90 |
| deepdive | 93 | 97 |
| qbr | 95 | 96 |
| launch | 91 | 92 |
| lecture | 94 | 95 |
| board-de | 90 | 93 |
| climate | 84 | 85 |
| training | 93 | 98 |
| grant | 78 | 78 |
| fleet | 90 | 91 |
| cargo | 69 | 70 |

| | ρ against the reader | decks at 90–95 | spread | largest single-deck move |
|---|---|---|---|---|
| base | +0.65 | 10 | 26 | none |
| A | +0.44 | 7, and 3 above 95 | 28 | training 93 → 98 |

ρ is Spearman's rank correlation of health's order with the reader's, tied scores taking their average rank, as above. A takes it down by 0.21. The decks that move it:

| deck | reader's rank | health's rank, base | with A | why it moves |
|---|---|---|---|---|
| training | 10 | 5 | 1 | its close is planned calm, so fidelity 81 → 93 |
| deepdive | 4 | 5 | 2 | its close, a bare "Questions?", is planned calm: fidelity 81 → 91 |
| board-de | 8 | 9 | 6.5 | its close, a bare "Vielen Dank", is planned calm: fidelity 79 → 86 |
| keynote | 1 | 2.5 | 5 | does not move, and is passed |
| lecture | 7 | 2.5 | 4 | rises a point, and is passed by deepdive and training |
| appeal | 2 | 5 | 6.5 | does not move, and is passed |
| pitch | 3 | 9 | 10 | does not move, and is passed by board-de |

Ranks are health's, ties sharing their average. Qbr (reader 5th) falls from health's first to third and launch (6th) from seventh to eighth, passed by the three that rise. Climate, grant, fleet and cargo hold their ranks.

The decks A lifts most each close calmly: deepdive on a bare Questions slide, board-de on a bare thank-you, and training on a close planned calm. The reader marks the first two down for exactly that. The score's charge on a calm close is part of what keeps them under the decks that end on their thesis.

The 13 decks are built as the method above builds them. Each base is the one the score-compression section gives, but cargo's, which main has since moved from 68 to 69. Outside the 13, dogfood 17 goes from 84 to 89 as made and from 86 to 90 as worked, and B1, which names no arc, stays at 72.

### What the numbers say

- **A makes the ranking worse.**
  - It rewards the decks that close calmly. Training, the reader's tenth, becomes the top deck at 98.
  - Deepdive (fourth) rises past all of the reader's top three, and board-de (eighth) past pitch (third).
  - Keynote, appeal and pitch, which a reader puts first, do not move.
- **The band empties the wrong way.** 90–95 holds seven decks instead of ten, but only because three rise past 95.
- **The charge on a calm close is doing work.** The score's charge on a close planned under the curve is inconsistent with item 6's finding. On this evidence, though, it keeps the score nearer the reader.

### Recommendation

- **A is not recommended.** ρ falls from +0.65 to +0.44.
- **The alternative is to say it in words.** If the inconsistency between item 6's finding and the score matters, close it in words: health can say that the score still reads the close against the arc's curve, though no finding asks for its plan. No figure moves.

### The question A was answering, which stays Jay's

Should a fix the Story asks for move the score when no measure counts it? On this deck that is `label` (2), `claim` (2), and `bigIdea` (1): five of the nine fixes. `turn` is counted by coherence, which has no score when read by words. The options, with what each would cost:

1. **No, as now.** The score is Vzestup's three measures, and coaching sits beside it.
   - The number stays still under five of these nine fixes, which is what W1's walk found ("Working the Story barely moves the score").
   - Nothing moves on any deck.
2. **Count them in a measure of their own.** The settled outline's findings (`claim`, `bigIdea`) and the headline's (`label`) would make an outline measure, weighed beside the three.
   - This departs from Vzestup's composite, a new ADR-0032 item.
   - It needs its own ρ read before it lands: these findings fire only on decks that keep a settled outline, and none of the calibration's 13 does. So the read needs decks made with their outline shown.
3. **Count them as warnings that cost points.** For example, the score less a point or two per warning of these kinds.
   - The calibration's C5 did this for every warning and took ρ to +0.30.
   - Narrowed to these kinds it has not been measured. Like option 2, it would read nothing on the 13 decks, which keep no outline.
4. **Leave the score and say it on the row.** Each Story row would say whether its fix moves the score.
   - No figure moves.
   - A row that calls a story fix "not counted" could teach people to skip the findings that make the talk better. That is why the rows say nothing of it yet.

On the 13 decks above, options 2 and 3 move nothing: none keeps a settled outline, and none has a `label`, `claim` or `bigIdea` warning. Their label findings are all infos. So ρ there stays +0.65 under either option. W2's outlined calibration (`docs/examples/health-calibration-outlined/`, #1256) gives six decks that keep their outline, ranked by a reader before any score was read. Both options are read on them below.

### Options 2 and 3, read on decks that keep their outline

The six decks are built as that calibration's README builds them, and read by health on main. Their base scores are the ones that calibration gives. Each option is read twice: with health's detectors on main, and with two fixes to them that are in review.
- **#1257:** a word or two, and a how or what clause, read as a topic headline, and an imperative read as a point.
- **#1259:** a part's claim is not counted as carried by a name every part shares.

With the second set, health finds 12 of the 16 topic headlines the reader counts, with none false. On main it finds 5, with one false.

**The options as measured.** The costs and weights are mine, chosen before reading the results and not fitted to them.
- **Option 3, a cost per finding.** The score without the cap, less 1, 2 or 3 points for each `label`, `claim` or `bigIdea` warning, then capped again where lint finds a slide broken.
  - Most of health's topic headlines are infos, not warnings, so a variant counts `label` findings of either severity.
- **Option 2, an outline measure.** It is weighed 0.25 beside pacing (0.20), balance (0.25) and fidelity (0.25), and read only for a deck that keeps an outline. It is the mean of three shares:
  - the parts whose slides carry their claim: 1 less the `claim` warnings over the parts;
  - the big idea landed: 1, or 0 where `bigIdea` warns;
  - the content slides whose headline states its point: 1 less the `label` findings over the content slides.

| | main's detectors, as scored | main's, without the cap | with #1257 and #1259, as scored | with them, without the cap |
|---|---|---|---|---|
| base | −0.20 | +0.88 | −0.20 | +0.88 |
| 3: −1 per warning | −0.14 | +0.94 | −0.14 | +0.94 |
| 3: −1 per finding, labels of either severity | −0.14 | +0.94 | +0.06 | +1.00 |
| 3: −2 per warning | −0.03 | +0.93 | +0.20 | +0.93 |
| 3: −2 per finding, labels of either severity | −0.03 | +0.93 | +0.71 | +0.94 |
| 3: −3 per warning | +0.58 | +0.89 | +0.58 | +0.89 |
| 2: an outline measure | +0.35 | +0.93 | +0.71 | +0.99 |

ρ is Spearman's rank correlation with the reader's order, ties at their average rank. The six decks' scores under the two options that read closest, with both fixes and without the cap:

| deck | reader | base, no cap | 2, no cap | 3: −2 per finding, no cap |
|---|---|---|---|---|
| lanes | 1 | 95 | 96 | 95 |
| sourdough | 2 | 93 | 95 | 93 |
| vans | 3 | 93 | 89 | 83 |
| support | 4 | 89 | 89 | 85 |
| larder | 5 | 87 | 77 | 79 |
| warehouse | 6 | 89 | 76 | 73 |

### What the numbers say

- **The cap decides more than either option.** As scored, lanes and vans (the reader's first and third) stay at 81 and 80 under every option. Each is capped by a lint error the builder left, so no option gets past +0.71 with the cap.
- **Without the cap, both options follow the reader more closely than the base does.** ρ rises from +0.88 to +0.89–1.00. Both put larder and warehouse (fifth and sixth) under support (fourth), where the base ties warehouse with support.
- **They need the corrected detectors.** With main's, health finds two of vans' eight topic headlines and two of warehouse's eight, so a cost per label barely reaches them. With #1257, option 2 and option 3, counting labels of either severity, separate them. But option 3 then also takes vans, the reader's third, ten points under sourdough: vans' headlines are topics, and its story is the strongest of the six.
- **How far to trust it.**
  - Six decks are few. One swap of neighbours moves ρ by about 0.1.
  - W2 wrote each deck to sit at a point on the three axes these findings read, so the decks favour any option that reads them, by construction.
  - The 13 decks a reader ranked for other reasons can't test either option, because neither moves them.
  - Read +0.99 and +1.00 as "orders these six as the reader does", not as a measured fit.
- **Each option is a new ADR-0032 item.** Option 2 is a new measure in Vzestup's composite. Option 3 is a cost outside every measure. Neither is recommended here; the question stays Jay's.

### The same read, with no lint error left

#1258 fixed the two errors the builder left: lanes' statement overflowing its place, and vans' close faint on its backdrop. These are the two that capped lanes and vans. The six decks are rebuilt on main at 031605545, which also holds #1257 and #1259. All six build with no lint error, so no score is capped, and every number here is health's score as scored.

| deck | reader | base | 2: an outline measure | 3: −1 a warning | 3: −2 a warning | 3: −3 a warning | 3: −1 a finding | 3: −2 a finding | 3: −3 a finding |
|---|---|---|---|---|---|---|---|---|---|
| lanes | 1 | 95 | 96 | 95 | 95 | 95 | 95 | 95 | 95 |
| sourdough | 2 | 93 | 95 | 93 | 93 | 93 | 93 | 93 | 93 |
| vans | 3 | 93 | 89 | 89 | 85 | 81 | 88 | 83 | 78 |
| support | 4 | 89 | 89 | 87 | 85 | 83 | 87 | 85 | 83 |
| larder | 5 | 87 | 77 | 83 | 79 | 75 | 83 | 79 | 75 |
| warehouse | 6 | 88 | 75 | 85 | 82 | 79 | 80 | 72 | 64 |
| **ρ** | | **+0.93** | **+0.99** | **+0.94** | **+0.93** | **+0.89** | **+1.00** | **+0.94** | **+0.94** |

"A warning" counts `label`, `claim` and `bigIdea` warnings. "A finding" counts `label` findings of either severity, since most of health's topic headlines are infos. The outline measure is as defined above. The findings each option counts:

| deck | `label` warnings | `label` findings | `claim` | `bigIdea` |
|---|---|---|---|---|
| lanes, sourdough | 0 | 0 | 0 | 0 |
| vans | 4 | 5 | 0 | 0 |
| support | 0 | 0 | 2 | 0 |
| larder | 0 | 0 | 3 | 1 |
| warehouse | 2 | 7 | 0 | 1 |

### What the clean numbers say

- **The base already follows the reader at +0.93.** With the cap gone, the one pair it gets wrong is warehouse (88) over larder (87), the reader's sixth and fifth, by a point.
- **Every option puts that pair right, and most leave the rest as they were.**
  - A point a finding (+1.00) orders all six as the reader does.
  - The outline measure (+0.99) does too, but for vans and support, which it ties at 89.
  - A point a warning (+0.94) puts warehouse under support, but leaves larder two points under warehouse.
- **Heavier costs start to cost the reader's third.**
  - At −2 a warning, vans (3rd) ties support (4th) at 85.
  - From −2 a finding it falls under support, 83 to 85.
  - At −3 a warning it falls under support too, 81 to 83, and ρ drops to +0.89.
  - Vans' headlines are topics, but its story is the strongest of the six. Its claims and its close are carried, and only `label` costs it.
- **Each option moves ρ between −0.04 and +0.07 on six decks.** One swap of neighbours is about 0.1. The table can show which options hold the reader's order, but not that any option is better than the base by a measured margin.
- **The 13 calibration decks still can't test them.** None keeps an outline or has a warning of these kinds, so ρ there stays +0.65 under every option.
- **No recommendation.** The cap's noise is gone; the question stays Jay's.

# Health against a reader, on decks that keep their outline

This extends the calibration in `../health-calibration.md`. Its 13 decks keep no settled outline, so
three things health's Story asks for most can't be measured against a reader there:

- topic headlines;
- claims the slides don't carry;
- a big idea the close never lands.

These six decks each keep the outline they were written to (`meta.narrative.outline` once
built). They vary on those three things on purpose. Health's code is not touched; R2 measures
the options on these decks afterwards.

## The decks

Each is an outline (`NAME.outline.json`: the arc, the big idea, and a claim for each beat) and the
slides written to it (`NAME.slides.json`, the JSON Narrato's slides are written in). I wrote both,
some as a careful author would and some as a careless one.

| deck | what it is | headlines | the slides and their part's claim | the close |
| --- | --- | --- | --- | --- |
| lanes | A neighbor asks a city committee to fund a protected-lane pilot (What Is → What Could Be, 11 slides) | claims | carry them | lands the big idea |
| sourdough | A Saturday class on why bread rises (Learning Journey, 10) | claims | carry them | lands the big idea |
| support | Support's Q3 update to its VP (Data Storytelling, 10) | claims | drift: the middle reports the office move, hiring, training, morale, and on-call, never the bot or the wait times | asserts the big idea the slides never showed |
| warehouse | A dock-scanning project update for shift leads (Roadmap / Future-State, 10) | topics | carry them thinly, as bare lists; the notes read each slide aloud | bare: "Thank You" |
| larder | A seed pitch for a meal-planning app (Problem–Solution, 10) | claims | drift: features, an award, and the team where the outline promises waste and retention figures | bare: "Questions?" |
| vans | A benefit-dinner talk for a school dental van (Case Study, 10) | topics | carry them, one child's story running through the notes | lands the big idea |

Each is built with no model called, the brief given as below:

```
scaena generate lanes "A protected-lane pilot on Harmon Avenue, for the city council's transportation committee" \
  --slides lanes.slides.json --from-outline lanes.outline.json --no-look --no-notes --pictures none \
  --audience "the city council's transportation committee" --goal Persuade --minutes 12 --persona government
scaena generate sourdough "Why bread rises: a Saturday class for home bakers" \
  --slides sourdough.slides.json --from-outline sourdough.outline.json --no-look --no-notes --pictures none \
  --audience "home bakers" --goal Educate --minutes 10 --persona students-educators
scaena generate support "Support's Q3 update: waits halved after the triage bot" \
  --slides support.slides.json --from-outline support.outline.json --no-look --no-notes --pictures none \
  --audience "the VP of Customer Experience and her staff" --goal "Update Status" --minutes 10 --persona executives
scaena generate warehouse "Dock scanning project update for shift leads" \
  --slides warehouse.slides.json --from-outline warehouse.outline.json --no-look --no-notes --pictures none \
  --audience "warehouse shift leads" --goal Inform --minutes 15 --persona colleagues
scaena generate larder "Larder seed pitch: meal planning from what is in the fridge" \
  --slides larder.slides.json --from-outline larder.outline.json --no-look --no-notes --pictures none \
  --audience "seed investors" --goal Persuade --minutes 10 --persona investors
scaena generate vans "Ridley Smiles: a dental van for rural schools, at the spring benefit dinner" \
  --slides vans.slides.json --from-outline vans.outline.json --no-look --no-notes --pictures none \
  --audience "donors at the spring benefit dinner" --goal Inspire --minutes 12 --persona conference-public
```

## The reader's ranking

I read each deck's slides and notes in order, as a careful member of its audience would, before
building any of them or running health on any. This ranking was committed before the scores. The
record shows it: the commit that adds this section comes before the one that adds health's.

| # | deck | what placed it there |
| --- | --- | --- |
| 1 | lanes | One crossing, one vote. The danger is the city's own figure, then a person (the crossing guard). A nearby street proves the design. It is honest about parking and cost, and offers a small step that can be undone. The close returns to the guard and the child and lands the big idea. Weakest point: the forty families who'd let their kids ride are in the notes only. |
| 2 | sourdough | Every headline is a claim the slide carries. It opens on an experiment (one dough, two rise times), builds yeast, gluten, and warmth in order, practices the poke test, checks it with a quiz, and closes on the idea and a single change to try. It is a clear lesson rather than a moving one. |
| 3 | vans | The strongest story of the six: Jamie runs from the problem to the result, and the close lands the $250. But every headline is a topic ("The Problem", "Access", "Results"). A reader skimming the slides gets labels, and the claims live in the bullets and the speaker. |
| 4 | support | It opens on the right complaint and ends on the right ask. But the six slides between report the move, hiring, training, morale, and on-call. The wait times and the bot that the close credits never appear. So "the bot halved our waits" is asserted, not shown, and slide 8 waves away the hiring that slide 4 celebrated. The notes are thin. |
| 5 | larder | A real problem figure opens it. Then the deck becomes a feature tour (a scanner, fourteen diets), an award, and a team slide, where the outline promised waste and retention figures. The ask is there but unsupported, and it ends on a bare "Questions?". The notes are one line each. |
| 6 | warehouse | Every headline is a topic, and every note reads its slide aloud. There is no person and one number (four hours). The freezer risk the outline names is one bullet among four. It ends "Thank You. Any questions?". Its outline's claims are reachable from the bullets, but nothing on the slides argues them. |

**Caveat.** This reading is blind to health, but not to the authoring: I wrote each deck to sit at
a point on the three axes above, and the ranking follows those choices. The order of the middle
three (vans, support, larder) is a judgment between a topic-headline deck that tells a story and
two claim-headline decks that drift. A second reader might reasonably put vans second.

## What health says

Each deck was built as above, then read by `scaena health BUNDLE --json`. Health ran on main as of
`cef71d62`. One brief changed before the build, and the ranking above doesn't depend on it:
sourdough's brief asked 20 minutes, and a 20-minute brief asks 11–15 slides, which `generate`
refused for a 10-slide deck. It asks 10 minutes instead.

| deck | reader's rank | health score | health's rank | without the cap | rank without it |
| --- | --- | --- | --- | --- | --- |
| lanes | 1 | 81 (capped) | 5 | 95 | 1 |
| sourdough | 2 | 93 | 1 | 93 | 2–3 |
| vans | 3 | 80 (capped) | 6 | 93 | 2–3 |
| support | 4 | 89 | 2–3 | 89 | 4–5 |
| larder | 5 | 87 | 4 | 87 | 6 |
| warehouse | 6 | 89 | 2–3 | 89 | 4–5 |

ρ against the reader (Spearman, tied scores at their average rank) is **−0.20 as scored** and
**+0.88 without the cap**.

| deck | pacing | balance | fidelity | intensity | tone | claim | bigIdea | label |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| lanes | 95 | 97 | 92 | 83 | 63 | 0 | 0 | 1 |
| sourdough | 100 | 97 | 83 | 92 | 70 | 0 | 0 | 0 |
| vans | 100 | 93 | 86 | 80 | 70 | 0 | 0 | 2 |
| support | 100 | 82 | 88 | 91 | 70 | 2 | 0 | 0 |
| larder | 100 | 90 | 74 | 92 | 78 | 2 | 1 | 0 |
| warehouse | 100 | 97 | 73 | 88 | 71 | 0 | 1 | 2 |

The last three columns count warnings of the three kinds the Story asks for most. None of them
moves the score, which is the question this calibration was made to answer.

### The cap decides the order

Lanes and vans, the reader's first and third, are capped at 81 and 80 by one lint error each.
Both are errors the builder left, not the author:

- **lanes, slide 5.** The Statement layout sets "A twelve-year-old could ride to Lincoln Middle
  School on Harmon by June" in four lines that need 530 units of a 432-unit box (E100), and the
  build's fit pass didn't fix it.
- **vans, slide 10.** The closing slide stands on the theme's backdrop shader, as a made deck's
  close does, and its statement reads 2.77:1 against it, where display text needs 3:1 (E111).

A reader of the built deck would see an overflowing statement and a faint close, so the cap
isn't wrong to find them. But it puts the two strongest stories below the four weaker ones.
Without it, health follows the reader closely, warehouse aside. Both errors are for the builder
to look at, apart from this calibration.

### What the outline findings catch

- **Claims the slides don't carry.**
  - On **support**, health finds two: Data (slides 3–5) and Insight (6–7), the two parts that
    drift furthest.
  - On **larder**, it also finds two: Impact (slide 3) and Proof (7–8).
  - It misses larder's Existing Approaches and Solution. Their slides share a word with the
    claim ("recipe apps", "Larder"), but where the claim promises what the product does, the
    slides only tour features.
- **A big idea the close never lands.**
  - It finds both bare closes, larder's and warehouse's.
  - It doesn't find support's close unearned. Support's close does say its big idea; the slides
    before it never showed it. Health reads the close's words, not whether the deck earned them.
- **Topic headlines.** It finds 5 warnings, and a reader counts 16 topic headlines on content
  slides:
  - **vans:** 2 of 8 (Access, Our Program). It misses The Problem, How It Works, Results,
    Jamie, What We Learned and The Ask.
  - **warehouse:** 2 of 8 (Background, Training). It misses Agenda, Goals, Project Overview,
    Timeline, Risks and Metrics.
  - The missed ones are one or two words, set on layouts health leaves out (Big Number, Quote,
    Statement, Agenda), or have no sentence under them to offer as the claim.
  - The one it finds on **lanes** is false: "Vote Tuesday to fund the pilot" is the ask, an
    imperative, not a topic.

### What else disagrees with the reader

- **Warehouse ties support at 89, above larder.** Its notes read nine slides aloud (one warning),
  its headlines are topics, and its close is bare. None of that moves the score, and warehouse's
  balance is 97, the best of the six, because every slide has notes and a list.
- **Tone is lowest on the strongest deck.** Lanes has six tone warnings and tone at 63: its
  slides are about crashes, near misses and cost, so they "read graver than written". It is the
  pattern the first calibration found on proof and problem slides.
- **Fidelity does follow the reader at the bottom.** Warehouse (73) and larder (74) are the two
  lowest, but fidelity is a quarter of a score the cap overrides.

### For R2

These six decks are the case the first calibration couldn't make. Each keeps its outline, so the
claim, bigIdea and label findings fire. On them:

- the score that would follow the reader is the one without the cap (+0.88);
- weighing the claim and bigIdea warnings would separate support, larder and warehouse (89, 87,
  89) from the three a reader ranks above them;
- the label warnings undercount topic headlines by two-thirds, so weighing them as they stand
  would barely move vans or warehouse.

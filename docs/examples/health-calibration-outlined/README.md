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
  --audience "home bakers" --goal Educate --minutes 20 --persona students-educators
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

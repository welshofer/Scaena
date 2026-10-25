# Dogfood pass 7: four decks the five don't cover

Built on main 095f4263 from slides written here, with no model call:

```sh
dogfood7/run.sh target/release/scaena OUT
```

Each slide's PNG was looked at, at 1280 × 720. The contact sheets are in `sheets/`, and lint, health, and generate's output in `out/`.

| Deck | Theme, arc | Slides | Shapes it uses |
|---|---|---|---|
| `deepdive`: "How we cut p99 checkout latency by 60%" | Dusk, PARL | 13 | a code before/after, a line along weeks, an architecture, a decision brief, a quote |
| `board-de`: Vorstandsbericht Q3 2026, `--language German` | Daybreak, Data Storytelling | 11 | "48,6 Mio. €", "+18 %", a line in Mio. €, a risk matrix, a quote, a timeline, decisions |
| `appeal`: Harbor House winter appeal | Ember, Conflict → Struggle → Resolution | 11 | one child's story, a big number, a quote, giving tiers, testimonials, an ask, a closing with an address |
| `training`: postmortem training | Salon, Learning Journey | 12 | an agenda, a definition, numbered steps, icons, a quiz, a checklist, takeaways |

The catalog the model is shown has no layout for code, an architecture, a decision brief, a risk matrix, or a quiz, though the library has each. So the decks do what a model would do. Three slides name the layout as the talk calls it ("Architecture", "Decision Brief", "Quiz"), and two use the nearest catalog shape ("Before / After" with code in it, "Quadrant" for the risks).

## Findings, ranked by what a person notices first

### 1. A figure is shown wrong, in two decks (owner: `scaena_ops::narrative`, figure parsing)
- **appeal, slide 3 (Big Number):** `keyBullets[0]` is "1 in 7"; the slide shows **"1"**, so it reads "1 / children in Halverson County slept somewhere unstable last winter". The number is wrong, and so is the sentence it makes. `she-is-one-of-many-number` is `"1"`.
- **board-de, slide 3 (Scorecard):** the bullets are "48,6 Mio. €: Umsatz im Q3, +9 %" and "1,2 Mio. €: EBITDA, +18 %". The figures drawn are **"3"** (from "Q3") and **"+18"** (its "%" lost). The real figures stand below the rule as uppercase labels, "48,6 MIO. €" and "1,2 MIO. €". A German figure, with a decimal comma and "Mio. €", is not read as a figure, so a number is pulled out of the detail instead. A board member reads revenue as "3".
- Neither lint nor health says anything about either.

### 2. Code is rewritten and set as prose (owner: `narrative.rs:2697`, the detail's first word capitalized; and no route to code)
- **deepdive, slide 6 (Before / After → `myth-fact`):** "for _, item := range cart.Items { … }" is drawn **"For _, item := …"**, and "prices, err := pricing.GetMany(…)" **"Prices, err := …"**.
  - It is no longer valid Go, in front of engineers.
  - It is set in the body sans, not as code (no `code`, no monospace, no syntax colors), and sits at the foot of each card.
  - A detail that reads as code should keep its case and be made a code text. Code has no slot shape in `SLOT_SHAPES` and no catalog layout, so a model has no way to ask for it.

### 3. Layouts the talk needs fall to a bullet list that builds by click (owner: `scaena_narrato::layouts` `BY_SUGGESTION` and the builder's mapping; the library has each)
- **deepdive 5, "Architecture":** a four-bullet list built in four clicks, filling the left half; the right half is empty. The library has an Architecture layout (S.2).
- **deepdive 10, "Decision Brief":** 54 words of bullets in four clicks (W210). The library has Decision Brief (S.1, Briefing).
- **training 10, "Quiz":** bullets in three clicks, with FAQ's "question?: answer" split left raw ("…skip review"?: Config pushes skip review"). The library has Quiz (S.1, Classroom).
- **board-de 8, the risks as "Quadrant":** set as four labelled text blocks ("Hoch / hoch", "Gering / gering"). There are no axes, no shading, and each detail runs into the next label. The library's risk matrix, "shaded toward the accent", can't be reached.
- Each unknown name falls to `body`. A layout name the library has should map to it, and the catalog or `SLOT_SHAPES` should list it, so the model names it.

### 4. The closing with an ask and an address doesn't become Call to Action (owner: the builder's closing rule, lift-up ATTR-3)
- **appeal 11:** "Thank you" with ["Give a family a warm bed tonight", "harborhouse.org/winter · (555) 014-2200"] is set in **`bullets`**: a title top-left and two bullets. It is not Call to Action, and not even Closing.
- CLAUDE.md says "a bare thanks or questions with an ask and an address take Call to Action, the ask its words and the address its button".
- Either the address with a phone number after it isn't read as an address, or a closing with bullets is not a "bare thanks". The appeal's last slide looks like a body slide.

### 5. A phone number's hyphen made an en dash (owner: the typesetter, its range rule)
- **appeal 11:** "(555) 014-2200" is drawn and kept as **"(555) 014–2200"**. The run's text has U+2013.
- The rule that sets "a range of figures with an en dash" takes a phone number for a range. A phone number, a ZIP+4, or an ISBN is not a range.

### 6. German numbers set in English (owner: the builder's number formatting, and the alt text)
- **board-de 4 (Line Chart):** the side figure reads **"48.6 Mio. €"**, where the axis label beside it reads "48,6".
- **board-de, the charts' `alt`:** "Q3 2025 38.3 Mio. €, …", "Wind 21.4 Mio. €". The reader and the PDF's tags hear English decimals in a German deck.
- **board-de 9 (Timeline):** "60 Mio. / € Quartalsumsatz" breaks the unit over two lines.
- **board-de 1:** the subtitle breaks "12. / November 2026" over two lines.

### 7. A figure parted from its unit, and an inconsistent figure (owner: the builder, typesetting)
- **deepdive 8 (Bar Chart):** the note breaks "…to 140 / ms at p99"; a figure and its unit should not part.
- **deepdive 4 (Line Chart):** the side figure reads **"1840 ms"**, while the chart's own label reads "1,840" and the note "1,840 ms".

### 8. Health is wrong in two ways that every deck shows (owner: `scaena_narrato::health`)
- **Tone and intensity in German:** board-de's tone flags slide 6 "reads neutral, where it was written to sound hopeful". The lexicons read English. CLAUDE.md says a measure the lexicons can't read "has no score and says so"; here it scores German as neutral.
- **The closing:** all four decks get "Slide N was planned cooler than its place in the talk" on their last two or three slides (closing at 0.3 against a curve at 0.90–0.96). A calm "Questions?" or "Vielen Dank" is right, so the advice is noise on every deck.
- **Smaller:** plainly hopeful English reads "neutral" (appeal 8, "moved into a family room… signed a lease"; appeal 9's testimonials). It's a lexicon gap, but it shows up in a deck's Story tab as a warning.

### 9. The wrong bar lit (owner: the builder's highlight)
- **appeal 4 (Bar Chart):** the takeaway "The wait has tripled since 2022" lights **2022**, the smallest and oldest bar. The point is 2025's 34 days.
- A takeaway that names a starting year ("since …") should light the latest, as a line's band does (G.19).

### 10. A personal story set as bullets (owner: the builder, Image + Text with no picture)
- **appeal 2:** Maya's story, three sentences, becomes three bullets built in three clicks, 50 words (W210), in the left half; the right half is empty.
- With no picture, "Image + Text" takes a layout that waits for none, but a story reads as a paragraph, not as points.

### 11. Small type where the meaning is (owner: the layout library, `build_layouts.py`)
- **training 11 (Checklist):** the status, "DONE" or "NEXT", the thing a checklist is for, is the smallest text on the slide, tucked in each row's top-right corner.
- **The quotes' attributions** (deepdive 11, board-de 7, appeal 5) are about a sixth of the quote's size and read as a footnote.

### 12. Smaller things a person sees
- **training 1 (Salon, `title-center`):** the subtitle sits about 2 px under the title's descenders ("actually"). It is clear by ink, so E101 is silent, but it reads as touching.
- **deepdive 9 (Scorecard):** the uppercase labels set "p99" as **"P99"**. A label that holds a case-significant token shouldn't be capitalized.
- **Much of the slide left empty:** the scorecards (deepdive 9, board-de 3), Steps (deepdive 7) and the bar chart (deepdive 8) sit in the top half. The numbered lists (deepdive 12, training 4) sit in a half-width column.
- **deepdive 3:** "1.84 s" in Fraunces at display size sets the 4's flag against the 8.
- **board-de 10 (Next Steps):** the action column is set a size smaller than the owner and date columns beside it.

### 13. What lint doesn't catch
Findings 1, 2, 5, 6 and 7 are about words and figures set wrong, and none raises a lint finding. A figure the builder writes into a figure place that differs from the slide's own figure, or code whose case changed, would be cheap to check where the deck is made.

## What worked
- German quotation marks „…“ on board-de 7, and "+18 %" with its space.
- The vertical timeline (appeal 8), the testimonials (appeal 9), the icon rows (training 5: clock, users, target and wrench all fit), the definition (training 3), Takeaways for three closing points (training 12), and Next Steps as a table (board-de 10).
- Health's pacing and balance read right, and its "written for Result, now stands in Learning" (deepdive 11) is a true catch: the quote was placed after the decision.

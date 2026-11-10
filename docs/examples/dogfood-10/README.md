# Dogfood pass 10

Three decks' slides written as Narrato's model would answer, each with the outline it was written to, built with no model called:

```
scaena generate board "Calder Health Q3 FY26 board update: a hard quarter and the turnaround plan" \
  --slides board.slides.json --from-outline board.outline.json --source board.csv --no-look --pictures none \
  --audience "the board" --goal "Update Status" --length "11-15 slides" --persona executives
scaena generate teach "How to read a scientific paper, for first-year biology students" \
  --slides teach.slides.json --from-outline teach.outline.json --no-look --pictures none \
  --audience "first-year students" --goal Educate --length "11-15 slides" --persona students-educators
scaena generate launch "Produktvorstellung Lagerfux: Palettensensor gegen Fehlbestände" \
  --slides launch.slides.json --from-outline launch.outline.json --no-look --pictures none \
  --audience "Lagerleiter im Mittelstand" --goal Sell --length "11-15 slides" --language German --persona customers
```

- `board`: a board update on a bad quarter, grounded in a CSV (Conflict → Struggle → Resolution).
- `teach`: a first lecture for first-year students, with a quiz and a closing Q&A (Learning Journey).
- `launch`: a product launch in German, its figures in German's marks (Problem–Solution).

`layouts.slides.json` sets a short list, Three Numbers, the Risk Matrix, and Scorecard on one deck,
which `layouts-before-after.png` shows before and after the pass's layout round, and
`scorecard-after.png` the Scorecard after it.

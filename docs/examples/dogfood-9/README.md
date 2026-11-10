# Dogfood pass 9

Three decks' slides written as Narrato's model would answer, each with the outline it was written to, built with no model called:

```
scaena generate board "Harborline Q3 FY26 board update" --slides board.slides.json --from-outline board.outline.json \
  --source board.csv --no-look --pictures none --audience "the board" --goal "Update Status" --length "11-15 slides"
scaena generate tech "How we cut checkout p99 latency" --slides tech.slides.json --from-outline tech.outline.json \
  --no-look --pictures none --audience engineers --goal Educate --length "11-15 slides"
scaena generate keynote "Taking the library to where people are" --slides keynote.slides.json \
  --from-outline keynote.outline.json --no-look --pictures none --audience "the library board and city council" \
  --goal Persuade --length "11-15 slides" --modality keynote
```

- `board`: a board update grounded in a CSV, with charts and figures (Data Storytelling).
- `tech`: a technical talk with code and an architecture slide (PARL).
- `keynote`: a persuasive keynote with a hook, a turn, and an ask, and no pictures (What Is → What Could Be).

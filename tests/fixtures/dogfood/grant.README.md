# Dogfood 5: the grant renewal (W1)

A 9-slide grant renewal for a review panel that checks every figure, made with no model call.

- `grant.slides.json`: the slides as a model would write them (slide 7 has `chartData`).
- `grant.outline.json`: the settled outline (Problem–Solution, five beats, the big idea).
- `grant.trails-budget.csv`: `docs/examples/data/trails-budget.csv`, unchanged.
- `season-report.pdf` is not committed (1.7 MB). Make it from the trails example:

      scaena export docs/examples/trails.deck.json --format pdf --out season-report.pdf

To make the deck, run this from a folder holding `season-report.pdf`, `trails-budget.csv` (this
CSV, renamed), and the two JSON files:

    scaena generate grant "High Country Trails' renewal of its State Trails Fund grant, for the review panel" \
      --slides grant.slides.json --from-outline grant.outline.json \
      --source season-report.pdf --source trails-budget.csv \
      --no-look --pictures none \
      --audience "the State Trails Fund grant review panel, who check every figure" \
      --goal Persuade --length "5-10 slides"

The weaknesses built in:

- slide 6's 27% names no source;
- slide 9's $184,000 names none, though the PDF holds it;
- slide 4 says slide 3 again in other words;
- every slide's notes are its headline again;
- slide 9 closes on "Questions?".

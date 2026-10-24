# Dogfood decks, 2026-10-23 (main e4a32428)

Narrato slides written for five generated decks and a probe of unused layouts, made with no key:

```sh
scaena generate decks/pitch "Seed pitch for Manifest, automated freight invoice audit for mid-market shippers" \
  --audience Investors --goal Persuade --length "11-15 slides" --arc "Problem–Solution" --theme dusk \
  --slides dogfood/slides/pitch.json --no-look
scaena generate decks/qbr "Q3 FY26 quarterly business review for Northwind Analytics' executive team" \
  --audience Executives --goal "Update Status" --length "11-15 slides" --arc "Data Storytelling" --theme daybreak \
  --slides dogfood/slides/qbr.json --no-look
scaena generate decks/lecture "A first-year statistics lecture on Bayes' theorem and why positive test results mislead" \
  --audience Students --goal Educate --length "11-15 slides" --arc "Learning Journey" --theme salon \
  --slides dogfood/slides/lecture.json --no-look
scaena generate decks/keynote "Keynote: how a Maine fishing town built its own microgrid after an eleven-day blackout" \
  --audience "Conference / Public" --goal Inspire --length "11-15 slides" --arc "Hero's Journey" --theme ember \
  --slides dogfood/slides/keynote.json --no-look
scaena generate decks/launch "Launch event for Relay, the support inbox where AI drafts every first reply" \
  --audience Customers --goal Sell --length "11-15 slides" --arc "The Reveal" --theme ember \
  --slides dogfood/slides/launch.json --no-look
# The keynote in Japanese (dogfood pass 6, ADR-0032 item 5, #568, #624, J1):
scaena generate decks/keynote-ja "基調講演：11日間の停電のあと、メイン州の漁師町が自分たちのマイクログリッドを築いた話" \
  --audience "Conference / Public" --goal Inspire --length "11-15 slides" --arc "Hero's Journey" --theme ember --language Japanese \
  --slides dogfood/slides/keynote-ja.json --no-look
```

`probe.json` is 12 slides of layouts the five did not reach. The pass's findings are in the coordinator's report; this branch holds inputs only and is not for merging.

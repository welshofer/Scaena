# Dogfood 13: the outline path end to end

A 20-minute Persuade talk in German for a city council, grounded in a PDF and a CSV. Its outline
was written with `--outline`, settled by hand, and made with `--from-outline`. Then came the look,
the fit, and the notes. Built at main 982b623a, release CLI.

## How it was run

`scripted.py` stands in for Anthropic on this machine. It answers each step from a script:

- **The arc:** Problem–Solution.
- **The outline:** `biotonne.asked.outline.json`, six beats and 13 slides.
- **The slides:** written to the settled outline's beats, as the slides' prompt lists them.
- **The look**, with three changes:
  - it shortens a settled claim's title;
  - it rewrites a list's figures as "34%";
  - it turns the topic "Kosten und Einsparungen" into a claim that states the slide's own figures.
- **The fit:** rewrites whatever it is sent.
- **The notes:** written to each slide's budget.

The talk is "Biotonne für alle Haushalte bis 2028". It is grounded in `bericht.pdf`, a German
report, and `abfall.csv` (`jahr,restmuell_t,gebuehr_eur_t`).

```
python3 -I scripted.py LOGDIR &      # each request and its answer go to LOGDIR
export ANTHROPIC_API_KEY=sk-ant-dummy
scaena generate biotonne "Eine Vorlage für den Stadtrat: die Biotonne für alle Haushalte bis 2028" \
  --provider anthropic --model claude-test --base http://127.0.0.1:8787 \
  --audience Stadtrat --goal Persuade --minutes 20 --language German \
  --source bericht.pdf --source abfall.csv --outline biotonne.asked.outline.json
# The outline is written and no deck made. Settle it by hand into biotonne.outline.json, then make
# the deck, with its look, fit, and notes, from the same command with:
#   --from-outline biotonne.outline.json   in place of   --outline biotonne.asked.outline.json
```

The key is a dummy, and nothing leaves the machine.

The outline was settled by hand (`biotonne.outline.json`):

- Two claims retyped: Proof's, and Solution's, which took the removed beat's words.
- Proof moved before Solution.
- Existing Approaches removed.
- "Einwände" added. Its slides are written about the Winterdienst, off its claim on purpose, to
  see the claim finding fire.

The close is "Vielen Dank" over "Fragen?" and an email.

## Findings, worst first

Each finding has its own pull request off main.

1. **The fit undid settled claims (#1155).** Three of the six settled claims stood on Statement
   slides and ran past their place (E100). `overflowing` handed them to the fit, which rewrote
   them. The look is barred from dropping a word of a settled claim; the fit was not.
   - **Fix:** a text that says its beat's settled claim is set `fit: shrink`, with no `maxSize` or
     `maxLines` of its own, and is never offered to the fit.
   - **After:** every claim stands in the person's words, and E100 is gone.
   - **Still warned:** the three longest claims take 4 lines at the display role's smallest size,
     against its `maxLines` of 3, so W202 stays on them. That is the person's to shorten.
2. **A bare German close fell to a list, and the ask was on no slide (#1158).** The closing layout
   holds one item, so "Vielen Dank" with two lines failed `holds` and became bullets. "Vielen Dank"
   was not in `bare`'s list either.
   - **Fix:** a bare closing with a settled claim keeps `closing` whatever its line count. The claim
     heads it, and every line stands in the support: "Bewilligen Sie 2,4 Mio. € für die Tonnen im
     Haushalt 2026." over "Vielen Dank · Fragen? · biotonne@lindenau.de", the email linked.
   - It works in any language and reads no more asks than before.
3. **The look refused a topic turned into its claim with the slide's own figures (#1160).** "Die
   Tonnen kosten 2,4 Mio. € und sparen 3,1 Mio. € im Jahr" was refused as adding 2,4 and 3,1,
   though both are on the slide's cards. `look::read` had compared the new title with the old one
   alone.
   - **Fix:** `figures_from`. A title may add a figure the slide's subtitle or points write. It may
     never add one they do not write, nor lose one of its own.
4. **Every section was titled in English (#1164).** The model echoes each beat whole, "Problem:
   Define core challenge and stakes". `arcs(n)` compared that whole string with the arc's names,
   which are split at the colon, so the label passed for the model's own.
   - **Fix:** split it too.
   - **After:** the sections take their German headlines.
5. **The fee chart stayed on its own rows (#1168).** The series "Gebühr je Tonne" was read as
   "gebührjetonne" against the column `gebuehr_eur_t`, read as "gebuehrt".
   - **Fix:** `measured` reads ä, ö, ü, and ß as a file writes them out, both ways. It sets aside as
     the unit:
     - a "per" phrase: "je", "pro", "per", "par", or "por" and the word after;
     - a mass: `t` or `kg`, where it is not the name's first word.
   - **After:** the chart reads `@abfall` on `gebuehr_eur_t`.
6. **The look's claim refusal counted German small words (#1170).** Its stoplist is English, so a
   title dropping only "und" or "des" was refused, and every refusal listed them.
   - **Fix:** a German stoplist, for German decks alone.
   - **Not changed:** health's "says again" also fired on "jedes Jahr", shared by two headlines.
     English counts "every year" the same way (`content_words` drops no small words in any
     language), so that half moves a health measure. It is Jay's call (item l).

## What held

- **The outline as settled.** `meta.narrative.outline` keeps the six beats as settled: moved,
  removed, added, and retyped. The sections and beats follow it, and nothing settled was lost.
- **The claim finding** fired on the drifting beat alone: "Slides 9–10 say nothing of the claim
  settled for the Einwände part of the talk …". No other beat was flagged.
- **The big idea.** The close says the ask's claim, but health does not read the big idea in
  German, and says so.
- **The notes** run about 21 minutes against the brief's 20, and pacing scores 100.
- **German typesetting:**
  - "34%" is set "34 %" with a no-break space;
  - "2,4 Mio. €" and "4.200" stay joined;
  - quotation marks are set „…“.
- **Health** scores 88 before the first fix, with the overflowing claims rewritten. With the first
  fix alone it scores 85, for two reasons:
  - the three W202 warnings;
  - the script's two identical Einwände headlines (W425).

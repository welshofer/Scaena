# Agent dogfood 12 — plan (run after #1283 lands)

A Spanish pitch to Seville's city council: extend a 12-fountain pilot of public drinking
fountains to 120 by 2027. MCP alone, no hand edits to deck.json.

## Inputs (`in/`)

- `informe.pdf`: the city's 2025 urban heat report, one page of prose.
- `piloto.csv`: the pilot's monthly use, May to September 2025, with columns `mes`,
  `litros_servidos` and `botellas_evitadas`.

## Brief

- language: Spanish
- goal: Persuade
- audience: Pleno del Ayuntamiento de Sevilla
- **minutes: 12, no length.** #1278's path over MCP: the brief op should write `5-10 slides`.
- voice: plain and direct; the figures carry the case.

## Where each figure lives (the answer key)

| figure | informe.pdf | piloto.csv | kind |
|---|---|---|---|
| 41 días > 40 °C | yes | no | PDF only |
| 46,2 °C | yes | no | PDF only |
| 62 % sin agua | yes | no | PDF only |
| 3.800 € / fuente | yes | no | PDF only |
| 410 € / año | yes | no | PDF only |
| −18 % urgencias | yes | no | PDF only |
| 18.400 litros (julio) | no | yes | CSV only |
| 36.800 botellas (julio) | no | yes | CSV only |
| 66.400 litros (suma) | no | no | worked out from the sheet |
| 132.800 botellas (suma) | no | no | worked out from the sheet |
| 120 fuentes en 2027 | no | no | the talk's own target |
| 460.000 € | no | no | the talk's own ask |

## Checks

1. **Brief.** `deck_create`, then `deck_patch` with a `brief` op that has minutes 12 and no length.
   The brief should now hold `length: "5-10 slides"`.
2. **Generate.** Call `deck_generate` with both files as `sources`. Then:
   - `deck_read` should show `meta.narrative.sources[].figures` for both;
   - the PDF's kept figures include 41, 46.2, 62, 3800, 410 and 18;
   - the CSV's kept figures include 18400 and 36800, and not 66400.
3. **Wrong source.** Patch one beat to cite `source:informe.pdf` for the slide that writes
   18.400 litros. `deck_health` should give a `source` warning naming piloto.csv. Its fix cites
   piloto.csv, and applying the fix should clear the warning.
4. **Silent cases.**
   - The sum 66.400 and the target 120 / 460.000 € give no `source` finding, whichever source
     the beat cites.
   - A beat citing a source that keeps no figures gives no finding.
5. **The usual loop.** lint, health, patch, render, and a PDF export with notes. Any finding
   that matches an open PR of mine is named in the report: #1132 (h, code as one idea) and
   #1143 (i, length from minutes outside the op).
6. **Score-moving findings** go to the coordinator before any fix.

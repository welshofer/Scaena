# Dogfood 19: a French conference talk from a paper, over MCP alone

W1, 2026-11-13, on `main` at 7aef4956 (then e00e488f for the PRs).

An agent made a 15-minute talk for an urban-ecology conference with the MCP server alone. Its source was a research paper it was handed as a PDF, with the paper's results as a CSV. The paper is fictional: `paper.py` writes it with reportlab. It has:

- French accents throughout;
- `œ` and the ligatures ﬁ, ﬂ, and ﬃ, as codepoints in a TrueType font;
- one paragraph in Helvetica (WinAnsi, no ToUnicode);
- a results table;
- decimal commas ("2,4"), and thousands grouped by a narrow no-break space ("1 240 000"), with "3,7 %";
- a reference list.

## Files

| File | What it is |
|---|---|
| `paper.py` | Writes `toitures.pdf`, the paper |
| `resultats.csv` | The results, five quartiers, as the agent attached them |
| `slides.json` | The twelve slides the agent wrote, in French, notes included |
| `calls.json` | The calls, in order |
| `mcp.py` | The stdio client that made them |
| `slides.png` | Each slide at its last step, after `deck_lint` with `fix` |

## The calls

1. **`deck_create`** with Salon, then **`data_attach`** of the CSV. Both worked.
2. **`deck_generate`** into the same bundle was refused: "is there and is not an empty directory: a new bundle needs a place of its own". See call 1.
3. **`deck_generate`** into a new bundle, `toitures`, worked. It was given:
   - `language: "French"`, `minutes: 15`;
   - the outline's five beats: Contexte, Méthode, Résultats, Discussion, Recommandations;
   - `sources`: the PDF and the CSV;
   - `data`: the CSV.
4. **`deck_health`**: 86.
5. **`deck_lint`** with `fix`: two W210s left (41 and 54 words against 40), no errors.
6. **`deck_export`**: PDF with notes, and PNGs.

Two figures were planted to test sourcing:

- **Slide 4** cites `resultats.csv` for "412 toits" and "1 240 000 relevés", which only the paper holds.
- **Slide 8** writes "57 % des toits", which no source holds.

## 1. The PDF reader on French

**Words** (`scaena_sources::pdf::words`):

- **Read as written:** every accent, "œ" in "cœur" and "œuvre" through Helvetica's WinAnsi, the curled apostrophe, « », the en and em dashes, the degree sign, and m².
- **Ligatures stay as their presentation codepoints:** "eﬃcacité" (U+FB03), "ﬂore" and "réﬂexion" (U+FB02). This is what the font's ToUnicode says, and pdftotext reads them the same way. Nothing reads the PDF's words but the figure reader, so it costs nothing today. Decomposing them would matter once a source's words are matched as words.

**Figures** (`meta.narrative.sources[].figures`, from `scaena_narrato::source_figures`): wrong on two counts.

| The paper writes | Kept as |
|---|---|
| "2,4", "3,7", "1,6" | 24, 37, 16 |
| "1 240 000" (narrow no-break space) | 1, 240, 000 |
| "184 500" | 184, 500 |

So none of the paper's decimals or grouped thousands matches the CSV's cells (2.4, 184500), though both hold the same results.

- **Decimal commas:** #1304, open, reads each source in its own marks, which fixes these.
- **Space-grouped thousands:** #1304 does not cover them. Neither `figures` nor `source_figures` reads "1 240 000", with a narrow, no-break, or plain space, as one figure; each reads 1, 240, and 0. The builder does read it as one (it sets it with no-break spaces), so the two disagree. This is a follow-up once #1304 lands, since it changes the same functions.

The CSV's own figures are kept right, but they also include **2**, which is no cell: it is the digit of the column name `toitures_m2`.

## 2. French typesetting

Right everywhere it was looked for: on the slides, in the notes, and in the PDF's subject.

- A no-break space before `:`, `;`, `!`, and `?`, and inside « »: "La question : pourquoi les toits ?", "« Une toiture sèche ne rafraîchit rien ! »".
- "57 %" and "1 240 000" are set with no-break spaces.
- "Source : resultats.csv" is in the chart note.
- The bar chart's value labels are "2,4", "1,9", and so on.

The space used is U+00A0. The Imprimerie nationale sets a fine space (U+202F) before `;`, `!`, and `?`; that is a typographer's choice, not a fault.

Lower case after a figure on a card was not exercised. The "Trois quartiers, trois leçons" slide, three points each led by its figure, was set as a list, because each point writes two figures ("2,4 °C à Confluence, grâce à 184 500 m²"), as `faithful` asks.

## 3. Sourcing on French figures

| Slide | Health said | Right? |
|---|---|---|
| 4 (planted, cites the CSV) | "Cite toitures.pdf for 412" | Right, by #1283 |
| 4, its "1 240 000" | nothing | The paper's 1 240 000 is kept as 1, 240, 0 (above), so no source holds it |
| 8 (cites the CSV) | "Cite toitures.pdf for 2,4" | **Wrong.** The CSV holds 2.4. Health read the slide's "2,4" as 24, and the paper's 24 matched. #1304 fixes both sides. |
| 8, its "57 %" (planted, no source holds it) | nothing | By #1283's design: "a figure no kept source holds … is no finding". See call 5. |
| 2 (the agenda) | "Say where 412 comes from" | An agenda's preview of a figure the method slide sources; harmless |
| 11 (references) | "Say where 108 comes from" | **Wrong:** a journal's volume. Fixed in #1317. |

## 4. Health on a French deck

- **Intensity and tone:** "12 slides declare intensity, but health reads it only in English, and this deck is in French". No score and no finding, as it should be.
- **Turns:** asked in French connectives, "Donc" and "Mais", with the slide's own French words to carry across ("quartiers", "leçons", "flore"). No English word leaked into a French example.
- **Coherence:** read, with no score.
- **What stays English:** the asks around those examples and every `says` line ("12 slides where the brief asked for 5–10"). That is the editor's language, not the deck's.
- **Pacing:** "12 slides where the brief asked for 5–10: 2 over". 15 minutes falls in `length_for_minutes`' 5–10 band, and a twelve-slide conference talk of fifteen minutes is ordinary. See call 3.
- **Arc:** "Slide 11 is planned at 0.20, calmer than its place in the story calls for (0.87): plan it more intense". Slide 11 is the references slide. See call 4.

## 5. A references slide and a results table

**References.** The library has a References layout, but an agent's "References" fell: "no layout is called that".

- Narrato's variety pass then set it as Numbers: each work on a card under its year, "2023", "1982", "2024", drawn large as a figure (see `slides.png`, slide 11).
- Each card's text did not fit and was shrunk by lint's fix.
- **Fixed in #1317:** References, Bibliography, Works cited, and Références now reach the layout, numbered as written.

**Results table.** "Table" fell to bullets: a one-line list of the slide's subtitle, the table not drawn at all. The library's Table layout ("a table that holds the facts") has no path from a made deck. See call 2.

**The bar chart** drew from its own rows, unbound from the CSV. Its series "Baisse de température" did not match the column `baisse_temperature_c`, for the accent and for "de". **Fixed in #1315.** It lit Vaise, the figure its headline lands on, by the existing rule.

## 6. The PDF with notes

- **Outline:** from the spine, "Contexte", "Méthode", "Résultats", "Discussion", "Recommandations".
- **Metadata:** title and subject the deck's, in French.
- **Notes:** each slide's own, in French, set in Inter.
- **Copying text** through the fonts' ToUnicode (pypdf; pdftotext reads ActualText and was right) was wrong wherever a figure held a 1 or a 3:

  | The slide or notes write | Copied as |
  |---|---|
  | 100 000 | 1.00 000 |
  | d'1,3 degré | d'1.,3. degré |
  | (1982) | (1.982) |
  | 412 toits | 41.2 toits |

  The agenda's list markers "1." and "3." gave their figure glyphs the text "1." and "3." in the font's ToUnicode, and every later figure set in those glyphs copied so. **Fixed in #1316**; the same export after it copies every figure as written.

## Fixes, each a draft PR

| PR | What |
|---|---|
| #1315 | A French series binds to its column, its accents and joining words aside ("Baisse de température" measures `baisse_temperature_c`; "Part de la surface" measures `part_surface_pct`) |
| #1316 | Each glyph of a list's marker says its own character, so a PDF copies a figure as the figure. 16 torture goldens and six layouts' theme digests re-blessed, clusters only. |
| #1317 | A talk's works cited take the References layout; health asks neither where their years and pages come from nor what point the heading makes |

## Calls

1. **`deck_create` → `data_attach` → `deck_generate`** does not compose. `deck_generate` makes its own bundle and refuses one `deck_create` made. An agent that starts a deck the documented way, then generates it, must make a second bundle, attach its data again, and leave the first behind.
   - *Recommendation:* let `deck_generate` take a bundle that still holds only `deck_create`'s blank title slide, keeping its theme and attached data. At least, the refusal should say that `deck_generate` makes the bundle itself and takes `data` and `theme`.
2. **No table in a made deck.** "Table" with `chartData`, a results table being the commonest slide of an academic talk, draws no table.
   - *Recommendation:* "Table" (and "Data Table", "Results Table") takes the Table layout, its node a table bound to the CSV as a chart binds, its subtitle the note. This is a medium change in the builder; I'd take it as its own item.
3. **Fifteen minutes is "5–10 slides"** (`length_for_minutes`, Narrato's bands), so a twelve-slide fifteen-minute talk is "2 over" and pacing 90. Moving the band moves a score: **Jay's.**
   - *Recommendation:* 15 minutes reads 11–15 slides, as 16–30 does now, or bands at 0–10 / 11–20 / 21–40.
4. **A references slide at the end bends the arc.** Health asks it planned more intense (0.20 against 0.87) and counts it in fidelity's 71. Exempting it, as the closing run is, moves fidelity: **Jay's.**
   - *Recommendation:* treat a references slide as the closing run is treated: no bend, whatever layout it took.
5. **A figure no source holds is no finding** (#1283's design), so the planted "57 %" passes in silence.
   - *Recommendation:* keep it so. Health cannot tell a figure the talk works out ("a change, a sum") from one it makes up. An info on such a figure, in a deck whose sources keep figures, would be useful but noisy, so it is not worth it now.
6. **One CSV given both as `data` and among `sources`** is kept twice: data sources `resultats` and `resultats-2`, and `resultats.csv` twice in `meta.narrative.sources`. Health then lists that slide's sources as `["resultats.csv", "resultats.csv"]`.
   - *Recommendation:* a source that is a file `data` already attaches is that source, not a second copy. This is a small fix; I'll take it if you agree.
7. **A sheet's header row gives figures:** the 2 of `toitures_m2`.
   - *Recommendation:* `source_figures` reads a CSV's cells after its header only. This touches #1304's function, so after it lands.

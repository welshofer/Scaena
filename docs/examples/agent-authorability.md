# Agent authorability (PLAN 0.13)

Phase 0's second hypothesis: semantic text documents are naturally agent-authorable. Five agents made the five edits PLAN 0.13 names to one deck. Each started fresh and knew the format only from its documents and CLI. The bundle is `docs/examples/authorability/`. `transcript.md` there is each agent's own record of commands, exit codes, decisions, and gaps, and `history/<step>.deck.json` is the deck after each edit.

**Verdict: the hypothesis holds, and gate 0 criterion 7 is met.** All five edits validate and lint clean. Every patch is local, and identity survives: no node was recreated or retyped, no state was reordered, and no state outside an edit's reach resolves differently.

The semantic layer kept the edits small:
- Data bound by name made rolling the chart forward one field.
- A theme reference made the retheme one field.
- Tracked deltas made a build one new state plus one trimmed one.

The agents checked their own work with `inspect` and `diff`, and that caught the one real trap: object values merge.

Two qualifications:
- **Clean is a low bar today.** `validate` and `lint` read nothing in the bundle besides the deck: not the data files, the chart fields, the fonts, or the theme. The judge checked theme names and figures by script and rendered what can render. It found one problem nothing checks: the deck as written cannot render. Its `fonts` list two of the theme's three families, leaving out the unused mono, and the renderer requires all three (finding 5).
- **Copy is literal.** Rolling the data forward was one field for the chart and 22 hand edits to the words, and lint stayed clean over a headline the new data made false (finding 6). That is the format's largest authorability gap.

Where the agents hesitated, the docs were usually silent about something the code already does. This PR writes those rules down (findings 1–4). The other findings point at Phase 1 tasks.

## Method

- **Agents.** One fresh agent per edit, with no memory of the others; the deck is the only thing handed on.
  - Each could read SPEC, both schemas, the example deck, theme, and data, the `author-deck` and `retheme` skills, CLI help, and the bundle.
  - Each could not read the code, tests, scripts, PLAN, ADRs, git history, or the earlier transcript and snapshots.
- **Tools.** Today's CLI: `validate`, `lint`, `inspect`, `diff`. Not `render`: the bundle has no font files.
- **Requests.** Worded as a user would; none mentions ids, deltas, identity, or locality.

| Step | Request (verbatim) | Added to the bundle first |
|---|---|---|
| (a) | "Here's our revenue by quarter and product (data/q3-revenue.csv). Q3 2026 revenue reached $57.6M, double Q4 2025's $28.7M, and Pro grew from a fifth of revenue to a third. Build a six-state deck for the board that shows the growth and the mix shift." | `data/q3-revenue.csv` (a copy of `docs/examples/data/q3-revenue.csv`), `themes/dusk.theme.json` |
| (b) | "Q4 closed. Here's the new data, data/q4-revenue.csv (2026-Q1 through 2026-Q4). Roll the deck's chart forward to it." | `data/q4-revenue.csv` |
| (c) | "Don't headline the slowdown. Change the headline “Growth slowed in Q4” to “Revenue grew every quarter”." | — |
| (d) | "On the $69.6M slide, show the number by itself first, then bring in what it means on the next click." | — |
| (e) | "We're presenting in a bright room. Switch the deck to the Daybreak theme (themes/daybreak.theme.json)." | `themes/daybreak.theme.json` |

- **Judging.** PLAN 0.13's three questions, for each edit:
  - **Clean:** `validate` and `lint --json` on the snapshot exit 0 with no findings.
  - **Local:** `scripts/deck_delta.py` between consecutive snapshots says what the edit touched, structurally. No state the request did not concern may change. The effect is measured too: which states' resolved snapshots (`scaena inspect --json`) changed. `scripts/judge_edit.py docs/examples/authorability a b` runs all of it for one edit.
  - **Identity:** every surviving node keeps its id and type; nothing is removed and re-added under a new id; states keep their order; data keeps its name.
  - **What clean misses,** checked by `scripts/judge_edit.py` for every snapshot: every theme name the deck uses (slots per state layout, roles, presets, durations, palettes, data scales) exists in its theme.
  - **Figures:** every figure in the copy, recomputed from the CSV it cites, for the edits that wrote figures: (a), (b), (c).

## Results

| Step | validate, lint | What the edit touched | State deltas changed | Snapshots changed | Identity |
|---|---|---|---|---|---|
| (a) | clean | a new deck: 11 nodes, 6 states, 6 beats in 4 sections | — | — | — |
| (b) | clean | 23 leaves: `data.revenue.source` and 22 pieces of copy (8 node texts, the chart's `alt`, 5 beat claims, 4 notes, `meta` ×2, and the 2 texts `close` holds) | `close` (its own text) | all 6, in text and `alt` only | kept |
| (c) | clean | 2 leaves: `headline.text`, one beat claim | none | `quarters` | kept |
| (d) | clean | a state added after `topline`, two props moved from `topline` into it, one beat's state list | `topline` | `topline`, plus the new state | kept |
| (e) | clean | 1 leaf: `theme` | none | none | kept |

Every theme name resolves in every snapshot. No snapshot lost a node, changed a node's type, or reordered states. To rerun the judge: `cargo build -p scaena-cli`, then `scripts/judge_edit.py docs/examples/authorability a b`, and likewise for `b c`, `c d`, and `d e`.

## (a) Create the deck

Six states over eleven nodes:

| State | Layout | Shows |
|---|---|---|
| `cover` | title | the mesh `bg`, `title`, `subtitle` |
| `topline` | stat | `q3-total` ($57.6M), what it means, the detail |
| `quarters` | full | `headline`, `rev-chart` (stacked bars by product), `chart-source` |
| `products` | — | a build on `quarters` (`slide: "quarters"`): a new headline, the bars regroup (`kind: bar`) |
| `mix` | stat | Pro's 34% beside the chart, restacked and moved aside |
| `close` | — | `from: "cover"`: the cover's nodes back, with the closing text |

One chart node runs through three states, keyed by product. The spine has six beats in four sections, one per state, each citing `@revenue`. The agent checked every figure in its copy against the CSV and caught a claim the data would not carry. Pro added the most revenue, but Enterprise grew faster, so the headline is "Pro added the most", not "grew fastest".

Its first draft validated and linted clean. `inspect` caught the only real problem. A state's `at: {in: "aside"}` on a chart that tracked `at: {col, row}` resolved to `{col, row, in}`, because object values merge. The agent restated the placement as grid cells. The engine would have placed the merged value in the slot, because `in` outranks `col`/`row`, but nothing it could read said so (finding 1).

## (b) Roll the chart forward

The roll-forward is one field: `data.revenue.source` now names `data/q4-revenue.csv`. The chart and every beat refer to the data by name, so nothing else had to be rebound.

The agent read "Q4 closed" as moving the whole deck's period, and that was the work. Every figure in the copy is a literal, and the new window made 22 pieces of copy stale. It recomputed each figure from the CSV, and they check out:
- Quarterly totals are 34.7, 43.3, 57.6, and 69.6.
- Quarter-over-quarter growth is 24.8%, 33.0%, and 20.8%.
- Pro's share went from 25.6% to 35.6% (×2.79); Enterprise grew ×3.20.

The new data also inverted a claim: "Growth accelerated every quarter" is false over the new window. The agent rewrote it as "Growth slowed in Q4" and flagged the change for sign-off. Request (c) overruled it.

- **Local.** Five state deltas are untouched. `close` changed only the two texts its own delta holds. Every state resolves differently, in text and `alt` only: the blast radius of a data change is the copy (finding 6).
- **Identity.** `revenue`, `rev-chart`, and every other id are kept. `q3-total` now holds the Q4 figure; the agent kept the id because ids are identity (finding 13).
- **Probes** on a throwaway copy:
  - These all validate and lint clean: a data file that does not exist, a chart field that is not in the data, and a CSV that does not match its `schema`.
  - Only an unknown `@name` fails, as E102.
  - E103, "chart field missing in data", is in the catalog but not implemented (finding 5).
- **Not done:** animating the roll. A state that switches the chart from the Q3 window to the Q4 window would scroll it by key, which is PLAN 0.10's data motion. The format can express it; the request did not ask for it.

## (c) Change the headline

Two fields:
- `nodes.headline.text`, the default that only `quarters` shows: `products` sets its own headline and `mix` removes it.
- The claim of the beat that headline expresses, now "Revenue grew every quarter from Q1 to Q4 2026: $34.7M, $43.3M, $57.6M, $69.6M."

All six state deltas are untouched, and only `quarters` resolves differently, in that one text. The agent kept the slowdown figures in the beat's notes on purpose: the deck stops headlining the slowdown without hiding it from the presenter.

Open:
- Do 26 characters fit a `headline` role whose `measure` is 24? Lint has no overflow rule yet, and nothing says whether `measure` breaks a line or only warns (finding 10).
- Nothing ties a beat's claim to the text of its claim node; the agent kept them in step by hand (finding 11).

## (d) Add a build

`topline` keeps the number alone. A new state after it, `topline-meaning` with `slide: "topline"`, brings in the meaning and the detail on the next click, and the beat `doubled` lists both states.

The number keeps its entrance on `topline`, and the new state gives the meaning its own (`rise`). The agent gave the build a transition, because a state without one cuts, and let the layout track. Five state deltas are untouched and resolve exactly as before. That includes `quarters`, which now follows the new state and still removes all three of the slide's nodes.

Open:
- The number-only click shows evidence with no claim. The reserved rule W421 would flag that, and it is exactly what was asked (finding 12).
- The value a build's `slide` takes came from the examples, not from SPEC (finding 3).
- `inspect` and `diff` show no transitions or choreography, although SPEC §7.1 lists a timeline under `inspect`. So the motion half of a build cannot be checked (finding 15).
- `timing: "with"` on a state that cuts is undefined (finding 14).

## (e) Retheme

One field: `theme` now names `themes/daybreak.theme.json`. All seven states resolve exactly as before: the theme changes what the names mean, not which names the deck uses.

`scaena theme --apply` exits 3 (PLAN 1.6), as the skill expects. The agent swapped the file by hand, then checked what lint cannot yet:
- Every theme name the deck uses resolves in Daybreak: 3 layouts, 8 slots, 6 roles, 3 presets, 2 durations, a palette, and a data scale.
- It computed contrast for every text pair by hand.

Its probes show why it had to. A theme whose ink equals its paper lints clean. So does a theme that fails its schema and lacks a layout, a palette, and a preset the deck uses. Neither command reads the theme (finding 5).

Two more things it noticed:
- It kept `bg.palette: "dusk"`, which in Daybreak names a light palette. Theme names are a contract between themes, and this one reads like the name of a skin (finding 17).
- The mesh's params are deck literals that no theme can retune, because a node cannot name the theme's `mesh-soft` preset (finding 9).

## Rendered, outside the criteria

PLAN 0.13 does not judge visual quality, but four of the seven states can render, so the judge rendered them. The theme's fonts, from google/fonts, went into a scratch copy of the bundle.

- **A clean deck that will not render.** `render` refused every state: a font family is "not in the bundle". The renderer requires a file for each of the theme's families, and the deck's `fonts` list two of the three.
  - Agent (a) left out the mono family, which the deck never uses, and agent (e) kept it out on purpose.
  - Nothing they could read says otherwise, and `validate` and `lint` do not check fonts (finding 5).
- **With JetBrains Mono added,** `cover`, `topline`, `topline-meaning`, and `close` render under both themes. The three chart states exit 3: stacked and multi-series charts are PLAN 1.9.
- **Contrast over the mesh, measured.** Agents (a) and (e) estimated the cover's worst case under Dusk at 2.3–2.5:1, assuming the mesh's `#FF6A3D` stop sits behind the text. The judge measured instead: it rendered the cover without its text and compared the ink against the mesh at every fully inked glyph pixel, at the cover's rest time.

  | Text | Dusk: min, median | Dusk: share of glyph pixels under 4.5:1 | Daybreak: min, median |
  |---|---|---|---|
  | title (display role) | 3.41, 5.34 | 41% | 9.78, 11.52 |
  | subtitle (title role) | 7.50, 12.60 | 0% | 12.36, 14.73 |

  - The blend never puts the pure stop under the title, so the cover passes the 3:1 display bar.
  - It passes by 0.4 at rest, and the mesh drifts as the deck plays.
  - That measurement is what E111 should make: against the shader's CPU reference, behind the glyphs, over the time the text is on screen (finding 16).

## Findings

**Fixed in this PR.** In each case SPEC was silent about, or contradicted the schema on, something the code already does. SPEC now says what the code does:

| # | Finding | Evidence | Fix |
|---|---|---|---|
| 1 | A delta replaces each property it names, but object values merge one level into the tracked value, and `null` deletes a key. SPEC did not say so. Moving a node from a slot to grid cells needs `in: null`, because `in` outranks `col`/`row`, and the hand-written schema rejects `null`. | (a) | SPEC §2.2. The schema learns `null` when it is generated from the types (PLAN 1.1). |
| 2 | One placement wins: `rect`, else `in`, else `col`/`row`. The built-in slots `canvas` and `grid` were documented nowhere; agent (a) found `canvas` in the example deck. | (a), judge | SPEC §3.4 |
| 3 | Three tracking rules SPEC did not state: a node that re-enters starts from its node defaults; a state without `slide` starts a slide named by its own id, which its builds name; ids are unique within each collection, not across them. | (a), (d) | SPEC §2.2, §3.2 |
| 4 | SPEC §3.7 called the data pipeline `transform`, which the schema names `dataTransform` (`transform` is geometry). The schema's chart-axes placeholder `axesSpec` was undocumented. | (a) | SPEC §3.7, §3.10 |
| 13 | Ids that name content go stale: `q3-total` now holds Q4's figure, and nothing renames an id while keeping its identity. | (b) | The `author-deck` skill: name ids for what a node is for. A rename that keeps identity is a semantic patch op (PLAN 1.16). |

The `author-deck` skill also says now that figures in copy are literals to re-derive when the data changes (finding 6), and the `retheme` skill that an empty lint delta proves nothing until PLAN 1.15.

PLAN 1.1 finished two of these. The schema, now generated from the typed model, accepts `null` in a delta (1), and charts take `axes` settings where `axesSpec` held the place (4). PLAN 1.2 finished most of open finding 5. `validate` reads the bundle: missing files, a theme that fails its schema or lacks a name the deck uses, and a theme family the deck's `fonts` omits are all errors now. What remains of it is E103 (PLAN 1.9), and whether fonts follow the theme (PLAN 1.4).

**Open**, each pointing at the task that owns it:

| # | Finding | Evidence | Proposal | PLAN |
|---|---|---|---|---|
| 5 | `validate` and `lint` read nothing in the bundle besides the deck. All of these pass: a missing data file, a chart field not in the data (E103 is catalogued, not implemented), a CSV that does not match its `schema`, a theme that fails its schema or lacks names the deck uses, and missing fonts. `render` requires a file for every theme family, so a clean deck cannot render. | (a), (b), (e), judge | `validate` reads the bundle: E102 for missing files and unknown theme names, E103 for fields and types. Fonts follow the theme, so a deck does not restate the theme's families. | 1.2, 1.4, 1.6 |
| 6 | Copy cannot reference data. Every figure in text, `alt`, claims, and notes is a literal, so a data update is a manual sweep (22 edits in (b)), and nothing notices a claim the new data makes false. | (b) | A text run that binds a value from data, in `dataTransform`'s expression language. SPEC §16 question 9 now records the proposal. | 1.9 |
| 7 | Mark identity in a multi-series chart is unspecified. The example deck sets `key: product` on a chart with `x: quarter` and `series: product`, where four marks share each product, and agent (a) copied it. | (a), (b) | `key` is the field, or list of fields, that identifies a mark, defaulting to x plus series. Two marks with one identity are an error, and the example deck drops its `key`. | 1.9 |
| 8 | Chart vocabulary the brief needed: share of total, labels on stacks, which category gets which color, units in `format`. `docs/spec/format.md` does not exist. | (a) | Part of the sprint's aesthetic. | 1.9 |
| 9 | A shader node cannot name a theme shader preset, so `mesh-soft`'s params were copied into the deck, where no theme can retune them. | (a), (e) | `preset` on shader nodes; params override it. | 1.10 |
| 10 | Is `measure` a break width or advisory? The theme schema says "max characters per line"; SPEC lists W201, "line exceeds role measure". | (c) | `measure` caps the line box, in the role font's `ch`; W201 fires only for runs that cannot break. | 1.8 |
| 11 | A beat's claim and its claim node's text can drift silently: W420 checks only that a claim node exists. | (c) | A narrative check judged by the user's model (BYOK, SPEC §11): does the slide say what its beat claims? | 1.15 |
| 12 | The reserved narrative rules would fire on sound patterns. W421 would flag a build that shows its evidence before its claim, which (d) was asked for. W422 would flag every stat layout, where the numeral is evidence and outranks its title-role claim. `takeaway` does not satisfy W420. | (a), (d) | Judge W421 and W422 per slide, at the build's last state; exempt stat numerals; count `takeaway` as a claim. | 1.15 |
| 14 | Motion semantics are unspecified: is a node whose entrance is `after` hidden until it runs? What does `with` mean on a state that cuts? | (a), (d) | On a cut, `with` and `after` coincide; a node waiting on `after` is hidden until its entrance runs. | 1.11 |
| 15 | The CLI cannot show motion or data. `inspect` shows no transition, choreography, or timeline (SPEC §7.1 lists the timeline), and not a chart's rows. | (b), (d) | `inspect --timeline` and `--data`. | 1.14 |
| 16 | E110/E111 are underspecified: which roles count as display (3:1) and which as body (4.5:1), and what the background is under text over a shader. | (e), judge | Each theme role declares its contrast class. Over a shader, measure against the CPU reference behind the glyphs, over the state's time on screen. | 1.15 |
| 17 | Theme names are the swap contract. Daybreak has to call its light palette `dusk` for a deck to swap themes, which invites an agent to "fix" it. | (e) | Name palettes, like slots and roles, for their job (`ambient`), not for a theme. | 1.6 |
| 18 | Notes live on beats and on states; SPEC does not say which a presenter reads. | (a) | A state's notes, else its beat's. | Phase 2 |

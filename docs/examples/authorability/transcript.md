# Authorability transcript

## (a) Create the deck

### Request (verbatim)

> "Here's our revenue by quarter and product (data/q3-revenue.csv). Q3 2026 revenue reached $57.6M, double Q4 2025's $28.7M, and Pro grew from a fifth of revenue to a third. Build a six-state deck for the board that shows the growth and the mix shift."

Constraints given: exactly six states, dusk theme, follow `skills/author-deck/SKILL.md`, `validate` and `lint` clean.

Read first: SPEC §0–§17 (focus on §2–§4 and §7), `deck.schema.json`, `theme.schema.json`, `docs/examples/revenue.deck.json`, both skills, `data/q3-revenue.csv`, and `themes/dusk.theme.json`.

### CLI commands

All commands were run from `docs/examples/authorability/` with `S=/home/user/Scaena/target/debug/scaena`.

| # | Command | Exit | Output |
|---|---|---|---|
| 1 | `scaena --help` | 0 | command list: validate, lint, inspect, diff work; compile/decompile/patch/theme/serve/mcp point at PLAN tasks |
| 2 | `scaena validate --help`, `lint --help`, `inspect --help`, `diff --help`, `render --help`, `export --help` | 0 each | usage only. `lint` has `--state`, `--severity`, `--fix` (PLAN 1.15). `inspect` without `--state` shows all states. |
| 3 | `scaena validate deck.json` (first draft) | 0 | `ok: no findings` |
| 4 | `scaena lint deck.json --json` | 0 | `[]` |
| 5 | `scaena validate .` | 0 | `ok: no findings` |
| 6 | `scaena lint .` | 0 | `ok: no findings` / `(layout-level rules — overflow, contrast, collisions — arrive with the engine: PLAN 1.15)` |
| 7 | `scaena lint deck.json --severity info` | 0 | same as 6 |
| 8 | `scaena inspect deck.json --state <id>` for all six states | 0 each | tracking as intended, except in `mix`: `rev-chart at={"col":[1,12],"row":[2,5],"in":"…` |
| 9 | `scaena inspect deck.json --state mix --json` | 0 | the chart's resolved `at` was `{"col":[1,12],"row":[2,5],"in":"aside"}`. The state delta was merged key by key into the tracked object, not replaced. **Fixed:** the delta became `at: {col:[8,12], row:[2,5]}`. |
| 10 | `scaena validate deck.json` | 0 | `ok: no findings` |
| 11 | `scaena lint deck.json --json` | 0 | `[]` |
| 12 | `scaena inspect deck.json --state mix` | 0 | `rev-chart kind=stackedBar … at={"col":[8,12],"row":[2,5]}`, + pro-share, + pro-share-meaning, exited headline, chart-source |
| 13 | `scaena inspect deck.json --state close --json` | 0 | layout `title` (tracked from `cover`). Entered bg, title (claim), subtitle (evidence). Exited rev-chart, pro-share, pro-share-meaning. |
| 14 | `scaena diff deck.json --from cover --to topline` | 0 | enter q3-total, q3-total-meaning, q3-total-detail; exit bg, title, subtitle |
| 15 | `scaena diff deck.json --from topline --to quarters` | 0 | enter headline, rev-chart, chart-source; exit q3-total, q3-total-meaning, q3-total-detail |
| 16 | `scaena diff deck.json --from quarters --to products` | 0 | `{"headline":{"change":{"text":"Pro added the most"}},"rev-chart":{"change":{"kind":"bar"}}}`: the build changes only these two |
| 17 | `scaena diff deck.json --from products --to mix` | 0 | rev-chart change `kind: stackedBar`, `at: {col:[8,12],row:[2,5]}`; enter pro-share, pro-share-meaning; exit headline, chart-source |
| 18 | `scaena diff deck.json --from mix --to close` | 0 | enter bg, title, subtitle; exit rev-chart, pro-share, pro-share-meaning |
| 19 | `scaena inspect deck.json` | 0 | all six states, slides: cover, topline, quarters (quarters + products), mix, close |
| 20 | `scaena inspect deck.json --json` (inside the copy-fit script below) | 0 | the snapshots the script read |
| 21 | **Final:** `scaena validate deck.json` / `--json` | 0 / 0 | `ok: no findings` / `[]` |
| 22 | **Final:** `scaena lint deck.json --json` / plain | 0 / 0 | `[]` / `ok: no findings` plus the PLAN 1.15 note |

`render` was not run, as instructed. The bundle has no font files.

Checks outside the scaena CLI (python3):
- Figures from the CSV:
  - Totals: $28.7M, $34.7M, $43.3M, $57.6M. Ratio 2.007. Change +$28.9M.
  - Quarter-over-quarter growth: +20.9%, +24.8%, +33.0%.
  - Pro share: 21.3%, 25.6%, 29.3%, 33.7%. Growth 3.18×, +$13.3M.
  - Enterprise share: 15.3% to 24.8%. Growth 3.25×, +$9.9M.
  - Core share: 63.4% to 41.5%. Growth 1.31×, +$5.7M.
  - Every product grew in every quarter.
- `jsonschema` (Draft 2020-12): `deck.json` is valid against `deck.schema.json`, and `themes/dusk.theme.json` is valid against `theme.schema.json`.
- Script checks: no id is used in more than one collection (nodes, states, sections, beats, data), and each state is in exactly one beat.
- Copy fit by character count against role `measure` and `maxLines` (greedy): every text is within its limits. Words per state: 9, 19, 9, 11, 13, 16 (limit 40).

### Decisions

- Bundle: `theme: themes/dusk.theme.json`; canvas 1920×1080 cu; no `formats` (a 16:9 board deck only); `meta` has title, lang, and description, with no invented author or date.
- `fonts`: Fraunces and Inter, with files and axes copied from the theme's `display` and `body` families (as in the example deck). The mono family is unused.
- Data: id `revenue` → `data/q3-revenue.csv`. Schema: quarter and product are `string`; revenue and customers are `number`.
- Spine (one beat per state, 6 beats in 4 sections):
  - opening/`thesis` [cover]
  - growth/`doubled` [topline]
  - growth/`accelerated` [quarters]
  - mix-shift/`pro-led` [products]
  - mix-shift/`pro-third` [mix]
  - closing/`takeaway` [close]
  - Every beat has `evidence: ["@revenue"]`, and its notes carry the derived figures.
- Ids are globally unique across nodes, states, sections, beats, and data.
- States and layouts:
  - `cover`: title layout
  - `topline`: stat layout
  - `quarters`: full layout
  - `products`: build on `quarters` (`slide: "quarters"`, layout tracked)
  - `mix`: stat layout
  - `close`: `from: "cover"`, with the layout and nodes tracked from the cover
- Nodes:
  - `bg`: shader/mesh
  - `title`, `subtitle`: text
  - `q3-total`, `q3-total-meaning`, `q3-total-detail`: text
  - `headline`: text
  - `rev-chart`: chart
  - `chart-source`: text
  - `pro-share`, `pro-share-meaning`: text
- Text placement uses slots: title/subtitle, number/meaning/aside, header/footer. Each text node sets its role explicitly, matching the slot's default role.
- Chart: one persistent node, `rev-chart`, keeps its identity through three states. Its kind goes `stackedBar` (quarters) → `bar` (products: the bars regroup) → `stackedBar` (mix: it restacks and moves aside).
- Chart encodings:
  - `x`: quarter, ordinal
  - `y`: revenue, quantitative; `format: "$,.1f"`; `domain: [0, null]`; `title: "Revenue ($M)"`
  - `series` and `color`: product, with the `categorical` scale
  - `labels.show: ends`; `legend: top`
  - `alt` gives each product's figures, start to end
- Chart key: `key: "product"`, following the example deck. Marks are presumably identified by (quarter, product).
- Chart placement uses grid cells, `col [1,12] row [2,5]`, not `in: main`. In the theme, `main` spans rows 1–6, which would overlap the `header` (row 1) and `footer` (row 6) slots.
- In `mix` the chart goes to `col [8,12] row [2,5]`, the theme's `stat.aside` geometry. A grid range, not `in: aside`, because of the merge finding (finding 1).
- The `headline` node persists across the build: "Growth accelerated" becomes "Pro added the most". The `chart-source` caption persists, unchanged.
- The mix shift is shown with absolute data (Pro's band in the stacked bars, plus the "34%" stat), not a 100%-stacked chart (finding 2).
- Transitions:
  - `cover`: none (the first state cuts)
  - `topline`, `quarters`, `close`: `standard`
  - `products`, `mix`: `slow` (the bars regroup, then the chart moves and restacks)
- Choreography gives one entrance per new focal object:
  - `cover`: `words` on title (`with`), then `rise` on subtitle with delay 240
  - `topline`: `rise` on q3-total
  - `quarters`: `grow` on rev-chart (`after`)
  - `mix`: `rise` on pro-share
  - None on `products` (the morph is the motion) or on `close`.
- No `hold`: the deck is presented live to a board, not exported to video. The skill makes `hold` conditional on video.
- `bg` (mesh, seed 7, palette `dusk`, the params of the theme's `mesh-soft` preset, z −100, `alt: ""`, `decoration`) appears on cover and close only. It is removed for the data states for legibility; `close` gets it back through `from: "cover"`.
- Semantics: exactly one `claim` node per state (title, q3-total-meaning, headline ×2, pro-share-meaning, title). Numbers and the chart are `evidence`, the caption is `source`, the subtitle is `context` on the cover and `evidence` on the close, and `bg` is `decoration`.
- Speaker notes live on beats only, not on states, to avoid duplicating them.
- Copy was checked against the data:
  - "Pro added the most" (in dollars), not "grew fastest": Enterprise grew 3.25× against Pro's 3.18×.
  - "In three quarters": Q4 2025 → Q3 2026 is three quarter-steps, so not "year over year" as in the example deck.
  - "Growth accelerated": quarter-over-quarter growth was +21%, +25%, +33%.
- Typographic apostrophes (’) are used in copy.
- Skill step 1 ("ask one question if the brief is thin") was not needed; the brief plus the CSV were enough. Skill step 7 (render) was skipped as instructed; `inspect` and `diff` were used instead.

### Could not express, or ambiguous

1. **Object-valued props merge across states.** A state delta `at: {in: "aside"}` on a node tracked with `at: {col, row}` resolved to `{col, row, in}`, and validate and lint both passed it. The schema has no way to unset a tracked key (`null` is not a valid `Range`), so a node can't switch between grid-cell and slot placement without restating grid cells (or using `mode: absolute`). SPEC §2.2 says unchanged *properties* carry forward. It does not say that object values merge, or which of `in` and `col`/`row` wins when both are present.
2. **No share-of-total chart from the CSV.**
   - There is no normalize / 100%-stack option.
   - The schema has `dataTransform`, but the derive/aggregate expression language is unspecified (Phase 1).
   - A hand-computed inline share dataset would go stale when the CSV changes, so I didn't add one.
   - SPEC §3.7 calls the pipeline `transform`. In the schema, `transform` is the geometric Transform, and the pipeline is `dataTransform`.
3. **Multi-series `key`.** The SPEC and the example use `key: "product"` with `x: quarter`, so four marks share each key value. Mark identity is presumably (x, key) but is not stated, and `key` takes a single field.
4. **`labels.show: "ends"` on stacked or grouped bars.** Unspecified whether the label is per segment or per stack total.
5. **Category → color order is unspecified.** There is no way to give Pro the accent color, and `Encoding.domain` holds only 2 items. The example deck's notes call Pro "the orange band", although Pro is not the first product in the data.
6. **Chart axes.** SPEC §3.7 says charts can't declare axes settings until PLAN 1.1. The schema's `axesSpec` has no documented meaning, so I left the defaults.
7. **`format` can't append a unit** (`$57.6M`), and `docs/spec/format.md` doesn't exist. I used `$,.1f`, plus the y title "Revenue ($M)" and the caption.
8. **Theme shader presets can't be referenced.** A node has no field to name a theme shader preset (`mesh-soft`), so I copied its params.
9. **Id uniqueness scope.** SPEC §3.2 says ids are unique "across `nodes`, across `states`, and across spine `beats`". That could mean within each collection or across all of them, and sections and data ids aren't mentioned. I kept every id globally unique.
10. **`slide`.** The example sets `slide: <first state id>` only on the follow-on build states. Not stated: whether the first state should carry it too, and whether slide ids share the state namespace. I followed the example; inspect shows `slide=quarters` for both states.
11. **Re-entering nodes.** Not stated whether a removed node resumes its last tracked props or its node defaults when it re-enters. I avoided the question with `from: "cover"` on `close`; inspect confirms the cover's props.
12. **Where speaker notes go.** Notes can live on beats or on states, and the docs don't say which one a presenter view reads.
13. **W422 vs. the stat layout.** The reserved rule W422 (evidence outranks the claim by role size) conflicts with the `stat` layout: a numeral `evidence` above a `title`-role `claim` would trip it. The rule isn't implemented yet.
14. **`takeaway` semantic vs. the claim rule.** The close's title is semantically a `takeaway`, but it is marked `claim` to satisfy the skill rule that each beat's states include a claim node.
15. **What "clean" covers.** `validate` and `lint` don't check that the font files listed in `fonts` exist (the bundle has none, and both are clean). Lint has no layout-level rules yet (overflow, contrast, collisions: PLAN 1.15). Copy fit, slot overlaps, and contrast over the mesh are therefore unverified; ink over the mesh's `#FF6A3D` stop is about 2.5:1 by hand calculation.
16. **Choreography timing.** Unspecified whether a node with an `after` choreography `enter` stays hidden until it runs, or fades in with the transition first.

## (b) Roll the chart forward

**Request (verbatim):** "Q4 closed. Here's the new data, data/q4-revenue.csv (2026-Q1 through 2026-Q4). Roll the deck's chart forward to it."

**Read:** SPEC §2–§4 and §7; deck.schema.json and theme.schema.json; docs/examples/revenue.deck.json and themes/; skills author-deck and retheme; CLI help; bundle deck.json, data/*.csv, themes/dusk.theme.json.

**Commands** (cwd = bundle; `scaena` = target/debug/scaena):
- `scaena --help`, `scaena {validate,lint,inspect,diff,render,export,patch} --help` → 0 each.
- Baseline: `scaena validate .` → 0, `ok: no findings`. `scaena lint .` → 0, `ok: no findings` + `(layout-level rules — overflow, contrast, collisions — arrive with the engine: PLAN 1.15)`. `scaena lint . --json` → 0, `[]`.
- `scaena inspect . --state quarters` → 0; `scaena inspect . --state mix --json` → 0 (resolved props; shows `data: "@revenue"` but no data rows); `scaena inspect .` → 0, all 6 states.
- After repointing the data source only: `scaena validate .` → 0, `ok: no findings`; `scaena lint . --json` → 0, `[]`.
- Probes on a throwaway copy outside the bundle: source → `data/missing.csv`: validate 0, lint 0, no findings. `rev-chart.y.field` → `revenu`: validate 0, lint 0, no findings. Schema key `customers` → `region`: validate 0, lint 0, no findings. `rev-chart.data` → `@revenu`: validate 1 and `lint --json` 1, `E102 /nodes/rev-chart/data: node `rev-chart` references unknown data source `@revenu``.
- Final: `scaena validate .` → 0, `ok: no findings`. `scaena lint .` → 0, `ok: no findings` (+ PLAN 1.15 note). `scaena lint . --json` → 0, `[]`. `scaena inspect .` → 0, new copy in every state, tracking as before. `scaena diff . --from quarters --to products` → 0, only `headline.text` and `rev-chart.kind`. `scaena diff . --from products --to mix` → 0, rev-chart kind + at, pro-share and pro-share-meaning enter, headline and chart-source exit (same shape as before).
- Not run: `render` (no fonts in bundle). Also hand checks in python, not the CLI: CSV header = schema, chart fields present, numbers parse, (quarter, product) unique; every figure in the new copy recomputed from the CSV (all pass).

**Decisions:**
- Rolled forward by repointing `data.revenue.source` to `data/q4-revenue.csv` (same columns and types). I kept the name `revenue`, so `rev-chart` (`@revenue`) and every beat's `@revenue` evidence follow with no rebinding.
- Chart spec unchanged (kind, encodings, `key`, format, domain). I rewrote `rev-chart.alt`, which listed the old values.
- Left `data/q3-revenue.csv` in the bundle, now unreferenced. I did not delete it.
- Scope: I read "Q4 closed" as rolling the deck's period forward, not only the chart. Every beat cites `@revenue`, so I re-checked each claim against the new window (2026-Q1..Q4) and updated every data-derived figure: meta title and description, 5 beat claims, 4 beat notes, 8 node texts, and 2 `close` state props. A chart-only roll would put a Q1–Q4 2026 chart under Q3-2026 / Q4-2025 copy.
- "Growth accelerated every quarter" is false on the new data (q/q +25%, +33%, +21%). The beat claim is now "Growth slowed in Q4, to 21% from 33% in Q3." and `headline` is now "Growth slowed in Q4". This changes the narrative and needs the author's sign-off.
- "Pro: a fifth → a third" became "a quarter → over a third" (25.6% → 35.6%). The thesis, takeaway, and close title follow.
- Notes said "Pro and Enterprise both more than tripled", now false for Pro (2.8x). They now say Enterprise more than tripled and Pro nearly tripled.
- Still true, wording kept: "Pro added the most" (+$15.9M vs +$13.2M vs +$5.8M), "every product grew every quarter", "doubled" (2.01x; the window starts at Q1 2026 instead of Q4 2025).
- Kept every id: `q3-total*` now hold Q4 figures, and beat `accelerated` now claims growth slowed. Ids are identity (SPEC §1.2), and renaming them would break continuity with history/a.
- Layout lints are not implemented, so I checked line counts against role measure/maxLines by hand. All are unchanged except the close title, 2 → 3 lines at measure 16, still within display `maxLines: 3`.
- No new state animating the window from Q3 to Q4 (2025-Q4 exits, 2026-Q4 grows in); it was not asked for.

**Could not express / ambiguous:**
- Copy cannot reference data: every figure in text, alt, and notes is a literal copy, so a data roll-forward is a manual sweep, and nothing flags a claim that the new data falsifies (lint stays clean with "Growth accelerated" over decelerating data).
- validate/lint do not check that the data file exists, that the CSV header matches `schema`, or that chart fields exist in the data. SPEC §7.5 lists E103, but it did not fire. Only an unknown `@name` is caught (E102).
- `inspect` shows a chart's data ref, not its resolved rows, so the CLI cannot confirm what the chart will draw.
- `key: "product"` with `x: quarter` and `series: product`: SPEC §3.7 does not say whether mark identity is the key alone or (x, key). It matters for any state that changes data (a scrolling window); not for this edit.
- "Roll the deck's chart forward" leaves open whether the copy should follow. I resolved it as above.
- Ids that encode a period (`q3-total`) go stale on a roll-forward. The format has no rename or alias that keeps identity.

## (c) Change the headline

**Request (verbatim):** "Don't headline the slowdown. Change the headline “Growth slowed in Q4” to “Revenue grew every quarter”."

**Commands** (cwd = the bundle; `scaena` = `/home/user/Scaena/target/debug/scaena`)

Baseline, before editing:
- `scaena validate deck.json` → exit 0: `ok: no findings`
- `scaena lint deck.json --json` → exit 0: `[]`
- `scaena lint deck.json` → exit 0: `ok: no findings` / `(layout-level rules — overflow, contrast, collisions — arrive with the engine: PLAN 1.15)`
- `scaena validate .` → exit 0: `ok: no findings`
- `scaena inspect deck.json --state quarters` → exit 0: `+ headline role=headline text=Growth slowed in Q4 at={"in":"header"}`, plus `+ rev-chart` (stackedBar), `+ chart-source`, `- exited: q3-total, q3-total-meaning, q3-total-detail`
- `scaena inspect deck.json --state products` → exit 0: `headline role=headline text=Pro added the most` (the state sets its own text)
- Not CLI: grep of deck.json for slow|21%|33%|accelerat. The slowdown appears in three places: the `headline` node text, beat `accelerated`'s claim, and that beat's notes. The `"slow"` hits are transition durations. Recomputed totals from data/q4-revenue.csv with python: 34.7, 43.3 (+8.6, 24.8%), 57.6 (+14.3, 33.0%), 69.6 (+12.0, 20.8%). Core, Pro, and Enterprise each rise every quarter, so the new headline is true.

After editing:
- `scaena validate deck.json` → exit 0: `ok: no findings`; `scaena validate .` → exit 0: `ok: no findings`
- `scaena lint deck.json --json` → exit 0: `[]`; `scaena lint deck.json` → exit 0: `ok: no findings`, with the same PLAN 1.15 note
- `scaena inspect deck.json --state quarters` → exit 0: `+ headline role=headline text=Revenue grew every quarter at={"in":"header"}`; the other lines are unchanged
- `scaena inspect deck.json --state products` → exit 0: headline still `Pro added the most`
- `scaena diff deck.json --from topline --to quarters` → exit 0: headline `enter` with text "Revenue grew every quarter" and semantic claim; rev-chart and chart-source enter; q3-total* exit
- `scaena diff deck.json --from quarters --to products` → exit 0: only `headline.change.text = "Pro added the most"` and `rev-chart.change.kind = "bar"`
- `scaena diff deck.json --from products --to mix` → exit 0: headline and chart-source exit; pro-share and pro-share-meaning enter; rev-chart changes to stackedBar at col 8–12
- `scaena inspect deck.json` (all states) → exit 0: the headline shows only in quarters ("Revenue grew every quarter") and products ("Pro added the most"), and exits in mix. "slowed" appears in no state.

**Decisions**
- Changed `nodes.headline.text` (the node's default text), not a `quarters` prop. `quarters` gets its text by tracking from the node, so the default is the only source. Overriding it in the state would leave "Growth slowed in Q4" behind as a dead default, and as the node's default alt.
- Left `products` (`headline.text` "Pro added the most") and `mix` (removes the headline) untouched. The diff confirms the build changes only text and chart kind.
- Rewrote the claim of beat `growth/accelerated` to "Revenue grew every quarter from Q1 to Q4 2026: $34.7M, $43.3M, $57.6M, $69.6M." The beat's claim is its headline in the spine, and the spine projects to every other format. The author-deck rule requires a `semantic: claim` node in the beat's states that expresses the claim, and the `headline` node is that node.
- The claim lists the quarterly totals, not the Q1→Q4 endpoints, so it does not repeat the `doubled` beat ($34.7M → $69.6M; cf. W425).
- Scoped the claim to "Q1 to Q4 2026" because @revenue (q4-revenue.csv) starts at Q1 2026. It cannot show Q1's growth over Q4 2025.
- Kept the beat id `accelerated`, because ids are stable identity (SPEC §1.2), although the id describes neither the old claim nor the new one.
- Left the beat notes unchanged. Speaker notes are not a headline, and they keep the deceleration (25% → 33% → 21%; Q4 +$12.0M vs Q3 +$14.3M) available to the presenter. The deck stops headlining the slowdown without hiding it.
- Added no `fit`, `measure`, or overrides, and no nodes, states, or choreography.

**Could not express / ambiguous**
- Fit is unverified. The new headline is 26 characters. The `headline` role has `measure: 24` (maxLines 3, wrap balance, minSize 56), and the `header` slot is one grid row (col 1–8, row 1). The text probably sets on two lines, where the old 19-character text took one. Lint has no overflow or collision rules yet (PLAN 1.15), and render is out of scope (no fonts). So I cannot rule out E100 in the header slot, or a collision with the chart's top legend in row 2.
- The docs don't say whether `measure` forces a break (wrap at the lesser of slot width and measure) or is advisory (W201 "line exceeds role measure" is only a warning). I cannot predict whether the result is one line with W201 or two lines.
- Nothing links a beat's claim to the text of the node that expresses it. The reserved W420 only checks that some `semantic: claim` node exists, so spine and slide can drift apart silently. I kept them in step by hand.

## (d) Add a build

**Request (verbatim):** "On the $69.6M slide, show the number by itself first, then bring in what it means on the next click."

**Commands** (cwd = bundle; `scaena` = `/home/user/Scaena/target/debug/scaena`)
1. `scaena --help`, `scaena validate|lint|inspect|diff --help` → exit 0 (usage text).
2. `scaena validate deck.json` (baseline) → exit 0, `ok: no findings`
3. `scaena lint deck.json --json` (baseline) → exit 0, `[]`
4. `scaena inspect deck.json --state topline` → exit 0. `state topline slide=topline layout=stat`; enters `q3-total` ($69.6M, `in:number`), `q3-total-meaning` (`in:meaning`), `q3-total-detail` (`in:aside`); exited bg, title, subtitle.
5. `scaena inspect deck.json` → exit 0. Six states: cover, topline, quarters, products, mix, close.
6. *(edit deck.json)*
7. `scaena validate deck.json` → exit 0, `ok: no findings`
8. `scaena lint deck.json --json` → exit 0, `[]`
9. `scaena lint deck.json` → exit 0, `ok: no findings` / `(layout-level rules — overflow, contrast, collisions — arrive with the engine: PLAN 1.15)`
10. `scaena inspect deck.json --state topline` → exit 0. `+ q3-total role=numeral text=$69.6M at={"in":"number"}`, `- exited: bg, title, subtitle`
11. `scaena inspect deck.json --state topline-meaning` → exit 0. `slide=topline layout=stat`; `q3-total` tracked unchanged; `+ q3-total-meaning`, `+ q3-total-detail`.
12. `scaena inspect deck.json --state quarters` → exit 0. Same as before: enters headline, rev-chart, chart-source; exits the three q3-total nodes.
13. `scaena diff deck.json --from topline --to topline-meaning` → exit 0. Only `enter` for q3-total-meaning and q3-total-detail; no entry for q3-total (it neither moves nor changes).
14. `scaena diff deck.json --from topline-meaning --to quarters` → exit 0. Enters headline, rev-chart, chart-source; `exit: true` for q3-total, q3-total-meaning, q3-total-detail.
15. `scaena diff deck.json --from cover --to topline` → exit 0. Enter q3-total; exit bg, title, subtitle.
16. `scaena inspect deck.json --state topline-meaning --json` → exit 0. Fields: state_id, slide_id `topline`, layout `stat`, nodes, entered `[q3-total-meaning, q3-total-detail]`, exited `[]`. No transition, choreography, or timeline.

**Decisions**
- "The $69.6M slide" = state `topline` (layout `stat`), whose numeral node `q3-total` reads "$69.6M".
- `topline` is now the first click: `props` holds only `q3-total`; its `remove`, `transition: "standard"`, and the number's `rise` choreography are unchanged.
- New state `topline-meaning` right after it, with `slide: "topline"` so both are one slide (same pattern as `products` → `quarters`; inspect confirms `slide=topline`).
- No `layout` on the build; it tracks `stat` from `topline` (SPEC §2.2).
- The build brings in `q3-total-meaning` and `q3-total-detail` together. The aside can't be on a "number by itself" click, and the request names only two clicks, so I didn't drop it or add a third click.
- `transition: "standard"` on the build, because a state without a transition cuts (SPEC §3.9); 420 ms matches the slide's entrance and the `rise` preset.
- One choreography item: `rise`, timing `with`, on `q3-total-meaning`. This matches the author's pattern of one enter for each cue's main node; the detail enters with the transition, as it did before.
- The number stays in slot `number` in both states, so it doesn't move when the meaning arrives. I didn't center it for the solo click.
- `semantic` labels are unchanged (q3-total = evidence, q3-total-meaning = claim, q3-total-detail = evidence). I didn't relabel them per state to satisfy lint.
- Spine beat `doubled`: `states` → `["topline", "topline-meaning"]`, which avoids W401.
- The id `topline-meaning` collides with no node, state, beat, or section id.
- No `hold`: no state in the deck has one, so it isn't set up for video.
- `quarters` is unchanged; its `remove` of the three q3-total nodes is still correct after the build.

**Ambiguous / not expressible**
- Does "what it means" include the aside (`q3-total-detail`) or only the `meaning` slot? It's the user's call; I put both on click 2.
- The number-only state shows `evidence` with no `claim` node. Reserved W421 ("evidence shown with no claim in the same state") would fire there. Lint is clean, so W421 isn't implemented yet. The author-deck rule "Every beat's states include a node with `semantic: claim`" holds if read per beat (true via topline-meaning). Read per state, it would forbid what was asked. The docs don't settle which reading is meant.
- SPEC §2.2 says only that `slide` groups consecutive states. That a build uses the first state's id as the value comes from the examples (revenue.deck.json `mix`, this deck's `products`).
- The CLI can't check the build's motion. `inspect` and `diff` show no transition, choreography, or timeline in text or `--json`, though SPEC §7.1 lists "timeline" under inspect. The transition and the `rise` item are checked by the schema alone. Per SPEC §3.9, Phase 0 doesn't run presets or choreography (PLAN 1.11–1.12), so today the meaning and detail simply fade in over 420 ms.
- The docs don't say what choreography `timing: "with"` means on a state that cuts (no transition). I avoided the question by giving the build a transition.

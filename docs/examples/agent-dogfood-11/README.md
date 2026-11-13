# Agent dogfood 11: a technical talk with code and a chart from a CSV

An agent with the MCP server alone, on main at 371e39a89, made a 15-minute talk for engineers: how a platform team cut median CI time from 41 to 18 minutes. It has two code slides (YAML and TypeScript) and two line charts drawn from an attached CSV, with figures quoted from it. It took the path the server's instructions name second:

1. `deck_create` with the CSV.
2. The `brief` op with `minutes: 15` and no length.
3. `spine_update` with six beats planned.
4. `slide_write` by beat, for eight slides.
5. `deck_health`, then its fixes by `deck_patch`.

Lint found nothing at any step. Health went from 73 to 83, after the arc fix it offered and a length set in the brief. `ci_minutes.csv` and `spine.json` beside this file are the material, and `calls.jsonl` is every call with its result.

## Where the tools steered it wrong, or took an extra call

| # | What happened | Cost | Status |
|---|---|---|---|
| F1 | `Big Number` with three figure-led items became a three-step bullets slide, and `fell` said nothing: `fell_back` reports an unknown name, a chart without `chartData`, and a missing picture, not a layout too small for its items. | A render to see it | Fix PR |
| F2 | `chartData` written as Chart.js writes it (`labels`, `datasets`) fell to bullets, said as "it is a chart but gives no `chartData` to draw". It gave one, in another shape. | A read of `ferry.generate.json` to find `categories` and `series[{ name, values }]` | Fix PR |
| F3 | A YAML step written as one multi-line item, `- uses: actions/cache@v4 …`, came out as `uses: …`: `item()` takes a model's bullet mark off every item, code too, so the slide showed YAML that no longer parsed. | Unseen until rendered | Fixed in this PR: an item on lines of its own is kept as written, with no label cut from it |
| F4 | A line chart of two series bound to the CSV ("Median" to `median_minutes`), but its highlight kept `at.series: "Median"`: E103, and `slide_write` refused the slide. The agent's way round, naming the series after the columns, drew raw column names in the legend and lit no series. | A refusal, a retry, and a worse slide | Fix PR |
| H1 | Pacing with `minutes: 15` and no length said "a talk with no length asked for is paced for about 18", while quoting "its brief asks 15 minutes". `deck_generate` takes a length from minutes; the `brief` op and health do not. | An extra `brief` call to set a length | For the coordinator: moves the score |
| H2 | Health read code as points. The TypeScript slide's ten lines were `dense` ("10 points in one place"), and both code slides were the `thin` pair in the same report. Neither ask can be acted on: a code block is not a list. | Findings with no right answer | For the coordinator: moves balance |

## Minor

- `spine_read` seemed to show the cover's notes twice, on its beat and on its state. They are kept once, on the beat; `spine_read`'s `states` list each state's notes as a presenter reads them, the beat's where the state has none. Pacing hears them once (`health::talk` reads each state's through `spine::notes`, each once).
- A code slide's subtitle, the path of the file its code is from, is set nowhere and is not listed in `unset`.
- `scripts/split-tests.ts` breaks at its own hyphen ("split-" / "tests.ts"), which reads as a hyphen put in. A path is not yet set as an address is.
- `deck_create`'s `data` given as a list of file names says "expected struct Attach", not the `{ id, file }` it expects.

## What worked

- `spine_update` then `slide_write` with `beat` kept every planned claim, and W429's hint named the call that tells a beat.
- Code asked for as `Code` took Title and Code, its language found by the file its subtitle names, and it rendered with its colors on its card.
- The single-series line chart bound to the CSV by its `sourceId`. Its headline's figures and its callout quote the cells, and its source line names the file.
- Health's arc finding carried a fix that wrote the plan its words carry, and it applied as a patch.

---
name: chart-from-data
description: Turn a data file into a Scaena chart or table that makes one point. Attach the data, choose the kind, shape the rows, place it in a figure, and check it with lint and a render. Works through the `scaena` CLI or its MCP server. Use when asked to "chart this", "visualize this CSV", "add a chart", "show the numbers", or "roll the chart to next quarter".
---

# chart-from-data

A chart is data bound to marks by key (SPEC §3.7). The theme draws it, and you choose what it shows.

The defaults draw as Tufte would, so leave them alone unless the slide needs something else:
- values on the marks;
- series named where they end;
- square, half-width bars;
- no frames, and at most five reference lines;
- the data in the theme's quiet palette, and one signal color for what the slide is about, which a `highlight` spends.

| To | CLI | MCP |
|---|---|---|
| attach a file | copy it to `data/`, then declare it: `bind_data` with a `source` as you bind the chart, or a JSON Patch `add` at `/data/<id>` | `data_attach`, with `id`, `file`, and optionally `schema` and `parse` |
| see the rows a chart reads | `scaena inspect <bundle> --data` | `deck_inspect` with `data` |
| see a source's rows as written, or fix a value, add a row, take one away | `scaena data <bundle> <id>`, then `--edits edits.json --dry-run`: one write of the file, every other byte kept | `data_edit`, with no `edits` to read, then with them and `dry_run` first |
| add or change the chart | `scaena patch <bundle> --ops ops.json --dry-run`, then without `--dry-run` | `deck_patch`, with `dry_run` first |
| check | `scaena lint <bundle> --json` | `deck_lint` |
| look | `scaena render <bundle> --state <id> --out chart.png` | `deck_render` |

## Procedure

1. **Read the data: columns, types, rows.**
   - Dates in ISO 8601 read as they are. Any other date needs a `parse` format for its column (`"parse": { "month": "%b %Y" }`, `docs/spec/format.md`).
   - Periods without a year (`Jan`, `Q3`) are not dates: keep them as text on an `ordinal` axis, in the data's order. A `parse` format with no year reads every date in 1900.
   - A value that does not fit its column's type is E103 when the deck validates, before any render. `scaena data` (`data_edit`) lists each such cell with why, and a `set` of it fixes it in the file.
2. **Say the one thing.**
   - Write the point the chart supports as a headline sentence first ("June to September beat the plan").
   - The headline is the claim, and the chart is its evidence (`semantic: evidence`).
3. **Choose the kind by the question:**
   - Comparing categories: `bar`, grouped by `series` when there are several.
   - Change over time: `line`, one or more series, named at their ends.
   - Parts of a whole over time: `stackedBar`, or `area` when time is continuous.
   - A relationship between two measures: `scatter` (`x.type: quantitative`).
   - A few values to compare closely: `dot`.
   - Shares of one whole in a few parts: `donut`. Prefer a bar when the parts are many or close.
   - Exact values to read: a `table`, its `columns` each with `field`, `title`, `format`, and `align`.
4. **Shape the rows with `dataTransform`** (SPEC §3.10; expressions in `docs/spec/expr.md`).
   - The steps are `filter`, `derive`, `sort`, `limit`, `aggregate` with `groupby`, `fold` (wide to long), and `pivot` (long to wide).
   - A wide table, a column per series, folds to long before a chart can take `series`.
   - Check the result with `inspect --data`.
5. **Encode.**
   - `x: { field, type }`: `ordinal` or `nominal` for categories, `temporal` for dates, `quantitative` for numbers.
   - `y: { field, format }` and `series: { field }`.
   - Format every number the audience reads: `$,.1f`, `,d`, `.0%`, or `$.2~k` (compact).
   - A line's or a dot plot's value axis spans its data. When the point is the distance from zero, start it there with `y: { "domain": [0, null] }`; bars and areas always do.
   - Set `key` only when x and series do not tell the rows apart. A key that repeats is E103.
   - A forecast or an estimate in a line or an area is `projected` (SPEC §3.7): `{ "field": "estimate" }` for a boolean column, or `{ "field": "kind", "value": "forecast" }`. The line runs dashed from the last actual point, the area lightens, and the end value says `est.`. Never draw a forecast as an actual: a reader takes a solid line as fact.
6. **Place it** in a `figure` layout's `main` slot (`narrow-figure` for two or three bars), or the theme's equivalent.
   - A chart is a slide of its own: `add_state` with `"layout": "figure"`, then `add_node` with that `state` (ops: SPEC §7.3, `docs/schema/patch.schema.json`).
   - A new state tracks the one before it, so the last slide's nodes stay on screen. List them in the state's `remove`, or give the state `"mode": "absolute"`.
   - Put the headline in `header` and the source in `note`, as `semantic: source`.
   - Give the chart `alt`: what a listener needs to hear, with the numbers that matter.
7. **Point at the answer with `annotations`**, only when the headline needs help:
   - `rule` at a `y` (a target, a plan) or an `x`. Its `text` says the value and what it means (`"Target: $20"`): a chart that prints its values has no axis to read the rule's value off.
   - `band` across x or y (a period, a range).
   - `callout`, with `text` at one x and, if needed, a series.
   - `highlight` of an x or a series: it takes the signal color, and the rest dims. In a theme whose data palette is quiet, as Dusk's is, it is how a chart shows which series or category the headline is about.
   - A line prints only its first and last values. A point about a middle one, such as a peak, needs a `callout` there, or `labels: { "show": "all" }`.
   - A callout's `text` is a literal, like any figure in copy: re-derive it when the data changes.
8. **Check and look.**
   - Lint for E103 (fields, types, keys), W310 (labels that collide or cover a mark, when you set `labels.show`), and E100 and E101 (the chart's cell).
   - Chart text is judged like any text. E110 and E111 read each label against the marks, rules, and bands under it, at the opacity a highlight dims it to; a finding names the label (`measure.part`, `measure.label`). A label over a mark usually means the chart is too tight for its values: give it a larger cell, or show fewer values.
   - W312 flags chart text under 12 pt at presentation size (24 cu on a 1920 × 1080 canvas). The size is the theme's: its `charts` roles, or the chart's own `labels.role` for its values.
   - W313 flags a chart squashed below a legible plot: under 120 cu across or down for its marks once its labels, axes, and legend have their room, as a one-row slot leaves it. Give it more rows of the grid, or put less beside it.
   - Render, and read it as the audience will: is the point visible in two seconds?
9. **Move the data, not the chart.**
   - For "next quarter" or "after the change", add a state that changes the chart's `data` or `dataTransform` (a new `filter`), not a new chart.
   - Marks match by key and move. A new key grows in from 0, and a removed one shrinks to 0.
   - Re-derive every figure in the copy from the new data.

## Rules

- No style fields on the chart. Colors, strokes, sizes, and label roles are the theme's.
- Leave `axes` unset where every mark prints its value: an axis beside labeled bars says each number twice.
- Leave `labels` and `legend` unset unless the slide needs otherwise. When it does:
  - `labels: { show: "all" | "ends" | "none" }`;
  - `legend: "direct" | "top" | "bottom" | "right" | "none"`.
- Every number the audience reads has a format.
- One chart, one point. Two points need two states.
- A chart's id names what it is for (`revenue`), not its period (`q3-revenue`): the chart persists while its data moves.

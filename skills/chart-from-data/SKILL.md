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
- no frames.

| To | CLI | MCP |
|---|---|---|
| attach a file | copy it to `data/` and declare it under `data` with a `schema` | `data_attach`, with `id`, `file`, and optionally `schema` and `parse` |
| see the rows a chart reads | `scaena inspect <bundle> --data` | `deck_inspect` with `data` |
| add or change the chart | `scaena patch <bundle> --ops ops.json` | `deck_patch` |
| check | `scaena lint <bundle> --json` | `deck_lint` |
| look | `scaena render <bundle> --state <id> --out chart.png` | `deck_render` |

## Procedure

1. **Read the data: columns, types, rows.**
   - Dates in ISO 8601 read as they are. Any other date needs a `parse` format for its column (`"parse": { "month": "%b %Y" }`, `docs/spec/format.md`).
   - A value that does not fit its column's type is E103 when the deck validates, before any render.
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
   - Set `key` only when x and series do not tell the rows apart. A key that repeats is E103.
6. **Place it** in a `figure` layout's `main` slot, or the theme's equivalent.
   - Put the headline in `header` and the source in `footer`, as `semantic: source`.
   - Give the chart `alt`: what a listener needs to hear, with the numbers that matter.
7. **Point at the answer with `annotations`**, only when the headline needs help:
   - `rule` at a `y` (a target, a plan) or an `x`.
   - `band` across x or y (a period, a range).
   - `callout`, with `text` at one x and, if needed, a series.
   - `highlight` of an x or a series; the rest dims.
8. **Check and look.**
   - Lint for E103 (fields, types, keys), W310 (labels that collide, when you set `labels.show`), and E100 and E101 (the chart's cell).
   - Render, and read it as the audience will: is the point visible in two seconds?
9. **Move the data, not the chart.**
   - For "next quarter" or "after the change", add a state that changes the chart's `data` or `dataTransform` (a new `filter`), not a new chart.
   - Marks match by key and move. A new key grows in from 0, and a removed one shrinks to 0.
   - Re-derive every figure in the copy from the new data.

## Rules

- No style fields on the chart. Colors, strokes, sizes, and label roles are the theme's.
- Leave `labels` and `legend` unset unless the slide needs otherwise. When it does:
  - `labels: { show: "all" | "ends" | "none" }`;
  - `legend: "direct" | "top" | "bottom" | "right" | "none"`.
- Every number the audience reads has a format.
- One chart, one point. Two points need two states.
- A chart's id names what it is for (`revenue`), not its period (`q3-revenue`): the chart persists while its data moves.

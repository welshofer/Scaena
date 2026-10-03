---
name: author-deck
description: Author a Scaena deck from a brief and, optionally, a data file. Interview for the spine, write states, lint to zero errors, render, and look. Works through the `scaena` CLI or its MCP server. Use when asked to "make a deck", "turn this into a presentation", or "build slides" in a Scaena project.
---

# author-deck

You are writing a Scaena document. It is either `deck.json` (schema `docs/schema/deck.schema.json`; over MCP, `scaena://schema/deck` and the parts it names) or the same document as `.scn` source (SPEC §4; `docs/examples/revenue.deck.scn` is a whole deck). Write whichever is easier.

`docs/examples/trails.deck.json` (`scaena://examples/trails.deck.json`) is fifteen slides that use most of what a deck can hold: a spine, a stat, a photograph, five kinds of chart, a table, rows of cards, a quote, and motion. Read it before you write your first deck.

Read SPEC §2–§4 once per session. Over MCP, `scaena://spec` is its index and each section a resource: `scaena://spec/2`, `scaena://spec/4`, and §3's parts as you need them (`scaena://spec/3.3` for nodes, `scaena://spec/3.7` for charts). The model:
- Nodes exist for the whole deck.
- States are cues, and unchanged properties track forward.
- The theme owns typography and layout. You name roles, slots, and presets, never pixels.

## The operations

Every step works through either surface. An MCP tool takes the same settings its command takes as flags.

| To | CLI | MCP |
|---|---|---|
| start a bundle from a theme | write `deck.json`, with the theme's fonts in the bundle | `deck_create`: copies the theme, its fonts, and data files in, and writes only if the deck validates |
| add a data file | copy it to `data/` and declare it under `data`: a JSON Patch `add` at `/data/<id>`, or `bind_data` with a `source` | `data_attach`: types its columns by inference |
| turn `.scn` into a deck | `scaena compile deck.scn -o <bundle>/deck.json` | `deck_create` with `scn` |
| check | `scaena validate <bundle>`, then `scaena lint <bundle> --json` | `deck_lint`, which validates first |
| edit | `scaena patch <bundle> --ops ops.json --dry-run --json`, then without `--dry-run` | `deck_patch` with `dry_run`, then without |
| see a state resolved | `scaena inspect <bundle> --state <id>`, with `--resolved`, `--timeline`, or `--data` | `deck_inspect` |
| see what a build changes | `scaena diff <bundle> --from a --to b` | `deck_diff` |
| look | `scaena render <bundle> --state <id> --out frame.png` | `deck_render`, which returns the PNG |
| read it back | `scaena decompile <bundle>` | `deck_read`, with `scn` for `.scn` |
| the spine | edit `spine` in `deck.json` | `spine_read`, `spine_update` |

Errors:
- Under `--json`, every command prints one JSON value, and an error is `{ "error": … }` (SPEC §7.1).
- An MCP tool that stops returns an error result whose text says why (SPEC §7.2).

## Procedure

1. **Spine first.**
   - From the brief, write `spine.sections[].beats[]`: one `claim` sentence per beat, `evidence` refs, and `notes`.
   - Aim for 3–7 beats. If the brief is thin, ask one question, then proceed.
2. **Data.**
   - Register sources under `data` (`data/*.csv|json`), each with a `schema`; `data_attach` writes both.
   - Charts bind with `"data": "@name"`. Each datum is a mark keyed by its x and series, which carries it from state to state (SPEC §3.7).
   - For anything beyond a plain chart, follow the `chart-from-data` skill.
3. **Nodes.**
   - Create each object once, with a meaningful id, a `type`, a `role` (text), and a `semantic` (`claim | evidence | annotation | context | comparison | takeaway | source | navigation | decoration`).
   - Place a node in a slot of the theme's layout templates with `at: { in: <slot> }` (the theme's `layouts`), or on its grid with `at: { col, row }`.
   - For rows of cards, stat blocks, and photo grids, place a container (`stack`, `grid`, `frame`) like any node, then put each child in it with `at: { parent: <container> }`. The child goes nowhere else, and `at.index` reorders.
   - In a stack, text and images size to their content, and shapes and charts share the rest (`size: { w: "fill" }` gives equal shares).
   - A container with `fill` draws a panel with its `radius`. SPEC §3.4 has the rules.
4. **States.**
   - One state per click. Set `layout`, then only what changes: text, `kind`, `at`, `remove`. Group builds with `slide`.
   - A state tracks the one before it, so a new slide keeps the last slide's nodes on screen unless it lists them in `remove`. Or it may set `mode: "absolute"` and list everything it shows, which keeps each slide self-contained.
   - Leave motion for the `motion-pass` skill, apart from at most one `enter` per new object.
   - Give every state a `hold` if the deck will run on its own (video, a kiosk). A player advances by holds too, so a deck presented live has none.
5. **Lint loop.**
   - Run `scaena lint <bundle> --json` (`deck_lint`). Fix every error, and take warnings seriously: W300 and W301 mean you reached for pixels, so use a role, slot, or token instead.
   - Many errors carry a `fix`. `lint --fix` (`deck_lint` with `fix`) applies the ones lint has checked by laying the state out again.
   - Re-run until clean. Then run the `tighten-copy` skill over the words.
6. **Inspect.**
   - `inspect --state <id>` confirms tracking did what you meant.
   - `--timeline` shows when each motion runs, and over how many words, lines, or marks.
   - `--data` shows the rows a chart or table reads after its `dataTransform`.
   - `diff --from a --to b` confirms that a build changes only what it should.
7. **Edit with patches.**
   - Once a deck exists, change it with ops (SPEC §7.3, `docs/schema/patch.schema.json`; `docs/examples/revenue.patch.json` is one). Dry-run first: the result is the patch as JSON Patch and what lint finds differently (`added`, `removed`).
   - Prefer the semantic ops:
     - `set_prop` and `set_text` with a `state` change what that state shows.
     - `show_node` and `hide_node` make a node enter or leave.
     - `add_state` with `beat` adds a build to its beat.
   - A patch applies whole or not at all. One that would make the deck invalid is refused, and the error names the op that failed.
8. **Look.**
   - Render a state, view the PNG, fix what is wrong, and repeat.
   - `--t <ms>` renders a frame inside the state's cue (its transition, then its motions), and `--format 9:16` another of the deck's formats.
   - Rendering needs the deck's font files in the bundle; `deck_create` copies them.
9. **Report:** beats, states, the lint summary, and anything you could not express without an override.

## What layout will tell you

Lint lays every state out, so these come back as findings. Expect them, and fix the cause.
- **A role's `measure` caps every line, whatever its box's width.**
  - A headline that needs two lines does not fit a one-row header (E100 says by how much).
  - Shorten it before you reach for `fit: shrink`, which sets it smaller than the theme meant.
- **Density counts every word on screen in a state** (W210, the theme's `density.maxWordsPerState`). Cut words, or split the slide into builds.
- **Containers fill their box.** A grid's rows and a stack's children stretch to it.
  - Give a row of panels a box about as tall as their text needs, two grid rows rather than four.
  - Otherwise the panels show empty space under their text.
- **A stat's figure can fill its slot.** `fit: grow` with `box: cap` sets it as large as fits, from its cap height to its baseline.
  - E101 counts ink, so a comma's tail below the baseline can reach the line under it.
  - Align that line to the end of its slot (`at: { in: …, align: { y: "end" } }`).
- **Text over a picture or a shader is judged against what is painted behind it** (E110, E111). A muted color that passes on the page can fail on a mesh gradient; use the ink color there. A chart's labels are judged the same way, over its own marks.
- **Shaders stay off data** (W311). A mesh or noise is a backdrop for title and section slides. A chart or a table reads on the plain surface, so drop the shader from its states (`remove`).
- **One claim on screen at a time** (W424), and a slide's last state shows one (W421).
  - A container around a claim needs no `semantic` of its own.
  - A quotation that carries the slide is its claim.

## Rules

- Never write a literal color, size, or coordinate anywhere but `overrides`, and avoid `overrides`.
- Every beat's states include a node with `semantic: claim` that expresses the claim.
- **Charts:**
  - No style fields, and `kind` is one of the v1 kinds only.
  - Set `key` only to a field that tells the chart's rows apart (a series repeats across x): a key that repeats is E103.
  - Leave `labels` and `legend` unset to get the defaults, values on the marks and series named where they end (SPEC §3.7). Set them only to change that.
- **Ids:** name each for what the node is for, not what it says today: `total`, not `q3-total`.
  - Ids are identity. They outlive the copy and the period.
  - When one goes stale anyway, `rename_node` renames it everywhere, and the node stays the same node.
- **Figures in copy** (text, `alt`, beat claims, notes) are literals.
  - When the data changes, re-derive every one from the data and re-read every claim.
  - Lint notices neither a stale figure nor a claim the new data makes false (SPEC §16, question 9).
- Keep to `maxWordsPerState` (the theme's `density`). This is a presentation, not a document.
- Do not invent fonts: use the theme's families.

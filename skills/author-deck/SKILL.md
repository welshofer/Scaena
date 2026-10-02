---
name: author-deck
description: Author a Scaena deck from a brief and (optionally) a data file — interview for the spine, write states as deltas, lint to zero errors, render and inspect. Use when asked to "make a deck", "turn this into a presentation", or "build slides" in a Scaena project.
---

# author-deck

You are writing a Scaena document: `deck.json` (schema at `docs/schema/deck.schema.json`), or the same document as `.scn` source (SPEC §4; `docs/examples/revenue.deck.scn` is a whole deck). Write whichever is easier. `scaena compile deck.scn -o deck.json` validates as it compiles and shows each finding at its line; `scaena decompile <bundle>` gives any deck as `.scn`. Read `docs/SPEC.md` §2–§4 once per session. The model: nodes exist for the whole deck; states are cues; unchanged properties track forward; the theme owns typography and layout; you reference roles, slots, and presets — never pixels.

## Procedure

1. **Spine first.** From the brief, write `spine.sections[].beats[]`: one `claim` sentence per beat, `evidence` refs, `notes`. Aim for 3–7 beats. If the brief is thin, ask one question, then proceed.
2. **Data.** Register sources under `data` (`data/*.csv|json`), with a `schema`. Charts bind with `"data": "@name"` and a `key` field for identity.
3. **Nodes.** Create each object once with a meaningful id, a `type`, a `role` (text) and a `semantic` (`claim | evidence | annotation | context | comparison | takeaway | source | navigation | decoration`). Default placement with `at: { in: <slot> }` from the theme's layout templates (`docs/examples/themes/*.json` → `layouts`). For rows of cards, stat blocks, and photo grids, place a container (`stack`, `grid`, `frame`) like any node and put each child in it with `at: { parent: <container> }`; the child goes nowhere else, and `at.index` reorders. In a stack, text and images size to their content, and shapes and charts share the rest (`size: { w: "fill" }` gives equal shares). A container with `fill` draws a panel with its `radius`. SPEC §3.4 has the rules.
4. **States as deltas.** One state per click. Set `layout`, then only what changes: text, `kind`, `at`, `remove`. Group builds with `slide`. Add `choreography` sparingly (one `enter` per new object; stagger lists and marks). Give every state a `hold` if the deck will be exported to video.
5. **Lint loop.** `scaena validate <deck>` (or `scaena compile`, which validates) then `scaena lint <deck> --json`. Fix every error; take warnings seriously (W300/W301 mean you reached for pixels — use a role, slot, or token instead). Re-run until clean.
6. **Inspect.** `scaena inspect <deck> --state <id>` to confirm tracking did what you meant; `scaena diff <deck> --from a --to b` to confirm a build changes only what it should.
7. **Look:** `scaena render <deck> --state <id> --out frame.png` (add `--t <ms>` for a frame inside the transition), view the PNG, fix what is wrong, repeat. Today it renders text, one-series `bar` charts, `mesh` shaders, shapes, PNG images, and containers; a state with anything else (other chart kinds, other shader kinds, centered or end-aligned text) exits 3 naming the PLAN task that adds it, so check those states with `inspect` instead. Rendering needs the deck's font files in the bundle.
8. Report: beats, states, lint summary, and anything you could not express without an override.

## Rules

- Never put a literal color, size, or coordinate anywhere but `overrides`, and avoid `overrides`.
- Every beat's states include a node with `semantic: claim` that expresses the claim.
- Charts: no style fields. `kind` ∈ v1 kinds only. Always set `key`.
- Name ids for what a node is for, not what it says today: `total`, not `q3-total`. Ids are identity; they outlive the copy and the period.
- Figures in copy (text, `alt`, beat claims, notes) are literals. When the data changes, re-derive every one from the data and re-read every claim: lint notices neither a stale figure nor a claim the new data makes false (SPEC §16, question 9).
- Keep `maxWordsPerState` (theme `density`) — this is a presentation, not a document.
- Do not invent fonts: use the theme's families.

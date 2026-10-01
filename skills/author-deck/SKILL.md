---
name: author-deck
description: Author a Scaena deck from a brief and (optionally) a data file — interview for the spine, write states as deltas, lint to zero errors, render and inspect. Use when asked to "make a deck", "turn this into a presentation", or "build slides" in a Scaena project.
---

# author-deck

You are writing a Scaena document (`deck.json`, schema at `docs/schema/deck.schema.json`; the DSL `.scn` is equivalent once PLAN 1.5 lands). Read `docs/SPEC.md` §2–§3 once per session. The model: nodes exist for the whole deck; states are cues; unchanged properties track forward; the theme owns typography and layout; you reference roles, slots, and presets — never pixels.

## Procedure

1. **Spine first.** From the brief, write `spine.sections[].beats[]`: one `claim` sentence per beat, `evidence` refs, `notes`. Aim for 3–7 beats. If the brief is thin, ask one question, then proceed.
2. **Data.** Register sources under `data` (`data/*.csv|json`), with a `schema`. Charts bind with `"data": "@name"` and a `key` field for identity.
3. **Nodes.** Create each object once with a meaningful id, a `type`, a `role` (text) and a `semantic` (`claim | evidence | annotation | context | comparison | takeaway | source | navigation | decoration`). Default placement with `at: { in: <slot> }` from the theme's layout templates (`docs/examples/themes/*.json` → `layouts`).
4. **States as deltas.** One state per click. Set `layout`, then only what changes: text, `kind`, `at`, `remove`. Group builds with `slide`. Add `choreography` sparingly (one `enter` per new object; stagger lists and marks). Give every state a `hold` if the deck will be exported to video.
5. **Lint loop.** `scaena validate <deck>` then `scaena lint <deck> --json`. Fix every error; take warnings seriously (W300/W301 mean you reached for pixels — use a role, slot, or token instead). Re-run until clean.
6. **Inspect.** `scaena inspect <deck> --state <id>` to confirm tracking did what you meant; `scaena diff <deck> --from a --to b` to confirm a build changes only what it should.
7. **Look:** `scaena render <deck> --state <id> --out frame.png`, view the PNG, fix what is wrong, repeat. Today this renders text nodes only; a state with any other node type (chart, shader, shape, image, group) or with centered or end-aligned text exits 3 naming the PLAN task that adds it, so check those states with `inspect` instead.
8. Report: beats, states, lint summary, and anything you could not express without an override.

## Rules

- Never put a literal color, size, or coordinate anywhere but `overrides`, and avoid `overrides`.
- Every beat's states include a node with `semantic: claim` that expresses the claim.
- Charts: no style fields. `kind` ∈ v1 kinds only. Always set `key`.
- Keep `maxWordsPerState` (theme `density`) — this is a presentation, not a document.
- Do not invent fonts: use the theme's families.

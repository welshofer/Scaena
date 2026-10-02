---
name: tighten-copy
description: Tighten the words on a Scaena deck's slides. Aim for one claim per slide in as few words as carry it, headlines that fit their measure, and figures that match the data. Works through the `scaena` CLI or its MCP server. Use when asked to "tighten the copy", "cut words", "make the slides punchier", or "fix the overflow", or after lint reports density, fit, or claim warnings.
---

# tighten-copy

A slide is read in seconds while someone talks. The theme sets two limits, and lint holds the deck to both:
- how many words a state may show (`density.maxWordsPerState`, 40 in Dusk);
- how long a line may run (each role's `measure`).

Cut until each slide says its one thing.

| To | CLI | MCP |
|---|---|---|
| find what to cut | `scaena lint <bundle> --json` | `deck_lint` |
| read every word in context | `scaena decompile <bundle>` | `deck_read` with `scn` |
| change text | `scaena patch <bundle> --ops ops.json`, with `set_text` | `deck_patch` |
| see where text sets | `scaena render <bundle> --state <id> --out frame.png` | `deck_render` |

## Procedure

1. **Read the spine** (`spine_read`, or `spine` in `deck.json`). Each beat's claim is the sentence its slides must make. Copy that does not serve a claim goes.
2. **Fix what lint finds.**
   - **W210, density:** cut words, move detail to the state's `notes`, or split the slide into a build.
   - **E100, text that does not fit:** shorten it first.
     - A role's `measure` caps every line, so a headline that wraps needs fewer words, not a wider box.
     - `fit: shrink` is the last resort. **W203** means it shrank to its floor and still does not fit.
   - **W200, widow:** change a word near the paragraph's end, so its last line carries two.
   - **W202, more lines than `maxLines`:** say less.
   - **Claims:** rewrite so each slide claims one thing.
     - **W420, W421:** a beat or a slide with no claim.
     - **W424:** two claims at once.
     - **W425:** two beats in a row with the same claim.
3. **Headlines are sentences that claim.**
   - "Revenue doubled", not "Revenue".
   - Active verbs, concrete nouns, no hedges.
   - A headline that needs a second line usually holds two ideas.
4. **Numbers.**
   - One number per point, rounded to what the audience needs: `$1.2M`, not `$1,213,544`.
   - Every figure in copy (text, `alt`, beat claims, notes) is a literal that nothing checks against the data. After any data change, re-derive each one from the data.
5. **Cut** "very", "really", "in order to", "the fact that", restated titles, and labels that repeat the headline. Keep the noun that carries the meaning.
6. **Check.**
   - Lint should be clean of W2xx and W4xx.
   - Render each changed state and read it at a glance.

## Rules

- One claim per slide, said by a node with `semantic: claim`.
- Fewer words beat smaller type: never shrink text to keep words.
- Do not change a figure without the data that backs it.
- Keep each node's id: change its `text` with `set_text`, so it stays the same node and morphs between states.

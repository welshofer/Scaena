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
| change text | `scaena patch <bundle> --ops ops.json --dry-run`, with `set_text` | `deck_patch`, with `dry_run` first |
| change a word wherever it shows | `scaena find <bundle> WORD --replace NEW --dry-run`, then without `--dry-run` | `deck_find` with `replace`, `dry_run` first |
| check a figure | `scaena inspect <bundle> --data` | `deck_inspect` with `data` |
| see where text sets | `scaena render <bundle> --state <id> --out frame.png` | `deck_render` |

## Procedure

1. **Read the claims.** The spine's beats carry them: `spine_read`, or from the CLI `scaena export <bundle> --format spine` (the spine lives in `deck.json`, or in `spine.json` beside it). Each beat's claim is the sentence its slides must make. Copy that does not serve a claim goes.
   - With no spine, the claims are the nodes marked `semantic: claim`. W420, W423, and W425 judge beats, so without a spine they find nothing.
   - A label marked as a claim ("Process", "Storm damage") claims nothing. Make it a sentence, or mark it `navigation`.
2. **Fix what lint finds.** Fix overflow first: an E101 overlap often follows from an E100, text grown out of its box.
   - **W210, density:** cut words, move detail to the notes (the beat's `notes`, or the state's when there is no spine), or split the slide into a build.
   - **E100, text that does not fit:** shorten it first.
     - A role's `measure` caps every line, so a headline that wraps needs fewer words, not a wider box. In Dusk, a headline line holds about 32 characters.
     - `fit: shrink` is the last resort. **W203** means it shrank to its floor and still does not fit.
   - **W200, widow:** change a word near the paragraph's end, so its last line carries two.
   - **W202, more lines than `maxLines`:** say less.
   - **Claims:** rewrite so each slide claims one thing.
     - **W420, W421:** a beat or a slide with no claim.
     - **W424:** two claims at once.
     - **W425:** two beats in a row with the same claim.
     - **W426:** a beat out of the order its states play in. Move the beat, or its states.
3. **Headlines are sentences that claim.**
   - "Revenue doubled", not "Revenue".
   - Active verbs, concrete nouns, no hedges.
   - A headline that needs a second line usually holds two ideas.
4. **Numbers.**
   - One number per point, rounded to what the audience needs: `$1.2M`, not `$1,213,544`.
   - Every figure in copy (text, `alt`, beat claims, notes) is a literal that nothing checks against the data. Check each against the rows a chart or table reads (`inspect --data`), and after any data change, re-derive each one.
   - Keep a figure on one line: join its number and unit with a no-break space (U+00A0), so `52 miles` never breaks between them.
5. **Cut** "very", "really", "in order to", "the fact that", restated titles, and labels that repeat the headline. Keep the noun that carries the meaning.
6. **Change it with a patch,** one `set_text` per node, in the state that shows it:
   ```json
   [{ "op": "set_text", "node": "miles-title", "state": "miles", "text": "Crews rebuilt 52 miles" }]
   ```
   - A node set in `runs` (several looks in one text) would lose them to `set_text`. Change the words with `replace_text` instead: the characters from `from` to `to` of the text the state shows become `text`, and each run keeps its look.
     ```json
     [{ "op": "replace_text", "node": "miles-title", "state": "miles", "from": 0, "to": 12, "text": "Crews rebuilt" }]
     ```
   - A word that recurs across the deck ("revenue" in five headlines and a note) changes in one go: `scaena find <bundle> revenue --words` lists each text that holds it, once for each place it is written, and `--replace income` replaces every match in one patch, each where its text lives, runs keeping their looks (`deck_find`). Without `--case`, case is ignored, so check that each match is one you mean.
   - Run it with `--dry-run` first. The lint delta (`added`, `removed`) shows what changed. `errors` counts what remains, including a finding that only changed its figures, which the delta counts as the same.
   - `applied` says whether the deck was written. Exit 1 means it was refused, or errors remain.
7. **Check.**
   - Lint should be clean of W2xx and W4xx.
   - Render each changed state and read it at a glance.

## Rules

- One claim per slide, said by a node with `semantic: claim`.
- Fewer words beat smaller type: never shrink text to keep words.
- Do not change a figure without the data that backs it.
- Keep each node's id: change its `text` with `set_text`, so it stays the same node and morphs between states.

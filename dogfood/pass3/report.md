# Dogfood pass 3: the AI-first editing loop

main c45b5143, 2026-10-24. Ten requests a person makes of their assistant, each done through `scaena mcp` alone, then checked with the CLI (`lint`, `health`, `export`).
- Paths are relative to `ROOT=$S/dogfood3`. Code paths are relative to the clone at `$S/dogfood2/src`.
- Client: `tools/mcp.py`, a stdio JSON-RPC client. Every request and response is logged to `transcripts/<task>.jsonl`; rendered PNGs are in `evidence/<task>/`.
- Decks: pass 2's, copied to `decks/`, with untouched copies in `pristine/`. Lint and health snapshots are in `evidence/checks/<deck>-<tag>-{lint,health}.json`.
- The Spanish and the speaker notes were written by the agent; no model was called.

(The subagent could not save this file itself. This copy, written by the coordinator, keeps its findings and evidence pointers in full.)

## Verdict

**Faster and better checked than Keynote or PowerPoint.**
- 113 tool calls, each under 0.75 s (`deck_patch` median 0.18 s).
- Every write can be dry-run, and every write reports the lint delta with measured overflow and often a fix.
- A re-theme took 3 calls plus one headline shortened. A 66-string translation took one patch. A cut to 8 slides, three backgrounds, a chart slide, and a comparison slide each took minutes.

**But the story rots.**
- Structural edits leave the spine orphaned, out of order, or merged, and lint and health say nothing (W426 on a move aside). The repair is to rewrite the whole spine by hand.
- Health gives advice that cannot clear its own findings, penalizes the length the user asked for, and reads Spanish with English lexicons. Its offered "fixes" raise the score by rewriting the plan, not a pixel.
- An MCP-only agent cannot undo anything.

## Findings

### Broken result

**1. Structural edits desync the spine** (T1–T3)
- **Remove.** `remove_slide` keeps beats with no states, along with their claims, evidence, and durations. Lint adds nothing, and health rose from 89 to 91. The spine export keeps an orphan as `{"section": "impact"}`.
- **Move.** `move_slide` never moves beats. W426 fires, and health's drift "fix" only relabels `media.narrato`.
- **Add.** `state_start` appends each new slide to the previous slide's beat.
- **Cause.** `scaena-core/src/patch/ops.rs` (`remove_slide` at 2303, `move_slide` at 2190); `scaena-ops/src/states.rs` (`starting`); `lint/narrative.rs` (W420 skips a beat whose states show nothing; W426 and W423 do not judge a beat with no states).
- **Fix.**
  - `remove_slide` removes a beat it leaves empty, or a lint rule finds a beat with no states, with the removal as its fix.
  - `move_slide` moves a one-slide beat with its slide.
  - `state_start` takes an optional `claim` that starts a beat of its own.
- **Evidence.** `evidence/T1/orphan-beats.txt`, `orphan-spine-export.json`, `evidence/T2/move-drift-w426.txt`; `transcripts/T1.jsonl` line 35, `T2.jsonl` lines 25 and 70, `T3.jsonl` line 15.

**2. Health reads every Latin-script deck as English** (T6)
- **What happened.** On a Spanish deck (`meta.lang es-ES`), intensity read about 0.42 on 12 of 13 slides and tone 0.00 on 11. 12 findings were bogus, each with a "Plan its … as it reads" fix.
- **Cause.** `scaena-narrato/src/health/mod.rs:276-285`: `measured()` calls `detected(&s.text(), None)`. `scaena-ops/src/health.rs:404` calls `health::metrics(&read, None)`.
- **Fix.** Pass `meta.lang`, else `brief.language`, through to `detected`.
- **Evidence.** `evidence/T6/health-reads-spanish-as-english.txt`.

**3. A category highlight leaves the legend in colors no mark uses** (T3)
- **What happened.** Under `at: {x: "EMEA"}`, the legend keeps its undimmed series colors while every bar is dimmed or lit.
- **Cause.** `scaena-engine/src/charts/compile.rs`, the highlight's `for e in &mut out.legend` (about lines 2427-2445): an entry picked in part keeps its full color.
- **Fix.** An entry picked in part takes its unpicked marks' dimmed color.
- **Evidence.** `evidence/T3/legend-bug.png`.

**4. No undo with MCP alone** (T10)
- **What happened.** `deck_history` says "the bundle keeps no history: `scaena save --history` begins one", a CLI command, and no MCP tool begins a history. Once the CLI begins one, history and restore work well.
- **Cause.** `scaena-cli/src/main.rs:637-640`: only `save --history` sets `SaveOptions.history`.
- **Fix.** `deck_history {begin: true}`, or `deck_create`/`deck_generate` take `history`, or the server begins one on its first write.

### Misleading

**5. Health's "turn" advice cannot clear its own finding.** It says to put the bridging line in the notes, but coherence reads only the title (the beat's claim) and the body: `narrato/src/health/coherence.rs:132`, with the ask at `scaena-ops/src/health.rs:565-570`.

**6. Pacing punishes the requested cut.** `asked()` (`scaena-ops/src/health.rs:331`) reads only `N-M slides` and `N+ slides`. "8 slides" is paced for 18 (pacing 50, score 81). Accept `N slides`, and say in `deck_health` that pacing reads `brief.length`.

**7. W220 fires on the theme's own Icon Cards layout,** and its hint ("let the slots' align decide") cannot be followed. `lint/text.rs` W220MixedAlignment counts every text of two lines or more. Count only alignments the deck sets, or leave the header out.

**8. "Best first" layouts rank by lint, then theme order.** `layouts.rs` `ranked` sorts by `(errors, warnings, !current, theme order)`, so Three Numbers ranked tenth for a three-figure slide. Rank fully filled layouts with the same item count before theme order.

**9. `deck_find` misses the words charts and tables draw:** axis titles, categories, series, and annotations (`find.rs`). Search them too, or report them as found and not replaced.

### Friction

**10. `hide_node` leaves debris.** It adds 9 × W322 and 9 × I401 with no fix. The editors' smart delete (`inspect.rs:1608 deleting`) is not an op, and the author-deck skill steers agents to `hide_node`. Add a `delete_node` op, have `hide_node` drop the node's idle choreography, and give W322 a fix.

**11. Notes live in two places.**
- The web presenter shows a state's notes, else its beat's (`web/src/worker.ts:1412`).
- The Mac and iPad show the state's notes only (`Presenting.swift:549`), until #466.
- The PDF has no notes, and the spine export carries both.
- The fix: one rule in `scaena_core` that every presenter, the remote, and the exports use, said in the tools' descriptions, with optional notes pages in the PDF.

**12. Slides started with `state_start` lag generated ones.**
- The headline has no `fit: shrink` or `semantic: claim`, the body is at plain size, ids are the layout's name, and the empty slots are not named.
- Cause: `states.rs:453` `slot_node`.

**13. Structural patches echo the whole states array:** 59 KB for 7 removals. Echo the semantic ops and a summary, with the compiled patch behind `verbose`.

**14. No tool returns the bundle's own theme.** Add `deck_read {theme: true}`.

### Polish

**15. Taking a background away keeps its recolor.** `backgrounds.rs` `setting` only adds recolors.

**16. A malformed argument's error doesn't name the field.** Passing `size` as `[960,540]` returned "invalid type: sequence, expected a string".

**17. History says ops, not changes.** Versions read "patch: set_text", and `compare` gives each new value without the old one.

### Re-confirmed, not counted

These pass 1 findings still stand on current main:
- #4: descenders, after the Salon swap.
- #10: the stale running head.
- #12: siblings at different sizes, worse with Spanish.
- #2: numbered cards lose their lead figure.
- #15: health's fixes rewrite the plan (93 → 94 with no state changed).

## What works well

- Fast, dry-runnable, lint-delta-reporting writes. Errors point at the exact JSON pointer.
- Slide-level semantic ops: `remove_slide`, `move_slide`, `state_start` (172 layouts), `background_set`, `theme_apply`, `data_attach`.
- Re-theming: `theme_apply` plus `deck_lint {fix}` cleared 5 of Salon's 8 errors.
- Translation in one patch. `meta.lang` drives the PDF `/Lang`, number formats, and hyphenation.
- History, where it exists, is precise.
- `deck_render` returns the PNG inline, which is how the legend, the overflow, and the overlap were caught.

## Per task

| Task | Deck | Calls | Lint | Health | Succeeded |
|---|---|---|---|---|---|
| T1 Cut to 8 | pitch | 21 | 3 W210 → 3 W210 | 89 → 95 (85 stale brief; 81 "8 slides") | yes, after rewriting the spine by hand |
| T2 Board rework | qbr | 20 | 4 W210 → 0 | 93 → 88 | yes |
| T3 Chart slide | qbr | 12 | 0 → 0 | 88 → 89 | yes; legend bug |
| T4 Fix lint and health | lecture | 15 | 6 → 0 | 93 → 94; 3 fixes changed no state | partly |
| T5 Salon | launch | 10 | 3 → 3 | 85 → 85 | yes |
| T6 Spanish | keynote | 9 | 3 → 3 | 89 → 89; 12 of 21 findings bogus | yes; health broken |
| T7 Notes | lecture | 3 | 0 → 0 | 94 → 94 | partly: no notes in the PDF |
| T8 Side by side | lecture | 5 | 0 → 0 | 94 → 90 | yes |
| T9 Aurora | pitch copy | 8 | 3 → 3 | 89 → 89 | yes |
| T10 Undo two | pitch copies | 10 | 3 → 3 | 89 → 89 | partly: no with MCP alone |

## What the schemas and resources did not say

- **T1:** that `remove_slide` keeps beats; that pacing reads `brief.length`, and its form.
- **T2:** which op deletes a node cleanly; that `move_slide` leaves the spine; that "best first" means lint-clean, then the theme's order.
- **T3:** the empty slot's name.
- **T4:** that W210 and W220 have no lint fix; that health's fixes change only metadata; that turn findings ignore notes.
- **T6:** where every word lives (a translation checklist).
- **T7:** which notes each presenter shows.
- **T8:** that the comparison layouts are never offered for four items.
- **T10:** that MCP cannot begin a history.

The evidence index is in `evidence/`: contact sheets before and after, single slides, PNGs per state, `tools-list.json` (154 KB, 111 KB of it output schemas), and `resources/`.

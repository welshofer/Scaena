# Dogfood 18: a developer talk from a GitHub repository

A 15-minute Educate talk for engineers, grounded in a real public repository: how to benchmark
command-line tools with [hyperfine](https://github.com/sharkdp/hyperfine). It is the first dogfood
of a code-heavy deck — four code slides, their languages detected, a walkthrough stepped through
one block — and the first of `--source github:OWNER/REPO`.

Made with a release CLI built at main 434d8e3b. Health was read again at main 3fda55d6, after
#1275, and the two readings differ where #1275 changed it (see "For Jay").

## How it was run, in two halves

The two halves answer different questions, and only the first uses the fetch plan.

### (a) The fetch and the pack: `github:welshofer/Scaena`

This half asks what the network path does with a large repository, and what the model is shown.
Scaena's own repository was the only one this machine could reach through GitHub's API.

```
python3 -I scripted.py LOGDIR &
export ANTHROPIC_API_KEY=sk-ant-dummy
scaena generate scaena-talk "How Scaena draws one deck to every screen, for engineers" \
  --provider anthropic --model claude-test --base http://127.0.0.1:8787 \
  --audience Engineers --goal Educate --minutes 15 \
  --source github:welshofer/Scaena --outline scaena.outline.json
```

**What it did.** 5.4 s, about 100 MB over the network. The tarball is 49 MB, well under
`MOST_ARCHIVE`'s 256 MB, so it was read whole: no bound was reached, and the read never needed to
stop early. The pack came to 586 KB: 45 documents included, 27 named as ranked out.

**What was cut, and whether it reads sensibly.** Mostly yes. Each document is listed with its
size, a truncated one marked "(truncated mid-document)", and the ranked-out list names what did
not fit. The header counts the repository honestly (1691 files, 784 source files, 174 documents,
languages by file count) and says plainly that no implementation code appears below.

One cut did not read sensibly, and is finding 1.

### (b) The talk: sharkdp/hyperfine

This half skipped the fetch plan and the `github:` provenance. hyperfine was cloned, and its
`git archive` was run through the same reader the fetch plan uses (`github::unpack`, then
`github::compose`), giving `hyperfine.md`, the pack as the model would have seen it. The deck is
grounded in that file, so its source is recorded as `hyperfine.md`, kind markdown, not as a
repository: no `github:` address reaches `meta.narrative.sources`, and the deck's one link to
github.com is then a host no source names, which health warns about (see "Not findings").

```
scaena generate hyperfine-talk "A 15-minute talk for engineers: benchmark command-line tools properly with hyperfine" \
  --provider anthropic --model claude-test --base http://127.0.0.1:8787 \
  --audience Engineers --goal Educate --minutes 15 --length "11-15 slides" \
  --source hyperfine.md --outline hyperfine.asked.outline.json
# settle the outline by hand into hyperfine.outline.json, then the same command with
#   --from-outline hyperfine.outline.json   in place of   --outline …
```

`scripted.py` stands in for Anthropic on this machine, as dogfood 13's does:

- **The arc:** Learning Journey.
- **The outline:** a big idea and six claims, written from hyperfine's README.
- **The slides:** 15, with four code blocks (three shell, one Rust lifted from
  `src/outlier_detection.rs`), a bar chart of the README's own `find`/`fd` benchmark, and a quiz.
- **The look:** one change, a topic headline made a claim.
- **The notes:** sentences to each slide's budget.

The outline was settled by hand: Application and Mastery each got a slide more, and Mastery's
claim was retyped to state its point ("Below 5 ms, a shell's startup is more noise than your
command: run it without one").

## Findings, worst first

Each has its own pull request off main.

1. **Two documents were read by nobody and named by nobody (#1293).** `docs/SPEC.md` (617 KB) and
   `docs/PLAN.md` (702 KB) are each past `MAX_FILE` (512 KB), so `unpack` counted them without
   reading them. `compose` then filtered its candidates on `data.is_some()`, so they fell out
   before ranking and appeared in neither list. The model was told the repository has 174
   documents, shown 45, told 27 more were ranked out, and left to infer the remaining two did not
   matter. They are the specification and the plan.
   - **Fix:** a section of its own, "Present in this repository but NOT read — too large for this
     pack", largest first, with sizes. Nothing of their content is read.
2. **A quiz's answer letters were lint findings (#1289).** The made quiz drew W305 three times:
   its `A`, `B`, and `C`. The quiz layout's letter slots carry those as prompts, and the letter is
   what the slide says there. A `how-it-works` slide's step numbers and a divider's "01" are the
   same.
   - **Fix:** a prompt of one or two alphanumeric characters is a place's label, never a prompt
     left to type over. A label counts as neither prompt-left nor words-written, so a quiz just
     inserted still reads as "being written".
3. **A command-line flag lost its dashes (#1290).** The quiz's answers were written `A: --warmup`,
   `B: --prepare`, `C: --runs`; the deck drew **warmup**, **prepare**, **runs**. `item()` stripped
   every leading dash as a bullet's mark.
   - **Fix:** a dash, star, or en dash is a mark only before a space. `--warmup` and `-S` keep
     theirs; `- Measure our travel` still loses its.
4. **A chart rounded away the figures its writer wrote (#1292).** hyperfine's README times
   `find -iregex` at 2.275 s and `fd` at 0.232 s. The slide and the PDF drew 2.27 and 0.23. A
   chart's format was capped at two decimals, and its float check was one-sided.
   - **Fix:** four places at most, and "whole at `d` places" measured from either side. A made deck
     draws no figure its writer did not write (DF7-1).
5. **An unordered list came out numbered (#1291).** Slide 9, "What the shell costs", was written
   as four points with no order and drawn 1 to 4. Narrato's variety pass breaks a run of one
   layout, and its change may be to a Numbered List; Scaena kept it. Scaena's own run-breaker
   already refuses to number unordered points, and so does the look.
   - **Fix:** a change to `numbered` is undone unless the writer asked for a numbered list.

## What worked

- **Code detection and language.** All four blocks were set as code in the right language: shell
  for the three `.sh`-shaped blocks, rust for the block whose subtitle names
  `src/outlier_detection.rs`. None was typeset: no quotation mark curled, no `-` made an en dash.
- **Focus and the walkthrough.** `coding::walking` on the Rust block gave four steps (line 1, then
  3–5, then 7–8, then the whole block), each a state of the same slide, gliding in at `slow`. Lint
  found nothing: "lint delta: none". Each step dims the lines outside its span, numbers included.
- **The PDF with notes.** 15 pages, each slide at the head of an upright page with its notes
  under it. The walkthrough prints at its last step, whole. Code copies as written, its
  indentation intact.
- **Pacing.** 100/100: "15 slides, as the brief asked (11–15). About 15 minutes of talk, where its
  brief asks 15 minutes."
- **Health's reading of the talk.** Its coherence, turn, and bridge findings read correctly, and
  its asks name the right words ("in a word of slide 7's, such as 'flag'").

## For Jay: what moves a score

Neither was built here.

- **Health counts a code slide in balance.** At 434d8e3b, health read the code blocks as list
  points: "slides 4, 7 and 10 list the fewest points" (each a code slide making one "point"), and
  "Slide 10 makes 9 points in one place" (the Rust block's nine lines). At 3fda55d6, after #1275,
  neither finding is there: `dense` and `thin` count no code text's lines. Balance still reads 70,
  "1 point each, over 11 of the 15 slides, those made to list points", because the three code
  slides are still among the slides it averages. **#1132** takes them out of it: a code text is
  read as shown, not said. This run is a second calibration for it.
- **The source check fires on citation alone.** Each beat here cites `source:hyperfine.md`, so
  every figure counts as sourced. Adding "it cut our CI benchmarks from 14 to 6 minutes", a figure
  the repository does not hold, raised nothing. **#1283** finds a figure cited to the wrong source
  where another source holds it. A figure *no* source holds, cited all the same, is the known limit
  of that check, n′: the stricter n, as first written, misfired on figures a slide computes and
  on targets no source states, so it stays as it is.

## Open questions

- **A closing's "Thank you" is W305.** The closing layout's statement slot carries the prompt
  "Thank you", and a made deck's bare close writes exactly that. Telling "the slide says this" from
  "nobody typed over this" needs the theme to mark the slot, which is a theme-format change.
- **Focus and walkthrough are editor-only.** `coding::focusing` and `coding::walking` have no CLI
  command and no MCP tool, so an agent writing a code talk must hand-write `add_state` patches
  with `code/focus` props. They are the natural shape of a `patch` op.
- **Code is set small.** A three-line block sits in a card that hugs it, leaving much of the slide
  empty. Whether a short block should grow is Jay's call. `code-before-after.png` shows slide 4 as
  made (the code role's 32 px on a 1920 slide) over the same block at 56 px, set by a one-off
  override for the comparison alone.

## Not findings

Two health findings are artifacts of how this run was built, not bugs:

- **"Slide 12 says again what slide 6 said"** names the quiz, which recalls slide 6 on purpose.
  Health has no way to know a quiz is meant to repeat.
- **"Slide 15 links github.com/sharkdp/hyperfine, which neither the brief nor the sources name"**
  is right about this bundle: half (b) skipped the `github:` provenance, so the deck's only source
  is a markdown file. Through the real fetch the repository's address would be in
  `meta.narrative.sources` and the link would pass.

## The files

- `scripted.py` — the provider on this machine, answering each step from a script.
- `hyperfine.md` — the pack as `github::compose` wrote it from hyperfine's `git archive`
  (sharkdp/hyperfine at c5161524), which the talk is grounded in. It quotes hyperfine's own README
  and changelog, which are MIT (`LICENSE-MIT-hyperfine`, copied from that repository).
- `hyperfine.asked.outline.json` — the outline as the model wrote it.
- `hyperfine.outline.json` — the outline as it was settled by hand.
- `code-before-after.png` — slide 4's code block as made, and the same block larger, for the
  call on code size.
- `hyperfine.slides.json` — the 15 slides as they were written, which `--slides` replays with no
  call to a model.

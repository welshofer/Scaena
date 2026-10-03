---
name: motion-pass
description: Review and set a Scaena deck's motion so it explains rather than decorates and stays within the theme's limits. Covers transitions between states, entrances and exits, builds, emphasis, and holds. Works through the `scaena` CLI or its MCP server. Use when asked to "add animation", "make it move", "polish the transitions", "build this slide", or before exporting a deck to video.
---

# motion-pass

Motion is a cue: the transition into a state and the state's motions run on one clock (SPEC §2.4, §3.9).
- Nodes present in both states morph: text word by word, shapes point by point, chart marks by key. Builds on one slide get most of their motion free.
- A node in only one of the two states enters or leaves instead. With no motion of its own, it fades with the transition. So between slides that share nothing, a long transition lays the old slide's words under the new one's.
- Add motion only where it says something: what is new, what changed, or where to look.

| To | CLI | MCP |
|---|---|---|
| see each state's cue | `scaena inspect <bundle> --timeline` | `deck_inspect` with `timeline` |
| set motion | `scaena patch <bundle> --ops ops.json --dry-run`, then without `--dry-run` | `deck_patch`, with `dry_run` first |
| check | `scaena lint <bundle> --json` | `deck_lint` |
| see a frame mid-cue | `scaena render <bundle> --state <id> --t <ms> --out frame.png` | `deck_render` with `t` |
| watch it play (needs ffmpeg) | `scaena export <bundle> --format mp4 --states <id>,<id> --size 960x540 --fps 30 --out preview.mp4` | `deck_export` with `format: "mp4"` |

The theme names what you may use, under `motion` in the theme file the deck names (`themes/*.theme.json`):
- `presets` (Dusk: `fade`, `rise`, `grow`, `draw`, and `words` to enter; `pulse` for emphasis);
- `durations` (`fast`, `standard`, `slow`), `easings`, and `springs`;
- the limits, `maxConcurrent` and `maxBuild`.

## Procedure

1. **Read the timeline.** `inspect --timeline` lists each state's cue:
   - where it falls on the deck's timeline;
   - its transition;
   - each motion as placed, with how many words, lines, or marks it moves.
2. **Transitions.** A state without `transition` cuts.
   - The forms are a duration, by name or in ms (`"transition": "fast"`); `{ "duration", "ease" }`; or a spring (`{ "spring": "snappy" }`). A spring's name alone is not a duration, and is E102.
   - Between states that share nodes, the transition is the motion: give it `standard` or `slow` when the change is the point (a number that grows, a chart that moves to the next period, a word that changes).
   - Between slides that share nothing, keep it `fast` or cut, and bring the new slide's content in after it (step 4). Then the old slide clears before the new one builds.
3. **Entrances and exits.**
   - At most one `enter` per object that enters, from the theme's presets. An `exit` (from the state the node leaves) moves it out.
   - A node's own `enter` and `exit` run with the transition. A choreography item runs after it unless it says `timing: "with"`.
   - So put a motion on the node when the node always moves that way, wherever it appears. Put it in the state's `choreography` to order it after the transition or against other motions.
4. **Builds.** Order matters more than style.
   - A build of clicks is several states on one slide (`slide`), each adding what the next click shows. A timed build is one state's `choreography`.
   - Bring the claim in first, then the evidence. A setup and its punchline come in the order they are read.
   - `timing: "after"` (the default) waits for the transition, and `"with"` runs alongside it. `delay` waits that much more.
   - A `sequence` runs its items one after another, and a `parallel` starts them together.
   - `split` divides a target into units that a `stagger` brings in one after another: `lines`, `words`, or `glyphs` of text, a container's `children`, a chart's `marks`.
5. **Emphasis.**
   - `emphasis` is a preset with a `to` look, such as `pulse`. It points at something already on screen, once, after the transition. Set it on the node in a state's `props`, or as a choreography item.
   - It never replays on the next state.
   - A slide that is one state has nothing already on screen to point back at: a pulse right after an entrance is decoration.
6. **Charts.**
   - A chart's own `enter` preset (`grow`) moves its marks one by one, also when its data changes and it stays.
   - Moving to the next period is a data change in a new state, not an animation you write.
7. **Write it as a patch** (SPEC §7.3, `docs/schema/patch.schema.json`).
   - `apply_preset` with `motion` sets a node's `enter`, `exit`, or `emphasis`.
   - No semantic op sets a state's `transition`, `choreography`, or `hold`. Write JSON Patch at `/states/<i>/…`, and guard each with a `test` of the state's id, so a patch made against another order fails rather than lands on the wrong state:
   ```json
   [
     { "op": "test", "path": "/states/3/id", "value": "miles" },
     { "op": "add", "path": "/states/3/transition", "value": "fast" },
     { "op": "add", "path": "/states/3/choreography", "value": [
       { "sequence": [ { "target": "miles-title", "enter": "rise" }, { "target": "miles-chart", "enter": "grow" } ] } ] }
   ]
   ```
8. **Check.**
   - Lint for:
     - **W320:** more nodes moving at once than `motion.maxConcurrent`. It counts nodes, not the marks or units a split moves, so judge those by eye.
     - **W321:** a state's motions running past `motion.maxBuild`.
     - **W322:** a motion that moves nothing, such as an entrance on a node that does not enter, or a draw-on on a node with no outline.
   - Then render frames inside the cue (`--t`):
     - the transition's middle;
     - the middle of each motion `--timeline` lists;
     - a quarter, a half, and three quarters of the span.
   - Nothing should jump, overlap, or flash.
9. **Holds.**
   - A `hold` is how long a state rests before the timeline moves on. Video export and a player both advance by it (SPEC §2.4), so give holds to a deck that runs on its own, not to one presented live.
   - Size each to its reading: about four words a second, plus 1.5 to 2 seconds for each figure.
   - `inspect --timeline` shows the timeline the holds make, and `export --format mp4` plays it: each state's cue, then its hold. A state with neither has no frame in a video.

## Rules

- Motion explains: it shows what is new, what changed, or where to look. Nothing moves for decoration.
- Use one entrance style for like things across the deck; the theme's presets carry the brand.
- Keep a state's motions short of `maxBuild`: the audience waits through every millisecond.
- Text that changes between states morphs word by word on its own. Do not animate it by hand.
- Never write keyframes (`anim`) where a preset says the same thing.

---
name: motion-pass
description: Review and set a Scaena deck's motion so it explains rather than decorates and stays within the theme's limits. Covers transitions between states, entrances, builds, and emphasis. Works through the `scaena` CLI or its MCP server. Use when asked to "add animation", "make it move", "polish the transitions", "build this slide", or before exporting a deck to video.
---

# motion-pass

Motion is a cue: the transition into a state and the state's motions run on one clock (SPEC §2.4, §3.9).
- Nodes present in both states morph: text word by word, shapes point by point, chart marks by key.
- So most motion comes free from the states themselves.
- Add motion only where it says something: what is new, what changed, or where to look.

| To | CLI | MCP |
|---|---|---|
| see each state's cue | `scaena inspect <bundle> --timeline` | `deck_inspect` with `timeline` |
| set motion | `scaena patch <bundle> --ops ops.json` | `deck_patch` |
| check | `scaena lint <bundle> --json` | `deck_lint` |
| see a frame mid-cue | `scaena render <bundle> --state <id> --t <ms> --out frame.png` | `deck_render` with `t` |

## Procedure

1. **Read the timeline.** `inspect --timeline` lists each state's cue:
   - where it falls on the deck's timeline;
   - its transition;
   - each motion as placed, with how many words, lines, or marks it moves.
2. **Transitions.**
   - A state without `transition` cuts.
   - Give a state a transition when the change between states is the point: a number that grows, a chart that moves to the next period, a word that changes.
   - The forms are `"transition": "standard"`, `{ "duration", "ease" }`, or a `spring`.
   - Use the theme's names (`fast`, `standard`, `slow`, and its easings and springs), never milliseconds unless the theme has no name for one.
3. **Entrances.**
   - One `enter` per new object, from the theme's `motion.presets`. Dusk has `fade`, `rise`, `grow`, `draw`, `words`, and `pulse`.
   - Put it on the node (`enter`) when the node always enters the same way, or in the state's `choreography` when this state is special.
4. **Builds.** Order matters more than style. In `choreography`:
   - Bring the claim in first, then the evidence.
   - `timing: "after"` (the default) waits for the transition; `"with"` runs alongside it.
   - A `sequence` runs its items one after another.
   - Split lists and charts into units with a `stagger`, so the eye follows the order: `split: "lines" | "words" | "children" | "marks"`.
5. **Emphasis.**
   - `emphasis` points at something already on screen, once. It is a preset with a `to` look, such as `pulse`.
   - It never replays on the next state.
6. **Charts.**
   - A chart's own `enter` preset (`grow`) moves its marks one by one, also when its data changes and it stays.
   - Moving to the next period is a data change in a new state, not an animation you write.
7. **Check.** Lint for:
   - **W320:** more nodes moving at once than the theme's `motion.maxConcurrent`.
   - **W321:** a state's motions running past `motion.maxBuild`.
   - **W322:** a motion that moves nothing, such as an entrance on a node that does not enter, or a draw-on on a node with no outline.
   - Then render frames inside the cue (`--t`) at a quarter, a half, and three quarters of its span. Nothing should jump, overlap, or flash.
8. **Video.**
   - Give every state a `hold` (ms of dwell after it comes to rest) before export.
   - The timeline is the states' spans and holds, end to end.

## Rules

- Motion explains: it shows what is new, what changed, or where to look. Nothing moves for decoration.
- Use one entrance style for like things across the deck; the theme's presets carry the brand.
- Keep a state's motions short of `maxBuild`: the audience waits through every millisecond.
- Text that changes between states morphs word by word on its own. Do not animate it by hand.
- Never write keyframes (`anim`) where a preset says the same thing.

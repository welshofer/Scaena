# Projections: integrating the infographic, motion, and podcast pipelines

The deck is one projection of the spine. The infographic, the motion piece, and the podcast are others, made by pipelines that already exist (SPEC §10). They integrate against two things: `spine.json` and the `scaena` CLI. None of them reads `deck.json`, links the engine, or lays out text. If a pipeline needs something the spine does not carry, change the projection (`scaena-core::spine`), its schema, and this page together. Do not reach into the deck.

## The contract

```
scaena export <bundle> --format spine --out out/spine.json [--size 480x270]
```

```
out/
  spine.json            the projection: docs/schema/spine.schema.json
  renders/
    <beat>.png          the beat at rest, as a thumbnail: 480 px wide, or --size
    <beat>@9x16.png     the beat at rest in each other format the deck lists, at that format's canvas size
```

`spine.json` holds:
- **The deck.** `scaena` (the format version of the deck it came from), `title`, `lang` (BCP 47), `canvas`, and `formats`.
- **`spine`**, as the deck holds it (SPEC §3.11). It has sections, each with beats. Each beat has `claim`, `evidence` (`@data` refs, assets, URLs), `states`, `notes`, `duration` (estimated seconds), and `media`, which holds free-form hints for each projection (`infographic`, `podcast`, …).
- **`states`**, every state in cue-list order. Each has `id`, `slide`, `notes`, and `hold`. Its `start` and `span` (ms) place it on the global timeline (SPEC §2.4): its cue runs from `start` for `span`, then its `hold` follows.
- **`beats`**, keyed by id, in spine order. Each has:
  - `section`;
  - `state`: the state that shows the beat, the last of its states in the deck's order (a build at its fullest, as a PDF page shows it);
  - `start` and `end` (ms) on the global timeline;
  - `thumbnail`, and `formats` (each format's render).

  Paths are relative to `spine.json`. A beat that names no state of the deck has no `state`, no times, and no renders.

Write it outside the bundle: SPEC §3.1 reserves a bundle's own `spine.json` for an externalized spine, which is a different file. Without `--out`, the projection prints to stdout, timed but without renders. Over MCP:
- `deck_export` (`format: "spine"`) does what the command does.
- `spine_read` returns the projection without times or renders, and runs no layout.
- The schema is the resource `scaena://schema/spine`.

**Versioning.** The schema's `$id` carries the format version (`spine-0.9.json`). Build a pipeline against one version, and check `scaena` in the file before reading it.

**Errors.** Under `--json`, stdout is one JSON value: the result, or `{ "error": { "exit", "message", "plan"? } }` (SPEC §7.1). Exit 2 is bad input. Exit 3 means something is not built yet, and `plan` names the task that builds it.

## Infographic

Read the beats in spine order (`spine.sections[].beats[]`):
- `claim` is the beat's headline.
- `evidence` lists what supports it.
- `media.infographic` says how to treat the beat (in the example deck, `priority`).
- The portrait render is `beats[<id>].formats["9:16"]`. It is the beat laid out again on the 1080 × 1920 canvas, with the theme's 9:16 grid and slots (SPEC §3.4). It is not a crop. A deck that does not list `9:16` in `formats` has no portrait render; add it there. `scaena lint` then checks the layout in that format too.

For other sizes or formats, render the beat's state yourself:

```
scaena render <bundle> --state <beats[<id>].state> --format 9:16 --size 1080x1920 --out beat.png
```

For a chart's numbers, ask for the rows each chart and table reads, after its transform:

```
scaena inspect <bundle> --state <state> --data --json   # [{ state_id, …, data: { <node>: { source, columns, types, rows } } }]
```

## Motion

```
scaena export <bundle> --format mp4|webm|prores --out deck.mp4 [--size 1920x1080] [--fps 60] [--audio track.wav] [--states a,b]
```

- **Timing.** The video plays the global timeline: each state's cue, then its hold. With `--states`, it plays only those states, in that order (SPEC §10).
- **Chapters.** The file carries a chapter for each beat, titled by its claim. In MP4 and QuickTime this is a chapter track; in WebM, Matroska chapters. If no beat names a state, that state gets a chapter of its own, titled by its slide's id. A deck without a spine gets no chapters.
- **The `--json` result** lists the same `chapters` as `{ beat, title, start, end }`, in ms into the video. It also lists `timeline`: each state's `start`, `span`, and `hold`.
- **Finding a beat.** Played whole, the video's clock is the global timeline, so `spine.json`'s `beats[<id>].start` and `end` are the beat's place in the video. With `--states`, read the result's `chapters` instead.
- **A clip per beat.** Cut the video at a chapter's marks. Or export the beat's states alone, in the deck's order, with `--states`.
- **Single frames.** Frame *k* is the moment `scaena render --state <state> --t <ms>` draws. A pipeline that composites its own motion can render stills at the moments it needs.

## Podcast

- **Read** `title`, and `lang` to pick the voice.
- **Script each beat** in spine order. Use `media.podcast.script` where the beat has one; otherwise use its `claim` and `notes`.
- **`duration`** is the author's estimate of the beat's length in seconds: a budget for its script.
- **Section `title`s** mark the breaks.

To lay the narration under the video:
1. Synthesize a clip for each beat.
2. A beat lasts `end − start` ms on the timeline.
   - Where the clip is longer, lengthen the beat. Raise the `hold` of its `state` by the difference. `i` is that state's position in `states`:

     ```
     echo '[{ "op": "add", "path": "/states/<i>/hold", "value": <its hold + the difference, ms> }]' | scaena patch <bundle> --ops - --json
     ```

     `patch` refuses a change that would leave the deck invalid, and reports the lint delta.
   - Where the clip is shorter, pad it with silence.
3. Export the spine again for the new times.
4. Join the clips in beat order. Fill the gaps with silence, including the stretches of any states no beat names.
5. Pass the joined track to the video export with `--audio`. It plays from the first frame.

## Not done

- A video plays only in the deck's own format. A portrait video for a 9:16 motion piece is not built yet (PLAN 1.21).
- The timeline in `spine.json` is the deck's own format's. A cue that counts lines can run longer or shorter in another format, where the text breaks differently (SPEC §3.4).

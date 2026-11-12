# An agent makes a deck with MCP alone (gate 1)

Gate 1's first criterion asks for this run. From Claude Code, using only MCP, an agent makes a 12-state deck from a CSV and a one-paragraph brief. It lints the deck to zero errors, renders every state, and exports a PDF and a 1080p60 video. The second criterion asks it to re-theme the deck with `theme_apply` and to explain the lint delta. The fifth asks for a narrative lint finding on the authorability deck that is a true positive, repaired by an agent. This file records all three, and what the runs found in Scaena on the way.

**Verdict: criteria 1, 2, and 5 are met.**

- **Criteria 1 and 2.** In attempt 3, an agent with nothing but the scaena MCP server made the deck in 8.7 minutes and 80 turns. It linted the deck to zero errors, rendered every state, and exported a 12-page PDF and an 87-second 1080p60 video with a chapter per beat. It then re-themed the deck to Ember and took the 14 errors that added back to none.
- **Criterion 5.** A fresh agent repaired a true W423 on the authorability deck.
- **Fixes.** Attempts 1 and 2 found eight problems in Scaena, each fixed in #53–#58.
- **Open.** The runs found more than those eight. They are under Findings, with what became of each.

## Method

- **The client.** Claude Code 2.1.288, headless (`claude -p`), with one MCP server: `scaena mcp`.
  - `--strict-mcp-config --mcp-config mcp.json` gives it that server and no other.
  - `--tools "ListMcpResourcesTool,ReadMcpResourceTool"` leaves it no built-in tools but the two that read MCP resources: no shell, no file reads or writes, no web.
  - `--allowedTools mcp__scaena` lets it call the server's 12 tools without asking.
- **The server.** Attempt 3 ran a release build of main at f348af6 with #55–#58 applied, the fixes then in review.
- **What it knew.** The prompt below, and what the server serves:
  - the tools' descriptions and schemas;
  - the format's schemas, the lint catalog, and SPEC;
  - the skills and the example decks and themes.

  It could not read the code, the docs folder, or any file on disk, the CSV included, except through the tools.
- **Judging.** I read the transcript and lint the bundle it left. I also render every state, read the PDF with `pdfinfo` and `pdftotext`, and probe the video with `ffprobe` and by its frames.

The prompt, verbatim, with the run's scratch directory written `<run>`:

> You are making a presentation with Scaena, a presentation engine. Your only tools are the scaena MCP server's tools and its resources (`scaena://...`): the format's schemas, the SPEC, the lint catalog, example decks, and skills for authoring. Read what you need from them; you have nothing else.
>
> The brief, from the city's transportation office:
>
> > Ridgeline's bike-share pilot ran in three neighborhoods through 2026, and <run>/ridgeline-rides.csv has its monthly trips by neighborhood. Trips grew from 4,200 in January to 18,900 in December. Hilltop, the steepest neighborhood, barely moved until e-bikes arrived in June, then grew sixfold by December. The pilot cost $1.4 million to set up, and by December a trip cost about $1.20 to run, against $2.90 for the bus. Next Tuesday the city council votes on expanding to two more neighborhoods in 2027 for $1.2 million. Build the deck for that meeting: make the case, and end on the ask.
>
> Please:
> 1. Make a bundle at <run>/ridgeline (a new directory) from the theme docs/examples/themes/dusk.theme.json, with the CSV attached.
> 2. Write a 12-state deck for the meeting: a spine of beats with their claims, charts from the data, one state per click.
> 3. Lint it until it has no errors, and look at every state by rendering it. Fix what looks wrong, not only what lint finds.
> 4. Export a PDF to <run>/out/ridgeline.pdf and a 1080p video at 60 frames a second to <run>/out/ridgeline.mp4.
> 5. Then re-theme the deck with docs/examples/themes/ember.theme.json, using theme_apply. Lint and look again, and fix or explain whatever the new theme changes.
>
> As you go, note what you did, and anything in the tools or the docs that was unclear or got in your way. End with a short report: the beats, the states, the lint before and after the re-theme, the exports, and those notes.

The CSV is `docs/examples/data/ridgeline-rides.csv`: monthly trips for three neighborhoods, January to December 2026. Its totals run from 4,200 to 18,900. The first two attempts' brief said "by December a trip cost about $1.20". The second agent pointed out that this cannot be $1.4 million over the year's trips, so the third's says "to run".

## Three attempts

The first two runs found eight problems in Scaena. The third ran with all eight fixed.

### Attempt 1: the resources would not load

Claude Code opens with `server/discover` and names protocol 2026-07-28 in every request's `_meta`. That protocol makes a list or read result without `ttlMs` and `cacheScope` invalid (SEP-2549). The server, on rmcp 3.5, set neither, so every `ReadMcpResourceTool` failed:

```
Invalid result for resources/read: [ { "expected": "number", "path": ["ttlMs"], … },
  { "values": ["public", "private"], "path": ["cacheScope"], … } ]
```

`ListMcpResourcesTool` said "No resources found", and I stopped the run. Logging the handshake turned up two more problems:

- **History.** The server recorded its client's name in `initialize`, which discovery never sends, so an agent's edits went into a bundle's history as plain `agent`.
- **Schemas.** The tool schemas carried number formats (`uint32`, `double`) that the client's validator warned about, 24 times a session.

#53 fixes all three (SPEC §7.2).

### Attempt 2: the video timed out, and the retry broke it

With the resources readable, the agent built the deck in 6.9 minutes over 73 turns. It linted the deck to zero errors under Dusk, exported the PDF, re-themed to Ember, and linted to zero again. The video failed:

- **Claude Code gives a tool call 60 seconds.** A 1080p60 video of this deck takes about 70 seconds on this container, so `deck_export` timed out. Progress notifications do not extend the limit: a probe server sent eleven over 75 seconds, and the call was still cut off at 60 with `notifications/cancelled`.
- **The export did not stop when the call did.** The agent retried, which started a second export of the same file. Both ffmpeg processes wrote `ridgeline.mp4.partial`, and the file that remained had no streams.

#54 keeps a long export going past the call:

- The call answers within 40 seconds with `running` and how far the export has got.
- The same call again waits for the rest.
- Another export of a file still being written is refused.
- Each export writes a partial file of its own.

Four more findings came from attempt 2's notes and renders, and from the files it left:

- **Re-theming meant guessing names.** After `theme_apply` to Ember, lint found nine E102s: a layout, slots, and a shader preset that Ember does not have. Nothing listed what Ember has, and the agent could not read the theme file. So it guessed names in dry-run patches until lint stopped objecting. #55 makes each E102 for a theme name list the names of that kind the theme has.
- **A chart cut off its series names, and lint said nothing.**
  - Under Ember, "Riverside" and "Old Town" ran 7 cu past the line chart's side.
  - The room beside the plot assumed a name starts `lead` past the line's last point, but the names were drawn `lead` past the point's dot.
  - The agent saw it in the render and moved the names to a legend.
  - #56 places the names where the room assumes. It also makes E100 report chart text cut off at the chart's side.
- **Dates printed as ISO and ran together.** A stacked bar of months printed 2026-01-01 to 2026-12-01 under its bars, each label wider than its band. #57 prints dates on a category axis by their unit (Jan 2026, Feb, …, Dec) and keeps every k-th label where they crowd.
- **The PDF was 31 MB.** Its two mesh backdrops were lossless images at twice the canvas, 14.7 MB each. #58 writes an opaque shader as JPEG at quality 90, about 1 MB, and keeps a translucent one whole.

### Attempt 3: met

The agent took 80 turns and 8.7 minutes: 79 tool calls, among them 9 resource reads, 17 patches, 6 lints, and 32 renders.

1. **Reading** (calls 1–8). It read the author-deck, chart-from-data, retheme, and motion-pass skills, the trails example, Dusk's theme, and the lint catalog.
2. **Making the bundle and reading the data** (9–18).
   - `deck_create` made the bundle on Dusk with the CSV attached.
   - It checked the brief's figures in a throwaway line chart: 4,200 in January, 18,900 in December, and Hilltop 680 in May to 4,200 in December, so "sixfold" holds from May.
   - A probe table was refused (E103: its first column repeats, and a table keys each row by it).
   - The deck schema (74 KB) and SPEC (157 KB) came back as files the agent had no tool to open. It learned `aggregate`'s syntax from the E106 that a dry run returned.
3. **Writing and checking** (19–42).
   - It wrote the deck in two patches, of 3 and 12 ops.
   - Lint found one error, a title too long for its box, which it shortened.
   - It rendered all 12 states, re-placed the card grids it judged too tall, and linted to zero.
4. **Exporting** (43–49).
   - The PDF came back at once: 12 pages, 1.6 MB.
   - The video answered `running` five times (376, 3,448, 4,896, 5,024, and 5,144 of 5,219 frames), then returned it on the sixth call, about 4 minutes in. In attempt 2 the export took about 70 seconds. Here the container's four cores were also building Scaena: I ran a `just check` in that window.
5. **Re-theming** (50–79): below, under criterion 2.

Judged afterwards:

| | |
|---|---|
| Lint | 0 findings, on Ember as the agent left it |
| PDF | 12 pages of 960 × 540 pt, tagged, titled "Ridgeline bike-share: expand in 2027", 1.6 MB; the text copies as the slides read |
| Video | H.264, 1920 × 1080 at 60 fps, 5,219 frames, 87.0 s, 6.8 MB, 10 chapters; frames at 3, 25, 47, and 84 s show the cover, the Hilltop chart, the cost cards, and the ask |

The video's chapters are the beats' claims:

```
 0.0 s  The Ridgeline pilot worked, and the case to expand is strong.
 5.8 s  Monthly trips grew from 4,200 in January to 18,900 in December.
20.2 s  Hilltop, the steepest neighborhood, barely moved until June.
28.2 s  E-bikes arrived in June, and Hilltop trips grew sixfold by December.
42.9 s  By December a bike trip cost about $1.20 to run, against $2.90 for the bus.
51.2 s  The $1.4 million set-up cost bought a system that now runs cheaply.
57.7 s  E-bikes unlock steep neighborhoods.
63.2 s  Demand, cost, and proof all point to expanding.
71.8 s  The proposal is two more neighborhoods in 2027.
80.0 s  Approve $1.2 million to expand to two more neighborhoods in 2027.
```

## The deck

The spine is 5 sections of 10 beats over 12 states, one state per click:

| Section | Beat | Claim | States |
|---|---|---|---|
| opening | open | The Ridgeline pilot worked, and the case to expand is strong. | `cover` |
| growth | growth | Monthly trips grew from 4,200 in January to 18,900 in December. | `growth`, `total` |
| | hilltop-stalled | Hilltop, the steepest neighborhood, barely moved until June. | `hoods` |
| hilltop | ebikes | E-bikes arrived in June, and Hilltop trips grew sixfold by December. | `ebikes`, `sixfold` |
| | lesson | E-bikes unlock steep neighborhoods. | `lesson` |
| cost | cheaper | By December a bike trip cost about $1.20 to run, against $2.90 for the bus. | `cost` |
| | setup | The $1.4 million set-up cost bought a system that now runs cheaply. | `setup` |
| ask | recap | Demand, cost, and proof all point to expanding. | `recap` |
| | proposal | The proposal is two more neighborhoods in 2027. | `plan` |
| | ask | Approve $1.2 million to expand to two more neighborhoods in 2027. | `ask` |

- **The charts read the CSV directly.**
  - `total` is a bar of monthly trips, which a `dataTransform` sums from the rows by month.
  - `hoods` draws a line per neighborhood, with Hilltop in the accent.
  - `ebikes` is the same chart node filtered to Hilltop, with an "E-bikes arrive" callout at June, so the marks move from one state to the next.
- **The rest.** Three stat slides (18,900, 6x, $1.4M), the cost in two cards ($1.20 against $2.90), a statement, a recap and a plan in cards, and the ask.
- **Backdrops.** Under Dusk the cover has a mesh and the ask has noise.

The spine and the states disagree in one place. The spine puts `lesson` before the cost beats, and the states put it after them. The PDF reads in spine order and the video in state order, so they tell the story in different orders (Findings). Lint now reports it as W426 (PLAN 1.36).

Under Dusk, from the PDF:

![The 12 pages of the PDF, under Dusk](agent-run/dusk.jpg)

Under Ember, as the agent left it:

![The 12 states rendered under Ember](agent-run/ember.jpg)

## Re-theme (criterion 2)

The dry run, then `theme_apply` to Ember, added 14 errors and removed none. Each is a name Dusk has and Ember does not:

| Errors | What the deck names | What Ember has (from the E102s) |
|---|---|---|
| 2 E102 | shader presets `mesh-soft`, `noise-fine` (the cover's and the ask's backdrops) | none |
| 6 E102 | layout `figure`, in six states | title, statement, poster, full, chart, narrow-chart, stat, art-left, art-right |
| 6 E102 | slots `meaning` and `aside` in layout `stat`, on three stat slides | kicker, number, claim, detail, under, canvas, grid |

The agent fixed them from those lists:

1. **Layouts and slots.** It moved the six states to `chart`, and the stat slots to `claim` and `detail`.
   - Its first patch also kept the chart states' slots `main` and `footer`. The patch was refused, with E102s naming `chart`'s slots (kicker, header, chart, side, canvas, grid).
   - The next patch used `chart` and `side` and was applied.
2. **Shaders.** It removed the two backdrops, since Ember has no shader presets. The cover and the ask are plain dark under Ember.
3. **Cards.** Once every name resolved, the states laid out under Ember for the first time, and lint found 19 errors the dry run could not show: it lays out only the states whose names resolve. Ember's numerals are larger, and in the cost, recap, and plan cards they no longer fit:
   - 3 E100s, numerals too wide for their cards;
   - 16 E101s, numerals over their labels.

   `fit: shrink` on eight numerals cleared 6, and larger gaps in the cards the other 13.
4. **The cost slide.** By eye, it moved a source note that lay over the cost cards, which no lint reported.

Lint after: 0 findings. The agent's account of what Ember changed: the numerals are very large and orange, the charts narrower with a dot on each point, the source notes in a column at the right, and no overrides in use. It also noted that the deck no longer re-themes back to Dusk without the same edits in reverse.

## Narrative lint, repaired (criterion 5)

The authorability deck (`docs/examples/authorability/`, PLAN 0.13) lints to two warnings:

```
warn  W310 /nodes/rev-chart/labels: chart `rev-chart`: the value labels of `2026-Q4Core` and `2026-Q4Pro` overlap
warn  W423 /spine/sections/0/beats/0/evidence/0: beat `thesis` cites `@revenue`, and none of its states shows it
```

**W423 is a true positive.**

- **The finding.** The `thesis` beat opens the deck. Its claim is "Revenue doubled since Q1 2026, and Pro grew from a quarter of it to over a third", and it cites `@revenue`. Its one state, `cover`, shows a title and a subtitle ("Q4 2026 board update") and no figure, chart, or table. So the spine says the opening shows evidence it does not show.
- **Why it matters past the slide.** The spine drives the PDF's outline, the video's chapters, and the infographic and podcast projections. Those would cite data for a beat that shows none.
- **The right pattern is in the same deck.** Its closing beat restates the claim with a subtitle marked `evidence` that carries the figures.

**A fresh agent repaired it,** with MCP alone and set up as above.

- **The setup.** It worked on a copy of the bundle that keeps history (`save --history`).
- **The prompt.** It named the narrative rules (W420–W425) and asked the agent to repair what is real, leave what is not, and change nothing else. It did not name the finding or a fix.

The agent's report, verbatim:

> **Narrative finding.** Lint reported one: W423 on beat `thesis`. W420–W422, W424 and W425 found nothing.
>
> **W423: real, repaired.**
> - **Finding:** `thesis` cited `@revenue` as evidence, but its only state is `cover`. That state is a title and subtitle with no chart or table, so the citation pointed at nothing the audience could see.
> - **Why it was real:** The spine claimed evidence the slide doesn't show. The cover is meant to state the answer, and its notes say "the next five cues prove it". The proof is in the later beats (`doubled`, `accelerated`, `pro-led`, `pro-third`), and each of those cites and shows `@revenue`.
> - **Change:** I dropped the citation from `thesis`, so the spine now says the opening makes the claim and doesn't cite evidence. I didn't put a chart on the cover. That would duplicate the later states and crowd the title.
>
> **What I changed:** One edit, removing `/spine/sections/0/beats/0/evidence/0`. No state or node changed, so there was nothing to render.
>
> **Lint before and after:**
> - **Before:** 0 errors, 2 warnings (W310, W423).
> - **After:** 0 errors, 1 warning (W310).
>
> W310 is a layout finding, not a narrative one. In state `products`, the Q4 value labels on `rev-chart` for Core and Pro overlap. I left it as instructed.

The agent took 5 turns and 18 seconds. It read no resources: the tools' descriptions and lint's hint ("Show it in one of the beat's states …, or drop the citation") were enough.

It is a fair repair, of the two the hint offers. The other was to show the figures on the cover, as the closing slide does. Either one makes the spine true. The bundle's history records the change as the agent's:

```
user               history begins   (268 ops)
agent:claude-code  patch: remove    (1 op)
```

## Findings

### Fixed

| What the runs found | Where | Fix |
|---|---|---|
| Resource reads invalid under protocol 2026-07-28 (no `ttlMs`, `cacheScope`) | attempt 1 | #53 |
| An MCP client's edits recorded as `agent`: discovery sends no `initialize` | attempt 1 | #53 |
| Number formats in the tool schemas that the client's validator rejects | attempt 1 | #53 |
| A video longer than the client's 60 s wait cannot finish, and a retry corrupts it | attempt 2 | #54 |
| E102 says what is missing, not what the theme has | attempt 2 | #55 |
| A line's end names drawn the point's radius past the chart; lint silent | attempt 2 | #56 |
| Dates on a category axis printed as ISO, overlapping | attempt 2 | #57 |
| A 31 MB PDF: opaque shaders kept lossless | attempt 2 | #58 |

### Open

Each is a task in PLAN 1.33–1.36, or a note for a theme.

- **Large resources do not reach an MCP-only agent** (1.33).
  - Claude Code saves a resource over its output limit to a file, and this agent has no tool to open files. The deck schema (74 KB) and SPEC (157 KB) never reached it.
  - Agents learned `dataTransform`'s syntax from E106 messages, and attempt 3 said so.
  - Serve SPEC by section, `docs/spec/format.md` and `expr.md` on their own, and the example themes, each small enough to arrive whole.
- **The shipped themes do not share a vocabulary** (1.34).
  - Dusk's `figure` layout, with its `main` and `footer` slots, is Ember's `chart`, with `chart` and `side`.
  - Dusk's `stat` slots `meaning` and `aside` are Ember's `claim` and `detail`.
  - Ember has no shader presets.
  - The retheme skill calls names the swap contract, and our own two themes break it, so a deck cannot move between them without edits.
- **`theme_apply` leaves a deck invalid** (1.35).
  - Applied for real, it writes the new theme even when that adds errors. Attempt 3's deck stayed invalid for four patches.
  - `deck_patch` can re-theme and fix in one all-or-nothing patch (the `retheme` op), but the skill does not say so.
- **Lint missed what the renders showed** (1.36):
  - E101 does not judge a container's children against the nodes around it: a source note over the cost cards (attempt 3).
  - No rule reports a spine whose order differs from the states', which leaves the PDF and the video in different orders (attempt 3).
  - Nothing flags a chart squashed into a short cell by a theme's grid (attempt 2).
- **`at.align` on a grid placement did nothing** in attempt 3's cost slide. It goes with 1.36, once I know whether SPEC means it to apply there.
- **For the themes** (Jay):
  - Dusk's `figure` header is too narrow for a 30-character headline on one line.
  - Card panels stretch to fill their rows.
  - Fraunces draws × hairline-thin at display sizes, so both agents wrote "6x".
- **`pdfinfo` warns** "Suspects object is wrong type (boolean)" on every PDF Scaena writes, attempt 2's included. The files read correctly; the warning is about krilla's `MarkInfo`.

## Files

- `docs/examples/ridgeline.deck.json`: the deck attempt 3 made, as it left it, on Ember, but for the names PLAN 1.34 changed in the shipped themes: its layout `chart` is now `figure`, and its slots `chart` and `side` are `main` and `note`. And for W426 (PLAN 1.36): its `lesson` state plays after `sixfold`, where the spine tells it, so the video and the PDF tell the story in one order. The rest of its bundle was byte for byte the shared files beside it: `data/ridgeline-rides.csv` (the CSV the agents were given), `themes/`, and `fonts/`. It lints clean, and a test keeps it so.
- `docs/examples/agent-run/`:
  - `attempt-2.md`, `attempt-3.md`, and `narrative-repair.md`: the condensed transcripts, as tool calls and the agents' reports. Attempt 1 stopped before writing anything.
  - `dusk.jpg` and `ember.jpg`: the contact sheets.

## A fourth run: a French deck from a PDF and a workbook (agent dogfood 4)

On 2026-11-11, an agent with nothing but the scaena MCP server made a deck grounded in what a person hands over. It ran on #1084's build, which adds `deck_generate`'s `sources`.

**The brief.** A ten-minute proposal to Montclar's city council, in French:

- **The ask:** extend the city's bike share, VéloMontclar, to its northern districts.
- **What it was given:** a two-page PDF annual report (`rapport-2025.pdf`) and a workbook of three sheets (`velo-2025.xlsx`). The sheets are `Annuel` (trips, bikes, and stations by year), `Quartiers` (trips per station by district), and `Mensuel` (trips by month).
- **How it called:** one `deck_generate` call, with its seven slides written in Narrato's JSON after `ferry.generate.json`. The call carried an outline of four beats, `language: "French"`, `minutes: 10`, and both files in `sources`. Each slide named its file in `sourceRef`, and each chart named its sheet in `chartData.sourceId`.
- **The read:** `deck_lint`, `deck_health`, and `deck_render` of three slides.

**What it found**

| What the agent met | Fix |
|---|---|
| The Big Number "1 240 000", typed as French is typed with plain spaces, drew **"1"**. The rest of the figure was gone, its claim set under it. Lint and health found nothing. | #1105: thousands a plain space groups are one figure, set with spaces no line breaks at, so the hero quotes its cell, and a group a letter or a hyphen runs on from stays a figure of its own ("5 100-metre"). Slide 6's three bullets led by "18", "2,4 M€", and "310 000 €" take Numbers with it. |
| Charts naming their sheets as the workbook does, "Annuel" and "Quartiers", stayed on their own rows. The sheets are kept as `velo-2025-annuel` and `velo-2025-quartiers`. `unbound` said so and named the ids, but naming them by id set "Source : velo-2025-annuel · velo-2025.xlsx" on the slide. | #1115, on #1083's binding: a chart binds to a kept sheet by the sheet's own name, as #1083 binds one that names the workbook, the sheet that holds its figures found. Its source line reads "Source : Annuel · velo-2025.xlsx". |
| `deck_render` of a state guessed from its headline (`les-trajets-ont-doubl-depuis-2021`) said only "unknown state". | #1106: it names the state meant, or the deck's first few, as every other tool does. |
| The bar chart "Une station au nord fait cinq fois moins de trajets qu'au centre" lit Centre, the largest bar, where the headline is about Nord. | #1111 (W1): a headline that compares two bars lights its subject. |
| Health's pacing scored 100 while the notes ran about a minute of the ten-minute slot. | Routed: no beat is timed, so the talk's words are a floor. `deck_generate`'s `notes` had said the slot (10) and the run (2.9) before any read; #1110 says that the run is the talk as health times it, not the notes alone. |

**What held**

- **Sources.** Each source is kept in `meta.narrative.sources` by its file's name, the PDF's bytes nowhere in the bundle. Each slide's `sourceRef` is cited in its beat's evidence (`source:rapport-2025.pdf`, `source:velo-2025.xlsx`). Each sheet is kept as a data source, `Mensuel` included, which no slide used.
- **Quoting.** The line chart's callout quotes the 2025 cell of `Annuel`, so a `data_edit` of the cell sets the slide again.
- **Health's findings.** Its one source finding was true: the closing writes "18" and names no source. Intensity, tone, and the big idea say they are not read in French, as designed.
- **The renders** agreed with lint's zero, but for the "1" above.

The agent guessed in three places: a sheet's id, a state's id, and whether a chart may name a workbook. The first and third are what the binding fix answers, and `deck_render`'s refusal now answers the second.

## A fifth run: a technical talk from a repository and a docs page (agent dogfood 5)

On 2026-11-11, an agent with nothing but the scaena MCP server made a technical talk grounded in a repository and a web page. It ran on main at 3e8c3d4b.

**The brief.** A twenty-minute talk to engineers, in English, to inform:

- **The talk:** "One core, every screen, the same frame", how one Rust core draws a deck the same in the browser, on the Mac, in a PDF, and in a video.
- **What it was given:** `github:welshofer/Scaena` and `https://docs.rs/parley/latest/parley/` in `sources`.
- **How it called:** one `deck_generate` call of eleven slides in Narrato's JSON, with an outline of four beats and `minutes: 20`. Two slides were asked as `"Code"`: a Rust signature, and the two TOML lines of `.cargo/config.toml`.
- **The read:** `deck_lint`, `deck_health`, `deck_render` of three slides, repairs by `deck_patch`, and `deck_export` as a PDF with notes (eleven pages, its metadata the deck's).

**What it found**

| What the agent met | Fix |
|---|---|
| Both slides asked as Code fell to Bullet List, never set as code. The TOML's quotes were curled (`[“-C”, …]`), which corrupts it, and the Rust was set in Inter, `->` drawn as an arrow. The agent's repair by hand took three patches and then met E100 and W203. | #1122: a slide asked as Code, or one item that reads as code in the language of its subtitle's file, takes Title and Code, its item set as code in that language (else `plain`) and never typeset. Lint is 0, and the TOML is colored with its quotes straight. |
| A `sourceRef` written as the source was given, `github:welshofer/Scaena`, matched no source: not the repository's name ("GitHub: welshofer/Scaena (Rust)") nor its address. So 9 of 11 beats cited nothing, and health warned "Slide 9 writes 0.44, and names no source", which was false. | #1121: a `sourceRef` matches a source's address too, `github:OWNER/REPO` and a URL alike, case and a closing slash aside. |
| Ten slides for a twenty-minute talk were refused with "the model wrote 10 slides, and a deck of this length has at least 11", which said neither the length asked nor the ways on. | #1123: "10 given, and the length asked, `11-15 slides`, the length 20 minutes take, holds at least 11: write 1 more, or ask a `length` that holds 10". |
| `github:casey/just` came back 403, "a token allows more". | None: this container's proxy reaches only the repositories in its scope. A repository in scope was fetched in about 5 s. |
| Health's pacing scored 100 while the notes ran about 3 minutes of the 20-minute slot. `deck_generate`'s `notes` said the slot (20) and the run (5.7). | Routed to Jay (decision d): no beat is timed, so the talk's words are a floor, as in the fourth run. |

**Routed, not fixed here**

- Health read slide 3's headline as counting five while the slide lists one: the five were a signature's parameters, its code read as a list of one.
- The tone lexicon read "emergency" in "emergency breaks", a typesetter's term, as grave.
- "Three Column" is no layout's name; `fell` said so, and the slide was set as its items read.
- A "Before:" and "Between:" pair took Myth and Fact. It reads acceptably.

**What held**

- **Sources.** Each source is kept in `meta.narrative.sources` with its address, and the docs page's `sourceRef` was cited.
- **Refusals that said why.** A `deck_patch` that made a text code and kept its `list` was E106, which says why.
- **Health's other findings,** the arc's two bends and the turns, read as true.

## A sixth run: a deck revised from a reviewer's notes (agent dogfood 7)

On 2026-11-12, an agent with nothing but the scaena MCP server revised a deck to answer a reviewer. It ran on main at 9c56bf98.

**The deck.** The pitch made by `deck_generate` from `tests/fixtures/dogfood/pitch.slides.json`, with three faults put in:

- **A repeat.** Slide 6 says slide 4's accessorials again, with slide 5 between them.
- **A dragging middle.** Slides 10–12 are three bullet slides that tell slide 8's How It Works again, a step each.
- **A vague ask.** The ask (17) is "Join us as we transform freight audit", with no amount.

Its brief is twelve minutes, so 5–10 slides; the deck has 18.

**What the agent was handed:** "The middle drags, the ask is unclear, and slide 6 says the same as slide 4."

**The tools it used:**

- `deck_health` and `deck_read` to find the faults;
- `slide_write` with `replace` and `id` to write slide 6 and the ask again;
- `deck_patch` for `restated`'s fixes, `remove_slide`, `set_state`, and `brief`;
- `deck_history` to compare, and to take a cut back;
- `deck_render` to look;
- `deck_export` of a PDF with notes (15 pages).

**What it found**

| What the agent met | Fix |
|---|---|
| Health named none of the reviewer's faults. Its cut, "slides 2, 3, 4, 8, 9, 11, 16 and 17 add least … Fold them in, or cut them", named the hook, the $9B big number, the accessorial chart, How It Works, and the ask. It read each slide against the slide just before it, so a coherent talk's best-joined slides were named for cutting. Its repeat finding read only neighbours, so slide 6 (two after 4) and 10–12 (two after 8) went unseen. The agent found slide 6 and the ask only by the `claim` warnings on slides 5–6 and 15–18. | #1161: a repeat is read against the nearest earlier slide that makes a point, unless the later writes a figure of its own. The cut names the slides an earlier slide said most of, never a cover, divider, agenda, close, or ask. On the faulted deck: "Slide 6 says again what slide 4 said", "Slide 11 says again what slide 8 said", and a cut of 4, 6, 10–15. No score moves on the nine decks compared. |
| After a bad cut (slide 3, as the old cut named it) and a notes edit after it, `restore` could only take the whole deck back, the notes edit with it. Bringing the slide back alone took five JSON Patch ops rebuilt from `at`'s deck: its state, three nodes, its beat, and the section's index. | #1172: `restore` with `slide` puts back one slide as the version held it, with its steps, the objects it alone shows, its data, and its beat. Every edit made since stays, and a dry run names it "Slide 3: …". |
| The dragging run could only be cut. The editors' Fold Into Slide N was no op or tool. | #1175: `fold_slide`, alone in its patch, folds as the editors' Fold does, `folded.says` saying what goes where beforehand. |
| `deck_render` with `out: "r/vs.png"` failed: "No such file or directory". | #1163: a render makes its folder, as an export does since #1140. |
| A `brief` op answered `summary: {}`, and the history said it as "An edit". Three slides cut were "A slide deleted". | #1167: the summary lists the brief's fields changed, and the history says every op and counts a run of one (`remove_slide ×3`, "3 slides deleted"). |

**Routed, not fixed here**

- **Health didn't move.** It read 83 on the faulted deck and 83 on the revised one, through every step.
- **Why.** Cutting the three padded slides took pacing from 60 to 75 and balance from 98 to 87 ("1.8 points each"): balance rewards three bullets of filler. The coordinator took it to Jay.

**What held**

- **`slide_write`'s `replace`.** With an `id`, it wrote slide 6 and the ask again in their beats. Its `restated` gave the `replace` that sets each beat's claim to the new headline, which `deck_patch` takes as `ops`.
- **The cut.** `remove_slide` took the three dragging slides with their beats.
- **The history.** `deck_history`'s `compare` showed the cut slide and its beat as they were.
- **The look.** The renders agreed with lint.

The agent guessed in two places:

- **Which slides to cut.** Health's list misled it; #1161 answers that.
- **How to bring one slide back.** It rebuilt the patch by hand; #1172 answers that.

## A seventh run: a pitch built by hand, its spine first (agent dogfood 8)

On 2026-11-12, an agent with nothing but the scaena MCP server built a 10-minute seed pitch from nothing, with no `deck_generate`. It ran on main at 7cc0a108.

**What it was given**
- **The brief.** Seep, acoustic sensors that find water-main leaks, pitched to a climate fund for a $3M seed.
- **A CSV.** A pilot city's water lost each month, 2025.
- **One photo.** A sensor.

**The path it took**
1. `deck_create`.
2. The `brief` op, with `minutes: 10` and no length.
3. `data_attach` for the CSV.
4. `spine_update` with six claims written first, across problem, solution, proof, and ask.
5. `slide_write` over the blank slide for the cover, then a `slide_write` with `beat` for each claim.
6. `state_start` with `claim` for the head-to-head of what a burst costs against a leak found early.
7. `image_add` for the photo.
8. `deck_lint` and `deck_health` until nothing it could act on was left.
9. `deck_export` as a PDF with notes, and as HTML.

**Calls.** It took 27 calls in all:
- 4 to set the deck up: `deck_create`, the brief, the data, and the spine;
- 7 for slides;
- 7 to repair what the tools did;
- 6 of lint and health;
- 3 exports.

Lint was 0 after the last slide. Health went from 71 to 74 and stopped asking for anything the agent could act on but its own words.

**What it found**

| What the agent met | Fix |
|---|---|
| W429 said the beat for a burst main's cost was untold: "Start a slide for it (`state_start` with its claim)". Doing so made a second beat, `cost-2`, with the same claim, and left `cost` untold. So W429 traded for W425, and the spine needed a JSON Patch. | #1181: a claim a planned beat already makes joins that beat. W429's hint names `slide_write` with that beat. |
| The cover, written over `deck_create`'s blank slide, got a beat of its own at the spine's end, after the ask. Each later slide given a `beat` stood before it, and the PDF would have told the cover last. The beat was named `cover-they-burst`, and the result named another. | #1182: the beat goes where its slide plays (first, here) and is named as the slide. |
| Health paced the 10-minute brief as "a talk with no length asked for … about 18" slides, pacing 45. | #1143, already open on Jay's word, reads the minutes as 5–10 slides. |
| Health asked slide 6 for intensity 0.78, the most its words carry, and the next read found its words "flatter than it was written to be". 0.7776 had been rounded past the gap. | #1186: a plan is rounded toward the words it is held to. No score moves. |
| The PDF with notes printed the sensors slide's beat's notes, not the slide's own. The slide builds by bullet, `slide_write` keeps its notes on its first step, and the page is its last. | #1187: a page prints the notes its steps keep before its beat's. |

**Routed, not fixed yet**
- **The cover's notes budget was 1,427 words.** The budget divides the slot by the slides written so far, not by the beats the spine plans.
- **`set_state` with an unknown `layout`** got no "did you mean". E102 listed the theme's first 24 of 181 layouts.
- **`image_add` answers with the JSON Patch,** where `deck_patch` answers with a summary.
- **`fell` doesn't map the library's names.** "Image Left" and "Call to Action" fell to bullets, and `fell`'s list of names offers neither.
- **The chart's credit says "Source: losses",** the id the CSV was attached by.
- **The HTML export** needs a scaena built after `just web`, and this one wasn't, which it said.

**What held**
- **`slide_write` with `beat`.** It put each slide where the spine planned it, and the pilot's chart bound to the CSV by its `sourceId`, three figures quoted.
- **`image_add`.** It filled the picture's place once `set_state` gave the slide a layout with one.
- **The PDF with notes** followed the spine, cover first, once the spine was right.

The agent guessed in three places:
- **`replace_text` against `set_text`.** It was refused with its shape.
- **Layout names.** "Image Left", "Call to Action", and "image-left" were guesses.
- **How to put a duplicated beat right.** It rebuilt the spine by hand; #1181 answers that.

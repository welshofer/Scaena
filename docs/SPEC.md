# Scaena — Technical Specification

**Status:** Draft 0.1 — 2026-10-01
**Working name:** Scaena (Latin, *stage*). Rename at will; the crate prefix is the only thing that cares.
**Owner:** Jay Welshofer

> A presentation is a timeline of states over one persistent scene graph.
> Slides are named states. Transitions are interpolation by object identity.
> The document is semantic text; the engine is a pure function; agents are first-class authors.

---

## 0. Reading guide

| If you want to… | Read |
|---|---|
| Understand the model in five minutes | §1, §2 |
| Author or patch a deck (human or agent) | §3 (document), §4 (DSL), §7 (agent surface) |
| Build the engine | §5 (pipeline), §6 (display list), §13 (determinism), §14 (testing) |
| Build a client | §9 |
| Hook up exporters you already have | §10 |
| Wire in an LLM | §11 |
| Know what we are *not* building | §1.3 |

Terminology is fixed in §17 (Glossary). Normative words: **MUST**, **SHOULD**, **MAY**.

---

## 1. Vision, principles, non-goals

### 1.1 Vision

A presentation tool designed in 2026 with no inherited model. Gorgeous, precise typography and layout by construction. Charts, motion, shader backgrounds, and text animation as primitives rather than features. One engine that renders identically in the browser, in native clients, and headlessly for export. A document that agents can create, lint, look at, and patch without a UI. A narrative spine that projects to a deck, an infographic, a PDF, a motion piece, or a podcast.

### 1.2 Principles (ranked; earlier wins conflicts)

1. **Determinism.** `render(document, theme, state, t, viewport)` is a pure function. Same inputs → identical display list, bit for bit. No wall clock, no unseeded randomness, no system-font dependence in the render path.
2. **Identity is the primitive.** Stable semantic identity is the invariant of the document model, second only to determinism. Objects have ids that survive every state, every patch, every re-theme, and every projection. Motion is what happens to an object's properties between states, never an "effect" attached to a slide. Diffs, patches, CRDT changes, data updates, accessibility continuity, and cross-format projection all rest on this.
3. **Semantics over pixels.** Authors (especially agents) express *roles* and *placement*, not pixel values. The design system turns roles into typography and geometry. Pixel overrides exist, are explicit, and are lintable.
4. **The logical document is the truth; text is its canonical interchange form.** The typed document model is the truth. `deck.json` is its deterministic, canonical interchange representation (git, diffs, agents). While editing, a CRDT holds persistence authority and history; it exports exactly that JSON. The DSL is an authoring projection. §3.1 states the authority rules.
5. **Layout once, animate geometry.** Layout runs per snapshot; frames only sample resolved geometry. Nothing may feed interpolated properties back through layout inside the frame loop.
6. **One engine.** Layout, text shaping, animation sampling, and display-list generation live in one Rust core compiled to every target. Clients paint; they do not lay out.
7. **The agent is a user.** Everything a UI can do is reachable through the CLI/MCP with machine-readable results, including errors that carry fixes.
8. **Local-first.** A deck is a bundle on disk. No account, no server, no telemetry by default.

### 1.3 Non-goals (v1)

- PowerPoint/Keynote/Google Slides compatibility **inside the document model or engine**: no import filter, no shared abstractions, no concessions to their model. A purely external, lossy projection (Scaena → "dumb" PPTX, information deliberately discarded, nothing flowing back) would be permitted the way PDF export is. It is not planned.
- WYSIWYG-first editing. Source + live preview first; direct manipulation later, emitting patches.
- 3D, physics beyond springs, arbitrary user shader code, plugin code execution inside the engine.
- Real-time multi-user sync (the document model is designed for it; the server is deferred — see §8.4).
- Video and audio *nodes* (deferred; see §3.8 and §16).

---

## 2. Conceptual model

### 2.1 The scene graph

A deck owns one **scene graph**: a set of **nodes**, each with a stable `id` (a slug), a `type`, and properties. Nodes exist for the whole deck, not per slide. A node is *visible* in a state only if the state (or tracking from a previous state) gives it properties.

### 2.2 States (the cue list)

A deck is an **ordered list of states**. A state is a named point on the timeline where the scene graph has particular property values. Every click is a state. The state list is the cue list; the engine's lineage is a theatrical lighting console, not a slide sorter.

**Tracking.** By default a state declares only *changes* relative to the previous state; unchanged properties carry forward ("track"). A delta replaces each property it names, except an object value, which merges one level into the one it tracks: `at: { "col": [1, 6] }` moves a node's columns and keeps its row. `null` deletes a property, or one key of an object value. An object value with nothing to merge into (the node has none, or has a value of another kind) is taken as it is, less the keys it deletes. A text's `text` and `runs` are one property written two ways: a delta that sets either takes the other away, so a state's `text` shows over runs it would track. The schema's `StateDelta` says what a delta may hold: any node type's property, in any form it takes, an object value with every key optional, or `null` (§3.3). A node that is not visible in the state a delta starts from enters with its node defaults under the delta, not with the props it had when it left; to bring a look back, track `from` the state that had it. A state MAY instead declare `from` (branch from an arbitrary earlier state) or `mode: "absolute"` (no tracking; everything explicit). The state's `layout` template tracks the same way, so a build on the same slide need not restate it. Per-state-only properties (`anim` keyframe tracks and `emphasis`, deck format 0.7) never track: a motion must not replay on the next cue. The engine resolves every state to an **absolute snapshot** before layout. Authors edit deltas; the engine thinks in snapshots; tooling shows both.

**Slides.** A `slide` key groups consecutive states for navigation and export (a build sequence is one slide with several states). A state with no `slide` key starts a slide named by its own id; the states that build on it set `slide` to that id.

### 2.3 Transitions

A transition is the interpolation between two resolved snapshots over a duration with an easing. Nodes are matched by `id`:

- present in both → **morph** (geometry, color, opacity, text, data interpolate; per-node `transition` policy may force `crossfade` or `cut`)
- present only in the target → **enter** (preset or explicit keyframes)
- present only in the source → **exit**

Text morphs at word granularity (PLAN 1.12): the words both texts share, in order, move from where they stood to where they stand, mixing their color in Oklab, or, when their size or face changes, scaling between their two boxes as the two drawings cross-fade. The rest fade where they stand, words that leave over the first half of the transition and words that arrive over the second, so neither shows under the words moving past. Punctuation before or after a word is a word of its own: "grew" and "grew." share "grew". Two texts too long to match word by word, with more than about 4 million pairs of words between them (2,000 words a side), share only the words their starts and their ends have in common, and the words between leave and arrive. Shapes morph point by point when their outlines line up (§3.3), and shader nodes interpolate their uniforms (§3.8). Charts move their data at mark granularity: marks match by data key and interpolate, so values animate in, the next period arrives, and growth shows. A change of chart kind morphs only between kinds that draw the same marks (bars that regroup, `bar` ↔ `stackedBar`); any other change of kind cross-fades.

### 2.4 Time

- `t` is milliseconds since the start of the *current* state's cue: its transition, then its motions (§3.9). At or past the state's **span**, when both have ended, the state is at rest.
- Within a state, nodes may own **keyframe tracks**, presets, and **choreography** (staggers, delays, sequences) that run with the transition or after it (`timing: "with" | "after"`).
- A state MAY declare `hold` (ms) for auto-advance; this is how a deck becomes a video. The **global timeline** lays the states end to end: each state's span, then its hold. A frame `t` into a state is at the state's start plus `t`; at rest, at the moment the state comes to rest. Shaders keep this time (§3.8), video export samples it (§10), and a player auto-advances by it (`Player.timeline()`). A state whose span and hold are both 0 gets no time on the timeline; in a deck with holds, lint warns of it (W323, §7.5).
- A state starts where the ones before it end. The engine works that out as far as a frame needs, and lays a state out for it only where a motion splits a node into what layout counts (lines, words, glyphs, a chart's marks); the rest is timed from the document.

### 2.5 The design system

A **theme** is a design system: tokens (color, type, space, motion, shader palettes, data palettes), **roles** (what a `display` text is), **layout templates** (what a `split` state looks like), and named springs. The document stores references, never resolved values. Re-theming is re-rendering. Theme values are properties, so a theme change is itself animatable.

### 2.6 The narrative spine

Beneath the states sits a **spine**: sections → beats, each beat carrying a claim, evidence references, speaker notes, and the states that express it. The deck is a projection of the spine; so are the infographic, the PDF, the motion piece, and the podcast (§10). The spine is also the accessibility reading order.

---

## 3. Document model

### 3.1 Bundle

A deck is a **bundle**: a directory `name.scaena/` or a zip of the same layout with extension `.scaena`. Every command reads either, or a bare deck file, whose directory then stands in for the bundle.

```
name.scaena/
  manifest.json          # format version, deck.json's sha256, every other file's, created/modified (docs/schema/manifest.schema.json)
  deck.json              # CANONICAL INTERCHANGE form of the logical document (schema: docs/schema/deck.schema.json)
  deck.scn               # OPTIONAL authoring projection (DSL); regenerated on save
  theme.json             # theme used by this deck (copied in; decks are self-contained)
  data/                  # CSV/JSON data sources referenced by @name
  assets/<sha256>.<ext>  # images, content-addressed
  fonts/<family>-<hash>.ttf|otf|woff2   # subsetted fonts, content-addressed (hash: the subset's sha256, first 16 hex digits)
  history/deck.loro      # OPTIONAL CRDT document with its history (§8): kept from `save --history` on; absent in "flat" bundles
```

Rules:
- **Authority.** The *logical document* (the typed model in `scaena-core`) is the truth. The JSON schemas in `docs/schema/` are generated from it and never edited by hand (ADR-0007). `deck.json` is its canonical interchange representation: deterministic serialization, the thing git diffs and agents patch. While a bundle is open in an editor, the CRDT (`history/deck.loro`, §8) holds persistence authority and history; `deck.json` is regenerated from it on every save and the two never disagree. (The web editor loads the CRDT as a module of its own to save, §9.2: each save records its edits as the user's, and its assistant's as its agent's.) `deck.scn` is an authoring projection: edits to it are compiled into the logical document (through the CRDT when one is open) and it is regenerated on save. A `deck.json` found to say otherwise than the history (edited by hand, or by a tool that writes only files) goes in as a change authored `fs`: `deck.json` is the tiebreaker, so whatever writes the history writes `deck.json` with it.
- Assets and fonts are referenced by content hash; the bundle is self-contained and portable.
- Fonts are **subsetted** into the bundle at save time. The render path MUST NOT consult system fonts (§13). System fonts are only enumerated in editors for picking.
- **Saving** (`scaena save`, PLAN 1.4) writes `deck.json` and the theme file in canonical form. It subsets each font the deck or its theme names to what the deck can draw: the characters in its strings and data, plus ASCII, Latin-1, Latin Extended-A, and general punctuation. Every glyph keeps its id, so a saved bundle draws the frames it drew before (ADR-0004 finding 10). Fonts and images are named by their content and every reference is rewritten; other files are carried as they are. A zip lists its files in path order, each dated 1980-01-01, so the same bundle zips to the same bytes. `--history` starts keeping the bundle's history (§8); a bundle that keeps one records the save in it, files renamed and all. A font the subsetter cannot read is an error that names it, and the save writes nothing.
- `manifest.json` records `"scaena": "<format version>"` (semver; majors break), the sha256 of `deck.json` and of every other file as last written, and when the bundle was created and last saved. Nothing in the render path reads it.

### 3.2 Top-level document

```jsonc
{
  "scaena": "0.15",
  "meta":   { "title": "...", "author": "...", "created": "...", "lang": "en-US" },
  "canvas": { "width": 1920, "height": 1080, "unit": "cu" },   // canvas units; 1 cu = 1 px at 1080p
  "formats": ["16:9", "9:16"],                               // the formats it is laid out in too (§3.4)
  "theme":  "theme.json",                                     // path in bundle, or inline object
  "fonts":  [ { "family": "...", "file": "fonts/...", "axes": {...} } ],
  "data":   { "q3": { "source": "data/q3-revenue.csv", "schema": { ... } } },
  "spine":  { "sections": [ ... ] },
  "nodes":  { "<id>": { "type": "...", ... } },               // the scene graph
  "states": [ { "id": "...", ... } ],                         // the cue list, ordered
  "overrides": { "<nodeId>": { "<prop>": value } }           // a delta per node that wins in every state (§3.6)
}
```

**Fonts.** `fonts` lists every font file the deck's theme names in its families: rendering registers only listed fonts (E102 otherwise, with the entry to add as its fix). Whether fonts should follow the theme instead, so a deck need not restate them, is open (§16 Q10).

**IDs.** Slugs: `^[a-z][a-z0-9_-]{0,63}$`, unique within `nodes`, within `states`, and within spine `beats` (E105); a node and a state may share one. An id written twice as a key (`nodes`, `data`, a state's `props`) is E105 too: a parser would keep the second and drop the first. Agents SHOULD choose meaningful IDs (`title`, `rev-chart`). The CRDT layer assigns internal IDs independently; slugs are for humans and agents.

### 3.3 Node types

Every node has a `type`, an id (its key in `nodes`), and the properties every type shares: `name`, `alt`, `semantic`, `z`, `tags`, `visible`, `opacity`, `transform`, `fill`, `stroke`, `blur`, `shadow`, `clip`, `blend`, `at`, `size`, `align`, `transition`, `enter`, `exit`, `emphasis`, `anim` (`NodeProps` in the schema). Each type adds its own, below; a property of another type is an error, so a chart's `axes` (axis settings) and a text node's `axes` (variable-font axis values) are two properties that share a name. A state's delta is not typed by node (it carries no `type`): the schema checks each property's forms there, and validation checks the resolved state against the node's type (PLAN 1.2).

**Two semantic axes.** `role` (on text nodes) and the theme describe *what something looks like*. `semantic` describes *what it does in the argument*: `claim | evidence | annotation | context | comparison | takeaway | source | navigation | decoration`. They are independent: a claim may be a `headline` on one state and a `caption` on another. `semantic` is optional in v1 and reserved for the narrative lint family (§7.5) — e.g. "this beat's claim has no visible expression" or "evidence outranks the claim in visual hierarchy". Agents SHOULD set it; nothing renders differently because of it.

| type | purpose | key properties |
|---|---|---|
| `text` | typographic text | `role`, `text` or `runs`, `fit`, `wrap: greedy\|pretty\|balance`, `maxLines`, `minSize`, `maxSize`, `box`, `features`, `axes`, `lang`, `measure`, `hyphenate`, `opticalMargins`, `hangingPunctuation`, `numeric`, `minLastLineWords`, `split` |
| `shape` | vector geometry | `path` (SVG path data) or `kind: rect\|ellipse\|line\|arrow\|polygon\|path`, `points`, `radius` |
| `image` | raster/vector image | `src` (asset ref), `fit: cover\|contain\|fill`, `focal: [x,y]`, `crop`, `radius` |
| `chart` | data-bound visualization | §3.7 |
| `table` | data-bound table | `data`, `dataTransform`, `columns` (`field`, `title`, `format`, `align`), `key`, `header` |
| `shader` | GPU/CPU parametric background or fill | §3.8 |
| `stack` | layout container (axis) | `axis: x\|y`, `gap`, `distribute`, `padding`, `radius` |
| `grid` | layout container (grid) | `cols`, `rows`, `gap`, `areas`, `padding`, `radius` (+ child `at.area`, `at.col`/`at.row`) |
| `frame` | layout container (absolute) | `padding`, `radius` (+ child `at.rect`) |
| `group` | nodes drawn together | (+ child placement as on the slide) |

A node is in a container when its `at.parent` names one (§3.4, ADR-0008); containers do not list their children.

Deferred node types (not in v1 schema): `video`, `audio`, `code`, `embed`.

**Tables** (PLAN 1.9, deck format 0.5) set a data source's rows, through its `dataTransform` (§3.10), in the theme's `tables` styles (§3.6).
- `columns` lists the columns in order. Each is a `field`, with an optional `title` (default: the field), `format` for numbers and dates (`docs/spec/format.md`), and `align` (`start`, `center`, `end`; numbers at the end and everything else at the start by default). Without `columns`, the table shows every column of its data.
- The header row (`header`, default true) prints the titles in `tables.header` (role `label` in `onSurfaceMuted`) over a rule, `tables.rule`. Cells print in `tables.cell` (role `body`), numbers in tabular lining figures with the font's minus sign or a hyphen-minus. A null cell is empty.
- Each column is as wide as its widest text, `tables.columnGap` apart (an em of the cell text by default), and the table as wide as its columns, at its cell's start, so the eye travels no farther across a row than the data needs. A theme that sets `tables.stretch` (theme format 0.5) spans the cell instead, its first column taking the room the cell has to spare. Rules span the table.
- Each row is its tallest text with `tables.rowGap` (half a space unit by default) above and below, its cells' first baselines on one line. `tables.rowRule` rules between rows.
- So a table has a size of its own, as text does: its columns across (under `tables.stretch`, the width it is given) and its rows down, the header's included. Rules lie on the lines between rows and take no room. In a stack it takes that room and stretches across, so what follows starts a `gap` under its last row; `size: fit` gives it that size anywhere, and `size: fill` shares what is left, as a chart does (§3.4). A root with no `size` fills its cell, as every root does, its rows from the top.
- A table that needs more room than its cell is an error that says what to cut: columns across, or rows down (keep fewer with `dataTransform`'s `limit`). In a container a table asks for no more room than it is offered and needs none of it: short of room, it takes what it is given and this error says what to cut (rows that do not fit are lint E100), rather than pushing what follows past the container's edge.
- Rows are identified by `key` (default: the first column), which must be unique (E103). Between states a row moves to where it now stands. A cell whose text changed cross-fades, aligned to its column. Rows and columns on one side only fade in or out where they stand, and rules move with their rows.

**Shapes** fill their box (§3.4). A `rect` is the box, its corners rounded by `radius` (a length or a `radius.*` token, at most half the shorter side); an `ellipse` is inscribed in it. `line`, `arrow`, and `polygon` join `points`, given as fractions of the box: `[0, 0]` is its top-left, `[1, 1]` its bottom-right. A line or arrow with no `points` crosses the box's middle, left to right (`[[0, 0.5], [1, 0.5]]`, which an editor shows as its points, §9.2.4); a line or an arrow takes two points or more, and a polygon three or more; an arrow ends in a filled head sized from its stroke width. A `path` (`kind: "path"`, or `path` alone) is SVG path data, scaled uniformly to fit the box and centered in it, so its own coordinates only need to agree with each other. `fill` and `stroke.paint` are paints: a theme color (or `{ "solid": color }`), or a gradient across the shape's box, `{ "gradient": { "kind", "angle", "center", "stops" } }` (PLAN 1.10). A `linear` gradient runs along `angle`, degrees clockwise from up (default 180, top to bottom), through the box's middle and as long as the box is that way, as CSS's does; a `radial` one runs from `center` (fractions of the box, default `[0.5, 0.5]`) out to the box's farthest corner; a `conic` one turns about `center` from `angle` (default 0, up) all the way round. `stops` are `{ "at": 0–1, "color" }`, two or more, in theme colors, blended in Oklab. An arrow's head takes its stroke's paint. `stroke.width` is a length or a stroke token, and `cap`, `join`, and `dash` read as in SVG. A stroke that names no paint is `onSurface`, and one with no width is the theme's `thin` stroke. A line or arrow with no `stroke` draws that thin rule; a closed shape draws only the fill and stroke it is given. The geometry is made for whatever box the shape has, so a shape whose box changes between states morphs, and its frames make it again at the box they reach, laying nothing out. A shape whose outline also changes morphs point by point when the two line up (PLAN 1.12): the same kind; as many `points` (a line, an arrow, a polygon) or path data of the same commands in the same order; a fill and a stroke on both sides or neither, each paint a color on both or a gradient of one kind with as many stops; and strokes of the same cap, join, and number of dash lengths. Its box, a rect's radius, and each point move (a path's once both are fitted to the box reached), its paints mix in Oklab, and its stroke's width and dashes move. Anything else cross-fades.

**Images** are PNGs and JPEGs, each read by one decoder in Rust, the same pixels on every target (§13): a PNG by the `png` crate, a JPEG by `scaena_core::jpeg`, in integers alone (ADR-0017, PLAN 2.66). `src` is the file's path in the bundle (`assets/<sha256>.<ext>` once saved, §3.1), and the display list names the image by `sha256:` of its bytes. An image is at most 8192 px a side, vello's image atlas, which the engine checks when it registers the file. Pixels are sRGB, straight alpha, with no color profile applied. A JPEG:
- is the picture as it is meant to be seen: its EXIF orientation turns it as it is decoded, so its size, `crop`, and `focal` are the turned picture's;
- may be baseline or progressive, gray or in color, its color sampled at any factors that divide the largest. One coded arithmetically, lossless, hierarchical, at 12 bits a sample, or in CMYK is refused, saying why;
- stays in the bundle as it is. A PDF carries it as its own file, without what it says beyond its picture (§10).

How the image meets its box:
- `crop: [x, y, w, h]`, fractions of the image, cuts it first, so a sharper file of the same picture keeps the crop. Default: all of it.
- `fit: cover` (default) scales the crop to fill the box and cuts what overflows; `contain` scales it to fit inside the box, the rest of the box empty; `fill` stretches it to the box.
- `focal: [x, y]`, fractions of the crop (default `[0.5, 0.5]`), lines the box and the image up at that point, as CSS `object-position` does with percentages: under `cover` the focal point stays in view, and under `contain` it places the image in the box's spare room.
- `radius` rounds the corners of what shows: the box under `cover` and `fill`, the drawn image under `contain`.

Images are filtered bilinearly in every painter (vello's GPU path has no bicubic, so a finer CPU filter would only make the painters disagree), without mipmaps: an image drawn at under half its size aliases. Between two states an image's box and radius morph; a change of `src`, `fit`, `focal`, or `crop` is a new picture, which crossfades.

Common animatable properties: `opacity`, `transform` (`translate`, `rotate`, `scale`, `skew`, `anchor`), `fill`, `stroke`, `blur`, `clip`, `blend`, `shadow`, plus type-specific ones (text content/style, chart data/encodings, shader uniforms).

**Transforms** (PLAN 2.51). A node's `transform` draws it somewhere other than where it is laid out, at rest and in every frame, without laying anything out again: its layer's transform (§6) maps its box as laid out to where it is drawn.
- **Its parts, in order:** `scale` (a number, or `[x, y]`; a negative one mirrors), then `skew` (`[x, y]`, degrees: x leans by the tangent of the first for each unit down), then `rotate` (degrees, clockwise), each about `anchor` (`[x, y]`, fractions of the node's box, its middle by default), then `translate` (`[x, y]`, canvas units). Sines, cosines, and tangents are `libm`'s (§13). A skew stops 1° short of a right angle, a scale at 1000 either way, a translation at 100,000 canvas units, and a turn at a hundred turns.
- **What holds it carries it.** A container's transform draws what it holds turned with it, about the container's box; a group's turns its layer, members and all, about the box around its members. Each node's own is drawn inside what holds it.
- **A cue moves a node in its own frame.** A cue's looks (§3.9) are drawn inside the node's transform: a node turned a quarter clockwise rises in from its left, and scales along its own axes.
- **Between two states** a node in both turns, scales, leans, and moves from one transform to the other part by part, never as a matrix, about its box as that moves: a turn from 0° to 350° goes most of the way round. Where one side has none, it takes the other's anchor, so a node scaled from a corner grows back from that corner. A node that cross-fades or cuts keeps its own on each side.
- **A transform that flattens a node** (a scale of 0 along either axis) draws nothing of it, and nothing points at it.
- **It is where the node is found.** A box carries the map that draws it, and a point is read back through it: a turned text is selected, dragged, and typed into where it is drawn (`inspect --boxes`, `--at`, §7.1). Lint judges what is drawn: E101 and W311 by the outlines drawn, E110 and E111 over the pixels around each run where it is drawn, and W312 and W313 at the size it is drawn.

### 3.4 Layout

Placement is declared with `at`, resolved against the state's **layout template** (from the theme) or an explicit container.

```jsonc
"at": { "col": [1, 7], "row": [1, 1] }            // grid cell range on the template grid
"at": { "in": "left", "align": "center" }          // slot in the layout template
"at": { "rect": [120, 80, 900, 420] }              // canvas units — an OVERRIDE (lint W301)
"at": { "parent": "stats", "index": 2 }            // child of a container node
```

One placement wins: `rect`, else `in`, else `col`/`row` (either omitted spans the grid). A node with no `at` fills the grid's margin box. A cell past the theme's grid, a node's or a slot's, in the deck's format or one it lists, is lint **E102**, which names the grid's size: a deck placed on 12 rows does not move onto a theme of 6 until it is placed again. Besides its template's slots, every state has two: `canvas`, the whole canvas, and `grid`, the margin box.

**Containers** follow CSS flex and grid as `taffy` implements them: `stack` is flex along one axis, `grid` is CSS grid, and `frame` places its children absolutely (ADR-0008).
- **Membership.** A node is in a container when its `at.parent` names one; nothing else declares it. A container's children flow in `at.index` order (default 0), ties in `nodes` order, as CSS `order` does. A container must be in every state its children are in (E102), a node's `at.parent` must be a container (E106), and containers do not nest in a loop, nor more than 64 deep (E106). Inside a container, `at.offset` moves a node, and what is in it, after layout, and `at.inset` shrinks the node's own box, as they do on the slide.
- **Roots.** A node with no container, or in a group, is a root: `at` places it on the theme grid as above. A root container lays its subtree out inside that box. Any root but text fills its box unless it says otherwise, and then aligns in the box by its alignment. On an axis its `size` sets, it takes that size. On an axis its own `align` or `at.align` names, other than `stretch`, a container takes its content's size, a sized image its picture's, and a sized table its rows and columns (§3.3): `at: { col: [1, 12], row: [2, 5], align: { y: "center" } }` centers a grid of cards in those rows instead of stretching the cards to fill them. A slot's `align` is for text, and shrinks no container.
- **`stack`.** `axis: y` (the default) runs top to bottom, `x` left to right; `gap` sits between children, `padding` inside the edge, and `distribute` (`start` by default, `center`, `end`, `between`, `around`, `evenly`) places leftover room along the axis. Text, tables, and images take the room their content needs (a table its rows down and its columns across, §3.3), and containers wrap theirs. Shapes, charts, and shaders, which have no size of their own, share what is left. A table short of room takes what is left and says what to cut (§3.3). Every child stretches across the stack unless its `size` or `align` says otherwise. An image keeps its picture's proportions as it stretches. Text in a row stays in a box the row's height, so `align: { y: "baseline" }` puts the last baselines of a row's texts on one line, and `cap` puts their cap tops on one line.
- **`grid`.** `cols` and `rows` are a count of equal tracks or each track's size; without them, as many equal tracks as `areas` has. `areas` names cells CSS-style (`["head head", "left right"]`, `.` for none), and each name must be a rectangle. A child takes `at.area`, or `at.col`/`at.row` lines of this grid (1-based, inclusive, like the theme grid's), or the next free cell. It fills its cell unless its `size` or `align` says otherwise, and never widens its track unless its `minW` or `minH` asks (CSS's `min-width: 0`): an image whose row sets its height does not ask for the width its picture's shape would take.
- **`frame`.** A child's `at.rect` is `[x, y, w, h]` from the frame's padding edge. A child with no `rect` fills the padding box.
- **`group`.** Its children are placed on the slide by the rest of their `at`, and composited together (PLAN 1.12): the group draws them into one layer, each at its own opacity, and that layer at the group's `opacity`, so two overlapping children never show through each other. Motions on the group move its layer, and it comes and goes with the transition as one; a `children` split moves each child inside it. A group lays nothing out, and cannot sit in a stack, grid, or frame.
- **Sizing values** (`size: { w, h, minW, maxW, minH, maxH, aspect }`): a length (canvas units, a token, or a percentage of the container), `fit` (the content's size: text's lines, a table's rows and columns, an image's picture, a container's children; a shape, chart, or shader has none), `fill` (an equal share of the room), or `fraction(n)` (n shares). `aspect: "w:h"` fixes the proportions, and the bounds clamp.
- **Panels.** A container with `fill` or `stroke` draws them as a rectangle with its `radius`, under its children.
- **Order and timing.** Siblings paint by `z`, then `nodes` order, and a container paints under its children. Layout runs once per snapshot. A child that changes container between states morphs from one box to the other, like any box.

**Formats** (PLAN 1.13, theme format 0.4). A deck is made for its `canvas`, and `formats` names the other shapes it is laid out in too: `16:9`, `4:3`, `9:16`, `1:1`, `A4` (210:297), and `Letter` (8.5:11), the paper sizes upright. A format the canvas already has is the canvas.
- A format's canvas keeps the canvas's shorter side and sets the longer one by the format's proportions, in whole canvas units: a 1920 × 1080 deck is 1080 × 1920 in `9:16`, 1080 × 1080 in `1:1`, and 1080 × 1527 in `A4`.
- Its template set is the theme's: `formats.<format>.grid` is the grid there, and a layout's `formats.<format>.slots` take the place of its slots of the same names there, the rest staying as they are (§3.6). A format the theme says nothing about keeps the theme's grid and slots.
- Nodes, states, the spine, and every motion are shared; the deck lays out again on the format's canvas. A node placed in a slot moves with the slot, one placed by `col`/`row` takes those cells of the format's grid, and a `rect` stays where it is in canvas units. So a deck meant for several formats places by slot.
- A node lays out anew in a format where its `formats` says so (PLAN 2.84, ADR-0020; deck format 0.13): `"formats": { "9:16": { "at": { "col": [1, 12], "row": [3, 12] }, "cols": 1 } }`. Each property there takes the place of the node's own in that format: `at`, `size`, `align`, and `transform`; a container's `axis`, `gap`, `distribute`, `padding`, `cols`, `rows`, and `areas`; and a text's `maxLines`. A state's delta still sets what it sets, in every format. `formats` is the node's own: a state's delta or the deck's overrides that set it is **E106**, as is a property there that the node's type does not have; a format the deck does not lay out in, or its own canvas's shape, is **E102**, as is a cell past that format's grid. A node placed anew in each format the deck lays out but its own is not **W302**.
- A frame names its format (`FrameRequest.format`, `scaena render --format 9:16`, `Player.setFormat`); one the deck does not list is an error. The global timeline is worked out per format: a cue on lines counts them after layout.

**Alignment** in a cell is `x: start|center|end|stretch` and `y: start|center|end|stretch|cap|baseline|x-height`; a single keyword sets both axes. The slot's `align` is the default, the node's `align` overrides it, and `at.align` overrides both; `start` when nobody says. For text, `x` aligns each line across the box, in the paragraph's direction: `start` is the left edge for left-to-right text and the right edge for right-to-left text, and `stretch` is `start`. Lines break at the box's width, or at the role's or node's `measure` if that is narrower, and still align across the whole box. The typographic anchors align text of any size to a shared line: `y: "cap"` and `y: "x-height"` put the first line's cap height or x-height on the cell's top edge, and `y: "baseline"` puts the last line's baseline on the cell's bottom edge. `box: "cap"` trims the text's box from the first line's cap height to the last line's baseline (CSS `text-box: trim-both cap alphabetic`), so a cap top can sit exactly on a grid line; `box: "line"` (default) uses line boxes. Cap height and x-height come from the font's OS/2 table, never from glyph bounds.

**Text fit policy** (`fit`): `wrap` (default) | `shrink` (down to `minSize`) | `grow` (up to `maxSize`) | `clip` | `error`. Text fits when its box, as `box` trims it, is no taller than the cell, no line is wider, and it has no more lines than `maxLines`. `shrink` and `grow` set the text at the largest size between its bounds at which it fits, found by bisection (12 steps), so the same text in the same box always gets the same size. Every span scales together, with leading, tracking, and `measure` following. The bounds are the node's `minSize`/`maxSize`, else its role's, else half and twice its size. `clip` cuts the text to its cell; `error` refuses to draw a text that does not fit. Overflow under `wrap`/`clip` is lint **E100**; under `shrink` it is **W203** once the minimum is reached.

**Baseline grid** (PLAN 1.25, theme format 0.7). A theme MAY give its grid a baseline grid: `grid.baseline`, the distance between its lines in canvas units. Its lines run that far apart from the grid's top margin, up and down the canvas, and a format's grid has its own. A text role opts in with `snap` (§3.6). A role without it keeps the place its alignment gives it, and so does text in a chart or a table.
- `snap: "baseline"`: every baseline sits on a grid line. Lines are set whole grid lines apart: each gap between baselines is rounded up to whole lines, the room going above the line that moves, so `fit` and a container measure the text as set. A role whose leading (`size × leading`) is not a whole number of grid lines is lint **W221**.
- `snap: "cap"`: the first line's cap height sits on a grid line (the line's top, in a font without one), and the lines below keep the role's leading. This is display type's alignment.
- After its alignment, a snapping text moves to the next grid line down. Aligned to the foot of its box (`y: end` or `baseline`), it moves to the line above instead, so it stays in the box. A text that fills its box can pass its edge by less than a grid line, and its fit is judged as set. Texts aligned to one line in a row move together when they snap alike. Text in a container moves inside the box the container gives it.
- Dusk and Daybreak snap body and caption by their baselines, at leadings of 5 and 4 lines of their 8 cu grid. Display, headline, title, and numeral snap by their cap heights.

### 3.5 Typography

Text nodes carry a `role` from the theme (`display`, `headline`, `title`, `body`, `caption`, `label`, `numeral`, `code`, …). The role supplies family, size, weight, leading, tracking, measure (max line length), case, numeric features, variable axes (`wght`, `opsz`, `wdth`), and what of it sits on the baseline grid (`snap`, §3.4). `measure` counts characters in `ch`, the advance of `0` in the text's look (CSS `ch`). `case` is `upper`, `lower`, `title` (each word's first letter capitalized), or `smallcaps`. Small capitals are the font's `smcp`, and none are synthesized: a family without them shows the letters as written. The case mappings are Unicode's defaults, the same in every language. A text node's `style` refines its role's look (§3.6).

Italic is a family's own italic face (PLAN 2.40). A role, or a text's `style` (a node's or a run's), sets `italic: true` to ask for it, and `false` to set the text upright where its role is italic. The theme's family names the face, a file of its own (`italic: { "file", "axes"? }`, §3.6), and it is set as the family is: the same weights, axes, and features. None is synthesized: a family without an italic face sets the text upright, and lint says so (**W231**), as does text that falls back to a family without one. The themes that ship, Dusk, Daybreak, and Ember, name an italic for each of their families. Rich text is `runs: [{ "text", "role"?, "emphasis"?: "high"|"low", "style"?: {...} }]`.

**Links** (PLAN 2.70). A run's `link` says where it goes: `{ "href" }`, a web address (`https://`, `http://`, or `mailto:`), or `{ "state" }`, a state of the deck (E102 where the deck has no such state). A linked run is drawn underlined, at its font's own underline position and thickness, in its words' color; the space a line ends with is not underlined, and a link that wraps is underlined on each of its lines. The frame's display list says where each stretch of a link is followed: a `link` op, the words' box on their line, which draws nothing (§6). The player follows a click there: a state is shown, and a web address opens in a window of its own. A PDF carries both as link annotations, a state's going to the page of its slide; how a state reads gives each as an `a` (§3.12). Renaming a state renames its links; removing it takes them away, and their words stay. An underline is drawn at rest, and between states a link's words morph as any words do.

**Quotes** (PLAN 2.72, ADR-0019). A run's `quote` names the value of a data source its text is: `{ "data": "@name", "dataTransform"?, "row"?, "column", "format"? }`.
- **The value.** The source is read as a chart reads it, through the quote's `dataTransform` (§3.10), so an `aggregate` gives a sum. `row` is the one row whose columns hold the values it names (`{ "quarter": "2026-Q3", "product": "Pro" }`), or an index from 0, negative from the end (`-1` is the last row, which rolls forward with the data). Without `row`, the table has one row. The cell in `column` is written by `format`, a number format for a number and a date format for a date (§3.7), else as a chart writes a category.
- **The text is the figure as last set.** The engine draws, reads, and exports the run's `text`; a frame reads no data for it. Each patch and each data edit (§3.10, §7.3) works the value out again from the bundle as it leaves it, and writes each figure that changed in the same change, with each beat's `claim` that holds the old figure as a word and whose states show the text. The old figure is one the same quote gave: the words a patch first makes a quote were no figure, and a claim that says them keeps them. A data file changed by other means leaves its figures as they were, which lint **W427** finds, with the fix.
- **Errors.** A source the deck lacks is E102. A column, a row that is not there, or one that many rows match, and an empty cell, are E103. A format or a transform that does not parse is E106.
- **Editing.** `style_text` gives characters a quote (`look: { "quote": … }`), making them one run, and `null` takes it away, the words staying. A look given to part of a figure is given to all of it. Typing into a figure makes it words: it quotes nothing. Typing beside one goes into a run of its own.

**Lists** (PLAN 2.69, ADR-0018; deck format 0.12, theme format 0.10). A text's paragraphs end at its hard line breaks (`\n`, `\r`, `\r\n`, U+2028, U+2029), and its `list` makes them a list's items: one entry for each paragraph, in order, `null` for one that is no item, or `{ "kind": "bullet" | "number", "level"? }`, `level` from 0 (the default, the outermost) to 8. Paragraphs past its end are no items, and entries past the last paragraph mean nothing.
- The theme marks and indents them (`type.lists`, §3.6): `indent`, how far each level's words start from the one above, and `gap`, the least room between a marker and its words, both in ems of the text's size; `bullets`, a marker for each level; and `numbers`, a pattern for each level whose first `1`, `a`, `A`, `i`, or `I` is the item's number in digits, letters (`z`, then `aa`), or roman numerals (`1.`, `a)`, `(i)`). A level past a list's end takes its last. A theme without `type.lists` takes 1.2 em, 0.4 em, `• – ·`, and `1. a. i.`: characters every Latin font has.
- An item hangs. Each of its lines breaks at the box's width less its indent, `(level + 1) × indent`, and starts there; in right-to-left text, from the right. Its marker is set on its first line's baseline, in the look of its first character, and ends `gap` short of where its words start. An empty item shows its marker.
- Numbers count among the numbered items of one level that follow each other, from 1. An item at a shallower level starts the deeper counts again, a bullet starts its own level's, and a paragraph that is no item starts them all.
- A marker its fonts have no glyph for is **E120**, which names the marker's character. `balance` and `pretty` fall back to `greedy` for text with hard line breaks, a list of several items among them; a list of one item breaks as asked, at its indented width.
- Editing keeps `list` in step with the paragraphs (§7.3): `replace_text` writes it beside the text where a paragraph splits or joins, a paragraph split off an item an item like it and paragraphs joined keeping the first's mark, and the `list` op marks the paragraphs a range of characters touches.

The engine MUST implement:
- Shaping via `harfrust` (the HarfBuzz port) through `parley` (ligatures, kerning, contextual alternates, OpenType features, variable axes), with per-run `features` and `axes` overrides. Font tables and metrics are read with `skrifa` (ADR-0004).
- Line breaking: `wrap: "greedy" | "pretty" | "balance"`. `pretty` minimizes raggedness and avoids short last lines (Knuth–Plass or equivalent); `balance` equalizes line lengths (titles).
- Hyphenation by `lang`, where the node or its role sets `hyphenate` (off unless one does). Words break at the hyphenation points of the TeX patterns for the language of `lang` (its first subtag; the node's, else the deck's). The engine carries 17: English, German, French, Spanish, Italian, Portuguese, Dutch, Swedish, Danish, Finnish, Polish, Czech, Russian, Ukrainian, Turkish, Greek, and Catalan. Text in another language does not hyphenate.
  - The patterns are hyph-utf8's, compiled to tries by `hypher` 0.1.8. The engine walks them as `hypher` does (`scaena_engine::hyphen`, ADR-0015).
  - Native builds and the player's module have every trie compiled in. The editor's module is handed a language's the first time a text hyphenates in it (§9.2). A trie handed over is held to its SHA-256, so a frame is the same bytes either way. A language whose trie cannot be had is an error that says so, never text set without hyphens.
  - A soft hyphen (U+00AD) in the text is a hyphenation point in any language, and a word with soft hyphens of its own breaks only there.
  - A line that ends inside a word draws a hyphen: `-` in the look of the text it ends, its width counted when lines break. `pretty` charges a line that ends in one half an extra line, so a hyphen has to buy real evenness. With `hangingPunctuation`, the hyphen hangs past an end-aligned edge. Right-to-left lines draw no hyphen.
- Widow control: `minLastLineWords`, from the node, else its role, else 1. Themes set it per role (Dusk and the torture theme: 2 for body, 1 for display).
  - Every breaking holds a paragraph's last line to that many words. `pretty` plans for it; `greedy` and `balance` move the break above the last line back, as few words as it takes, while both lines still fit.
  - A word is the text between two places a line may break (UAX #14), with the punctuation and space after it. A word that hyphenation splits is one word, and its tail is not a word, so no paragraph ends on a tail alone, whatever the setting.
  - A last line that cannot take enough words takes as many as fit, and the layout reports the widow (lint **W200**).
  - Text does not flow from box to box in v1, so there are no orphans: no paragraph's first line is left at the foot of a column.
- Hanging quotes, always: quotation marks (Unicode `Quotation_Mark`, less the CJK corner brackets and fullwidth forms, whose spacing JLREQ governs) hang outside an aligned edge, in every role. It is not a theme option. In start-aligned text, the marks that open a line hang outside its start edge; in end-aligned text, the marks that close a line hang outside its end edge. Nothing hangs at a ragged edge, so centered text hangs nothing. Breaking measures the line without its hung marks (CSS `hanging-punctuation`), so they take nothing from the measure and the letter beside them sits on the edge.
- Hanging punctuation beyond quotes (`hangingPunctuation`) and optical margin alignment (`opticalMargins`), set per role or per node; themes turn them on for display roles, as Dusk and the torture theme do.
  - With `hangingPunctuation`, these hang fully past an aligned edge, as quotation marks do: opening brackets at a start edge; closing brackets, stops and commas (CSS `allow-end`'s set), and hyphens at an end edge.
  - With `opticalMargins`, when nothing hangs at an aligned edge, the character on it moves part of its advance past the edge: 5% for A T V W X Y v w x y, 70% for a period, 50% for a comma, colon, or hyphen. These are microtype's defaults for Latin text. Breaking does not count the protrusion, so it never changes where lines break.
- Numeric styles: `tabular`/`proportional`, `lining`/`oldstyle`; charts default to tabular lining.
- Vertical metrics by cap height and x-height from the font tables (not bounding boxes).
- Text splitting for animation: `split: "lines" | "words" | "glyphs"` cuts a text into units that choreography moves one by one (§3.9): its lines; its words, as above, a hyphenated word one unit with its hyphen; or its clusters, a ligature one unit. Units come in reading order, also in right-to-left text. Only what sets ink is a unit: spaces and invisible characters belong to a line or a word, never to a unit of their own. Splitting reads the layout; it never lays out or shapes again (§5).
- Bidi and script fallback via `fontique` restricted to bundle fonts.
- An empty text is one empty line in its look: it draws nothing and keeps the line's height, as a text box an editor has just cleared does. Its words fade out of a morph and back in.
- Layout runs in `f32` (§13). A text whose size, line height (size × leading), or letter spacing (size × tracking) is more than `f32` holds, about 3.4 × 10³⁸ canvas units, is an error that names the node: it is not laid out, as a painter refuses a raster it cannot make.
- A glyph a text sets that a painter could not draw is an error that names the font and the glyph: a font whose `head` table does not read, an outline that does not read at the instance set, or a color glyph with a gradient of no stops or a clip whose outline does not read. The engine checks each glyph it places as the painters draw it, once per font instance, so a damaged font stops a frame with an error, never a painter.

### 3.6 Theme (design system) schema

See `docs/schema/theme.schema.json`. Shape:

```jsonc
{
  "scaena-theme": "0.10",
  "name": "Dusk",
  "tokens": {
    "color":  { "ink": "#...", "paper": "#...", "accent": "#...", "muted": "...", "...": "..." },
    "roles":  { "surface": "paper", "onSurface": "ink", "accent": "accent", "onAccent": "paper" },
    "data":   { "categorical": ["..."], "sequential": ["..."], "diverging": ["..."] },
    "space":  { "unit": 8, "scale": [0, 4, 8, 16, 24, 32, 48, 64, 96, 128] },
    "radius": { "scale": [0, 4, 8, 16, 32] },
    "stroke": { "hairline": 1, "thin": 2, "thick": 4 }
  },
  "type": {
    "families": { "display": { "family": "...", "file": "fonts/...", "axes": {...}, "italic": { "file": "fonts/...", "axes": {...} } }, "body": {...}, "mono": {...} },
    "scale":    { "ratio": 1.25, "base": 32 },
    "roles": {
      "display":  { "family": "display", "size": 128, "weight": 650, "leading": 0.95, "tracking": -0.02, "opsz": 96, "wrap": "balance", "box": "cap", "snap": "cap", "minSize": 72, "measure": 18 },
      "headline": { "...": "..." }, "body": { "...": "..." }, "caption": { "...": "..." }, "numeral": { "numeric": "tabular-lining", "...": "..." }
    },
    "lists":    { "indent": 1.2, "gap": 0.4, "bullets": ["•", "–", "·"], "numbers": ["1.", "a.", "i."] }
  },
  "grid":   { "columns": 12, "gutter": 24, "margin": 96, "baseline": 8 },
  "layouts": {
    "title":  { "slots": { "title": {...}, "subtitle": {...} } },
    "split":  { "slots": { "body": { "col": [1, 6] }, "main": { "col": [7, 12] } },
                "formats": { "9:16": { "slots": { "body": { "col": [1, 12], "row": [1, 3] }, "main": { "col": [1, 12], "row": [4, 6] } } } } },
    "full":   { "slots": { "main": { "col": [1, 12], "row": [1, 6] } } }
  },
  "formats": { "9:16": { "grid": { "columns": 12, "rows": 8, "gutter": 24, "margin": [96, 64] } } },
  "motion": {
    "durations": { "fast": 180, "standard": 420, "slow": 800 },
    "easings":   { "standard": [0.2, 0, 0, 1], "out": [0, 0, 0, 1], "in": [0.4, 0, 1, 1] },
    "springs":   { "snappy": { "stiffness": 420, "damping": 34, "mass": 1 }, "gentle": { "stiffness": 170, "damping": 26, "mass": 1 } },
    "presets":   { "rise": { "from": { "opacity": 0, "transform": { "translate": [0, 24] } }, "ease": "out", "duration": "standard" }, "grow": {...}, "fade": {...} }
  },
  "shaders": { "palettes": { "ambient": ["#...", "#...", "#...", "#..."] }, "presets": { "backdrop": { "kind": "mesh", "params": {...} } } },
  "charts": { "axis": { "role": "label" }, "label": { "role": "numeral", "show": "auto" }, "legend": { "role": "label", "place": "direct" },
              "strokeWidth": "thin", "cornerRadius": 0, "barGap": 0.5, "groupGap": 0.1, "pointRadius": 0, "dotRadius": 6, "donutHole": 0.72, "tickCount": 5,
              "maxTicks": 5, "signal": "accent",
              "annotation": { "role": "label", "stroke": "thin", "color": "accent", "band": 0.12, "dimmed": 0.5 } },
  "tables": { "header": { "role": "label", "color": "onSurfaceMuted" }, "cell": { "role": "body" }, "rule": { "stroke": "hairline" },
              "rowRule": { "stroke": "hairline", "opacity": 0.4 }, "rowGap": 4, "stretch": false }
}
```

**Cascade** (later wins): theme role defaults → node `style` → state `props` → `overrides`. Only `overrides` may contain raw pixel/color literals; literals elsewhere are lint **W300**. Overrides are tracked per node so a client can show "3 overrides, not theme-safe." In detail (PLAN 1.6):

- **`style`** is on text nodes and runs. It names role properties: `family` (a key in `type.families`), `size`, `weight`, `leading`, `tracking`, `opsz`, `case`, and `color` (a color token or role). Each replaces the role's value. A run set in the node's role starts from the node's look, so the node's `style` applies to it, and the run's own `style` comes on top. A run with its own `role` starts from that role alone. Any `size`, and any literal color, is a pixel value: theme-legal only in `overrides`.
- **State props.** Tracking (§2.2) merges a node's defaults, its `style` included, with each state's delta. An object merges one level, so a state can change `style.color` alone.
- **`overrides`** holds a delta per node, checked against the node's type like a state's (E106; E104 for `type`; E102 for a node that is not there). It merges into the node after tracking, in every state, the same way a delta does: it wins over the theme, the defaults, and every state, and `null` deletes. Each key it sets is one override, and an object counts by its keys, so `style: { size, color }` is two. `scaena inspect --resolved` shows each node's overrides and each text node's look, and lint I402 names them.
- **Colors** are a token or color role the theme names (role → token → literal), or a literal written out: `#rrggbb[aa]`, `oklch(L C H [/ A])`, or `oklab(L a b [/ A])`, read as CSS Color 4 reads them. Oklab converts to sRGB through `libm` (§13), and a color outside sRGB is clipped.
- **Names are the swap contract.** Two themes swap cleanly when they define the same names (roles, layouts and slots, presets, palettes), so name them for their job: a background palette is `ambient`, not `dusk`. `scaena theme --apply` reports what a swap breaks (§7.1).
- **A theme is edited by patches of it** (PLAN 2.61, ADR-0016). An edit is RFC 6902 operations on the theme the deck names, each path a JSON Pointer into it (`/tokens/color/accent`, `/type/roles/body/size`, `/grid/gutter`); an inline theme's are the same, written into `deck.json`. It is checked as a re-theme is: an edit after which the deck would not validate (a name it uses taken out, E102; a value the schema refuses, E106) is refused, and nothing is written. Otherwise the theme file is written in canonical form, as `save` writes it, and every node that names what changed draws in it. A theme that ships is edited in the bundle's copy; the one the CLI, the MCP server, and the page carry never changes. `scaena theme --edit` and the MCP tool `theme_edit` make one (§7.1, §7.2), and so does the editor's Theme tab (§9.2). The history keeps the theme beside the deck (§8.1).
- **The shipped themes share one vocabulary** (PLAN 1.34). Dusk, Daybreak, and Ember define the same names for the same jobs, on grids of 12 columns and 12 rows, so a deck moves between them by swapping the file: no name is missing, and a cell is a cell in each. What changes is the look. Each theme puts its slots where its design wants them, and lint says what the new type and colors break.
  - **Layouts and their slots:** `title` (kicker, title, subtitle, art); `statement` (kicker, statement, support); `poster` (kicker, statement, support, note, art); `full`, `figure`, and `narrow-figure` (kicker, header, main, note); `stat` (kicker, number, claim, detail, under); `split` (body, main); `art-left` and `art-right` (kicker, header, art, body); `quote` (quote, who).
  - **A slot is named for what goes in it.** `main` is the one thing a slide shows, `note` what is said of it (its source, or the facts beside it), `header` the slide's claim, and `kicker` the line that names its section. `claim` and `detail` are what a stat's number means and what stands behind it.
  - **Roles:** display, headline, title, lede, body, caption, kicker, label, axis, value, numeral, figure (a number among facts), quote, and code.
  - **Colors:** ink, paper, paper-2, accent, accent-2, muted, and line, with the same color roles and data palettes.
  - **Shaders:** the presets `backdrop` (a mesh behind a title) and `texture` (a slow noise field), and the palettes `ambient` and `texture`.
  - **Motion:** the same durations, easings, springs, and presets.

### 3.7 Charts

A chart is a declarative spec compiled to **marks**; it never stores pixels.

```jsonc
{
  "type": "chart",
  "kind": "bar",                       // v1: bar | stackedBar | line | area | scatter | dot | donut | slope | range
                                       // deferred (not v1, not scheduled): waffle | heatmap
  "orient": "horizontal",              // bar | stackedBar: bars across, categories down the side (PLAN 1.29)
  "interval": { "low": "p10", "high": "p90" },  // range: the fields of each value's interval (PLAN 1.30)
  "data": "@q3",                       // data source ref, optionally through a transform pipeline (§3.10)
  "dataTransform": [ { "filter": "region == 'NA'" }, { "sort": "-revenue" }, { "limit": 8 } ],
  "x": { "field": "quarter", "type": "ordinal" },
  "y": { "field": "revenue", "type": "quantitative", "format": "$,.0f", "domain": [0, null] },
  "series": { "field": "product" },
  "color": { "field": "product", "scale": "categorical" },
  "key": "product",                    // identity for morphing marks across states
  "axes": { "x": { "show": true }, "y": { "show": false, "gridlines": true } },
  "labels": { "show": "ends", "role": "numeral" },
  "legend": "none",                    // or "direct", "top", { "place": "right", "title": "Product" }
  "annotations": [
    { "kind": "callout", "at": { "x": "Q3", "y": 42 }, "text": "Launch" },
    { "kind": "rule", "at": { "y": 50 }, "text": "Target" },
    { "kind": "highlight", "at": { "series": "Cloud" } }
  ],
  "enter": { "preset": "grow", "stagger": 40, "spring": "snappy" }
}
```

Rules:
- **Kinds are normative here.** v1 ships exactly `bar`, `stackedBar`, `line`, `area`, `scatter`, `dot`, `donut` (PLAN 1.9), and `slope` and `range` (PLAN 1.30; deck format 0.15); the schema enum matches. `waffle` and `heatmap` are deferred and unscheduled; adding one is a schema change plus a PLAN task.
- All color, type, stroke, and radius come from the theme (`charts` section + tokens). Charts have no style literals.
- **Defaults** (PLAN 1.9; deck format 0.9, theme format 0.6). Unset, a chart draws as Tufte would: ink for data and little else. No frames, panels, or gradients; narrow bars with square corners; values printed on the marks rather than an axis to read them off; series named where they end rather than in a legend; quiet axes with at most five reference lines; and one signal color, spent by a highlight on what the slide is about, while the rest of the data stays in the theme's palette. Each is a theme token or a chart prop, so a theme can draw any other way, and the defaults below are what a theme leaves unset.
  - A theme carries the rest of the look. Dusk, the example theme, holds to a presentation chart style: data in graphite and quiet grays (`tokens.data.categorical`), each of which reads as text on the surface (4.5:1), with the accent as the signal; a one-hue sequential scale; chart text set as written, in sentence case rather than capitals, axis labels at 12 pt and value labels and series names at 13 pt (24 and 26 units on a 1920-unit canvas, 2 units to the point); and lines 1.5 pt wide (`regular`, 3 units). Chart text under 12 pt at presentation size is lint **W312** (PLAN 1.27). A plot squashed under 120 units across or down, as a short slot or a few rows of a grid can leave it, is **W313** (PLAN 1.36).
- Marks are matched by `key` across states. A data update interpolates each matched mark, so axis rescaling animates; added and removed keys enter and exit with the chart's presets. Value labels ride their marks and count from the old value to the new.
- A change of `kind` morphs only between kinds that draw the same marks: bars that regroup (`bar` ↔ `stackedBar`). Any other change of kind (a bar chart to a line) cross-fades the chart.
- Value labels that overlap, or that cover another mark, are lint **W310**, unless `labels.collide` resolves them (`"hide"` or `"nudge"`; Labels below).
- Numbers and dates print through an encoding's `format`, in d3's grammar plus a compact type `k` (`$.2~k` prints `$1.2B`). They print in the deck's language (`meta.lang`), and identically on every platform (`docs/spec/format.md`).

**Compile (PLAN 0.10, 1.9).** A chart compiles once per snapshot to keyed marks, labels, axes, and a legend; frames only sample them (§5).
- **Data.** Each row is a datum. A null `y` is a gap: the row draws nothing. Any other `y` that is not a number is an error. Categories (the `x` values) and series keep the order they first appear in.
- **Keys.** A datum's key is its `key` field (default: its `x` value), joined with its series when the chart has one, so `Q3` of `Cloud` is its own mark. A donut's keys are its categories. A gap has no key. A key that repeats is an error that names it, E103 (`validate` finds it in every state that shows the chart).
- **Kinds.** Every kind draws one mark per datum.
  - `bar`: a bar `1 − charts.barGap` (default 0.5) of its band wide, square on the baseline and rounded by `charts.cornerRadius` (default 0, square) at its free end. With a series, the series share that width side by side, each `1 − charts.groupGap` of its slot.
  - `stackedBar`: each category's data stack in series order, positive values up from the baseline and negative values down. Only the outermost segment each way is rounded.
  - `line`: a stroke per series (`charts.strokeWidth`, default `thin`) through its points in x order, with a dot of `charts.pointRadius` (default none) on each.
  - `area`: like `line`, filled down to the baseline. With a series, the areas stack as bars do.
  - `scatter`: a dot at each datum's x and y. Without `sizeEncoding` every dot is `charts.dotRadius` (default 6). With it, a dot's area is proportional to its size value: the largest dot's radius is 2.5 times `charts.dotRadius`, and no dot is smaller than `charts.dotRadius`, so the smallest still reads as a dot.
  - `dot`: a dot of `charts.dotRadius` at each category's value.
  - `slope`: two states compared (PLAN 1.30). `x` holds exactly two categories among the rows the chart draws, the states in data order (`2020`, `2025`); any other count is E103. Each series is a line, stroked as a line's is, from its first state's value to its second's, the two points on the plot's left and right sides, with a dot of `charts.pointRadius` at each (default none), and the states are named under their points. Each first value ends a space before its point and each second value begins a space past it, the middle of its cap height level with the point, and the second says the change from the first, signed, in `y.format`: `$31 (+$19)`. A slope's values nudge apart unless `labels.collide` says otherwise, since each is read; its names (`direct`) stand a space past the widest second value, each level with its own. Its marks are keyed by series and state, first or second, and a `key` field is not read: so a state that moves a slope to another pair of states (2020 → 2024, then 2020 → 2025) tilts its lines rather than scrolling them, its second values cross-fading as their changes do.
  - `range`: a span of the value axis at each category (PLAN 1.30), in one of two forms.
    - With a `series`, a dumbbell. Each series' value is a dot of `charts.dotRadius` in its color, and a stroke of `charts.strokeWidth` in `charts.axis.color` joins the category's dots from the lowest to the highest. The lowest value stands under its dot, and the others over theirs.
    - With `interval: { "low": field, "high": field }`, each value with its interval: a dot at its `y`, and a stroke in its color from its `low` to its `high`, its value over the interval's top. An interval with an end missing is none. With a series too, a category's values stand side by side in its band, as grouped bars do.
    - A range with neither, and `interval` on any other kind, are E106; an end that is no column of numbers is E103. The value axis spans the values and the intervals' ends.
  - `donut`: each datum a slice of a ring, its share of the total, clockwise from twelve o'clock in data order. The hole is `charts.donutHole` (default 0.72, a thin ring) of the radius. A negative value is an error. A donut has no axes.
- **Forecasts and estimates** (`projected`, PLAN 1.28; deck format 0.10, theme format 0.8). A line's or an area's rows can be marked projected: a forecast, or an estimate. `"projected": { "field": "estimate" }` marks each row whose `estimate` is true, a `boolean` column. `{ "field": "kind", "value": "forecast" }` marks each row whose `kind` is `forecast`.
  - A line runs dashed from its last actual point through what is projected: a segment is projected where either of its ends is. Its dash and the gap after it are `charts.projected.dash` (default `[3, 2]`) widths of the line, with square ends.
  - An area is filled under the same stretch at `charts.projected.opacity` (default 0.5) of its color.
  - A projected value says it is an estimate: its label is the value, a no-break space, and `projected.note`, else `charts.projected.note`, else `est.` (`$31 est.`). In a transition it cross-fades where a value alone counts.
  - In a transition, a row that turns actual, or projected, does so halfway.
  - A `field` the data lacks is E103. So is a `value` its column cannot hold, and, with no `value`, a column that is not `boolean`. `projected` on any other kind of chart is E106.
- **Bars across** (`orient`, PLAN 1.29; deck format 0.14). `"orient": "horizontal"` runs a `bar`'s or a `stackedBar`'s bars across from a baseline at zero, right for a value above it and left for one below, its categories down the side in data order, a band each; `vertical`, the default, stands them up. `orient` on any other kind is **E106**. The channels keep their meanings, and so do `axes.x` and `axes.y`, annotations, keys, and colors: `x` is the category, `y` the value.
  - A bar is `1 − charts.barGap` of its band thick, square on the baseline and rounded at its free end; a series shares the band, one under another, as bars that stand up share theirs side by side, and a stack's segments go in series order out from the baseline.
  - Each category's name stands in a gutter at the plot's left, right-aligned a space before the plot (before the values of bars below zero), the middle of its cap height level with its band's middle. Dates print as on a category axis; dates and numbers that would come within a space unit of each other keep every k-th name, as there, and text that overlaps is W310.
  - A value stands a space past its bar's free end, the middle of its cap height level with the bar's middle; a stack's total stands past its last segment. The plot leaves room at its right for the widest value past a bar above zero and a callout's words, and at its left for the values past bars below it.
  - The value axis (`axes.y.show`) prints its ticks under the plot, each centered on its tick inside the plot's sides; `axes.y.gridlines` rule each up the plot, and the baseline at zero runs down it. `axes.x.gridlines` rule between the categories' bands. The category axis's title stands over the names at the chart's left; the value axis's is centered under its labels.
  - Bars across have no column past their ends to name their series in: `auto` places their legend `top`, and `direct` is an error.
  - Annotations turn with the bars. A `y` rule runs up the plot, its words beside its top; an `x` rule runs across through its category's middle, its words over it, clear of the marks and words behind them. A band of values runs up the plot from top to foot, one of categories across it. A callout's leader runs on from its bar's value end, past the value, to its words, which stand past every mark and word in their row, the middle of their cap height level with the leader.
  - In motion they move as bars that stand up, turned: a bar grows from the baseline across, a stack opens across, a window of categories scrolls down the chart, and bars that regroup do so in two stages: into a stack, lengths and then thicknesses; out of one, thicknesses and then lengths. A change of `orient` cross-fades the chart.
- **Continuous x.** `line`, `area`, and `scatter` run along a continuous x when `x.type` is `quantitative` or `temporal`; a scatter always does, so its x column must hold numbers or dates. A number axis widens to round values as the value axis does, so no datum sits on the plot's edge. A date axis spans the data and ticks on calendar boundaries, at the step nearest a `charts.tickCount`th of the span as d3's time scale picks it: 1, 3, 6, or 12 hours; 1 or 2 days; weeks (Sundays); 1, 3, or 6 months; or 1, 2, 5, 10, 25, or 100 years. Its ticks print in `x.format`, else by the step's unit as a category axis's dates print (below): short (`%b` for months, `%b %-d` for days, `%-I %p` for hours, `%Y` for years), and the first label kept, and any whose year (an hour's day) is not the last kept label's, long, naming it too: `Sep 2025`, `Oct`, `Nov`, `Dec`, `Jan 2026` (PLAN 2.81). Each tick is keyed by its date written whole, in the step's long format. Any other x is a category axis: a band per category.
- **Color.** By series, in `tokens.data.categorical` order, cycling. A `color` field of text with no `series` groups the data as a series would. A donut colors its slices by category. A series or category keeps its color from state to state: colors go in the order each first appears in the chart's data across the cue list, so one that leaves does not recolor the rest. A `color` field of numbers shades each mark along `tokens.data.sequential` from the data's minimum to its maximum, or along `tokens.data.diverging` around zero with `color.scale: "diverging"`, mixed in Oklab between stops. Otherwise every mark is the first categorical color.
- **Legend.** One entry per series, or per slice of a donut, when there are two or more, in `charts.legend.role` (else the axis's role) and its color. `legend` places it. `auto`, the default, is the theme's `charts.legend.place`, else `direct`; but bars grouped side by side have no end to stand a name by, so an `auto` that would be `direct` is `top` for them.
  - `direct` names each series where it ends, with no swatch, the middle of the name's cap height level with the end: a line's last point, a dot plot's or a range's last dot, the middle of an area's last span or a stacked bar's last segment. A line's name starts a space past the value label that begins at its last point; any other a space past the end, a dot's past the dot's edge. The names stand in one column past the farthest end, nudged apart as `nudge` moves value labels (Collisions) and kept inside the chart. The plot gives up only the room the column needs past the last point, so on a category axis names often fit in the last half band. A scatter's series end apart from one another, so each name stands a space past its own series' last point.
  - A donut names each slice beside its value, on the value's far side from the ring: over it on the ring's upper half, under it on the lower.
  - `direct` takes no title (an error).
  - The other places draw each entry as a swatch, a square of the label's cap height rounded like the bars, on the label's baseline: `top` above the plot, under the value axis's title, wrapping across the chart's width; `bottom` at the chart's foot; `right` in a column beside the plot from its top. `none` hides the legend.
  - `{ "place", "title" }` places it (as `auto` does when unset, but `top` for a titled legend `auto` would place `direct`) and gives it a title in `charts.title.role`: the first line of a column, or the start of a row, its entries wrapping under one another after it, every entry and the title on one baseline.
- **Scale.** The value axis runs from `domain[0]` to `domain[1]`. An unset bound is the data's extent, and for kinds whose length is the value (`bar`, `stackedBar`, `area`) it includes 0. Stacked kinds take the extent of their stacks, and a range the extent of its intervals too. The plot leaves room above the marks for value labels.
- **Labels.** Category labels (or x ticks) sit under the plot in `charts.axis.role`, cap tops one `tokens.space.unit` below it, over a baseline rule at 0 (`charts.axis.stroke`, `charts.axis.color`) where the value axis reaches 0. A value axis that starts above 0, as a line's or a dot plot's may, has no rule at its foot, which would read as zero. `labels.show` (`auto` | `all` | `ends` | `none`) prints values in `labels.role` or `charts.label.role`, with tabular lining figures, in `y.format`. `auto`, the default, is the theme's `charts.label.show`, else by kind: the data goes on the marks, so every bar, stack total, dot, and slice prints its value, a slope both its ends, and a line its first and last; an area and a scatter print none, and show their value axes instead. With no format they print as d3 prints a number: `0.1 + 0.2` prints `0.3`. `ends` labels each series' first and last datum. A bar's label sits one space unit above it (below a negative bar), and a point's sits above the point. A line's first value ends at its point and its last begins at its point, away from the line, which leaves the one and comes to the other, so neither crosses it. Where a point stands on or near the plot's side, as on a time axis or in a plot narrower than its values, the plot moves in to leave its value the room. That room is what the value needs of the plot that is left once the names and last values beside it have theirs. The first values stand two space units from the value axis's labels. A stack prints one label, its total, over its top segment: every stack with `all`, the first and last with `ends`. A donut's label sits outside its slice's middle, on that side of the ring, a space unit past it and half its cap height more toward twelve and six o'clock, so it clears the ring all round; the ring is as large as it can be with every label and name inside the chart. Every other value label, and every category label and x tick, is centered on its mark or tick but stays inside the plot's sides. Category labels print in `x.format` when the column holds numbers or dates. A date column with no format prints by the calendar unit its dates step by, read off what they all share: `%Y` where each is a year's first day, `%b` a month's, `%b %-d` a midnight, else `%-I %p` (`%-I:%M %p` with minutes). The first label, and any whose year (a time's day) is not the last label's, names it too: `Jan 2026`, `Feb`, …, `Dec`, `Jan 2027`. Where a date prints alone, as in a donut's legend, it always does. Where an ordered axis's labels (dates or numbers), or the ticks of a continuous x, would come within a space unit of each other, it keeps every k-th from the first, at the smallest stride that clears them, counted in categories or ticks: for months 2, 3, 4, 6, or 12; for days 2, 7, or 14; for hours 2, 3, 6, 12, or 24; for years and numbers 2, 5, or 10; past those, the last times 2, 5, 10, 20, …. A time axis counts by the unit its ticks step by, and its gridlines stay at every tick. A text axis keeps every category, and labels that overlap are lint W310.
  - A value label never covers another mark. A dot's that would goes under its dot. A bar's wider than its bar leans off the mark it would cover: it starts at its bar's left edge or ends at its right, away from that mark first, and inside the plot's sides. One that still covers a mark hides, unless the chart asked for its values (`labels.show` set), when it shows and the layout reports it for W310.
- **Collisions.** Value labels collide when their cap-height boxes come within a quarter space unit of each other.
  - `labels.collide: "hide"` keeps the labels of the largest values that touch no label kept before them (the earlier in data order on a tie), and hides the rest.
  - `"nudge"` moves labels up and down as little as it can in all (least squares), keeping their order: labels whose spans across overlap form a column, and each run of a column that would touch moves as a block centered on where its labels want to be. A nudged label rides its mark at its new height.
  - Without `collide`, the layout reports each pair that collides, for W310 (PLAN 1.15 runs layout lints), with each value label that covers a mark. Values nobody asked for (`auto`) that collide hide instead, as with `hide`.
- **Axes** (PLAN 1.9). Category labels and x ticks show unless `axes.x.show` is false.
  - `axes.y.show` shows the value axis (default: on for a scatter and an area, which print no values by default; off otherwise): tick labels right-aligned in a gutter left of the plot, in `charts.axis.role`, each with the middle of its cap height on its tick.
  - `axes.y.gridlines` rules each tick across the plot (`charts.gridlines`: stroke, color, opacity), except the one a baseline at 0 already rules. `axes.x.gridlines` rules the plot from its top to its foot: at each x tick on a continuous x; across a category axis, between the bands of bars and through each category of the other kinds. A category's gridline is keyed by the category after it, so it moves with its bars.
  - With either one, the domain widens to round values at the tick step (d3's `nice`), and a `domain` bound the author sets stays. Ticks fall about `charts.tickCount` (default 5) to the axis, on d3's steps of 1, 2, and 5 × 10ⁿ, and no more than `charts.maxTicks` (default 5): the axis asks for fewer, down to two, until they fit, so by default it draws at most five reference lines (theme format 0.6).
  - Tick labels print in `y.format` at the precision the step needs, as d3's `tickFormat` does: `$,f` ticks read `$0`, `$5`, and `s` and `k` ticks share one prefix. A format's own precision is the most its ticks take: `$,.1f` values read `$12.5`, but their ticks a whole step apart read `$10`, not `$10.0`. With no format they print as d3's `,f`.
  - Titles (`axes.x.title` or `axes.y.title`, else the encoding's `title`) print in `charts.title.role`, else the axis's role. The value axis's title sits above the plot at the chart's left; the category axis's is centered under the category labels.
  - In a transition the value axis rescales as d3's does. A tick on both sides moves. A tick on one side only rides from or to where its value sits on the other side's scale, fading.
- **Clip.** A chart clips at its cell's sides, so marks that ride out of a scrolling window pass under them. When something sits beside the plot (a value-axis gutter, a legend at the right), the plot clips at its own sides instead, so a mark leaving passes under the plot's edge, not over the tick labels or the legend. Vertically it draws on the whole canvas, so a figure that overshoots the cap height keeps its top.
- **Motion.** A matched mark interpolates its shape: a bar its box, a dot its center and radius, a slice its angles. Lines and areas are redrawn each frame through their marks as they move. A key on one side only comes from, or goes to, where it would stand on the other side, moving with its nearest matched neighbor (the earlier on a tie), so a window that advances a period scrolls:
  - A bar grows from the baseline, or shrinks onto it.
  - A point of a line or an area whose series runs on the other side lies on that series' path where its x falls, level with the path's end past it: a vertex bends out of the line, and a new period slides in off its end. With no such series, a line's point rises from the baseline and an area's span grows from it.
  - A member of a stack (a stacked bar's segment, a stacked area's span, a donut's slice) opens or closes with no extent where it stands among the stack's members, in an order that keeps both sides' orders. A stack never gaps, and a donut sweeps open from twelve o'clock. Members that move on different clocks (a stagger, or a member entering on its cue while the rest move with the transition) stack as they stand at each frame: each keeps the extent its own clock gives it and starts where the member before it ends, so a staggered stack builds member on member and a ring sweeps open slice by slice. A stack whose members change order or change stacks moves each member by its own clock.
  - A dot of a scatter, a dot plot, or a range opens or closes where it is, and a range's stroke fades in or out with it.
  - Bars that regroup (`bar` ↔ `stackedBar`) move in two stages, as d3's do: into a stack, heights and then widths; out of one, widths and then heights. Any other change of kind cross-fades the chart.

  A value label rides its mark and counts at as many decimals as either end shows, from 0 for a new key and to 0 for a removed one, fading in or out with it. A stack's total counts with the stack, by how much of it stands. A count stops at its ends, though a spring carries its mark past them (§3.9). Legend entries move and change color, and an entry on one side only fades. An entry that turns from a key's entry into a name at the ends, or back, as bars regrouping into stacks do, fades out where it stood and in where it will stand, rather than crossing the plot. A chart that enters grows its values in; one that exits shrinks them out. Counting labels are spelled from the label's figures shaped once per snapshot, so frames never shape (§5); text the figures cannot spell cross-fades.
- **Enter and exit** (`enter`, `exit`). Without them, marks grow in and shrink out as Motion says, with the transition.
  - Either names a theme motion preset (`motion.presets`), or calls one: `{ "preset": "grow", "stagger": 40, "spring": "snappy" }`, whose `duration`, `ease`, `spring`, `stagger`, and `delay` override the preset's.
  - A chart reads a preset one mark at a time. Its `from` look is where an entering mark starts and where a leaving one ends: `opacity`, `transform.translate` in canvas units, and `transform.scale`. A scale grows the mark from where it would stand with no value (Motion), whatever its `anchor`. A preset that does not scale keeps its marks whole: they ride in or out with their neighbors, fading or moving as it says.
  - Entering marks take the target state's `enter` and stagger in its data order; leaving ones take the source state's `exit`, in its order. A line's or an area's unit is a series: its points move together, so its path keeps its shape, and the series come in one after another.
  - A chart's own presets run with the transition, and choreography can move its marks too (`{ "target": "rev", "enter": "grow", "timing": "after" }`, §3.9). Either way the marks move as units on the state's clock (PLAN 1.11), each a mark or a line's or an area's series: unit k starts `k × stagger` ms after the first and runs for the preset's `duration`, else the theme's `standard`, or for its spring's settle time, easing or springing as §3.9 says (`libm`, so every platform agrees). A long schedule extends the state's span; nothing is squeezed into the transition. The chart's axes, legend, and titles come in with its first mark and go with its last.
  - A chart's own presets move its marks whatever the preset would split. A choreography item that moves the chart whole (no split) moves it as one, its values at rest. Marks take a look's opacity and translation, and a look that scales grows them from their baseline; its rotation, color, and params do not reach marks. A cue on the whole chart takes the whole look.
- **Annotations** (`annotations`), drawn in `charts.annotation`: `stroke` (default `thin`) and `color` (default `accent`) for rules, leaders, and bands, `opacity` for the strokes, and `role` (default the value labels') for text, which prints in that color. An annotation stands at `at`: an `x` is a category, as its datum reads (a date in ISO 8601), or on a continuous x a number or an ISO 8601 date; a `y` is a value; a `series` is a series. A value on the value axis widens it, as the data does, unless the author set that bound, when one past it is an error; an `x` on a continuous axis widens it too. A category, series, or x the data does not have is E103, and an annotation that stands where its kind cannot is E106 (`validate` finds both).
  - `rule`: one `x` or one `y`. A `y` rules the plot across, its text at the plot's start with its descenders half a space unit over the rule; an `x` rules it from top to foot through the category's middle, its text beside the rule's top, after it unless that passes the plot's end. Say its value and what it means (`"Target: $20"`).
  - A rule, or a callout's leader, breaks where it would cross text: a value label, a name, or another annotation's text, half a space unit clear of it either side. A rule that moves breaks only while it crosses the text: a break for text on one side only closes on its middle, or opens from it.
  - `band`: from one `x` to another, or one `y` to another, under the gridlines, filled at `charts.annotation.band` (default 0.12) of the color. Across categories it covers both ends' bands. Its text sits inside its top-left corner, half a space unit in, or a space unit under a `y` rule that would cross it.
  - `callout`: `text` at one `x`, and a `y` or the mark of a `series` there; with neither, the one mark at that `x`, an error if there are several. A leader rises from half a space unit over the point, or over the mark's value end and its value label, to two and a half space units over the highest mark, line, value label, or earlier annotation's text under the text. The text is centered on the point inside the plot's sides, with its descenders half a space unit over the leader's end. Its text never sits on a `y` rule: one within a space unit of it moves the text on past the rule, the leader crossing it. Under a bar below the baseline, the leader and text go down.
  - `highlight`: one or more `x`, `series`, or both, picking out the data that has them. What it picks takes the signal color, `charts.signal` (the `accent` color when unset): its marks, and a line, an area, or a legend entry all of whose marks it picks, a direct name with it. The rest of the chart dims to `charts.annotation.dimmed` (default 0.5): every other mark, a line or an area with none of its marks picked, and a legend entry whose marks are none of them. Their value labels and names dim half as far (0.75 by default), so the context stays legible. It says nothing.
  - A donut has no axes: it takes highlights of its slices (`at.x`) only.
  - The plot leaves room above it for a callout's text and a rule's text, as it does for value labels.
  - Annotations match from state to state by kind, axis, and place among the chart's annotations of both: the second `y` rule is the second `y` rule. A matched rule, band, or leader moves to its next place, as a new value moves the axis; changed text cross-fades as it moves; one on one side only fades where it is. A highlight that moves brightens and dims the marks, lines, labels, and legend entries it passes between.
  - The editor makes them from a chart's marks (§9.2.4, PLAN 2.67): a mark picked offers to highlight it, call it out, rule its value, or band from it to another, each an `annotate` op (§7.3).

`axes` takes `x` and `y`, each `{ "show", "gridlines", "title" }`; `labels` takes `show` (`auto` | `all` | `ends` | `none`), `role`, and `collide`. A text node's `axes` is another property (§3.3).

### 3.8 Shader nodes

```jsonc
{ "type": "shader", "kind": "mesh", "seed": 7, "palette": "ambient",
  "params": { "points": 5, "drift": 0.15, "softness": 0.8, "grain": 0.04 } }
```

Kinds in v1: `mesh` (mesh gradient), `gradient` (linear/radial/conic), `noise` (simplex/fbm), `grain`, `particles` (seeded field). Each kind has a **typed parameter schema**, a **seed**, and only depends on `t` for motion.

Requirements:
- Every kind has a CPU reference implementation in Rust **and** a WGSL implementation; the GPU is an optimization. Parity is tested per kind with a per-pixel tolerance (§14).
- No arbitrary shader source in documents. Ever.
- Uniforms are animatable properties like any other.
- For PDF/SVG export, shaders rasterize at a declared DPI (default 2× canvas) and embed as images.
- **Clock.** A shader's `t` is the frame's time on the global timeline (§2.4), in seconds, so a background that persists across states drifts on through every transition, motion, and hold instead of starting over. At rest, a state's shaders show the moment it comes to rest; a frame `t` past its span, in its hold, shows that later time. The clock is an `f32` in the display list (§6), so past 3.4 × 10³⁸ s it holds at its largest value.
- **Where the code lives.** Each kind's CPU reference and its WGSL twin sit side by side in `scaena-core::shader`, because painters run them and painters depend only on core. The engine resolves a node to a shader op (§6): the kind, the seed, the theme palette's colors, typed params, and its rect.
- **Fast, and the same.** The CPU painter computes `mesh`, `grain`, and `noise` a block of a row at a time, one step of the reference across the block before the next, which the compiler runs as SIMD; `noise` hashes each lattice cell's corners once, not each pixel's four. Each pixel does its reference's arithmetic in its order, so the bytes are the reference's, which a test holds pixel for pixel. Off the web, every kind's rows are spread in bands over the cores, up to 8; no row reads another, so the bytes do not depend on how many. There, sRGB encoding reads two tables made from the 255 thresholds, which give the threshold search's byte for every `f32`, all 2³² of them tested.

**Shaders stay off data** (PLAN 1.26). A shader is atmosphere for title and section slides. A chart or a table reads against a plain surface: a mesh, noise, or particles under its marks become noise in the data, and a gradient shifts the colors that encode it. Lint W311 warns about a shader painted behind a chart or a table where they overlap, in any state. The shipped themes' presets are for slides without data, and their example decks keep the mesh off data slides.

**Presets** (deck format 0.6, theme format 0.2). A theme's `shaders.presets` name shaders a node can take whole: `{ "kind", "palette", "params" }`, its params typed by its kind as a node's are. A node names one with `preset`; the node's own `kind` must be the preset's (E106), its `palette` and each of its `params` win over the preset's, and a preset the theme lacks is E102.

```jsonc
// theme
"shaders": { "presets": { "texture": { "kind": "noise", "palette": "texture", "params": { "octaves": 4, "scale": 0.0015 } } } }
// deck
"bg": { "type": "shader", "kind": "noise", "preset": "texture", "seed": 3, "params": { "contrast": 0.6 } }
```

**Params** are numbers, or a name a kind lists (a gradient's `shape`). The schema types them by kind, as the tables below do; the engine types them again when it resolves the node, and an unknown or out-of-range one is an error naming the node. A shader op carries numbers only: a name is its place in the kind's list (§6).

**`mesh` (PLAN 0.11).** `points` palette colors (cycling the palette) sit at seeded, evenly spread places over the rect (the R2 low-discrepancy sequence from a seeded start) and drift on slow Lissajous paths of amplitude `drift` (0.2–0.45 rad/s per axis, seeded phases). Each pixel blends them in Oklab, weighting a point at distance d by 1/(1 + d²/σ²)² with σ = 0.6 × `softness`; distances are in units of the rect's shorter side, so circles stay round at any aspect. Alpha blends with the same weights. `grain` adds seeded noise of that amplitude to Oklab lightness, per device pixel.

| param | type | range | default |
|---|---|---|---|
| `points` | integer | 2–16 | 5 |
| `drift` | number, in shorter sides | 0–1 | 0.1 |
| `softness` | number | above 0, up to 1 | 0.6 |
| `grain` | number, in Oklab L | 0–0.25 | 0.03 |

**`gradient` (PLAN 1.10).** The palette's colors (up to 16) run evenly spaced across the gradient, blended in Oklab. A `linear` gradient runs along `angle` over the rect's whole extent that way, as CSS's does; a `radial` one runs out from (`x`, `y`) to `radius` shorter sides of the rect, holding its last color beyond; a `conic` one turns about (`x`, `y`) from `angle` and runs on from its last color back to its first, so it has no seam. A linear or conic gradient turns `speed` degrees a second. Per pixel, a linear gradient is `+ − × ÷`; a radial one adds a square root and a conic one an arctangent, whose last bits a GPU may round differently (within §13.5).

| param | type | range | default |
|---|---|---|---|
| `shape` | `linear`, `radial`, or `conic` | | `linear` |
| `angle` | number, degrees clockwise from up | −360–360 | 180 |
| `x`, `y` | number, across and down the rect | 0–1 | 0.5 |
| `radius` | number, in shorter sides | above 0, up to 4 | 0.75 |
| `speed` | number, degrees a second | −360–360 | 0 |
| `grain` | number, in Oklab L | 0–0.25 | 0 |

**`noise` (PLAN 1.10).** 3D simplex noise (Gustavson's, its gradients picked by a seeded integer hash) at each pixel's place in canvas units times `scale`, and at `speed` × t along the third axis, so the field changes as time runs rather than sliding. `octaves` layers finer noise over it, each at twice the frequency and half the amplitude. The sum, spread by `contrast`, picks a color along the palette's ramp (up to 16 colors), blended in Oklab. The seed moves the field and keys its hashes. Per pixel it is `+ − × ÷`, `floor`, comparisons, and integer hashing.

| param | type | range | default |
|---|---|---|---|
| `scale` | number, cycles per canvas unit | above 0, up to 0.1 | 0.0015 |
| `octaves` | integer | 1–8 | 4 |
| `speed` | number, cycles a second | 0–2 | 0.05 |
| `contrast` | number | 0–4 | 1 |
| `grain` | number, in Oklab L | 0–0.25 | 0 |

**`grain` (PLAN 1.10).** Film grain over what lies under it. Each device pixel takes seeded noise between −½ and ½: below zero it shows the palette's first color, above it the last, at an opacity up to `amount` as far as the noise is from zero. The grain changes `fps` times a second, so it shimmers rather than crawls; `0` holds it still.

| param | type | range | default |
|---|---|---|---|
| `amount` | number, the strongest grain's opacity | 0–1 | 0.12 |
| `fps` | number, changes a second | 0–60 | 12 |

**`particles` (PLAN 1.10).** `count` soft discs start at seeded, evenly spread places (the R2 sequence) and drift in seeded directions at `speed` shorter sides a second, each at its own pace (½ to 1½ times). One that leaves a side comes back at the other once it is wholly out. Each is `size` shorter sides in radius (½ to 1½ times, seeded), its edge fading over `softness` of its radius, in the palette's colors in turn; later ones lie over earlier ones, blended in linear light. Places come from the seed and t once per frame (`libm`); per pixel it is `+ − × ÷` and comparisons.

| param | type | range | default |
|---|---|---|---|
| `count` | integer | 1–64 | 24 |
| `size` | number, in shorter sides | above 0, up to 0.25 | 0.01 |
| `speed` | number, shorter sides a second | 0–1 | 0.02 |
| `softness` | number, of the radius | 0–1 | 0.5 |

Each kind's CPU reference and its WGSL twin are tested against each other on the GPU (`shader_parity`), per seed, time, transform, and parameters. Between two states a shader morphs by its uniforms (PLAN 1.12): with the same kind and seed and as many palette colors, its rect moves, its palette's colors mix in Oklab, and its params move from one value to the other, a param one side leaves out at its default. Three sorts of param do not move: a name a kind lists (a gradient's `shape`), a count (mesh `points`, noise `octaves`, particles `count`), which has nothing between, and a rate (`speed`, grain `fps`), whose phase on the global clock would race through (b − a)·t on the way. A shader whose kind, seed, or number of palette colors changes, or one of those params, cross-fades.

### 3.9 Animation

A state's **cue** starts with the transition into it (`t = 0`) and runs the transition and the state's **motions** on one clock (PLAN 1.11, deck format 0.7). Its **span** is when the transition and its last motion have both ended; from then on the state is at rest, and the global timeline moves on to its hold (§2.4).

**Where motions come from.**
- A node's own `enter` preset, as it enters (it is not on screen in the state before), and its `exit` preset, read from the state it leaves, as it leaves. Both run with the transition. A chart's own presets move its marks one at a time, also while the chart stays and its data changes (§3.7).
- A node's `emphasis` (a preset) and `anim` (keyframe tracks) in a state. Neither tracks to the next state (§2.2): a motion never replays on the next cue. `anim` runs with the transition, `emphasis` after it.
- The state's `choreography`. A node's own preset of a kind gives way to a choreography item of that kind on the node.

```jsonc
"choreography": [
  { "target": "title", "split": "words", "enter": "rise", "stagger": 30, "timing": "with" },
  { "target": "rev",   "enter": { "preset": "grow", "stagger": 40, "spring": "snappy" } },
  { "sequence": [ { "target": "note", "enter": "fade" }, { "target": "arrow", "enter": "rise" } ], "delay": 200 }
]
```

**Choreography.**
- An item moves one or more targets with one motion: `enter`, `exit`, or `emphasis`, each a preset by name or a call with settings (`{ "preset": "grow", "stagger": 40, "spring": "snappy" }`), or `anim`. An item with none, or with more than one, is E106.
- `timing: "with"` starts an item with the transition; `"after"`, the default, once the transition ends. `delay` (ms) waits that much more.
- A `sequence` runs its items one after another: each starts when the one before it has ended, then waits its own `delay`. A `parallel` starts its items together, each after its own `delay`. A group's `timing` and `delay` place the group, and its items' own `timing` does not apply. A state's top-level items run side by side.
- `split` divides each target into units: the `lines`, `words`, or `glyphs` of text (§3.5), a container's `children` in flow order (`at.index`, then the deck's order), or a chart's `marks` in data order (a line's or an area's by series, §3.7). Without one, each target moves whole. A split the target's type does not have is E106.
- Unit *k* of an item starts *k* × `stagger` ms after its first unit, counting across its targets in order.
- Each setting comes from the most specific place that gives it: the item, then the preset call, then the theme's preset, then the theme's `standard` duration and easing. At one place a `spring` wins over an `ease`, and a unit on a spring runs for its settle time.

**Presets** (theme `motion.presets`): a look (`from` or `to`) and timing (`duration`, `ease`, `spring`, `stagger`, `split`).
- A look is relative to the unit at rest: `opacity` multiplies its own, and `transform` takes `translate` (`[x, y]` canvas units), `scale` (a number, or `[x, y]`), `rotate` (degrees, clockwise), and `anchor` (`[x, y]`, fractions of the unit's box, default its center), which scale and rotation turn about. A unit's box is its node's box, a child's box, or for text, its glyphs' advances across the line boxes they sit in.
- `color` (a theme color) mixes every paint the unit draws toward it in Oklab, each keeping its alpha; images and shaders have no paint. `params.progress` (0–1) is how much of each outline a shape strokes is drawn, from where the outline starts, an arrow's head riding the tip and growing in over its own length; a shape's fill is drawn whole, and nodes that are not shapes are drawn whole. Colors and progress stop at their ends on a spring. Theme format 0.3 types the look, so any other key is a schema error (PLAN 1.12).
- An `enter` runs from the preset's `from` look to rest; before it starts, its unit is not drawn. An `exit` runs from rest to the `from` look; once it ends, its unit is gone. An `emphasis` runs from rest to the `to` look and back: eased, out along the easing to its middle and back the same way; on a spring, as the spring answers a tap, reaching the look once and settling back (an under-damped spring swings past rest and back). Each reads the other key when its own is missing.
- An entrance moves only what enters in the state, and an exit only what leaves. A `children` entrance on a stack that stays brings in only its new children, each at its place in the stagger. An entrance on a node that stays does nothing.
- A node that enters or leaves with no motion fades in or out with the transition.
- A call's `params` win over its preset's: `{ "preset": "draw", "params": { "progress": 0.3 } }` draws an outline on from 30%.

**Keyframes** (`anim`). Per property (`opacity`, `translate`, `scale`, `rotate`, `progress`; deck format 0.8 names no other), keys `{ "t", "v", "ease" | "spring" }`, `t` in ms from the motion's start, values as looks are:

```jsonc
"anim": {
  "opacity":   [ { "t": 0, "v": 0 }, { "t": 400, "v": 1, "ease": "out" } ],
  "translate": [ { "t": 0, "v": [0, 24] }, { "t": 400, "v": [0, 0], "spring": "snappy" } ]
}
```

Before the first key a track holds the first value; after the last, the last value, until the state rests. Between two keys it follows the later key's curve (default the theme's `standard`), a spring fitted to the gap.

**Containers.** A motion on a container moves everything in it, and one on its children moves each child with everything in it. The container's own panel moves only with motions on the container. Looks compose from the outermost container in, a tint on a tint as one mix in Oklab and progress multiplying. A group's own motions move its composited layer instead (§3.4), and a draw-on reaches the shapes in it.

**State transition** (per state): `"transition": { "duration": "standard", "ease": "standard", "spring": "snappy", "match": "id" }`. Per node: `"transition": "morph" | "crossfade" | "cut"`.
- A state without `transition` cuts. A bare duration (`"transition": "slow"`, or ms) sets the duration; the object form defaults `duration` and `ease` to the theme's `standard`.
- A `spring` takes the place of both: the transition follows the spring and lasts its settle time. An under-damped spring carries positions and sizes past their targets and back; opacity, colors, and chart data stop at their ends.
- The transition into a state starts from the state before it in the cue list (what was on screen), whichever state it tracks `from`. Into the first state, every node enters.
- `t ≤ 0` is the previous state at rest, and `t` at or past the span this state at rest, exactly.
- Nodes present in both states interpolate (PLAN 0.10). Text whose layout is unchanged moves; changed text morphs word by word (§2.3). Shapes morph point by point (§3.3), shaders by their uniforms (§3.8), and chart marks match by key and move their data (§3.7).

**Curves.** Springs are solved analytically (a damped harmonic oscillator, through `libm`). One settles once it stays within 0.001 of its target, moving slower than 0.01 a second, for 50 ms; that settle time is part of the resolved timeline, so every duration is known ahead of time (video export). Easing curves are cubic Béziers, named in the theme. Interpolation: numbers linear; colors in Oklab; transforms decomposed; paths with equal command structure point-wise, else cross-faded; text by word diff; chart marks by key; shader params by kind (§3.8).

Lint **W320** flags more than N concurrent animated nodes (theme-tunable, default 12) and **W321** flags total choreography longer than the theme's `maxBuild` (default 2.5 s).

### 3.10 Data sources

`data.<name>` → `{ "source": "data/x.csv" | "data/x.json" | { "inline": [...] }, "schema": { "<field>": "number|string|date|boolean" }, "parse": { "<date field>": "%Y-%m" } }`. Sources are read at resolve time and cached by hash. Live sources are deferred (§16). The engine reads no files: the caller hands it the bundle's data files as bytes, as it does fonts. CSV is RFC 4180 with a header row; JSON is an array of objects, whose columns are the keys they name, in the order they first name them: an object may name them in any order, and a key it leaves out is null. Schema types are `number`, `string`, `boolean`, and `date`; a column the schema does not type is a string. A `date` column reads with its `parse` format, keyed by column (`"parse": { "month": "%b %Y" }`), else as ISO 8601. Dates are civil, with no time zone (`docs/spec/format.md`). `scaena-core::data` reads sources, for the engine and for validation, so `validate` finds a value that does not fit its type (E103) before a render does. Charts and tables reference `@name`. A CSV's columns are its header's, with rows or without; JSON's are its rows' keys, else the schema's.

**Editing** (PLAN 2.55, ADR-0014). The `data_edit` operation edits a source in place: `scaena data` (§7.1) and the MCP tool `data_edit` (§7.2) both call it.
- **Edits.** A cell set, a row added (at a place, or at the end), and a row taken away. They are made in order, all or none, and rows count from 0, the first after the header.
- **Values.** A value is typed as text, and an agent may send a JSON number, boolean, or null. It must read as its column's type, by the rules a chart reads it by: a finite number, `true` or `false`, or a date in its column's `parse` format or ISO 8601. Empty, or null, is nothing. A value that does not read stops the edits, saying why.
- **A file is edited byte by byte.** A cell rewrites that field's text alone, quoted where it was or where it has to be. Every other byte stays: the header, the quoting, the line endings, a JSON file's layout. A row added copies the row before it. A row taken away takes its line, or its element and one comma, with it.
- **Inline rows are the deck's.** Their edit is a JSON Patch of `/data/<name>/source/inline`.
- **Refusals.** Edits that would leave the deck invalid are refused, as a patch is (§7.3).
- **Reading.** Read with no edits, a source shows each cell as written, and each cell its column does not read, with why. Such a cell is no reason to refuse an edit elsewhere, and the `set` that makes it read fixes it.
- **History.** The history records each edit with the file's bytes (§8.1).
- **Quotes.** Each figure a run quotes from the source is set again in the same change, with the claims that hold it (§3.5, ADR-0019); the result names the texts (`quoted`).

**Transforms** (PLAN 1.9). A chart or a table MAY read its data through `dataTransform`: steps that run in order, each an object that names one step.
- `{ "filter": "<expr>" }` keeps the rows where the expression is true.
- `{ "derive": { "<column>": "<expr>", … } }` adds a column per expression, in order, each able to read the ones before it. A name that is already a column replaces it.
- `{ "sort": "<field>" }` or `{ "sort": ["<field>", "-<field>", …] }` sorts by each field in turn, `-` for descending. Rows that tie keep their order, nulls go last either way, and text sorts by code point.
- `{ "limit": n }` keeps the first n rows.
- `{ "aggregate": { "<column>": "<op>(<field>)", … }, "groupby": ["<field>", …] }` gives a row per group, in the order groups first appear, with its `groupby` fields and each aggregate; without `groupby`, every row is one group. The ops are `count()` (rows), `count(f)` (values), `distinct(f)`, `sum`, `mean`, `median`, `min`, `max`, `first`, and `last`. All but `count()` set nulls aside, and a sum of nothing is 0.
- `{ "fold": ["<column>", …], "as": ["<key>", "<value>"] }` turns columns into rows, wide to long. Each row becomes one per folded column, with the column's name and its value (named `key` and `value` by default) beside the columns not folded. Folded columns hold one type.
- `{ "pivot": "<field>", "value": "<field>", "groupby": […], "op": "<op>" }` turns rows into columns, long to wide: a row per group, and a column per value of the pivot field, in the order they first appear, holding `op` over the group's values (`sum` for numbers by default, else `first`). A cell no row reaches is null.

Expressions are `docs/spec/expr.md`'s. Each step reads the table the step before it left, and `validate` checks each expression, and each field the chart reads, against the table at that point. A column that is not there, or one of a type its use cannot read, is E103 at the step, or at the encoding. A malformed step, or an expression that does not parse, is E106. Every step keeps rows in a fixed order and sums in it, so a transform gives the same table on every platform (§13).

### 3.11 Spine

```jsonc
"spine": {
  "sections": [
    { "id": "growth", "title": "Growth", "beats": [
      { "id": "doubled", "claim": "Revenue doubled year over year.",
        "evidence": [ "@q3" ], "states": [ "revenue", "mix" ],
        "notes": "Pause on the Q3 bar. Mention the launch.",
        "duration": 45,
        "media": { "infographic": { "priority": 1 }, "podcast": { "script": "..." } } }
    ] }
  ]
}
```

- Every state SHOULD belong to exactly one beat; orphan states are lint **W401**.
- The spine order defines reading order for accessibility and the export order for every projection (§10). It SHOULD be the order the states play in: a beat that comes after another in the spine while its states play first is lint **W426**, since the PDF would tell the story in one order and the video in another.

### 3.12 Accessibility

- Every non-decorative node carries `alt` (text nodes default to their content). Images without `alt` are **W410**.
- Reading order = spine order, then z-order within a state.
- How a node reads (`scaena-core::reading`, from the state's resolved props): text in the `display` or `headline` role is a level-1 heading, `title` a level-2 heading, and any other text a paragraph, its `alt`, if it has one, said in place of its words. An image or a chart is a figure that its `alt` describes. A shape or shader is a figure only when it has an `alt`, and decoration otherwise. A table reads row by row, the header's cells as column headers. A group reads its members in turn, or as one figure when it has an `alt`. A node whose `semantic` is `decoration` or whose `alt` is `""`, and a container's own fill and stroke, are decoration and are not read. `navigation` is read: a cover's title and an agenda are content.
- A link in a text (§3.5) reads as an `a`: a web address as its `href`; a state as `#` and its id, with `data-state`, which the player follows, and the state's name in its title.
- A text that is a list (§3.5) reads as one: a `ul` for bullets and an `ol` for numbers, each item an `li`, a deeper level's list inside the item before it, and a paragraph that is no item a `p`, all inside a `div` that names the node. A text with `alt` says it in place of its list.
- PDF export is tagged from the same data (PLAN 1.20). Its structure follows the spine: a section per spine section, holding a division for each page of its beats' slides, then the pages no beat names. A page's nodes read in z-order; what no node reads (the page's background, decoration) is an artifact. A table keeps a cell for every column of every row, an empty one where the data is null. A list is an `L` of `LI`s, each its marker's `Lbl` and its words' `LBody`, a deeper list inside the item before it, and a paragraph that is no item a `P`. A link is a link annotation over each stretch of its words: a web address a URI action, a state a destination on the page of its slide. A group drawn as one layer (at an opacity below 1, or in a blend mode) is tagged as one element: a figure if it has an `alt`, else a division. A PDF can tag the content of such a layer only as a whole. The spine's sections are also the PDF's outline, each titled by its `title`, else its first beat's claim.
- A text a role sets in other letters' case (`case`: capitals, lower case, title case) reads in a PDF as written (PLAN 2.88): its words are a span whose actual text (`ActualText`) is the node's text as written, so a copy, a search, or a screen reader takes "Annual meeting" where the page shows ANNUAL MEETING, as the single file and the player read it. A text whose glyphs say other words than it writes (a list's markers, a hyphen at a break aside) keeps what its glyphs say.
- Single-file HTML export reads from the same data (PLAN 2.5, `scaena-export::html`). For each state it plays, the export writes how the state reads, as HTML: each node in paint order, an element that names it (`data-node`). A heading (`h1`, `h2`) or a paragraph says the node's text as the deck writes it, or its `alt`; a figure is `role="img"`, named by its `alt`; a table is a `table` by rows, its first row's cells column headers, with a cell for every column of every row. A group reads its members in turn, or as one figure when it has an `alt`. A node in another language than the deck's carries `lang`. What no node reads is not there. The page shows the state's reading, unseen, in a polite live region, and the canvas is hidden from screen readers. Between states, what reads as it did stays in place, so a screen reader says what the state changed.
- The web player reads the same way (PLAN 2.8, §9.2). The engine's module writes each state's reading (`scaena_core::reading::html`, `Player.reading`) in the format shown, the same HTML a single file carries. Its state picker is the spine's outline, in spine order.
- Lint **E110**/**E111** enforce contrast (4.5:1 body, 3:1 display) against what is painted behind the text (§7.5).

---

## 4. DSL (`.scn`)

The DSL is an authoring projection of the logical document. It is line-oriented and indentation-scoped, and agents may edit either it or `deck.json`. `scaena compile` turns source into `deck.json`, and `scaena decompile` turns any deck into canonical source (§7.1). The compiler lives in `scaena-core::dsl`. `docs/authoring.md` is the guide for people: how to write, check, see, and save a deck.

**The round trip is semantic and exact, not textual.** The promise is

```
compile(decompile(document)) == document        (always, byte for byte as canonical JSON)
decompile(compile(decompile(document))) == decompile(document)   (the canonical form is a fixed point)
```

and explicitly **not** `decompile(compile(source)) == source`. "Always" means every document the typed model reads, valid or not, so an agent can decompile a broken deck and fix it in the DSL. Whitespace, blank lines, column alignment, the order of declarations, and the choice between shorthand and `key:value` are not preserved. Keys keep the order the source writes them in, and the decompiler keeps the deck's order, because canonical JSON keeps it.

Comments: `#` starts a comment outside strings. Comment lines, and a line's trailing comment, become the `_comment` of the next deck header, node, override, state, or node line in a state, which is where the format has one. Comments anywhere else are dropped. The decompiler writes a `_comment` back as comment lines. If you want your formatting kept, keep your file and regenerate `deck.json` from it: the compiler is the stable direction.

### 4.1 Example

This is `docs/examples/revenue.deck.scn`, the canonical source of `revenue.deck.json`. A test holds the two together in both directions.

```scn
deck "Q3 Review" theme:"themes/dusk.theme.json" canvas:1920x1080 formats:[16:9, 9:16] author:Jay
  created:"2026-10-01T18:00:00Z" lang:en-US
  description:"Reference deck: one morph, one build, one shader background, one spine."

font Fraunces "fonts/Fraunces-VF.ttf" axes:{wght: [100, 900], opsz: [9, 144]}
font Inter "fonts/Inter-VF.ttf" axes:{wght: [100, 900], opsz: [14, 32]}
font "JetBrains Mono" "fonts/JetBrainsMono-VF.ttf" axes:{wght: [100, 800]}
font Fraunces "fonts/Fraunces-Italic-VF.ttf" style:italic axes:{wght: [100, 900], opsz: [9, 144]}
font Inter "fonts/Inter-Italic-VF.ttf" style:italic axes:{wght: [100, 900], opsz: [14, 32]}
font "JetBrains Mono" "fonts/JetBrainsMono-Italic-VF.ttf" style:italic axes:{wght: [100, 800]}

data q3 "data/q3-revenue.csv"
  schema:{quarter: string, product: string, revenue: number, customers: number}

section open "Open"
  beat opening "This quarter changed the shape of the business." states:[intro] duration:15s
    notes "One beat. Let the title sit."

section growth "Growth"
  beat doubled "Revenue doubled year over year, and Pro drove it." evidence:[@q3]
    states:[revenue, mix] duration:60s
    media infographic:{priority: 1}
      podcast:{script: "Revenue doubled, but the story is the mix: Pro grew three times faster than Core."}
    notes "Pause on the Q3 bar before the build. The mix shift is the point: Pro went from a fifth to a third."

section end "Close"
  beat thanks "Thank you." states:[close] duration:5s

state intro layout:title hold:4s
  bg shader:mesh seed:7 palette:ambient
    params:{points: 5, drift: 0.12, softness: 0.85, grain: 0.035} at:in(canvas) z:-100 alt:""
    semantic:decoration
  title text role:display "Q3 Review" semantic:navigation at:in(title)
  subtitle text role:title "This quarter changed the shape of the business." semantic:claim
    at:in(subtitle)
  choreo title split:words enter:words timing:with
  choreo subtitle enter:rise delay:240ms
  notes "Open on the title. Don't talk over the build."

state revenue layout:figure transition:{duration: standard, ease: standard} hold:6s
  -subtitle
  -bg
  title "Revenue doubled" role:headline semantic:claim at:in(header)
  rev chart:bar data:@q3 x:{field: quarter, type: ordinal}
    y:{field: revenue, type: quantitative, format: "$,.1f", domain: [0, null], title: "Revenue ($M)"}
    series:{field: product, type: nominal} color:{field: product, type: nominal, scale: categorical}
    labels:{show: ends} legend:top alt:"Quarterly revenue by product, Q4 2025 through Q3 2026."
    semantic:evidence at:in(main)
  note text role:caption "Revenue in $M. Enterprise recognized on delivery." semantic:source
    at:in(note)
  choreo rev enter:{preset: grow, stagger: 40ms, spring: snappy} timing:after
  choreo note enter:fade delay:600ms

state mix slide:revenue transition:slow hold:6s
  title "…and the mix shifted"
  rev kind:stackedBar legend:right
  notes "Same bars, stacked. Pro is the orange band growing quarter over quarter."

state close layout:title hold:3s
  -rev
  -note
  bg
  title "Thank you" role:display semantic:navigation at:in(title)
```

`bg`, `title`, `subtitle`, `rev`, and `note` are declared on the line that first shows them. In `revenue`, `title` is a change: the same id, so the text morphs. In `mix`, `rev` changes kind, and its marks are matched by key.

### 4.2 Grammar

```
file        := (declaration | comment | blank)*
declaration := deck | font | data | node | override | section | state      -- at column 0
block(x)    := the lines after a line that are indented deeper than it, each an x

deck        := "deck" string? prop* block(prop-line)          -- once; `canvas` is required
font        := "font" (word | string) string? prop* block(prop-line)
data        := "data" id string? prop* block(prop-line)
node        := "node" id type props block(props)
override    := "override" id prop* block(prop-line)
section     := "section" id string? prop* block(beat | prop-line)
beat        := "beat" id string? prop* block(notes | media | prop-line)
media       := "media" prop* block(prop-line)
state       := "state" id prop* block(state-line)
state-line  := "-" id                                         -- the node exits
             | id type? props block(props)                    -- a node line
             | "choreo" (id | list) prop* block(prop-line)    -- one choreography item
             | "choreo" "=" value | "choreo" map              -- an item as its value
             | ("sequence" | "parallel") prop* block(group-line)
             | notes
             | prop-line                                      -- the state's own keys
group-line  := "choreo" … | ("sequence" | "parallel") … | prop-line
notes       := "notes" string                                 -- `"""` for several lines
type        := "text" | "shape" | "image" | "stack" | "grid" | "frame" | "group"
             | ("chart" | "shader") (":" word)?               -- `chart:bar` is type and kind
props       := (prop | string)*                               -- the string: see below
prop-line   := prop+
prop        := key ":" value
key         := word | string
id          := word | string
value       := string | word | number | time | percent | ratio | "@" word
             | "true" | "false" | "null" | list | map
             | call+                                          -- `at:` only
             | number "x" number                              -- `canvas:` only
call        := word "(" arg ("," arg)* ")"                    -- arg: a value, or a range `1-7`
list        := "[" (value ("," value)*)? "]"
map         := "{" (key ":" value ("," key ":" value)*)? "}"
time        := number ("ms" | "s")
percent     := number "%"
ratio       := number ":" number
word        := [A-Za-z_][A-Za-z0-9_.-]*
string      := a JSON string on one line | `"""`, a newline, lines, then `"""` alone on a line
```

Rules:
- **The header.** `deck "Title"` sets `meta.title`. Of its keys, `scaena`, `theme`, `canvas`, `formats`, `spine`, and `meta` are the deck's; any other key is `meta`'s (`author:Jay` is `meta.author`). `canvas:1920x1080` is `{width, height}`. `scaena` defaults to the format version this build writes.
- **Fonts and data.** `font Family "file" …` is `{family, file, …}`. `data id "path" …` is a data source whose `source` is the path; `data id inline:[…]` gives the rows inline.
- **Declaring a node.** `node id type …` declares a node and its own props, wherever it stands. In a state, the first line that gives a node's type declares the node there: its props are the node's own (in `nodes`), and the state shows it as it is (JSON: `"bg": {}`). That is the usual way to write a deck: a node is introduced where it first appears.
- **A node line in a state** without a type, or with the type the node already has, is the node's delta: what changes in this state (§2.2). A kind it names (`rev chart:line`) is part of the change; a different type is an error (**E104**). A node line with no props (`bg`) shows the node as it is (`"bg": {}`). A line for an id that is no node keeps its delta, and `validate` reports it (**E102**).
- **A bare string** on a node line is a text node's `text` or an image's `src`, wherever it stands among the props. On any other node it is an error.
- `-id` exits a node. Nodes not mentioned track forward (§2.2), unless the state is `mode:absolute`. A state's `props`, `remove`, and `choreography` are written as these lines, never as keys.
- **Choreography.** `choreo` lines compile to the state's `choreography` in order. `choreo [a, b]` targets several nodes. `sequence` and `parallel` blocks nest. `choreo = value` writes an item as its JSON value, for an item the other forms cannot say.
- **`at:` calls.** `col(1-7) row(2) in(title) rect(120, 80, 900, 420)` is `{col: [1, 7], row: 2, in: "title", rect: [120, 80, 900, 420]}`. One argument is the value, several make a list, and a range `a-b` is `[a, b]`. Reserved slots: `in(canvas)` is full-bleed (the whole canvas), and `in(grid)` is the margin box; both exist in every layout template.
- **Units.** `ms` and `s` are times. A time is milliseconds in the deck (`1.5s` is `1500`), except a beat's `duration`, which is seconds (`15s` is `15`, `500ms` is `0.5`). `cu` is the canvas unit and the default (`12cu` is `12`). `50%` is the string `"50%"`, and `16:9` is the string `"16:9"`. There are no other units. Durations may also be theme names (`standard`).
- Any key, and any id, may be quoted. A word is a string, except `true`, `false`, and `null`. `@q3` is the string `"@q3"`. A path is a string, so quote it.
- The compiler reports an error with its line, its column, and, when it is about part of the deck, a JSON pointer into the compiled deck.

### 4.3 Canonical form

The decompiler's output is the canonical form. Every shorthand is used only where it compiles back to exactly what the deck holds:
- Order: the header; fonts; data; `node` lines; overrides; sections; states. A blank line separates kinds, and comes before each section and state.
- A node is declared in the state that first shows it when that state shows it unchanged, and when declaring it there keeps `nodes` in its order. Otherwise it gets a `node` line, placed so `nodes` keeps its order (between states if need be).
- Keys keep the deck's order. A text node's `text`, and an image's `src`, are bare strings where they stand. `at` is written as calls when every key is a word.
- An integer time takes a unit: `4s` for whole seconds, else `240ms`. This applies to `hold`, `transition`, and a choreography item's `delay`, `stagger`, and `duration`, and to a beat's `duration` (`15s`). A float stays a plain number, since a unit would read it back as an integer.
- A string is bare when it reads back as itself: a word other than `true`, `false`, and `null`; `@` and a word; a percentage; or a ratio. Otherwise it is JSON-quoted. Notes with newlines are a `"""` block when that holds them exactly.
- Lines fill to 100 columns. Props that do not fit continue on lines two deeper.

---

## 5. Engine architecture

One Rust workspace; crates are listed in §12. The pipeline:

```
parse/compile (DSL→JSON)  →  validate (schema + semantic)  →  resolve theme (cascade)
→  resolve states (tracking → absolute snapshots)  →  load data  →  compile charts to marks
→  layout (taffy) + text layout (parley)  →  resolve timeline (durations, spring settle)
→  sample(t): interpolate snapshot A → B  →  build display list  →  paint
```

Stages are pure and cached:

| Stage | Cache key | Notes |
|---|---|---|
| validate | document hash | errors are lint results |
| resolve theme | (document, theme) | produces `ResolvedStyle` per node per state |
| resolve states | document | tracking is deterministic; branching via `from` |
| layout | (snapshot, viewport) | the expensive one; text shaping lives here |
| timeline | (snapshot A, snapshot B) | computes transition + choreography schedule; spring settle times |
| sample(t) | — | interpolates *resolved geometry* (boxes, glyph runs, marks), not source props; O(nodes) |
| display list | (sample) | serializable (§6) |

Design notes:
- Interpolating resolved geometry (post-layout) is what makes morphs cheap and deterministic; text morph interpolates word boxes and cross-fades glyph runs, chart morph interpolates mark geometry.
- Layout is per-snapshot, not per-frame. Frames only sample. In code (PLAN 0.10), `Engine::transition` lays out a state and the one before it once, and `Transition::frame(t)` samples them. It holds no fonts or layout engine, so it cannot lay out.
- Multiple `formats` re-run layout with a different template set; the spine and nodes are shared. `scaena_engine::project` puts the deck on the format's canvas and the theme on its template set before anything is laid out (PLAN 1.13), so layout itself knows nothing of formats.
- The engine exposes a **C ABI** (`scaena-ffi`, via `cbindgen`) and a **WASM API** (`scaena-wasm`, via `wasm-bindgen`) with the same surface: load bundle, list states, resolve timeline, `frame(state, t, format) -> DisplayList`, `lint`, `patch`.

---

## 6. Display list

A serializable, painter-agnostic description of a frame. Versioned (`"dl": 1`). One type (`scaena_core::displaylist::DisplayList`), two encodings: JSON for tests and tooling, postcard for runtime. Coordinates are canvas units, `f32`.

```jsonc
{ "dl": 1, "viewport": [1920, 1080],
  "fonts": [ { "id": "fonts/RobotoSerif-VF.ttf", "index": 0 } ],     // bundle font ids, first-use order
  "ops": [
    { "fill":   { "path": "M0 0L1920 0L1920 1080L0 1080Z", "rule": "nonzero", "paint": { "solid": "#101014FF" } } },
    { "shader": { "kind": "mesh", "seed": 7, "t": 0.25, "rect": [0, 0, 1920, 1080],
                  "palette": ["#1B1430FF", "#3A1F4FFF", "#B0452CFF", "#FF6A3DFF"], "params": { "drift": 0.12, "points": 5 } } },
    { "layer":  { "node": "title", "cell": null, "transform": [1, 0, 0, 1, 0, 0], "opacity": 1, "blend": "normal", "clip": null, "ops": [
      { "glyphs": { "font": 0, "size": 128, "coords": [0, 8192, -4096], "paint": { "solid": "#F2F0E9FF" },
                    "text": "Hi", "glyphs": [[38, 96, 300], [72, 131.5, 300]], "clusters": [0, 1] } }
    ] } },
    { "image":  { "asset": "sha256:...", "src": [0, 0, 640, 480], "dst": [96, 96, 640, 480], "quality": "high" } },
    { "stroke": { "path": "M0 0L10 0", "paint": { "linear": { "start": [0, 0], "end": [10, 0], "stops": [[0, "#FF6A3DFF"], [1, "#FF6A3D00"]] } },
                  "width": 2, "cap": "round", "join": "round", "miterLimit": 4, "dash": [], "dashOffset": 0 } }
] }
```

Rules:
- Ops are stateless and in paint order: later ops draw over earlier ones. A `layer` scopes `transform`, `clip`, `opacity`, and `blend` for its children and names the scene `node` it draws; painters isolate it only when they must (opacity below 1, a blend other than normal, or a clip). Layers nest: a group's layer holds its members' (§3.4). Nodes are drawn in ascending `z`, ties in scene-graph order (`paint_order`), so op order is a pure function of the document.
- Coordinates are in canvas units; `viewport` is the canvas extent, and painters map it to their output pixels. Glyph positions are in their layer's coordinate space (a text node's layer translates to its box), so moving a node changes one transform, not every glyph. A node's `transform` (§3.3) is its layer's too: the layer maps the node as laid out to where it is drawn, through what holds it, and a cue's looks move it inside that.
- Fonts are referenced by index into `fonts` (bundle font ids, in first-use order); painters receive the subset bytes once. A variable instance is its normalized coordinates (F2Dot14, in the font's `fvar` axis order), the exact values `vello` and `vello_cpu` take.
- Glyph positions are final (post-shaping, post-kerning); painters never shape text. **This is the parity guarantee.**
- A glyph run says what it sets (PLAN 1.20): `text` is the text of its clusters, cut from the node's laid-out text (after `case`, with the soft hyphens hyphenation inserts), and `clusters` holds each glyph's cluster start in it, in bytes. A cluster runs to the next larger start or to the end of `text`, so a ligature says all its letters and a base and its marks share their cluster. A hyphen drawn at a break says the soft hyphen (U+00AD) it stands for. Painters draw from `glyphs` alone; exports a reader copies, searches, or hears (PDF, SVG) map glyphs to text through these. A run written before glyph runs carried text reads with none.
- A table cell's layer says where the cell stands: `cell` is `[row, column]`, with row 0 the header row, the body's rows from 1, and columns in the order the table shows them. A `link` op (PLAN 2.70) is where a link in a text is followed: `rect`, the words' box on their line in its layer's space, and `target`, `{ "href" }` or `{ "state" }`. It draws nothing; `DisplayList::links` gives each on the canvas through the layers that hold it. A text that is a list (§3.5) draws each paragraph in layers of its own, `cell` `[paragraph, 0]` its marker and `[paragraph, 1]` its words, and a text that is no list draws in one. Every other layer's `cell` is null. Painters draw without it; exports that read a table as a table (a tagged PDF, §3.12) take its rows and columns from it.
- Gradient paints (`linear` from `start` to `end`, `radial` about `center` out to `radius`, `sweep` about `center` from `startAngle` to `endAngle`, radians from the positive x axis, clockwise with y down) blend their stops in Oklab with premultiplied alpha. A linear or radial gradient holds its end colors past its ends; a sweep repeats, so one that starts off the x axis runs all the way round. Both painters build them through peniko from one conversion (`scaena-paint`'s `convert`), which adds sRGB stops between the given ones, close enough that blending them in sRGB stays within 0.01 of the Oklab blend: vello's GPU ramp blends stops in sRGB whatever the gradient asks (ADR-0004 finding 12).
- Shader ops carry parameters, not pixels: `kind`, `seed`, `t` (seconds on the global timeline), `rect` (in its layer's space), the resolved `palette`, and typed `params`, all numbers (a name a kind lists is its place in the list: a gradient's `shape` is 0 linear, 1 radial, 2 conic). A painter runs the kind's CPU reference or its WGSL twin (§3.8) at the centers of the device pixels the rect covers and places the result texel for pixel. Per-pixel noise such as grain is per device pixel, so it depends on the output size, which is a render input.
- In JSON, colors are `#RRGGBBAA` (sRGB, straight alpha) and paths are absolute SVG path data (`M L Q C Z`); in postcard they are four bytes and an element list. Every number is finite: the encoders refuse NaN and infinities rather than writing `null`.
- The display list is the unit of golden testing (§14). Goldens are written with `to_golden_json` (one op per line, one glyph per line, so a diff reads as "this op changed" or "this glyph moved") after `quantize` (§13).

**Painters:** `vello` (GPU via wgpu — WebGPU, Metal, Vulkan, DX12), `vello_cpu` (headless, CI, agents, export), `pdf` (krilla: vector paths, real text with embedded subsets, shaders as images, tagged as §3.12 says), `svg` (static frames), `png` (via vello_cpu), `video` (PNG/raw frame sequence piped to ffmpeg; frame `n` at `t = n / fps` over the global timeline).

---

## 7. Agent surface

### 7.1 CLI (`scaena`)

```
scaena new       <dir> [--theme dusk|daybreak|ember|theme.json] [--title T]   # a bundle from a theme and its fonts, as deck_create makes one
scaena compile   <deck.scn> [-o deck.json]            # DSL → JSON, validated
scaena decompile <bundle> [-o deck.scn]               # JSON → DSL (canonical form)
scaena validate  <bundle>                             # schema + semantic validation
scaena lint      <bundle> [--state ID] [--json] [--fix] [--severity error|warning|info]
scaena inspect   <bundle> [--state ID] [--resolved] [--timeline] [--data] [--boxes] [--at X,Y] [--targets NODE [--snap HOW --to X,Y,W,H [--fork]]] [--arrange A,B,… (--align EDGE | --spread WAY | --order HOW | --before NODE | --after NODE | --into NODE | --by DX,DY [--free]) [--fork]] [--choices NODE] [--look NODE [--onto A,B,…]] [--inserts] [--layers] [--format F]   # snapshot, styles, cue, rows, where nodes stand and may go, several arranged at once, what a node's look may be, a look copied and pasted, what may be inserted, the layers
scaena render    <bundle> --state ID [--t MS] [--format 9:16] [--size WxH] [--out frame.png] [--display-list out.json] [--painter cpu|gpu]
scaena export    <bundle> --format pdf|png|svg|mp4|webm|prores|html|spine [--states a,b] [--size WxH] [--fps 60] [--audio FILE] [--painter cpu|gpu] [--out DIR|FILE]
scaena patch     <bundle> --ops ops.json|- [--dry-run]  # JSON Patch (RFC 6902) + semantic ops (§7.3)
scaena find      <bundle> TEXT [--case] [--words] [--replace WITH [--dry-run]]   # the texts, descriptions, notes, and claims that hold TEXT, where each is written; every match replaced in one patch
scaena data      <bundle> SOURCE [--edits edits.json|- [--dry-run]]   # a data source as a table, or edited in place (§3.10)
scaena files     <bundle> [--remove PATHS [--dry-run]]   # the bundle's images, fonts, and data, and what uses each; those nothing names taken out
scaena history   <bundle> [--at VERSION [--scn] | --diff FROM[,TO] | --restore VERSION [--dry-run]]   # the versions its history keeps; one as it was, two compared, one made the deck again
scaena diff      <bundle> --from ID --to ID           # what changes between two states (resolved)
scaena save      <bundle> [--to DIR|FILE.scaena] [--keep-fonts] [--history]   # write the bundle as §3.1 lays it out
scaena theme     <bundle> --apply theme.json [--dry-run] [--force]   # re-theme; prints lint delta
scaena theme     <bundle> --edit ops.json|- [--dry-run]   # edit the theme the deck names (RFC 6902 on its JSON); prints lint delta
scaena serve     <bundle> [--port N]                  # the player and editor on a bundle's folder, on this machine; deck.scn compiled as saved
scaena mcp                                            # stdio MCP server exposing the same operations
```

Exit codes: `0` ok; `1` findings that are errors (`validate`, `lint`, `compile`, `theme`, `patch`, `find --replace`, `data --edits`, `history --restore`), or a file `files --remove` keeps; `2` invalid input; `3` not built yet, naming the PLAN task that builds it.

A command that writes a bundle that keeps history (`patch`, `find --replace`, `data --edits`, `theme --apply`, `lint --fix`, `history --restore`, `save`; §8) records its change there, by `$SCAENA_AUTHOR` (`user` without it), saying what it did.

**`scaena find`** (PLAN 2.47, 2.83) finds TEXT in the deck's words, in every state: its texts, each node's description (`alt`), each state's `notes`, and each beat's `claim` and `notes`. A text node's text is written in one place for each state that shows it: the deck's `overrides`, which hold it in every state; else the latest delta that sets it, from the state back along what it tracks; else the node's own (§2.2). A node's `alt` is written as a text is. Each is found once for each place it is written, with what it is (`kind`: `text`, `alt`, `notes`, or `claim`), its node or its beat, the states that show it from there (a beat's, for its claim and notes), its words (a text's runs' texts end to end), and each match, `[from, to]` in characters (Unicode scalar values), none overlapping. `--case` keeps upper and lower case apart; `--words` takes whole words only, a match that neither begins nor ends inside a word. With `--replace`, every match is replaced by WITH in one patch: in a text, a `replace_text` for each (§7.3), made in the first state that shows its text, so it is written where the text lives, the last match in a text first; in other words, a JSON Patch `replace` of them where they are written. The patch is checked, reported, written, and recorded as `patch` does one; `--dry-run` writes nothing.

**`scaena data`** (PLAN 2.55) shows data source SOURCE as a table (§3.10). Each column shows its name and type, each row its index from 0 and its cells as written, and each cell its column does not read is listed with why. With `--edits`, it makes the edits:
- **The edits.** A JSON array, `docs/schema/mcp/data_edit.json`'s `edits`, of `{ "op": "set", "row", "column", "value" }`, `{ "op": "add", "row"?, "values" }`, and `{ "op": "remove", "row" }`.
- **One write.** They are made in order, all or none, in one write of the source's file that keeps every other byte, or in one patch of rows written inline. They are checked, reported, written, and recorded as `patch` does one (`data_edit q3: revenue of row 3`).
- **Refusals.** An edit that does not apply (a row or column the source lacks, or a value its column does not read) stops them, exit 2, with its index (`op`). Edits that would leave the deck invalid are refused, exit 1.
- **`--dry-run`** writes nothing.

**`scaena files`** (PLAN 2.59) lists the bundle's images, fonts, and data: images, then fonts, then data, each by its path, with its size. Not the deck, its theme, its manifest, its history, or a license beside the fonts.
- **What a file is.** A data file is one a data source names, or a CSV, TSV, or JSON file under `data/`. A font is a `.ttf`, `.otf`, `.woff`, or `.woff2`. An image is a PNG, JPEG, GIF, or WebP, or a file an image node names.
- **What names it.** An image node whose `src` it is, in its own props, in a state's, or in the `overrides`; a beat whose `evidence` cites it; the deck's `fonts`, with the face's `style`; one of the theme's families, by its file or its italic file; a data source.
- **What uses it.** Each node drawn from it, with the states that show it so, in the deck's order: an image that shows it; a chart or a table that reads its source; a text set in its face, through the theme's role, the node's `style`, its runs, a chart's or a table's roles, and the family's fallbacks. A face nothing sets in is named all the same.
- **`--remove`** takes the paths out, all or none. Each must be one of the bundle's images, fonts, or data that nothing names: anything else is refused, with why, and nothing is taken out, exit 1. The deck draws and reads the same after. A saved bundle's manifest drops them too. The deck does not change, so a bundle that keeps a history records nothing; what it holds of a data file stays as that file's past (§8). `--dry-run` writes nothing.

**`scaena history`** (PLAN 2.60) lists the versions a bundle's history keeps (§8.2), the oldest first: each its number, its id, its author, when it was made (RFC 3339, UTC), what it did, and how many operations it holds. A version is named by its number as listed (`3` or `#3`) or by its id. One thing at a time:
- **`--at`** prints the deck as it was in that version, canonical, or with `--scn` as `.scn` (§4), with the data files its sources named that the history held then.
- **`--diff FROM[,TO]`** says what changed from one version to another, or, with one, to the deck and its data files as they are now: each state added, removed, or changed, in the later version's order. A changed state gives its own fields that changed (its layout, transition, choreography, hold, notes, …) and its nodes as `diff` says them, resolved with the deck's overrides. Then the deck's own fields that changed (its theme, canvas, formats, fonts, data sources, spine, …), with `order` where the states both have run in another order, and the data files whose bytes changed. Each value is as the later version has it, null where it is gone.
- **`--restore`** makes the version the deck again, with the data files its sources named as the history held them, as one change (`history --restore <id>`), checked and reported as `patch` does one: the lint delta, and the states it changes what shows in. A deck that would not validate in the bundle as it is now is refused, exit 1: a file it names taken out since is E102. `--dry-run` writes nothing.

A bundle that keeps no history exits 2, and says that `scaena save --history` begins one.

`--json` goes anywhere on the line. With it, stdout holds exactly one JSON value: the command's result, or, when the command stops with exit 2 or 3, `{ "error": { "exit", "message", "plan"? } }`. A usage error is one too. `compile` adds the `line` and `col` of source that does not compile, and the JSON pointer into the deck (`path`) when the error is about part of it. So an agent parses stdout, then reads the exit code. stderr is for people and is not part of the contract. The results:

| Command | `--json` |
|---|---|
| `validate`, `lint` | an array of findings (§7.4); `lint --fix` prints `{ fixed, findings }` |
| `inspect` | an array, one state each: `state_id`, `slide_id`, `layout`, `nodes`, `entered`, `exited`. `--resolved` adds `looks` and `overrides`, `--timeline` adds `timeline`, `--data` adds `data`, `--boxes` adds `boxes`, `--at` adds `hits`, `--targets` adds `targets`, `--snap` adds `snapped`, `--arrange` adds `arranged`, `--choices` adds `choices`, `--look` adds `look` and `--onto` adds `put`, `--inserts` adds `inserts`, and `--layers` adds `layers` |
| `diff` | an object by node id: `{ "enter": props }`, `{ "exit": true }`, or `{ "change": { prop: value } }` |
| `compile` | `{ out, findings, deck? }`: the deck when it is written to stdout. Findings exit 1 with `out: null` |
| `decompile` | `{ out, scn? }` |
| `render` | `{ state, format, t_ms, span_ms, painter, adapter, size, out, display_list, ms }`: `ms` holds the stage timings |
| `save` | `{ renamed, subset, manifest }` |
| `export` | `{ format, out }`. The spine adds the projection (`spine`, §10) when it is not written, and when it is, its renders (`files`), the thumbnails' `size`, and `bytes`. A PDF adds `pages`, the state each page draws, and `bytes`; png and svg add `pages`, `files` (in the order of `pages`), `size`, and `bytes`; a video adds `size`, `frames`, `fps`, `duration_ms`, `timeline` (each state's `start` in the video, `span`, and `hold`), `chapters` (each beat's `beat`, `title`, `start`, and `end` in the video), and `bytes` |
| `theme` | `{ theme, was, applied, mapped, added, removed, errors }`; `--edit`, `{ theme, paths, applied, refused, listed, added, removed, errors }`: the theme edited (its path, or `(inline)`), where each operation wrote, and the lint delta. An operation that does not apply adds `op`, its index, to the error object |
| `patch` | `{ applied, patch, added, removed, errors, states }`: the patch as RFC 6902, the lint delta, and the states it changes what shows in. An op that does not apply adds `op`, its index, to the error object |
| `find` | `{ found, matches, replaced? }`: each of the words found, `{ kind, node?, beat?, state, states, lives, text, matches }`, `lives` the JSON pointer to where they are written; the matches in all of them; and with `--replace` and a match, the patch's result, as `patch` prints it |
| `data` | `{ edited, source, file?, sheet, added, removed, errors, refused }`: the file the source is (none for rows written inline); the sheet, `{ columns, rows, problems? }`, each column `{ name, type }`, each row's cells as written, and each cell its column does not read, `{ row, column, why }`, as the edits leave it, or as it is with none or when they are refused; and the lint delta. An edit that does not apply adds `op`, its index, to the error object |
| `history` | `{ versions }`, each `{ n, id, author?, message?, at?, ops }`; `--at`, `{ seen }`: `{ version, deck?, scn?, files }`; `--diff`, `{ compared }`: `{ from, to?, states, deck, files }`, each state `{ "added": true }`, `{ "removed": true }`, or `{ "changed": { fields?, nodes? } }`; `--restore`, `{ restored }`: `{ version, applied, files, added, removed, errors, states }` |
| `files` | `{ files }`: each `{ path, type, bytes, named, used }`, `type` one of `image`, `font`, and `data`; `named` what names it, each `{ by: "node", node }`, `{ by: "evidence", beat }`, `{ by: "font", family, style? }`, `{ by: "theme", family }`, or `{ by: "source", source }`; `used` each node drawn from it, `{ node, states }`. With `--remove`, `{ removed, refused, applied }`: the paths taken out, all asked or none; each kept, `{ path, why }`; and whether the bundle was written |

`inspect --timeline` shows each state's cue (§2.4, §3.9) in ms, worked out as `render` does, so it reads the bundle's fonts:
- Where the state falls on the deck's timeline: `start`, `span` (its transition and motions), and `hold`.
- Its `transition`: `duration`, `curve`, and `match`.
- Each of its `motions` as placed on the state's clock: `node`, `motion` (`enter`, `exit`, `emphasis`, `anim`), `split` and `units`, `start`, `stagger`, `duration`, `end`, and `curve`.
- Where each is written, and what `time_motion` reads there (PLAN 2.44): `written`, a JSON pointer to the choreography item or to the node's own property where it lives; `delay` as written; and `moving`, when its first unit starts to change and its last comes to rest (an `anim` from its first key).
- The motion's look, as what it changes from rest: `from` (enter), `to` (exit), `peak` (emphasis), or `tracks` (anim).

A curve is `{ "ease": [x1, y1, x2, y2] }` or `{ "spring": { stiffness, damping, mass } }`.

`inspect --data` shows the rows each chart and table reads, after its `dataTransform`. Each is `{ source, columns, types, rows }`, and dates are ISO 8601.

`diff` without `--json` prints a line per node: `+ id` enters, `- id` exits, and `~ id: keys` changes those keys.

`export` writes where `--out` says: a file for pdf, video, html, and the spine (which prints without it), and a directory for png and svg, which get an image of each state at rest named for it (`<state>.png`, `<state>.svg`), over one of that name. A written spine draws each beat into `renders/` beside it (§10). `--states a,b` picks the states a frame export draws, in that order (png, svg, pdf, video, html). `spine` is the whole spine, and takes no `--states`. Without `--states`, png, svg, and html take every state; a PDF draws each slide once, at its last state, in spine order (§3.11): the slides of the states the beats name, then the slides no beat names, in deck order; and a video plays the whole timeline. An html file plays the states `--states` names, in that order, each with its own cue. A `scaena` built before the web player it carries (`just web`, §10) cannot export html, and exits 3 saying so. `--size` sets the pixels of png, svg, video, and the spine's thumbnails (480 wide without it), in the canvas's aspect ratio, as `render`'s does. `--fps` (default 60), `--audio`, and `--painter` are a video's: `--painter gpu` paints its frames with vello on the GPU, in a `scaena` built with `--features gpu` (one built without it exits 3), and any other format refuses it. A video needs `ffmpeg` on the PATH, and a width and height that are even (§10).

`validate` reads the bundle as it is on disk (PLAN 1.2). It checks the deck against `docs/schema/deck.schema.json` and the theme against `theme.schema.json`. It also checks what a schema cannot say: references (E102), what charts read from their data (E103), types (E104), ids (E105), and each state, resolved, against its nodes' types (E106). Findings are errors, so any finding exits 1. Input that is not a bundle, or a `deck.json` that is not JSON, exits 2. A schema violation comes first: a deck that does not parse gets no semantic findings until it does.

`theme --apply` points the deck at another theme and copies it into the bundle (to `themes/`, unless it is already inside), into a directory or a zip. A family whose file the bundle does not hold is set in the bundle font of that family, if it has one: a saved bundle names fonts by their content. The deck's `fonts` lists each of the theme's families the bundle holds, as rendering needs it to (`listed`). The deck is not otherwise touched, and it is written canonically. It reports the delta in what `validate` and `lint` find, before and after: what the new theme breaks, and what it fixes. Errors after the swap exit 1. `--dry-run` reports without writing.

A theme that would leave the deck invalid is refused, as `patch` refuses a patch that makes the deck invalid, and exits 1 (PLAN 1.35). Most often it lacks a name the deck uses (E102), and an invalid deck is not laid out, so lint could not say what else the theme breaks. The deck keeps its theme, and the new one is copied in all the same. Then one `patch` with the `retheme` op and the fixes swaps it with the deck valid throughout. `--force` applies the theme anyway.

`theme --edit` edits the theme the deck names (PLAN 2.61, ADR-0016, §3.6): RFC 6902 operations on its JSON, read from a file or stdin, applied in order, all or none. The theme file is written in canonical form, as `save` writes it; an inline theme is written into `deck.json`. A theme that ships is edited in the bundle's copy. The deck is checked in the theme as edited, as `--apply` checks it: an edit after which it would not validate is refused, exits 1, and writes nothing; one that would add no validation error is written, recorded in the history with the theme's bytes, and reports the lint delta. An operation that does not apply exits 2 with its index. `--dry-run` reports without writing.

`inspect --resolved` runs each state through the theme cascade (§3.6): the deck's overrides merged in, each text node's look (role, family, size, leading, weight, tracking, color), and what each node's overrides set.

`inspect --boxes` and `--at` say what stands where in each state at rest (ADR-0013), from the layout its frames at rest draw, so they read the bundle's fonts. They answer what a pointer needs to select a node and move it:
- **`boxes`:** each visible node by id, with its `rect` (`[x, y, width, height]`, canvas units) and the container or group it sits in (`parent`). The rect is its grid cell or slot, the box its container gave it, or a group's box around its members, as laid out. Where its `transform` or that of what holds it moves it (§3.3), `transform` is the map that draws the rect where it is drawn, `[a, b, c, d, e, f]` (`x' = a·x + c·y + e`, `y' = b·x + d·y + f`). Those that draw come first, in paint order; then the containers and groups that only hold others (`draws: false`).
- **`hits`:** the nodes that draw at `--at X,Y`, topmost first. Each has its `rect`, its `transform` as a box's, and the containers and groups it sits in, innermost first (`containers`). A point hits a node inside its box, read back through its transform, or within 6 canvas units of a box too thin to point at, as a hairline rule is. A node faded out entirely, or flattened by its transform, is not there to point at. A text says where a caret put at the point stands (`offset`): how many characters of its text come before it, as `replace_text` counts them (§7.3).

A caret stands between characters as a reader counts them: grapheme clusters, so a letter with its accents, an emoji with its modifiers, and a flag are each one. The engine reads where each stands from the glyphs it set, in the text as written: case that sets ß as SS, and soft hyphens hyphenation inserts, map back to it. In right-to-left text a caret before a character stands at its right edge (`scaena_engine::carets`).

`inspect --state S --targets NODE` says where the node may go in that state (ADR-0013), as a drag snaps it. What holds it says how it is placed (`by`):
- **`grid`**, the theme's, holds a root or a group's member: by cells, a slot of the state's layout template, or a `rect` on the canvas, an override (W301). `columns` and `rows` are the grid's tracks (`[start, end]` each), and `slots` the template's, then `canvas` and `grid`, each with its box.
- **`cells`**, a grid container (`parent`), holds its child by its cells or an area: `columns` and `rows` are its tracks as laid out, and `slots` its areas.
- **`stack`** holds its child by order: `flow` is its children in order.
- **`frame`** holds its child by a `rect` from its padding edge, `within`.

`cell` is the box the node's placement names now, before its inset, offset, alignment, and size: what a drag moves. `snaps` lists how a dropped box snaps here. With `--snap HOW --to X,Y,W,H`, the cell as a drag left it lands, and `snapped` says where (`cell`) and gives the patch that puts the node there (`patch`): `place` ops (§7.3), made in the state inspected, for `patch` or `deck_patch` to apply. With `--fork`, the ops keep the placement to that state (`place`'s `fork`).
- **`move`** keeps the cells it spans, from the track nearest the box's corner, inside the grid.
- **`resize`** takes each edge to the nearest track's.
- **`slot`** goes into the slot, or area, the box covers most.
- **`free`** places it where it was dropped, in whole canvas units.
- **`order`** puts it among a stack's children where the box's middle falls, and writes the `index` of each child whose place changes.

A way that does not place the node is an error that names those that do.

`inspect --state S --arrange A,B,…` arranges several nodes at once, children of one container, as the editor's canvas does with several selected (ADR-0013, PLAN 2.42, `scaena_ops::arrange`). `arranged` says where each lands (`landed`: each `node` and its `cell`) and gives one patch that puts them there (`patch`), made in the state inspected, for `patch` or `deck_patch` to apply; with `--fork`, kept to that state. One way at a time:
- **`--by DX,DY`** moves them together: the first as a drag of it alone snaps (with `--free`, off the grid, to whole canvas units, as Shift drags), and the rest as far as it went, each its own way: by cells on a grid, to whole canvas units where a `rect` places it or a frame holds it. Where the grid's edge stops one of them sooner, all of them stop there, so they keep where they stand about each other.
- **`--align EDGE`**, two nodes or more: each box takes the edge all of them reach farthest (`left`, `right`, `top`, `bottom`), or the middle of them all (`center`, `middle`), snapped as a move snaps, so on a grid they line up on its tracks.
- **`--spread WAY`**, three nodes or more, `across` or `down`: the first and the last stay, and the ones between move so the gaps between them all are equal, as near as the tracks allow.
- **`--order HOW`**: a node is painted after its container's other children by `z`, then by where the deck lists it (§3.2). `forward` puts each in front of the next of them it overlaps, `backward` behind the one before; `front` and `back` past all of them, keeping what the nodes named keep among themselves. Each is a `choose` of `z` (§7.3), written where `z` lives: the one `z` that does it where there is one, else the fewest that do.
- **`--before NODE`**, **`--after NODE`** (PLAN 2.50): one node, listed just before or just after NODE, as `--layers` lists them. Elsewhere it is painted just over NODE, or just under it: a `choose` of `z`, as `--order` makes one. In a stack it is laid out just before NODE, or just after it: a `place` of each child whose `at.index` that changes, renumbered from 0. Where another container holds NODE, or none does, the node goes there with it, a `place` with `parent` (§7.3), placed as that one places what it holds:
  - On the theme's grid (the canvas, or a group), by the cells it stands in now: on each axis the tracks its box overlaps by half the shorter of the two or more, else the one nearest its middle.
  - In a frame, by a `rect` where it stands, moved inside the frame's padding as little as it takes.
  - In a stack, at its place in the order.
  - In a grid container, after its flow (an `index` past the others'), in the first cell free.
- **`--into NODE`** (PLAN 2.50): one node, into the container NODE, listed first among what it holds: topmost, or first in a stack's order; placed as `--before` places it. A container goes into nothing it holds, and a node into nothing but a stack, a grid, a frame, or a group: each is an error that says so.

A node that lands where it stands is left out of the patch, so its placement stays as written, a slot's name and all. Nodes of two containers are an error, and so is any way but order in a stack, which places what it holds by its order.

`inspect --state S --choices NODE` says what an inspector offers for the node as that state shows it (ADR-0013, PLAN 2.33): each property it edits (`fields`), the node type's own first, then those every node has.
- **What each takes** (`takes`). Some take one of the theme's names of a kind (`name`, with `of`: a text role, a font family, a color, a motion preset, a shader preset of the node's kind, a shader palette, a stroke, or a step of the space or radius scale), in the theme's order (`Theme::names`). Others take a word, a number in range, or yes or no, as the schema generated from the model allows. An image's focal point and crop take fractions of the image, from 0 to 1 (`fractions`, with `names`: `x` and `y`, or `x`, `y`, `w`, and `h`), which are never overrides (PLAN 2.45). Where a value written out is legal too, a color or a length in canvas units, `overrides` says so: one goes in the deck's `overrides`. A text size is always written out, and always an override.
- **The fields by type:**
  - a text: `role`, `style/family`, `style/weight`, `style/italic`, `style/size`, `style/color`, `style/case`, `fit`, `wrap`, and `maxLines`;
  - a shape: `fill`, `stroke/paint`, `stroke/width`, and `radius`;
  - an image: `fit`, `focal`, `crop`, and `radius`;
  - a shader: `preset` and `palette`;
  - a chart: `data`, `kind`, `x/field`, `x/type`, `y/field`, `y/type`, `series/field`, `color/field`, `sizeEncoding/field`, `key`, `labels/show`, and `labels/role`;
  - a table: `data`, `key`, and `header`;
  - a stack: `axis` and `gap`; a grid container, `gap`;
  - every node: `opacity`, `transform/rotate` (how far it is turned, in degrees, clockwise: PLAN 2.51), `enter`, `exit`, and what a reader hears of it (PLAN 2.56, §3.12): its description (`alt`, words for people) and its part in the story (`semantic`, one of its words).
- **What a chart or a table reads** (PLAN 2.41). Its `data` takes the deck's sources (`@name`). Each channel's `field`, and its `key`, take the columns of the table it reads in that state, through its `dataTransform`, that the channel can read:
  - numbers for a quantitative channel, and for a `y` or a size that declares no type;
  - dates for a temporal one;
  - any column for the rest.

  Where its data cannot be read (a file the bundle does not hold), no column is offered, and its `data` still is.
- **The value shown** (`value`) is the deck's: tracked (§2.2), then the overrides; absent where the theme's shows.
- **Where it lives** (`lives`): `overrides`, a state's delta (`{ "state": id }`, the one shown or one it tracks from), or the `node`. That is where `choose` writes (§7.3).
- `literal` marks a value written out where the theme has names: an override in the overrides, W300's anywhere else.

A node the state does not show is an error that says so.

`inspect --state S --look NODE` says the node's look as that state shows it (PLAN 2.58, `scaena-core::looks`): what ⌥⌘C copies in the editor (§9.2). It is `{ node, type, props }`, each of `props` one property of the look of the node's type, `{ prop, value? }`, with the value the state shows as `--choices` says it, absent where the theme's shows. A look is how a node is drawn, not what it is or holds, where it goes, how it moves, or what a reader hears:
- a text's `role`, `style/family`, `style/weight`, `style/italic`, `style/size`, `style/color`, and `style/case`, its role first;
- a shape's `fill`, `stroke/paint`, `stroke/width`, and `radius`; an image's `radius`;
- a shader's `preset` and `palette`; a chart's `labels/show` and `labels/role`;
- a stack's or a grid container's `gap`. A table, a frame, and a group have none.

With `--onto A,B,…`, `put` says what pasting that look on those nodes in that state makes, as ⌥⌘V does. Its `patch` has one `choose` (§7.3) for each property a node takes whose value it does not show already, made in the state inspected, for `patch` or `deck_patch` to apply:
- **Where.** Each is written where that node's own value lives, as an inspector's choice is. Where the look's value is the theme's, `null` takes the node's own away there, so the theme's shows on it too. A value written out goes in the deck's overrides, as the look's own does.
- **Across types.** A node of another type takes what its look shares with the look: an image takes a shape's corners.
- **What it says.** `took` lists the nodes the patch changes, `same` those that look so already, and `refused` those that take none of it, or are where the look was copied from, each `{ node, why }`.

A node the state does not show is an error that says so.

`inspect --state S --state-choices` says what an inspector offers for the state itself (PLAN 2.36), as `--choices` says a node's, which `set_state` writes (§7.3): its `layout`, each key of its transition (`transition/duration`, `transition/ease`, `transition/spring`, `transition/match`), its `hold`, and its `notes`.
- **Its layout** shows the layout the state takes, which tracks as a property does (§2.2), and lives in the state that sets it: the one inspected, or one it tracks from. It takes the theme's layouts that have a slot for each node placed in one, in each state that takes its layout from there, since a slot the layout lacks is E102. A state that takes none shows none.
- **Its transition, hold, and notes** are its own. A transition that is a bare duration shows as its `duration`; a state that sets no key of it cuts in.
- Each takes what the theme names or the schema allows: a duration, an easing, or a spring of the theme's; `id` or `none`; a number of milliseconds from 0; or words (`text`).

`inspect --state S --layers` says the state's layers (PLAN 2.50, `scaena-core::layers`): its nodes nested as their containers and groups hold them, in the order a layers panel lists them. Each is `{ node, type, shown, children? }`.
- **Order.** Each list starts with the topmost: siblings paint by `z`, then the deck's order of nodes, and a container under what it holds (§3.4). A stack's children are listed in the order it lays them out (`at.index`, then the deck's order), its first first: they do not overlap, so that is the order the canvas shows.
- **What it does not show** stands beside what it does, `shown: false`: the nodes that leave in the state, and those another state of its slide shows, each where the nearest state that shows it places it; and those no state shows, where their own `at` places them. `show_node` shows one there (§7.3).
- A node whose container is not listed stands on the canvas.

`inspect --state S --inserts` says what an editor may insert (PLAN 2.34): the nodes the theme and the bundle name, each as `add_node` adds it (§7.3), unplaced (`node`), with what its id starts from (`id`), what a menu calls it (`label`), and the box it takes at first (`start`).
- **A text** in each of the theme's roles, its text the role's name: half the canvas wide and two of the role's lines tall (`{ "box": { "w", "h" } }`, each a share of the canvas's side).
- **A shape** of each kind that needs no points: a rectangle and an ellipse, filled with the theme's `accent` (or its first color), a quarter of the canvas wide and square; a line and an arrow, drawn in the theme's rule.
- **An image** of each PNG and JPEG in the bundle, a third of the canvas wide and square.
- **A chart and a table** of each of the deck's data sources (PLAN 2.41), half the canvas each way, from the columns it has:
  - The chart's `y` is the first column of numbers. Its `x` is the first column of dates, a line over time (`temporal`), or else the first of text, bars. Where `x` alone repeats among the rows, its `series` is the next column of text that tells them apart.
  - The table shows every column, its rows keyed by the first column that tells them apart, and its first 8 rows where it has more (`dataTransform`), which its box holds in any theme.
  - A source offers no chart without a column of numbers and one of dates or text to set them along, or where those and a series cannot tell its rows apart; and no table where no column tells its rows apart.
- **A shader** of each of the theme's presets, filling the `canvas` slot (`{ "slot": "canvas" }`) under what is there (`z` -1).

An editor puts a box about the pointer, snapped to the theme's grid as a dropped box snaps (`move`), and the node enters in the state shown: `add_node` with its `state`, then `place`. A text or an image fills instead the template's slot under the pointer where no node the state shows is placed in it (`slot`), so it moves with the deck's formats. Content (a text, an image, a chart, a table) whose box would overlap what lint E101 judges it against there, the content that stacks at `z` 0 at the root less a decoration, goes instead to the place on the grid clear of it whose middle is nearest the pointer, and stays about the pointer where none is (PLAN 2.79). Its id is the first of `id`, `id-2`, … that names no node, `id` the dropped file's name for a file dropped on the canvas (`Trail Head.jpg` is `trail-head`). A copy of a node is made the same way, of the props the state shows it with, with a copy of each node it holds there, held by the copy. It goes one span to the right of the node, below it, to its left, or above it: the first of those clear of the node and of what else draws there in front of the background, or else the first clear of the node. It is placed so in each format the node lays out in: the one shown, the deck's own canvas, and each where the node has a layout of its own (§3.4, ADR-0020), so it stands on the node in none (PLAN 2.86). A pasted node keeps a layout only for a format the deck lays out anew, and goes where it is pasted in each: its placements in its formats go, and what else it lays out by there stays. Deleting a node in a state is `hide_node` for it and for each node it holds there, and `remove_node` for one that no state shows after that, so a node inserted and deleted leaves nothing behind; deleting it from the deck is `remove_node` for each, what it holds first.

`--format` inspects the deck in one of its formats, laid out again with its template set (§3.4), for every view.

`compile` checks the deck it compiles as `validate` does, in the bundle it is written to: `-o`'s directory, or the source's when it writes to stdout. It shows each finding at the source that wrote that part of the deck, and the JSON pointer where it lands. It writes only a valid deck: findings exit 1 and source that does not parse exits 2, and either way nothing is written. `decompile` reads any deck, valid or not (§4).

`new` (PLAN 2.13) makes a bundle in a directory not there yet or empty, as `deck_create` makes one (§7.2): the theme, the fonts it names, and one state with nothing on it, titled `--title` (`Untitled` by default), on a 1920 × 1080 canvas. The theme is one that ships, by its name, in any case (`dusk` by default), or a theme file, whose fonts are beside it or above it. The CLI and the MCP server carry the themes that ship and their fonts, so a deck starts where no file of the repository's is at hand. The fonts cover Latin (U+0020–017F, the punctuation of U+2010–203A, and €), and each carries its copyright and license (OFL) in its name table. A directory that is not empty, or a theme that is neither a file nor one that ships, exits 2; a bundle that would not validate is not made, and exits 1.

`serve` (PLAN 2.11, ADR-0012) serves the web player at `/` and the editor at `/edit` on a bundle's folder (a directory with `deck.json` in it), at `http://localhost:4848/` or the port `--port` names (0 for any free one). It answers on 127.0.0.1 alone, to a page of its own, and writes only inside the folder (§9.2). A `deck.scn` saved there compiles into `deck.json` as `compile` writes it, and the pages show each change; one that does not compile is shown on stderr as `compile` shows it, and in the pages, and `deck.json` stays as it was. Under `--json`, stdout holds where it serves, once: `{ bundle, player, editor }`. A folder that is no bundle exits 2. A `scaena` built before `just web` carries no pages, and exits 3.

`render` without `--t` renders the state at rest; `--t` is milliseconds into the state's cue (its transition, then its motions, §3.9), and `--json` reports the state's span (`span_ms`). `--size` defaults to the canvas size and must keep the canvas's aspect ratio (to the nearest pixel): painters scale uniformly, never stretch. A raster holds at most 2²⁵ pixels (an 8K frame, 7680 × 4320, and a little more), and a larger `--size` is an error: a painter keeps a raster in memory, and an image of each shader beside it, and in the browser an allocation that fails stops the worker. The GPU painter also refuses a frame its device cannot hold: a target wider than its textures, or a shader whose pixels outgrow a storage buffer (128 MiB on many GPUs, a little more than an 8K frame). `--out` defaults to `<state>.png`. The display list it writes is unquantized (§13.4: only comparisons round).

### 7.2 MCP tools

`scaena mcp` serves the operations as MCP tools over stdio (`scaena-mcp`, on `rmcp`). The CLI and the server call the same functions (`scaena-ops`, ADR-0009), so a tool does what its command does.

| Tool | Does | Result |
|---|---|---|
| `deck_create` | Makes a bundle at `bundle`, a directory that is not there yet or is empty. It copies in the `theme`, a theme that ships by its name (`dusk`, `daybreak`, `ember`) with its fonts, or a theme file with the fonts its families name (found beside it or above it), and the `data` files. The deck comes from `deck` (JSON) or `scn` (source); without either it is one empty state titled `title`. Its `theme` and `fonts` are pointed at the copies. Written only if it validates. | `{ created, files, findings, errors }` |
| `deck_read` | The deck, canonical: as JSON, or with `scn`, as `.scn` (§4). With `files`, the bundle's images, fonts, and data too, each with what names it and the nodes drawn from it, as `scaena files` lists them (§7.1). | `{ deck }` or `{ scn }`, and `files` |
| `deck_patch` | `scaena patch` (§7.3). | `{ applied, patch, added, removed, errors, states }` |
| `deck_find` | `scaena find`: `find`, `case`, `words`, `replace`, and `dry_run`. | `{ found, matches, replaced? }` |
| `data_edit` | `scaena data`: `source`, `edits`, and `dry_run` (§3.10). With no `edits`, the source as a sheet, read. | `{ edited, source, file?, sheet, added, removed, errors, refused }` |
| `deck_lint` | `scaena lint`, with `state`, `severity`, and `fix`. | `{ findings, fixed?, errors, laid }` |
| `deck_inspect` | `scaena inspect`, with `state`, `resolved`, `timeline`, `data`, `boxes`, `at` (`[x, y]`), `targets`, `snap`, `to` (`[x, y, w, h]`), `fork`, `arrange` (the nodes) with one of `align`, `spread`, `order`, `before`, `after`, `into`, or `by` (`[dx, dy]`, with `free`), `choices`, `state_choices`, `look` with `onto` (the nodes), `inserts`, `layers`, and `format`. | `{ states }` |
| `deck_render` | `scaena render`: `state`, `t`, `format`, `size`, `painter`, and `out`. | the PNG as image content, and `{ state, size, span_ms, digest, painter, out?, ms }` as text |
| `deck_export` | `scaena export`: `format`, `states`, `out`, `size`, `fps`, `audio`, and `painter` (a video's: `gpu` in a server built with it). All but the spine needs `out`. An export still going after 40 s answers `running` (see **Long exports**). | `{ format, out?, spine?, pages?, files?, size?, frames?, fps?, duration_ms?, timeline?, chapters?, painter?, adapter?, bytes? }`, or `{ format, out, running }` |
| `deck_diff` | `scaena diff`. | `{ changes }` |
| `deck_history` | `scaena history`: with nothing else, the versions; `at` (with `scn`), `compare` (one version or two), or `restore` (with `dry_run`), one at a time. | `{ versions }`, `{ seen }`, `{ compared }`, or `{ restored }` |
| `theme_apply` | `scaena theme --apply`, `force` its `--force`. | `{ theme, was, applied, refused, mapped, listed, added, removed, errors }` |
| `theme_edit` | `scaena theme --edit`: `ops`, RFC 6902 operations on the theme the deck names, and `dry_run` (§3.6). | `{ theme, paths, applied, refused, listed, added, removed, errors }` |
| `data_attach` | Copies a CSV or JSON file into `data/` and declares it as data source `id`. Each column is typed by `schema`, or inferred: as narrowly as all its values allow (`number`, `boolean`, `date` in ISO 8601, else `string`). Written only if the deck validates no worse. | `{ attached, id, source, schema, rows, added, removed, errors }` |
| `spine_read` | The spine projection (§10), without the times and renders `export --format spine` adds: it runs no layout. | the projection |
| `spine_update` | Replaces the spine, as a patch. | as `deck_patch` |

- **Results.** A tool's result is structured content, with the same JSON as text. It is the command's `--json` result, with one difference: structured content is an object, so a command that prints a list or a map has it named here (`findings`, `states`, `changes`).
- **Failures.** A tool that stops returns an error result (`isError`), not a protocol error, so the agent reads why. Its text is `{ "message", "plan"?, "op"? }`: what stopped it, the PLAN task that builds what it needs, and the index of a patch's op that does not apply.
- **Renders.** `deck_render` returns image content so the agent sees what it made. The text carries the display list's digest (FNV-1a over its postcard bytes, as `tests/golden/torture/raw.fnv1a` holds them): one digest, one drawing.
- **Paths** are on the machine the server runs on, relative to its working directory. A bundle is a directory, a `.scaena` zip, or a `deck.json`. The server runs where the agent does and assumes no other (ADR-0006).
- **Protocol.** MCP from 2024-11-05 to 2026-07-28, and the server names itself `scaena` at its version. A client reaches 2026-07-28 by `server/discover`, then names itself and its protocol in every request's `_meta`, as Claude Code does. A client that shakes hands (`initialize`) settles on 2025-11-25 at most.
- **History.** A tool that writes a bundle that keeps history (§8) records the change as `agent:<name>`, by the name the client gives: in the request, or in its handshake.
- **Long exports.** A client gives a tool call about a minute. That is Claude Code's limit and the TypeScript SDK's default, and progress notifications do not extend it. A 1080p60 video of a dozen states takes longer on most machines. So `deck_export` waits 40 s at most, and an export still going then keeps going on the server.
  - The call answers `{ format, out, running: { done, of, unit, elapsed_ms, next } }`: how many frames, pages, images, or beats are done, of how many. It answers once the export has counted them, which it does as it starts.
  - The same call again waits up to another 40 s for the rest, then returns what the export wrote. "The same" means the same arguments, and the same deck and theme.
  - The server runs one export per file. Another export of a file still being written is refused, with how far the running one has got.
- **Schemas.** Each tool's input and output schemas are generated from the Rust types (`schemars`) and committed in `docs/schema/mcp/<tool>.json`. A test fails when they are not what the server lists, and `just bless` regenerates them. They keep only the formats JSON Schema defines: the widths `schemars` gives numbers (`uint32`, `double`) are dropped, since a client's validator warns of formats it does not know, and the type and its `minimum` already say what the width meant.
  - Two inputs are typed loosely, as objects: a patch's ops and `deck_create`'s `deck`. Each points at the resource that types it. Inlined, `scaena://schema/patch` alone would add 73 KB to every `tools/list`.
  - The server checks every op as `patch` does, and names the one that fails.

**Resources** let an agent learn the format without the docs:
- `scaena://schema/deck`, `scaena://schema/theme`, `scaena://schema/patch`, and `scaena://schema/spine`, the spine projection. The deck's and the theme's schemas are served in parts:
  - the deck's definitions are in `scaena://schema/deck/nodes`, `scaena://schema/deck/deltas` (what a state sets), and `scaena://schema/deck/values` (what both use);
  - the theme's are in `scaena://schema/theme/charts`, `scaena://schema/theme/shaders`, and `scaena://schema/theme/motion`.

  Each schema and each part is a JSON Schema whose `$id` is its uri, and a `$ref` names the part that holds its definition. The patch schema refers to the deck's parts for the definitions they share. Together, the parts are the files in `docs/schema/`.
- `scaena://lint/catalog` (§7.5);
- `scaena://spec`: this document's index. Each section is a resource of its own, and so is each numbered subsection and each of a subsection's numbered parts: `scaena://spec/7` is §7, `scaena://spec/3.7` is §3.7, and `scaena://spec/9.2.4` is a part of §9.2. A section too large to arrive whole, as §3 and §7 are, holds its text up to its first subsection and the uris of its subsections; a subsection too large, its text up to its first part and the uris of its parts.
- `scaena://spec/format` and `scaena://spec/expr`: the number and date formats, and the data expressions (`docs/spec/`);
- `scaena://skills/<name>`: the five skills (§7.6);
- `scaena://examples/*`: the example deck as JSON and `.scn`, and its patch; `trails.deck.json`, fifteen slides that use most of what a deck can hold; `charts.deck.json`, every kind of chart and a table with no style set; and three themes, `dusk.theme.json`, `daybreak.theme.json`, and `ember.theme.json`.

Each resource arrives whole (PLAN 1.33). Claude Code keeps an MCP result over 25,000 tokens in a file, which an agent with no tools for files cannot open. So no resource weighs over 40 KB as `resources/read` returns it, and a test holds every one under that. A JSON resource comes without the whitespace its file has: the same document, in fewer tokens.

They are compiled into the binary, so they describe the format it reads, the same for everyone. From 2026-07-28 a client rejects a list or read result that does not say how long it may keep it (SEP-2549), so `resources/list` and `resources/read` say `ttlMs` 3,600,000 (an hour) and `cacheScope` `public`. A client on an earlier protocol gets results without them.

### 7.3 Patch semantics

A patch is a JSON array of ops, applied in order, all or none (`docs/schema/patch.schema.json`, generated from `scaena-core::patch`). Each op is RFC 6902's, or a semantic op that compiles to RFC 6902's.

- **JSON Patch** (RFC 6902): `add`, `remove`, `replace`, `move`, `copy`, `test`, with JSON Pointer paths into `deck.json`.
  - A member moved within its object keeps its place, so a `move` there is a rename. The order of `nodes` is paint order at equal `z` (§3.4).
  - `test` compares numbers by value.
  - Members an op does not define are ignored, as RFC 6902 says.
- **Semantic ops** say what an author means. Each compiles against the deck as the ops before it leave it, so a patch can add a node and then set its props.
  - An op refuses what would not do what it says. The refusal names the op by its index (`op`, from 0) and what to do instead.
  - A misspelled member is an error, not a change somewhere else.
  - `state` names the state an op acts in, as a delta from that state on. Without it, an op acts on the node's own properties.

| Op | Members | What it does |
|---|---|---|
| `add_node` | `id`, `node`, `state?`, `props?` | Adds a node, last in scene-graph order. With `state` it enters there, `props` its delta |
| `remove_node` | `id` | Removes the node and every reference to it: deltas, exits, choreography (an item left with no target goes too), overrides. A container must be emptied first |
| `rename_node` | `id`, `to` | Renames the id everywhere it is used. The node keeps its place, type, and properties, so tracking and morphs see the same node |
| `show_node` | `node`, `state`, `props?` | The node enters in the state; one that leaves there stays instead |
| `hide_node` | `node`, `state` | The node leaves in the state; one that would enter there does not |
| `group` | `id`, `nodes`, `state?` | A new group (ADR-0008, PLAN 2.43) holding `nodes` where they stand: children of one container, the slide or a group, as `state` shows them (else as their own `at` places them). Each takes the group as its container wherever that one is written: in its own `at` on the slide, else wherever its `at.parent` names the container. A group lays nothing out, so nothing moves. It sits in their container at the highest `z` among them (0 for one that sets none), last among what stands there, and shows in each state that shows one of them in it, and only there: it enters where the state it tracks does not bring it, and leaves where it would come along with none of them. Refused for nodes of two containers, and in a stack, grid, or frame, which places what it holds itself |
| `ungroup` | `group` | A group's children out to the group's own container where they stand, each keeping its own `z`, and the group gone with everything that names it, as `remove_node` takes it; its look goes with it. `ungroup` after `group` gives the deck back as it was |
| `set_prop` | `node`, `prop`, `value`, `state?` | Sets a property (`kind`) or one key of one (`at/in`); a delta merges objects one level deep (§2.2). `null` takes it away. Refused for `type` (E104), and for a node not on screen in `state`, which a delta would make enter (`show_node` does that) |
| `place` | `node`, `at`, `state?`, `fork?`, `format?`, `anew?` | Moves or resizes a node, as a drag does (ADR-0013). `at` is one placement: cells (`col`, `row`), a slot (`in`), a `rect`, a grid container's `area`, or a stack's `index`. The placement keys of the node's `at` become `at`'s, and the others go. It is written where the placement lives: in the deck's `overrides` if they set it; else in the latest delta that sets it, from `state` back along what it tracks (§2.2); else in the node's own `at`. With `fork`, it is written into `state`'s own props instead, wherever it lives: the node goes there in that state and in the states that track it, as they take the rest of its props, and stays where it was in the others; refused where the overrides place the node. Its container is part of its placement: `at.parent` names another container the node goes into, written with the rest of it, or `null` takes it out onto the canvas; unsaid, the node stays in its own (PLAN 2.50). Refused for a placement the node's container, or the one it goes into, does not take: the theme's grid takes cells, a slot, or a `rect`; a grid container its cells, an area, or an `index`; a stack an `index`; a frame a `rect`. A container goes into nothing it holds: the deck refuses a loop (E106). With `format`, the format shown (ADR-0020, PLAN 2.85), a node with its own layout there (`formats`, §3.4) is placed in that layout's `at`, the rest of the deck as it was; one with none is placed as without it, in every format. `anew` gives it a layout of its own there, placed so; refused in a format the deck does not lay out anew. `fork` with `format` is refused where the layout is the node's own |
| `set_text` | `node`, `text`, `state?` | Sets a text node's `text`; its `runs` there go |
| `choose` | `node`, `prop`, `value`, `state?`, `fork?` | One property as an inspector chooses it (ADR-0013, PLAN 2.33): `prop` is a property or one key of one (`style/color`), and `value` is written where it lives, as `place` writes a placement: in the latest delta that sets it, from `state` back along what it tracks, else in the node's own. A value written out where the theme has names (W300's: a color, a text size, a length in canvas units) goes in the deck's `overrides`, the only place it is legal, and so does any value where the overrides set the property, since they win in every state. `null` takes it away where it lives, so what is under it shows: out of the overrides (which go once empty), out of the delta, or off the node. With `fork`, in `state`'s own props; refused for a value that goes in the overrides. A chart's or a table's `data` (PLAN 2.41) is another source, `@name`, that the deck declares; what that source cannot serve is pointed again where `data` is written. An axis whose field it lacks, or types otherwise than the axis reads (`inspect --choices`), reads a column it can that no other channel reads: one of the type its field had where there is one, else the first. A `series`, a `color`, a size, a projection, a `key`, or a table's column whose field it lacks is taken away. An `x` or a `y` with none refuses the source |
| `replace_text` | `node`, `from`, `to`, `text`, `state?`, `fork?` | Text typed where it stands (ADR-0013): the characters from `from` to `to` of the node's text as `state` shows it (its `text`, or its runs' texts end to end, the deck's `overrides` over them) become `text`. Offsets count characters (Unicode scalar values). Runs keep their looks: what is typed takes the look of the run it is typed into, the one before where two meet, and a run the edit leaves with no text goes. It is written where the text lives, as `place` writes a placement: in the overrides if they set it; else in the latest delta that sets it, from `state` back along what it tracks; else in the node's own. With `fork`, in `state`'s own props; refused where the overrides set the text. A text that is a list (§3.5) keeps its `list` in step, written beside the text: a paragraph a typed break splits off an item is an item like it, and paragraphs joined keep the first's mark |
| `list` | `node`, `from`, `to`, `kind?`, `level?`, `by?`, `state?`, `fork?` | A text's paragraphs as a list's items (§3.5, ADR-0018, PLAN 2.69): each paragraph the characters from `from` to `to` touch, counted as `replace_text` counts them, set to `kind` (`bullet`, `number`, or `none`, out of the list), at `level` or each at its own; or, with `by`, moved that many levels deeper (negative: shallower), from 0 to 8, a paragraph that is no item becoming a bullet as it goes deeper. The text's `list` is written where it lives, as `choose` writes a property; with `fork`, in `state`'s own props. A list left with no items goes, unless the node's own would show through. Refused: a `kind` with `by`, a `level` without a `kind`, neither, and offsets past the text |
| `style_text` | `node`, `from`, `to`, `look`, `state?`, `fork?` | The look of characters (ADR-0013, PLAN 2.38): the characters from `from` to `to` of the node's text as `state` shows it, counted as `replace_text` counts them, take each key of `look`: a run's `role`, `emphasis`, `lang`, `link` (PLAN 2.70), or `quote` (PLAN 2.72), or one key of its `style` (`style/weight`, `style/italic`, `style/color`, …). A key set to `null` is taken away, so the node's own look shows there. The text becomes runs split at `from` and `to`; neighbors left alike are joined, and runs that all read as the node does are its `text` again. It is written where the text lives, as `replace_text` writes typing; with `fork`, in `state`'s own props. A `quote` makes the characters one run, and one that cuts into a quoted run takes all of it; the patch sets its figure from the data (§3.5). Refused: no characters, a `look` of nothing, a size (a size comes with a role), and a color written out (a run takes the theme's names) |
| `annotate` | `node`, `annotation`, `index?`, `state?`, `fork?` | One of a chart's annotations (§3.7, PLAN 2.67), as the editor makes one from a mark: `annotation` is added after the others, and needs a `kind` and an `at`. With `index`, counted among the annotations `state` shows (the deck's `overrides` over them), each key it gives (`kind`, `at`, `text`, `role`) replaces that annotation's own, and a `text` or `role` of `null` is taken away; `null` there takes that annotation away. The list is written where it lives, as `choose` writes a property: in the overrides if they set it; else in the latest delta that sets it, from `state` back along what it tracks; else in the node's own. With `fork`, in `state`'s own props. A list emptied on the node goes; one emptied in a delta or the overrides stays, empty, so that what is under it does not show. Refused for a node that is not a chart, an `index` it does not have, and an annotation that stands where its kind cannot (E106) |
| `bind_data` | `node`, `data`, `source?`, `state?` | A chart or table reads `@data`; `source` declares or replaces the data source. In a state, it is a data update (§3.7) |
| `apply_preset` | `node`, `preset`, `motion?`, `state?` | With `motion` (`enter`, `exit`, `emphasis`), a theme motion preset as that motion. Without, a shader takes a theme shader preset whole: the preset's kind, and none of its own `palette` or `params` |
| `time_motion` | `node`, `motion`, `state`, `delay?`, `duration?` | When a motion of `node`'s in `state`'s cue starts and how long each unit runs (PLAN 2.44): `motion` is `enter`, `exit`, `emphasis`, or `anim`; `delay` is in ms; `duration` is ms or a theme duration. It is written where the engine reads the motion (§3.9). That is the first item of the state's choreography that moves the node so, else the node's own: its `enter`, `emphasis`, or `anim` as the state reads it, and its `exit` as the state before it in the cue list reads it, each where it lives. A preset by name becomes a call to take a setting, and a call goes back to its name once it sets nothing else. An item's setting goes where the motion's is now: the item's, else its call's, else the item's. `anim` keys keep their spacing: a node's own `delay` is when its first key falls, and `duration` runs from its first key to its last. A delay of 0 is written as none. Refused for a motion that is not there, and for a `duration` on a spring, which lasts as long as it takes to settle |
| `add_state` | `state`, `after?` or `before?`, `beat?` | Adds a state there (else last); with `beat`, one of that beat's states |
| `move_state` | `id`, `after` or `before` | Moves a state. A state in delta mode tracks from whichever state now comes before it |
| `remove_state` | `id` | Removes a state and its place in the spine, and the links to it in a text's runs, their words kept; the state after it tracks from the one before. Refused for a state that others build on (`slide`) or track from (`from`) |
| `rename_state` | `id`, `to` | Renames the id everywhere: `slide`, `from`, the beats, and the links to it in a text's runs |
| `set_state` | `id`, `prop`, `value`, `fork?` | One property of a state as an inspector chooses it (PLAN 2.36): its `layout`, its `transition` or one key of it (`transition/duration`, `transition/ease`, `transition/spring`, `transition/match`), its `hold`, or its `notes`. A layout tracks (§2.2), so it is written where it lives: in the latest state that sets it, from `id` back along what it tracks, and each state that takes it from there changes with it; in `id` where none sets it, or with `fork`. The rest are `id`'s own. A transition that is a bare duration, or none, stays bare while its duration is all it sets, and becomes an object to take another key. `null` takes a value away where it lives: a layout taken away shows the one under it, and a transition left with nothing cuts. A layout without a slot for a node placed in one is the deck's error (E102), which `patch` refuses |
| `retheme` | `theme` | Sets the deck's theme: a file in the bundle, or inline. `theme --apply` copies a file in |

**`scaena patch` checks a patch before it writes it:**
- It compiles the ops, then checks the deck they make as `validate` checks a bundle. A patch that adds a validation finding (E102–E106) is refused whole: exit 1, and nothing is written.
- It reports the patch as RFC 6902 and the lint delta: what `lint` finds that it did not (`added`), and what it finds no more (`removed`).
- It says which states the patch changes (`states`): each whose nodes, resolved with the deck's overrides, or whose layout, transition, choreography, hold, or notes are not as they were, and each it adds.
- A finding is the same before and after when its code, format, file, state, and node match, and its message matches but for its figures. A path that names a state by place counts by the state's id, and renamed ids count by their new names. So a state added early does not move every finding after it, and a widow that gains a word is the same widow.
- `--dry-run` reports without writing. An op that does not apply exits 2 with its index (`op`).

Every patch becomes a CRDT change (§8, PLAN 1.23).

### 7.4 Lint results

```jsonc
{ "code": "E100", "severity": "error", "message": "text `title` does not fit: it needs 141 cu of height in 2 lines, and its box has 128",
  "path": "/nodes/title", "state": "revenue", "node": "title",   // "file": "theme.json" when path points into a file other than deck.json
  "format": "9:16",                                                // when the deck was laid out in one of its formats, not its own
  "measure": { "lines": 2, "height": 141, "width": 1120, "box": [1144, 128] },
  "hint": "Shorten it, give it a larger slot, or let it shrink (`fit: shrink`, down to its role's `minSize`).",
  "fix": [ { "op": "add", "path": "/nodes/title/fit", "value": "shrink" } ] }
```

`fix` is optional and MUST be a valid patch (§7.3). A fix never changes content, and lint keeps one only after laying its state out again with it applied: the text fits, within its lines, at a size its bounds allow; a text reads in the color a contrast fix chooses wherever the choice reaches (§7.5). `lint --fix` applies every fix, each at most once, writes the deck canonically, and lints again. Under `--json` it prints `{ "fixed": [findings], "findings": [what remains] }`.

**What `lint` runs**, in order:
- The bundle's validation, as `validate` runs it (E102–E106).
- The document-level rules (`scaena-core::lint`), which read the deck, its theme, and its resolved states.
- Once nothing above is an error, the layout-level rules (`scaena-engine::lint`). They lay out every state in the deck's own format and in each of its `formats`, and judge motion in its own.

Layout lint lays out what a frame refuses: text under `fit: error` that does not fit, a table whose rows do not. It reports these instead of stopping at the first. Contrast (E110, E111) needs pixels, and the engine never paints, so the client lends a painter (`scaena_core::lint::Backdrop`); the CLI lends the CPU painter.

### 7.5 Lint catalog (initial)

Three families. **Mechanical** rules say "this cannot be shown" (1xx). **Design** rules say "this is shown badly" (2xx typography, 3xx theme/motion/charts). **Narrative** rules say "this does not make the argument" (4xx; most need `semantic` and the spine). The loop an agent runs is generate → critique → repair → look, and the narrative family is what turns the lint engine from a compiler into a critic.

| Code | Sev | Rule |
|---|---|---|
| E100 | error | text that does not fit its box, under `wrap`, `clip`, `grow`, or `error`; a table whose rows do not fit its cell; text a chart sets past its sides, where the chart cuts it off. Fix: `fit: shrink`, where it works |
| E101 | error | two nodes that draw content (text, a chart, a table, an image) overlap on one level, by more than 2 cu each way. Two nodes are on one level when, where their containers part, the two that stack there (the nodes themselves in one container, else the containers they are in, or a container and a node) have the same `z`: a card in a stack is judged against a note beside the stack. Text counts by its lines as set, not its cell, and each box where its `transform` draws it (§3.3): two turned boxes overlap by what their outlines share. What is meant to lie on top says so with a higher `z`, its own or its container's; `semantic: decoration` is exempt |
| E102 | error | reference to something that is not there: a node, state, or data source; a file (font, data, image, theme); a theme name (text role, layout, slot, motion preset, duration, easing, spring, shader or data palette, color), saying the names of that kind the theme has; a theme family the deck's `fonts` does not list; or a grid cell past the theme's grid, in the deck's format or one it lists: a node's `at.col` or `at.row`, or a slot a node stands in, beyond the grid's columns or rows. That finding names the grid's size, at the node's `at` or the slot in the theme |
| E103 | error | what a chart, a table, or a run's `quote` (§3.5) reads from its data: a quote's row that is not there, or that many rows match, and its empty cell; a field the data does not have, or has in a type the channel cannot read (`quantitative` reads numbers, `temporal` dates, a `format` numbers or dates), before or after its `dataTransform`; a transform step that reads a column that is not there, or uses one as the wrong type; a value that does not fit its column's schema type or `parse` format; an annotation's category, series, or x value the data does not have; a chart's or a table's key that repeats, in any state that shows it (§3.3, §3.7) |
| E104 | error | node type changed across states (a state's delta, or a node's overrides, sets `type`) |
| E105 | error | duplicate or invalid id: an id twice in its collection, a key written twice, an id listed twice, an id that is not a slug |
| E106 | error | the deck or its theme does not match its schema, or a resolved state, or a node with its overrides, does not match the node's type (another type's property, a value this type does not take); a format, a `dataTransform` step, or an expression that does not parse; a chart annotation that stands where its kind cannot; a choreography item that names no motion or more than one, or splits a target into what its type does not have; an `at.col` or `at.row` range that runs backward |
| E110 | error | body text whose contrast with what lies behind it is below 4.5:1 |
| E111 | error | display text whose contrast with what lies behind it is below 3:1 |
| E120 | error | characters a node sets (text, a table's cells, a chart's labels) that its family and its fallbacks have no glyph for |
| W200 | warn | widow: a paragraph's last line short of `minLastLineWords`, which breaking could not fix |
| W201 | warn | a line wider than its measure, inside its box: a word too long to break at the measure |
| W202 | warn | text with more lines than its `maxLines` (the node's, else its role's). Fix: `fit: shrink`, where it works |
| W203 | warn | `fit: shrink` reached its minimum size and the text still does not fit |
| W210 | warn | density: more words on screen in a state than the theme's `density.maxWordsPerState` (40) |
| W220 | warn | paragraphs (text of two lines or more) aligned more than one way in a state |
| W221 | warn | a role that snaps its baselines to the baseline grid (`snap: "baseline"`) with a leading (`size × leading`) that is not a whole number of grid lines, so its lines sit farther apart than it says; or a grid the deck is laid out on with no baseline for it to snap to. It points into the theme (§3.4) |
| W230 | warn | a font the deck lists whose license, in its OS/2 embedding bits (`fsType`), does not allow what Scaena does with a font: embedding only with its owner's permission (a bundle carries its fonts, as a PDF and a single-file export do), embedding only in documents opened read-only (a bundle is edited), no subsetting (Scaena subsets the fonts it saves and exports; `scaena save --keep-fonts` keeps a bundle's whole), or embedding only its bitmaps (exports embed outlines). Where more than one usage bit is set, the least restrictive holds, as OpenType says. One finding per font, at its entry in `fonts`, with its `fsType`. It warns and refuses nothing (§16 Q3) |
| W231 | warn | text that asks for italic (`italic` in its role or its `style`, a node's or a run's) set in a family with no italic face, the theme's family or a fallback the text takes: it is set upright, since no italic is synthesized (§3.5). One finding per text and state, naming each family, by its key and name, or each fallback's font, by its file |
| W300 | warn | style literal outside `overrides`: a color written out where a theme color goes, a text `size`, a length in canvas units where a theme token goes (`radius`, `gap`, `padding`, `inset`, a stroke's `width`, a child's `size`) |
| W301 | warn | a node placed on the canvas by `rect` in a state with a `layout`. A container's child placed by `rect` is placed in its container |
| W302 | warn | a deck with `formats` that places a node on the canvas by `rect` or by grid cells (`col`/`row`): it does not move with the formats' slots |
| W310 | warn | chart value labels within a quarter space unit of each other, or of another mark, unless `labels.collide` resolves them; category labels within a quarter space unit of each other on an axis of text, which keeps every one (an axis of dates or numbers keeps fewer, §3.7) |
| W311 | warn | a shader painted behind a chart or a table, where they overlap: data reads against a plain surface (§3.8). One finding per shader and chart or table, at the shader, with every state it happens in |
| W312 | warn | chart text under 12 pt at presentation size: 24 cu where the canvas's shorter side is 1080 cu, in proportion on others (§3.7). One finding per chart, at the chart, naming each kind of its text that is too small (category and value labels, axis labels, titles, series names, annotations) at its smallest |
| W313 | warn | a chart squashed below a legible plot: the room its marks have, once its axes, labels, titles, and legend have theirs, under 120 cu across or down where the canvas's shorter side is 1080 cu, in proportion on others, as a short slot or a few rows of a theme's grid can leave it. One finding per chart, at the chart, with its plot at its smallest and every state it is squashed in |
| W320 | warn | more nodes moving at once than the theme's `motion.maxConcurrent` (12) |
| W321 | warn | a state's motions run past the theme's `motion.maxBuild` (2500 ms) |
| W322 | warn | a motion that moves nothing, yet takes its time: an entrance on a node that does not enter, an exit on one that does not leave (under `match: none`, every node does both), an emphasis or `anim` on a node not on screen, a draw-on (a look whose only change is `progress`) on a node that strokes no outline |
| W323 | warn | in a deck that runs on its own (any state has a `hold`), a state that shows for 0 ms: its span and its hold are both 0, so a video has no frame of it and a player's auto-advance passes over it. The finding suggests a hold for its reading |
| W401 | warn | state not referenced by any beat |
| W410 | warn | image without `alt` |
| W420 | warn | a beat whose states show no claim (a node whose `semantic` is `claim` or `takeaway`). A beat whose states show only signposts (`navigation`, `decoration`, `source`) is not judged |
| W421 | warn | a slide whose last state shows evidence and no claim |
| W422 | warn | at a slide's last state, evidence text set larger than its claim, by role size or `style.size`; a stat's numeral (role `numeral`) does not count |
| W423 | warn | a beat citing a data source or an asset that none of its states shows (a chart or table reading the source, an image of the asset, or for a source any node marked `evidence`). URLs are not checked |
| W424 | warn | more than one `claim` on screen at once |
| W425 | warn | two consecutive beats with the same claim, ignoring case, spacing, and the closing stop |
| W426 | warn | a beat that comes after another in the spine while its states play before that one's: the PDF reads a deck in spine order and the video in state order, so the two would tell the story in different orders (§3.11). A beat stands where its first state plays; one with no states the deck has is not judged |
| W427 | warn | a figure a run quotes (§3.5) that its data no longer gives, a file changed by other means than a data edit, at the run's `text`; and each claim of a beat whose states show the text that holds the old figure as a word, at the claim. Each with the fix that writes the figure the data gives |
| I400 | info | state identical to previous (no-op cue) |
| I401 | info | node never visible |
| I402 | info | override makes node theme-unsafe (count) |

**Contrast** (E110, E111) follows WCAG. Text is a text node's, a table's cells', and a chart's: its category and value labels, axis labels, titles, series names, and annotations, in their colors and at the opacity a highlight dims them to (PLAN 1.27). What lies behind text is the state at rest with its text taken away, painted: the surface, shapes, images, shaders, and charts' marks, rules, and bands. A shader is judged at rest and at the end of the state's hold. Each run of text is judged over the pixels under its glyphs: its color is laid on each pixel at its alpha times its node's opacity, each pixel counts by how much of it the glyphs cover (painted on their own), and the run fails when more than 2% of its ink falls below the line. So a rule under a baseline, or a mark past a descender, does not count against the text; a mark or a line under the letters does. Text at no opacity is not judged. **Display** text is WCAG's large text: 24 px or more, or 18.67 px at weight 700 or more (the weight its role sets it in), on a screen whose shorter side is 1080 px (on a 1920 × 1080 canvas, a canvas unit is a pixel). Everything else is body. A node is reported once per format, at its worst; a chart once for each kind of text in it that fails, since each is set in its own role. A text node's finding carries the fix (PLAN 2.82): of the theme's colors, its roles and then its tokens as an inspector offers them, the one whose worst reading is best, chosen as an inspector chooses one, a `choose` of `style/color` at the state it reads worst in, written where the node's color lives (§7.3). Lint offers it only where the text reads in it in every state and every format the choice reaches, each run that takes the node's color in that color and a run with a color of its own (`style_text`) in its own. What lies behind a text has its text taken away, so a color changes the text alone, and the pixels painted judge every color. A chart's text and a table's are set in the theme's roles for them, and get no fix.

**Holds** (W323) are judged on the global timeline (§2.4), once, in the deck's own format. A deck with a `hold` on any state runs on its own: video export and auto-advance go from state to state by the timeline, and a state whose span and hold are both 0 gets no time on it. A deck with no hold is not judged: it is presented live, and each state shows until the next click. Exported as a video, it would play only its cues: it needs holds first. Nor is a state with a cue and no hold judged: it plays its cue and the next state starts, as a state on its way to the next means to. The hold W323 suggests is the time it takes to read the state, as the motion-pass skill sizes one (§7.6): four words a second for its text, plus 2 s for each chart, table, and image, decoration aside; rounded up to the half second, and never under 2 s. It offers no `fix`: how long a state holds is the author's pacing.

The narrative rules are warnings, never errors, and read only what `semantic` and the spine say. A build may show its evidence before its claim, so W421 and W422 judge each slide at its last state, where the build is complete. W424 allows one claim at a time; a contrast that needs two is `semantic: comparison`. Whether a slide's words say what its beat claims is a question for the user's model (BYOK, §11), not a rule.

Rules live in `crates/scaena-core/src/lint/` (document-level) and `crates/scaena-engine/src/lint/` (layout-level). Each rule is a struct implementing `Rule` with a stable code, and has fixtures under `tests/lint/<CODE>/`: one deck that triggers it, one that must not.

### 7.6 Skills

Prompts and procedures for agents ship in-repo as skills (`skills/<name>/SKILL.md`, PLAN 1.18). Each is a procedure an agent follows through either surface: every step names the command (§7.1) and the MCP tool (§7.2) that does it.

- `author-deck`: brief → spine → states → lint loop → look.
- `retheme`: swap the theme, then resolve what lint finds differently.
- `chart-from-data`: a data file → a chart or table that makes one point.
- `motion-pass`: transitions, entrances, builds, and emphasis that explain, within the theme's limits.
- `tighten-copy`: one claim per slide, in as few words as carry it.

Skills are versioned with the format, and the MCP server serves each as `scaena://skills/<name>`. A test (`crates/scaena-cli/tests/skills.rs`) holds every skill to what exists: the commands and flags it shows, the MCP tools and resources it names, its lint codes, its files, and its SPEC sections.

---

## 8. Storage, versioning, collaboration

### 8.1 CRDT document

The document lives in a **Loro** document (ADR-0002, PLAN 1.23; `scaena-store::crdt`), kept with its whole history in `history/deck.loro`. Containers:

- `deck` — map: `scaena`, `canvas`, `formats`, `theme`, `fonts`, `_comment`, and whether the deck has `meta` and a `spine`
- `meta`, `data`, `overrides` — maps
- `nodes` — map of maps: each node under a key of the CRDT's own, which nothing outside it sees, holding its id, its type, and its props. Renaming a node changes its id and nothing else.
- `order` — movable list of node keys: paint order (§3.4), which a map does not keep
- `states` — movable list of maps (the ordered cue list). A state's `props`, its `remove`, its choreography's targets, and `at.parent` everywhere name nodes by key.
- `spine` — tree: sections, with their beats under them
- a text node's `text` (in its defaults and in deltas) and the `notes` of a state or a beat — text, merged by character; `runs` — rich text, a mark per run
- `files` — map: each data file a source names (§3.10), and the theme file the deck names (§3.6, ADR-0016), by its path, its bytes now, as a binary value (ADR-0014). An edit to one sets them in the change that edits the deck, if it does; the history keeps every version, and an undo sets the bytes back. A history begins with the data files and the theme the deck names; a save sets the theme's bytes as it writes them. An inline theme is the deck's own.

Every other value is held as JSON text and replaced whole, the last writer winning, with its keys in their order; a map whose keys `deck.json` shows in an order keeps that order beside them. So the deck the CRDT exports is the canonical `deck.json` byte for byte, and a deck taken in is the smallest change that gets the CRDT there: text edited by character, runs re-marked only where they changed, states and beats moved rather than made again.

`deck.json` is an **export** of the CRDT state (canonical for git, diffs, agents). A `deck.json` that says otherwise than the history (edited by hand, or by an agent via files) is imported as a change authored by `fs` before anything else is recorded (§3.1). So is a data file or a theme that says otherwise than the history: replaced on disk, written by a spreadsheet, or one the history does not hold yet (`data/q3.csv changed outside Scaena`); a history from before it kept the theme meets the theme so, once. The file stays the truth of its data: the history records the bytes an edit wrote, and nothing writes a file but an edit of it.

**Merges.** Concurrent edits to different props, nodes, states, beats, characters, or data files all stand; one value edited on both sides keeps one side's, the same on every side, a data file edited on both sides included (ADR-0014). A node renamed on one side and edited on the other is the renamed node with the edit, its deltas and overrides included. Two nodes made apart under one id are both kept, the later in paint order taking a suffix (`id-2`). Text typed at the very edge of a run while the run beside it is restyled may land in either.

### 8.2 Ops, undo, branches

- Every edit (UI, CLI, MCP) is a change with an author (`user`, `agent:<name>`, `fs`), a timestamp, and a message: what the command did (`patch: rename_node`, `theme --apply themes/dusk.theme.json`). The CLI records its changes as `$SCAENA_AUTHOR` (`user` without it), the MCP server as `agent:` and the client's name. A data edit is one change, its file's bytes with it (`data_edit q3: revenue of row 3`, §7.1), and so is a theme edit, the theme's bytes with it (`theme_edit: tokens/color/accent`). The web editor records when it saves (§9.2): the user's edits as `user`'s, and each edit its assistant made as `agent:` and the model's name, with what the tool did; each data file as each edit wrote it, after the files as the page opened them, taken in as a command takes in what it finds on disk. A patch that renames a node or a state says so, and the CRDT keeps it the node or state it was.
- Undo/redo via the CRDT's undo manager, per author: an editor undoes its own changes, never another's or a file's.
- **Versions** (PLAN 2.60). A version is the deck as it was just after one change the history keeps, with the data files its sources named then and its theme (ADR-0016): a version from before the history kept the theme has none, and is compared and restored without it. Versions are numbered from the oldest, and named by their change's id (`counter@peer`) for as long as the history lasts. A commit the CRDT stores in pieces is one change.
  - `scaena history` lists them, each with its author, time, and message (`deck_history` over MCP).
  - `--at` gives a version's deck, and `--scn` gives it as source.
  - `--diff A[,B]` says what changed from one version to another, or to the deck as it is now. That is each state added, removed, or changed (its own fields, and its nodes resolved with the overrides, as `deck_diff` says them), the deck's own fields, the states' order, and the data files and the theme whose bytes differ.
  - `--restore` makes a version the deck again, with its data files and its theme, as one change (`history --restore <id>`). It is refused as a patch is where the deck would not validate in the bundle as it is: a file it names taken out since is E102. Nothing is lost: the restore is a change after the others, which undoes as any other.
  - The editor's Versions tab does the same (§9.2).
- Branch = fork at a version; merge = CRDT merge; the UI shows branches as "versions."

### 8.3 What is **not** in the CRDT

Resolved snapshots, layouts, display lists, render caches, lint results. All derived; all recomputable.

### 8.4 Multi-user (deferred)

Everything above is sync-ready. Adding collaboration means a sync endpoint (Loro's or `automerge-repo`), identity, and presence. Not before there is a second user who matters. No feature in v1 may assume a server exists.

---

## 9. Clients

### 9.1 CLI + MCP (Phase 1)

The first client. Headless; renders via `vello_cpu`. Used by Claude Code and any MCP-capable agent.

### 9.2 Web player / editor (Phase 2)

#### 9.2.1 The site and the engine

- Static site (Vite + TypeScript). No backend.
  - **The site** (PLAN 2.7): `just site` builds the player (`index.html`) and the editor (`editor.html`) into one directory, `target/site`, with a demo deck in `decks/NAME/` (the trails example by default), saved as `scaena save` saves it with its fonts whole. The pages open it when the address names no bundle. The player's Edit opens the editor on its bundle, and the editor's Play opens the player on the bundle as last saved.
  - **Any static host** serves it, at its root or under any path, since its paths are relative. The host serves it over HTTPS, which WebGPU, the browser's storage, and the file pickers need, and `.wasm` as `application/wasm`. It needs no code on the server, no rewrites, and no other headers. What a page loads only when asked comes from the site too: the subsetter (to download), what the assistant reads (to ask), the PDF writer (to export a PDF), and the single-file page (to export one).
  - **Offline** (PLAN 2.73). The site installs: its pages register `sw.js`, a service worker the build writes beside them, and name a web app manifest (`manifest.webmanifest`, `src/offline.ts`). The worker keeps every file the build wrote as it installs, and answers each from that cache, a query aside, so the player and the editor open, play, edit, export, and download with no network, and load each module asked for later from what it keeps. Anything else under the site, a deck's files, it asks of the network, and keeps the answer for when there is none. It lets requests to other origins (the assistant's provider) and a page's events (`scaena serve`) pass by. Its cache is named for what the files hold: a site built again is another cache, which takes over once no page uses the last. The pages then say "Works offline". A bundle kept in the browser (OPFS) or a folder opened needs no network as before. Only the static site's build registers it: the repository's pages and `scaena serve`'s have their network. `web/offline.mjs` installs the site, takes the network away, and plays, edits, and downloads its demo deck.
  - **The `site` workflow** builds it by hand from the branch chosen. It keeps the site as the run's artifact, and publishes it to GitHub Pages when asked. `web/site.mjs` serves it from a path under a plain static server, as a host would: the demo deck plays, edits and lints clean, downloads, and is asked about, and nothing is fetched from elsewhere.
- Engine as WASM in a **Web Worker**; rendering to an `OffscreenCanvas` via WebGPU (`vello`), falling back to `vello_cpu` onto the canvas's 2D context where WebGPU is unavailable (PLAN 2.1, `web/`). The page hands its canvas to the worker, which loads the bundle by URL (its directory, or its deck file, as the CLI opens it) and paints with `vello` on WebGPU where the browser has an adapter. The worker asks for one before WebGPU takes the canvas, since a canvas WebGPU holds takes no other painter. If WebGPU still fails, the page starts over on a new canvas with `vello_cpu`, whose pixels go onto the canvas by its 2D context (`putImageData`). The modules are built with WebAssembly SIMD (`.cargo/config.toml`), which every browser the player targets runs, so `vello_cpu` paints on its SIMD path: a 1080p frame of B1 takes 13 ms at the median in V8 (ADR-0004 finding 15). The module has no threads, so the CPU painter works a shader's rows out on helper workers as well, one fewer than the browser's cores (at most 7; `?helpers=` caps them): each holds the engine's module and no deck, makes the shader's job again from its spec, and works out a band of its rows, the bytes the worker would have (PLAN 2.28, ADR-0004 finding 17). A frame goes onto the canvas from the module's memory, copied once. The page asks the worker for a frame (a state, at rest or a time into its cue), for the timeline, or to run or stop the deck's clock, each in one of the deck's formats or on its own canvas (`web/src/protocol.ts`). Both painters show every torture frame within §13.5 of the goldens (`web/smoke.mjs`, then the parity harness).
- **Hyphenation patterns** (ADR-0015). The editor's module leaves out the 17 languages' tries (§3.5), 0.35 MB of it gzipped. The worker sets the engine's loader (`setHyphenation`). The first time a text hyphenates in a language, the engine asks for its trie in the middle of the layout. The worker fetches the file at once, beside the pages where the build put it (`web/src/hyphenation.ts`), as a worker may. The trie is held to its SHA-256 and kept, and one that cannot be had is the frame's error. The player's module, which a single-file export carries offline, has every trie compiled in, and the smoke page (`crates/scaena-wasm/www`) hands them over from the repository.

#### 9.2.2 The player

Player: keyboard/remote navigation, state scrubber, presenter view in a second window (`BroadcastChannel`), auto-advance for `hold` (PLAN 2.2).

- **Where the deck is** is a state and a time into its cue. The global timeline (§2.4) cannot say it: the states with no transition and no hold all stand at one instant, as most of a slide deck's do.
- **Going on** plays the next state's cue, and going on during a cue finishes it. **Going back** shows the state before at rest. The worker keeps the clock, a frame each time the display takes one.
- **Holds.** A state with a `hold` goes on to the next by itself once its cue and hold are over. One without, and the last, comes to rest and waits for the presenter.
- **Controls.** The keys are → ↓ PageDown Space Enter to go on, ← ↑ PageUp Backspace to go back, Home and End, F for fullscreen, and P for the presenter view. A click or a tap goes on, and a swipe goes either way. A click on a link at rest follows it instead (PLAN 2.70, `Player.linkAt`): a state is shown, a web address opens in a window of its own; a link in the reading's live region goes too.
- **The state scrubber** gives each state an equal step, however long its cue, and runs through the cue within the step.
- **The presenter view** follows the player frame by frame, and shows the state's notes (its beat's, where it has none), the next state at rest, and a clock. It steers the player back and on, and with Home and End to the first state and the last.
- **For a screen reader** (PLAN 2.8, §3.12): the canvas is hidden, and the page keeps how the state shown reads in a polite live region, as a single file does. The engine writes each reading in the format shown. The state picker is the spine's outline: a group for each section, titled by its title (else its first beat's claim), each state named with its beat's claim, then the states no beat names.
- **Less motion.** When the reader asks for less (`prefers-reduced-motion: reduce`, or `?motion=reduce`), each cue is a cut: going on shows the next state at rest at once, with no frame inside its cue. The deck keeps its pace: a state that holds goes on when its cue and hold are over, as it would with motion. `?motion=full` plays the cues anyway. A change of the preference takes hold where the deck is.
- **Frame meter.** `?fps` shows, while the deck plays, its frames a second over the run so far, its worst frame, how many frames came more than 25 ms after the one before (late for a 60 Hz display), and the mean paint. The worker times each frame of a run by the display's clock (`requestAnimationFrame`). It is how gate 2's first criterion is read on a real machine (`docs/gate-2.md`).
- **Keys and controls.** A focused control keeps the keys it answers to: Enter and Space for a button or a link, every key for the scrubber, a picker, or a text field. The deck's keys work everywhere else, so → still goes on after a click on ▶. The scrubber says which state it is at (`aria-valuetext`).

#### 9.2.3 The editor

Editor v1 (PLAN 2.3, `web/editor.html`): the deck as canonical `.scn` (§4) in CodeMirror 6, with a `.scn` mode, and the preview as a canvas that edits by patches (PLAN 2.31). The canvas comes first (PLAN 2.80): the page is the source at the left, the canvas with its controls and findings in the middle, and the inspector's tabs at the right, and the source folds away, the canvas taking its room. It opens folded, or as the address asks (`?source=open`), or as the browser last left it; Source and ⌘\ open and fold it, and a finding that stands in the source opens it there.

- **Each edit** compiles in the worker as it is typed (`scaena-ops`' compile, ADR-0009). A source that does not compile says where, and the preview keeps the deck it had. A deck that validates is shown from then on.
- **The preview** shows the state the cursor is in, at rest: the last state whose `state` line starts at or before it.
- **Lint, in two steps.** An edit lints the state shown, laid out alone after the one before it. The deck's other states keep what lint last found in them, in the formats the deck still lists, each while it shows every node it showed then. A state the edit took a node out of (deleted, renamed, or removed from it), or took out, keeps nothing until every state is linted again: what lint found there can be about that node, or owe something to it, as the contrast of text over it does. Once typing stops for half a second, every state is linted. Linting one state finds in it what the whole lint finds there. It finds more only where the whole lint reports the same finding once, at another state: where a collision starts, or where text reads worst (`crates/scaena-ops/tests/lint.rs`).
- **Findings** stand in the gutter and under the source, each where the source wrote what it is about: a node's prop in the state that sets it, or its declaration. A fix is one click. The fix patches the deck, which is written back as canonical source. The editor takes only the lines that changed.
- **The inspector** shows the state's cue (where it starts, its span, its hold, its transition, and each motion as placed). It shows each node, where it is placed, each text node's look, and how many props its overrides set. A node placed by a `rect` that lint flags (W301) is marked as an override.
- **Keys.** A finding under the source is a button that goes to where it stands. CodeMirror's lint keys move between findings (F8, and Shift-F8 back) and list them (Mod-Shift-M); `?` elsewhere lists every key the editor answers (§9.2.4). Tab indents, so Escape then Tab leaves the source, which is named for a screen reader. The arrow keys, Home, and End move between the inspector's and the assistant's tabs.
- **What a reader needs** is checked by axe-core against WCAG 2.1 A and AA on the player, the presenter view, the editor with each tab, and a single file (`web/a11y.mjs`).
- **Speed.** On B1, an edit's round trip (compile, the frame, lint of the state shown) takes a median of 114 ms in headless Chromium painting on the CPU, against gate 2's 200 ms. The lint of every state that follows takes 1.2 s (`web/editor.mjs`).

#### 9.2.4 The canvas

**The canvas** (PLAN 2.31, ADR-0013). The preview takes the pointer and the keys, and each gesture ends in one `place` patch (§7.3), made by `user`. It comes into the source as one change, one step to undo from the canvas (⌘Z) or the source. The engine answers every question about where things stand, from the state as laid out at rest, which the frames at rest draw too; the page lays nothing out.

- **Select.** A click selects what the engine says is topmost there (`hit`), and the inspector marks it, as a click on the node's row there does. A press in the node selected, or in what holds it, keeps it, so a drag moves it. Escape selects what holds it.
- **Drag.** The node and what it holds are drawn moved, over the rest, from the layout at rest: nothing is laid out while the pointer moves. The guides are where it may go (`targets`: the grid's tracks, the template's slots, a grid container's areas, a stack's order), and where it would land. Before the drop, the status says which states the patch changes. With Shift, the node goes off the grid, placed by a `rect`, or comes back onto it. With Alt, the move is kept to the state shown (`fork`).
- **Resize.** Handles resize a node placed by cells or by a `rect`, by tracks. While one is dragged, the text keeps its old wrapping. When the pointer pauses, the preview shows the state laid out as the patch would make it, which nothing has made yet.
- **Keys.** The arrow keys move the node selected a track, or a place along its stack (a canvas unit by `rect`). With Shift, they resize it the same way.
- `web/canvas.mjs` checks it in headless Chromium on the revenue example and the torture deck's containers.
- **Several nodes at once** (PLAN 2.42, ADR-0013). A selection is children of one container: the canvas's, or one container's.
  - **Selecting.** Shift+click puts a child of the same container in the selection or takes it out; a click in another container starts a new one. A drag across empty canvas, or across what fills it behind all, selects the children of the canvas that the marquee encloses whole, and with Shift adds them. The nodes selected are outlined, without handles, and Escape keeps the first.
  - **Moving.** A drag moves them together, and so do the arrow keys: the first snaps as it would alone, and the rest go as far as it went, each its own way; where the grid's edge stops one sooner, all stop there (`Player.arranging`, as `inspect --arrange --by`). Shift takes them off the grid, and Alt keeps the move to the state shown.
  - **Arranging.** The inspector says how many are selected, offers the fields all of them have alike, each with the value where all of them agree, and aligns (two or more), spreads (three or more), and orders them. ⌘] and ⌘[ (Ctrl+] and Ctrl+[) put what is selected in front of, or behind, the next thing it overlaps, and with Shift in front of, or behind, everything its container holds (`inspect --arrange --order`).
  - **Taking away and copying.** Delete, ⌘D, ⌘C, and ⌘X act on all of them. A clip of several holds each with its box (`more`), and a paste puts them about where the pointer last pressed, as they stood about each other, moved onto the grid where they fit so that its edge stops none of them alone, each snapped to it, all of them selected.
  - **Each** is one patch, by `user`: one change in the source, one step to undo. A choice in the inspector is a `choose` for each.
  - `web/several.mjs` checks it in headless Chromium on the torture deck's containers.
- **Groups** (PLAN 2.43, ADR-0013). ⌘G (Ctrl+G), or the inspector's Group, puts what is selected in a new group where it stands, the group selected (`group`, its id the first of `group`, `group-2`, … free; `Player.grouping`). ⌘⇧G, or the inspector's Ungroup with a group selected, takes the group apart, its children selected (`ungroup`). Each is one patch, one step to undo; what a stack, grid, or frame holds is refused, with why. `web/group.mjs` checks it in headless Chromium on the torture deck's containers.
- **Turning** (PLAN 2.51, ADR-0013). A node that its `transform` turns, scales, or leans (§3.3) is outlined where it is drawn, its handles at its corners as drawn, and a click, a drag, the marquee, and a caret find it there, each point read back through the map its box carries (`Player.boxes`' `transform`). A drag moves a turned node as far as the pointer goes, and one held in a turned container too, the move read back into the container's axes; a handle resizes a node along its own.
  - **The rotate handle**, on an arm above the top edge of the node selected as drawn, turns it about its anchor as the pointer goes round, in whole degrees, or with Shift by 15°, past a half turn and on; the status says the angle. Let go, it is one `choose` of `transform/rotate` (§7.3), by `user`, written where the transform lives, one step to undo; Escape leaves it as it was.
  - **The inspector's angle** (degrees, clockwise) is the same `choose`.
  - **A caret** in a turned text is drawn turned with it, and the keys go by its lines as set.
  - `web/rotate.mjs` checks it in headless Chromium on the torture deck's `transforms` case.
- **Zoom** (PLAN 2.46). The preview zooms in steps from the whole canvas to eight times as close: ⌘+ and ⌘− (Ctrl), about the middle of what it shows, or the zoom's buttons; pinching, or the wheel with ⌘ or Ctrl, zooms about the pointer, and ⌘0 shows the whole canvas again. Zoomed in, the wheel pans, and so do a drag with Space held and the middle button, which select and change nothing. What it shows is a view, `[x, y, w, h]` canvas units, which the worker paints through (`Player.setView`, `DisplayList::viewed`): the frame moved so that the view's corner is its origin, scaled to the same pixels, so the part shown is painted at the size shown, as sharp as the whole, and a shader works out only the pixels shown. A shader's grain is per device pixel, as at any size. The strip's thumbnails and every export see the whole canvas. Every point the pointer names is read through the view, so clicks, drags, handles, the marquee, carets, and drops answer in canvas units as at the whole canvas, and the overlay is drawn through it. ⌘+, ⌘−, and ⌘0 zoom while typing too. Another format shows its whole canvas. `web/zoom.mjs` checks it in headless Chromium on the revenue example.
- **Guides** (PLAN 2.57, ADR-0013). ⌘' (Ctrl+'), the Grid button, or the command draws the theme's grid of the format shown over the canvas, as the engine gives it (`Player.grid`, `scaena_engine::guides::grid`): each column and row as a band, the gutters between them and the margins around them, and each line of the baseline grid, a pitch apart from the top margin (none where the theme sets none, or where more than 2,000 would fill the canvas). Another format draws its own, and the grid follows the deck as it changes. As a node is dragged or resized, a line shows wherever one of its edges, or its middle, meets the edge or middle of another box that draws, or the canvas's, within half a canvas unit, across both (`Player.snap`'s `guides`); never into a slot or among a stack's children, nor for a node drawn turned. Off the grid (Shift), the box goes first the least way that brings one of them onto another's within six pixels at the zoom shown, a move whole, a resize by the edges it moves, then lands in whole canvas units: one `place` as any drop. With ⌘ (Ctrl) held too, it goes where the pointer says. Several dragged together go as the box around them (`Player.together`). `web/guides.mjs` checks it in headless Chromium on the torture deck's containers.
- **Every format at once** (PLAN 2.62, §3.4). Formats, beside the format picker, or the command, shows the state shown in each of the deck's formats side by side under the canvas: a figure for its own canvas, then one for each format it lists, as the picker lists them, all one height, each the shape of its format (`web/src/formats.ts`).
  - **As the canvas shows it.** The worker paints each after each frame of the canvas, by the CPU painter, whatever paints the canvas (`Player.pixelsIn`): the frame the canvas would show in that format, the same bytes. So a cue played, or the deck run, plays in every format, and comes to rest in each with the canvas. While it plays, the figures take a frame at most every 33 ms, so the canvas keeps its own pace. Each format lays a state out once, at rest and in its cue, as the canvas does (§5), and keeps it while the canvas shows another format, until the deck, its theme, or its files change.
  - **Its findings.** Each counts the findings about the state shown that hold in its format, in the color of the worst, as the strip counts a state's (PLAN 2.49): one laying a format out found holds there, and the rest where `formats` says (each finding names the formats it holds in, `""` for the deck's own canvas).
  - **Edit in it.** A click on a figure, or Enter, opens the canvas in its format, as the picker does; the one the canvas shows is pressed. An edit that adds a format to the deck, or takes one out, adds or takes away its figure and its item in the picker.
  - Hidden, it paints nothing. `web/formats.mjs` checks it in headless Chromium on the revenue example, and audits it with axe-core.
- **Rehearsal** (PLAN 2.63, §2.4). Rehearse, or the command, plays the deck as presented, from its first state, in the format the canvas shows (`web/src/rehearse.ts`): each state's cue as it comes, then the state at rest until the presenter goes on, whatever its hold.
  - **Keys.** → ↓ PageDown Space Enter, a click on the canvas, or On go on; ← ↑ PageUp Backspace, or Back, go back; Escape, or End, stops, and so does going on from the last state. A bar over the canvas says which state is shown, how long it has been, and how long the rehearsal has run.
  - **What each state took.** At the end, a table: each state's time (a state shown twice, both times), its cue's span, the hold that time keeps, and its hold now. A state's cue plays, then its hold, so the hold a state keeps is its time less its cue, to a tenth of a second, and none less than nothing. A state not reached keeps its hold.
  - **Keep** makes each state reached hold what it kept: a `set_state` of `hold` for each, one patch by `user`, one step to undo. The deck then plays at the pace rehearsed, holds going on by themselves (§9.2.2). Discard keeps nothing, and nothing the rehearsal does changes the deck until Keep.
  - `web/rehearse.mjs` checks it in headless Chromium on the revenue example, and audits the bar and the table with axe-core.
- **Find and replace** (PLAN 2.47). ⌘F (Ctrl+F) anywhere but the source, or Find, opens a bar above the preview that finds in the deck's texts, in every state, as `scaena find` does (`Player.find`): each text once for each place it is written. Enter, or ↓, goes to the next match and Shift+Enter, or ↑, to the one before: its first state shown, its node selected, and its characters marked on the canvas from the engine's carets. Replace replaces the match shown, then shows the next after what it put in, so a replacement the query still finds is not found again; Replace All replaces every match. Each is one patch of `replace_text`s written where each text lives (`Player.replacing`), by `user`, one step to undo, and the status says how many states it reaches. Match case and Whole words, as asked; Escape closes the bar. `web/find.mjs` checks it in headless Chromium on the revenue example.
- **Findings on the canvas** (PLAN 2.49, ADR-0013). Lint's findings about the state shown, those that hold in the format shown, stand on the canvas (`web/src/marks.ts`): a mark at the top right corner of each node's box, counting the node's findings in the color of the worst, and one at the canvas's top left corner for the state's own and for any about a node it does not show. A document rule's finding about a node, in no state (W410's image without alt text), stands on the node in each state that shows it. Zoomed in, a node out of view has none; a mark steps aside while a drag, a marquee, or a drawing goes.
  - **What holds where** is lint's to say (`Finding.shown`): a finding laying a format out found holds in that format; one laying the deck's own canvas out found holds there, and in a listed format of the same canvas, which lint does not lay out again; validation's and the document rules' hold in every format. Another format shown is linted again.
  - **Opened,** a mark says each finding, its hint, and its fix where lint has one, and selects its node. A finding no fix can make, about what the inspector sets, takes the person there: W410's Describe it opens the inspector on the image, its description in focus (PLAN 2.56). Lint keeps a fix only once laying the state out again with it took the finding away (§7.4). The fix taken is one patch, by `user`: one change in the source, one step to undo. In the source goes to where the source writes the finding.
  - **The strip** counts each state's findings in the format shown, in the color of the worst.
  - `web/findings.mjs` checks it in headless Chromium on the revenue example, and audits it with axe-core.
- **The cue** (PLAN 2.44, ADR-0013). Under the preview, the cue of the state shown is a timeline: its transition, then a bar for each motion, where `inspect --timeline` places it (`web/src/cue.ts`). Dragging a bar changes its motion's delay, and dragging its end changes its duration (`time_motion`); the transition's end sets the state's (`set_state`, `transition/duration`). Times move in whole tens of ms, and the arrow keys move a focused bar or end 10 ms a press, 100 with Shift. A motion on a spring has no end to drag. Add a motion offers the theme's presets for the node selected, as it enters or for emphasis, and for a node that leaves, as it leaves (`apply_preset`, an exit written in the state the node leaves). A press or a drag on the ruler shows the cue at that time, and the canvas's outlines step back until a press on the canvas brings the state to rest. Play plays the cue alone and comes to rest at its end. Each change is one patch, one step to undo. `web/cue.mjs` checks it in headless Chromium on the revenue example.
- **Text in place** (PLAN 2.32, ADR-0013). A double click on a text, or Enter on one selected, puts a caret in it where the engine says the character is, and typing there makes `replace_text` patches (§7.3), by `user`, written where the text lives. With Alt, what is typed is kept to the state shown (`fork`). The status says which states it changes.
  - **The caret** and the selection are drawn from where each character stands as the engine set it (`Player.carets`). An unseen textarea takes the keys, the clipboard, and an input method's composition. Left and right, by a character or a word, are its own, in the text's order. Up, down, Home, and End go by the engine's lines. A click moves the caret, a drag selects, a second click selects a word, and a third a line.
  - **Each change** to the text is one `replace_text`, validated as a patch is but not linted: the state shown is linted after, as after a keystroke in the source, so a text that overflows says so as it is typed. One change is made at a time; what is typed meanwhile goes in the next.
  - **A burst of typing** is one step to undo, from the canvas (⌘Z) or the source, and one change in the bundle's history (`type`). An undo waits for what was typed to be made. The text is then read again where the deck now reads it, as it is whenever the source changes under it, and the caret goes where it changed. A key typed meanwhile goes there too, as a step of its own.
  - **Leaving.** Escape, a click outside the text, or focus elsewhere leaves it, the node still selected. Focus in the inspector or the Assistant tab keeps it, and focus back on the canvas goes on to the text.
  - **A look for characters** (PLAN 2.38). With characters selected, ⌘B (Ctrl+B) makes them bold, or, all bold already, not, by the weight the engine sets each in (`Player.bolding`); and the inspector offers their look instead of the node's (`Player.characterChoices`): a run's role, emphasis, family, weight, italic, and color, each the first character's. ⌘I (Ctrl+I) sets them in italic, or, all italic already, not (`Player.italicizing`, PLAN 2.40), by whether each asks for italic: a family without an italic face sets them upright all the same (W231). Each is one `style_text` patch (§7.3), by `user`, written where the text lives, one step to undo and one change in the bundle's history; × takes the characters' own away. `web/runs.mjs` checks it in headless Chromium on the revenue example.
- **A theme's layouts on the canvas** (PLAN 2.71, ADR-0016). Layout (the button, or the palette's Edit the layout) shows the layout the state shown uses as its slots, each named, on the theme's grid in the format shown, with a handle on each side and corner (`scaena_engine::guides::layout`, `Player.layout`): each slot's box, its cells as the theme writes them, and whether the format shown writes it itself.
  - **A slot dragged** lands on the grid's tracks, its cells kept; **a handle** resizes it onto them. Let go, it is one `theme_edit` (§7.1, ADR-0016) by `user`: on the deck's own canvas, the slot's `col` and `row` in `layouts.L.slots`; in another format, the slot in `layouts.L.formats.F.slots`, the layout's others for it carried along, so the deck's own canvas keeps it. Every state that uses the layout shows it moved, refused as any theme edit is where the deck would not validate in it, and ⌘Z writes the theme back. Escape through a drag leaves the slot as it was; Escape again leaves the layout.
  - **New layout from this one…** asks for a name, copies the layout under it (one `theme_edit`), and the state shown takes it (a `set_state` of `layout` with `fork`), so it is the one edited.
  - `web/layouts.mjs` checks it in headless Chromium on the revenue example, in both of its formats, with axe-core.
  - **Links** (PLAN 2.70). With characters selected in a text typed in, ⌘K (Ctrl+K) asks where they go, in a field over them: a web address (`https://`, `http://`, `mailto:`) or a state's id (with `#` or without). Enter links them, as one `style_text` of `link` (§7.3), by `user`, one step to undo; nothing takes the link away, and words that are neither make nothing and say so. Outside a text, ⌘K is the palette's. `web/links.mjs` checks it in headless Chromium on the torture deck's `links` case, with the player following each kind.
  - **Lists** (PLAN 2.69, ADR-0018). In a text typed in, Enter at an item's end makes another like it (a `replace_text` keeps the `list` in step), and Enter in an empty item ends the list there; Tab and Shift+Tab move the items the selection touches a level in and out. ⌘⇧8 (Ctrl+Shift+8) bullets the paragraphs the selection touches, and ⌘⇧7 numbers them; where they are all of that kind already, they leave the list. With a text selected and not typed in, the two keys, and the palette's Bulleted list and Numbered list, act on all its paragraphs. The engine's carets say each paragraph's item (`Player.carets`' `items`), which is what the keys read. Each is one `list` patch (§7.3), by `user`, written where the list lives, one step to undo. `web/lists.mjs` checks it in headless Chromium on the torture deck's `lists` case, with the player's reading.
  - `web/typing.mjs` checks it in headless Chromium on the revenue example and the torture deck's paragraph and runs. On the revenue example, a key takes a median of 83 ms from the key to the source that holds it, the state shown painted and linted, on the CPU.
- **A look copied and pasted** (PLAN 2.58, ADR-0013). ⌥⌘C (Ctrl+Alt+C), or Copy the look, copies the look of the node selected as the state shown shows it (`Player.look`, as `inspect --look` says it): a text's role and style, a shape's fill, stroke, and corners, an image's corners, a shader's preset and palette, a chart's labels, a stack's or a grid's gap, each the value shown or the theme's. ⌥⌘V (Ctrl+Alt+V), or Paste the look, pastes it on each node selected that takes it (`Player.putting`, as `--onto` says it): one patch of `choose`s (§7.3), by `user`, each written where that node's own value lives, as the inspector writes one; one step to undo. Where the look's value is the theme's, the node's own is taken away, so the theme's shows; a value written out goes in the overrides. A node of another type takes what its look shares with it, an image a shape's corners. The status says which nodes looked so already, and which take none of it and why. The keys go by their place on the keyboard (`code`), since Option makes ⌥C a character of its own. A look stays copied until another is, while the deck is open. `web/looks.mjs` checks it in headless Chromium on the revenue example and the torture deck.
- **The inspector's edits** (PLAN 2.33, ADR-0013). The node selected shows above the state's nodes with what the theme offers for it (`Player.choices`, as `inspect --choices`): each property, the value the state shows, and where that value lives.
  - **Choosing.** A role, a family, a color, a preset, a word, or a number chosen there is one `choose` patch (§7.3), by `user`, written where the value lives. It comes into the source as one change, one step to undo, and the status says which states it changes. "Only in" keeps it to the state shown (`fork`).
  - **Written out.** A color or a length written out, and any text size, goes in the deck's `overrides` and shows as an override, in every state. A theme name chosen then is written there too, since the overrides win; the × beside a value takes it away where it lives, the override first. What cannot be kept to one state is refused, and says why.
  - **The status** keeps what a gesture on the canvas or a choice said, and what lint finds after it, until the source is edited by hand.
  - **The state** (PLAN 2.36). With no node selected, the inspector edits the state shown (`Player.stateChoices`, as `inspect --state-choices`): its layout, from the theme's layouts with a slot for each node placed in one; its transition's duration, ease, spring, and match, which say "cut" where it sets none; its hold, in seconds; and its notes. Each is one `set_state` patch (§7.3), by `user`. A layout is written where it lives, so the states that take it from there change with it, and the status says how many; "layout only in" keeps it to the state shown. The × takes a value away: a transition with nothing left cuts.
  - **A chart's data** (PLAN 2.41). With a chart selected, the inspector offers what it reads: its source, from the deck's; each channel's field, from the columns of its data that the channel can read; the type it reads `x` and `y` as; and its key. A table offers its source, its key, and its header. Another source chosen points again, in the same patch, what that source cannot serve (§7.3). `web/charts.mjs` checks it in headless Chromium on the revenue example, with a chart inserted from its data.
  - **An image** (PLAN 2.45). With an image selected, the inspector offers its fit, its focal point and its crop, each as fractions of the image, and its radius. Pick, then a press on the image, sets its focal point to the point of the image drawn under the pointer (`Player.focalAt`), in fractions of the part its crop keeps, so that point stays in view as the box changes. Escape leaves it as it was. A crop is kept inside the image, and keeps some of it. An image file dropped on an image node joins the bundle as one dropped on the source does, named by its SHA-256 under `assets/`, and the image shows it: its `src` is chosen, written where it lives. Each is one `choose` patch (§7.3), by `user`. `web/image.mjs` checks it in headless Chromium on the torture deck.
  - **An image's crop on the canvas** (PLAN 2.74). With an image selected, a handle stands inside each side of the part that shows, and its focal point is a handle at the point of the box it lines up with, as CSS `object-position` lines it up (`Scene::framing`, `Player.framing`: where the whole image is drawn at the scale its crop and fit draw it, the part that shows, its crop, and its focal point). A crop handle dragged crops the image from that side, kept inside the image and to at least a twentieth of it each way: as it goes, the whole image is outlined, what the crop cuts away shaded, and the part it keeps framed. The focal point dragged moves within the box, and that point of the crop lines up with the pointer's. Each, let go, is one `choose` of `crop` or `focal` (§7.3), by `user`, written where it lives, or kept to the state shown with Alt; a crop of the whole image takes `crop` away. Escape through a drag leaves it as it was. `web/crop.mjs` checks it in headless Chromium on the torture deck's `images` case.
  - **What a reader hears** (PLAN 2.56, §3.12). Every node offers its description (`alt`), which a text reads instead of its words, and its part in the story (`semantic`), each a `choose`. Under the choices, how the state shown reads, as the player's live region and the PDF's tags say it (`Player.reading`): what the node selected reads as (a heading or a text by its words, a figure or a table by its description) or why it is not read (decoration, a container, a shape or shader not described); with no node selected, each part of the state in reading order, each selecting its node. `web/reader.mjs` checks it on the trails example.
  - `web/inspector.mjs` checks it in headless Chromium on the revenue example, and `web/a11y.mjs` audits it.
- **Adding and taking away** (PLAN 2.34, ADR-0013). The Insert menu offers what the theme and the bundle name, as `inspect --inserts` says (`Player.inserts`): a text in each role, each shape, each image in the bundle, a chart and a table of each data source (PLAN 2.41), and each shader preset.
  - **Insert** puts what is chosen where the pointer last pressed the canvas, or in its middle, snapped to the theme's grid as a drop snaps; a text or an image fills the template's slot there if nothing fills it, and a shader fills the canvas, under the rest. It enters in the state shown, selected: one `add_node` and `place` patch (`Player.inserting`).
  - **Drawing** (PLAN 2.48). T, R, O, L, or A on the canvas arms it to draw a text (`body`'s, where the theme has one), a rectangle, an ellipse, a line, or an arrow; the key again, or Escape, stops. A drag draws it over the grid's cells it covers, each edge on the nearest track's as a resize snaps, or, with Shift, where it went (a `rect`, W301), the overlay showing where it lands as it goes (`Player.drawing`). A line or an arrow runs the way the drag went, straight across where it was within 14° of level or upright. A click places it as Insert does. It enters in the state shown, selected; a text takes the caret, its words selected, so what is typed takes their place.
  - **Delete** (or Backspace) takes the node selected, with what it holds, out of the state shown and the states that track it from there (`hide_node`); a node no state shows then goes from the deck. Shift+Delete takes it out of the deck (`remove_node`). ⌘D (Ctrl+D) adds a copy beside it, with copies of what it holds, clear of the rest where there is room, selected, under the next id free (`Player.duplicating`, `Player.deleting`).
  - **Each** is one patch, by `user`, linted as an agent's is: one change in the source and one step to undo. The status says what was made, and where.
  - `web/insert.mjs` and `web/draw.mjs` check it in headless Chromium on the revenue example.
- **A theme** (PLAN 2.39, ADR-0013). Beside the format picker, the theme picker offers the theme files the bundle holds, the deck's own chosen, and the themes that ship (`Player.themes`). One chosen re-themes the deck as `scaena theme --apply` does (`Player.retheme`, `scaena_ops::theme::theming`): a theme that ships is written into the bundle with the fonts it names that the bundle lacks, under the next free name where the bundle holds other bytes there. A theme the deck would not validate in is refused, the status says why, and the picker goes back to the deck's theme. Else it is one change by `user`, one step to undo, and the status says what lint finds in it that it did not before, and what it no longer finds. The session draws in the theme its deck names, so one named in an edit of the source is drawn in too. `web/theme.mjs` checks it in headless Chromium on the revenue example and the torture deck.
- **The clipboard** (PLAN 2.37, ADR-0013). ⌘C (Ctrl+C) puts the node selected, with what it holds, on the clipboard as a clip: JSON, as `application/x-scaena+json` and as text (`scaena_ops::clipboard`, `Player.copying`).
  - **A clip** is `kind: "scaena/clip"` and its `version`; the `theme` it was copied in; the `node` copied; its box, `share`, as shares of the canvas's width and height; the others copied with it (`more`, each `node` and its `share`, PLAN 2.42), a clip of several being version 2 and a clip of one version 1; its `nodes`, each node copied and each it holds, each its type and props as the state shown resolves them; the deck's `overrides` for them; the `data` sources they read, as the deck declares them; for each text role they name beyond the six every theme has, the one of those six nearest it in size where it was copied (`roles`); and the bundle's `files` they read (images, and the sources' files), in base64. A shader set by a preset names the preset's kind too.
  - **⌘V** (Ctrl+V) pastes where the pointer last pressed the canvas, or in its middle, as Insert places a node: the copy of the node copied takes its box, as a share of this canvas, on the grid about the point, or a text or an image the template's empty slot there. Each node comes in under an id new to the deck, held by the copy of what held it, entering in the state shown, selected: one patch, by `user`, of the sources it brings, `add_node` for each, their overrides, then `place` (`Player.pasting`). A file the bundle holds with other bytes comes in under a name of its own (`data/q3-2.csv`), and a source the deck declares otherwise too (`q3-2`); one it holds the same is shared.
  - **What the theme lacks** (a color, a preset, a motion) is taken out of the copy, with the finding that says so, which the status shows; the paste is not refused for it. A text keeps a role: one the theme lacks gives way to its stand-in in `roles`, else `body`, which every theme has.
  - **⌘X** (Ctrl+X) copies, then takes the node out as Delete does. Text that is not a clip pastes as a text in the theme's `body` role. A text typed in keeps the clipboard for its own characters.
  - **In the page,** a copy is answered at once, so the clip of the node selected is asked for when it is selected; and since Chromium and WebKit fire a copy, a cut, or a paste from the keys where nothing is editable only when the page cancels `beforecopy`, `beforecut`, or `beforepaste`, the canvas cancels each when it takes it. `web/clipboard.mjs` checks it in headless Chromium, through the browser's own clipboard too, on the revenue example and the torture deck.
- **The state strip** (PLAN 2.35, ADR-0013). Under the preview, the deck's states in order, each a thumbnail the engine painted at rest, in the format shown, with its cue's length. A thumbnail is painted again only when its state's drawing changed (`Player.digest`, its display list's digest), once every state is laid out after an edit, by the CPU painter, a state at a time between what else the worker is asked.
  - **Showing.** A click on a state shows it; the arrow keys, Home, and End move among them.
  - **Adding.** + Step adds a state after the state shown, in its slide, that tracks from it: it shows what that state shows until it is changed, and is named after it (`revenue-2`). + Slide adds an empty slide (`mode: absolute`) in the shown state's layout, after the last step of its slide, named `slide`, `slide-2`, …. Each joins the beat of the state it follows (`Player.addingState`, `add_state`).
  - **Moving, renaming, removing.** A state dragged to another place, or moved with Alt and an arrow key, moves there (`move_state`). F2, or a double click on its name, renames it (`rename_state`), and Delete removes it (`remove_state`).
  - **Each** is one patch by `user`, one change in the source and one step to undo, and the state it leaves shown is the one it is about. What the deck refuses, the status says why: a state another builds on stays.
  - `web/strip.mjs` checks it in headless Chromium on the revenue example.
- **Commands by name** (PLAN 2.53, ADR-0013). ⌘K (Ctrl+K), anywhere, or Commands…, opens a palette of every command the editor has, each with its key (`web/src/commands.ts`). It lists those that apply now: Duplicate only with a node selected, Ungroup only with a group, Align with two. The words typed narrow it, each word in a command's name, in any order; one whose name starts with the first word comes first. Enter runs the command, and Escape, or ⌘K again, closes the palette, focus back where it was.
  - **A command** is what its key or its button does, and nothing else: each edit is that gesture's one patch, by `user`, one step to undo. On the canvas: the nodes selected typed in, duplicated, copied, cut, grouped, ordered, aligned, spread, or deleted; what the deck offers inserted; a text or a shape drawn; the preview zoomed. On the strip: a state added, renamed, or deleted, and the next shown. In the layers: a node hidden or shown, or renamed. For the deck: a theme, find, undo and redo, save, download, play, a new deck, and a tab shown.
  - **The words typed,** last, are asked of the assistant (§9.2.9). The Assistant tab asks them with what is selected; where it has no key yet, it holds them in its question box, says what is missing, and focuses the key's field.
  - **A right click** on a node, on the canvas or in the layers, selects it as a click does, unless it is among those selected already, and offers what is done to a node. Where nothing is, it selects nothing and offers paste, insert, draw, and zoom: what is pasted or inserted lands there. On a state in the strip, it shows the state and offers to add one after it, rename it, or delete it. On a layer, it offers to show or hide it in the state shown, and to rename it. The menu key and Shift+F10 offer the same from the keyboard, beside what is selected. The arrow keys, Home, and End move in a menu; Enter runs an item; Escape, Tab, or a press elsewhere closes it.
  - `web/commands.mjs` checks it in headless Chromium on the revenue example, and audits the palette and a menu with axe-core.
- **The canvas by keys alone** (PLAN 2.75): every gesture has a key, §9.2.10.
- **The keys** (PLAN 2.65). `?`, outside a field and the source, the Keys button, or Show the keys in the palette, shows every key the editor answers, by what it does: select, edit, arrange, move and resize, type, draw and insert, see, the states, the layers, the cue, find and replace, the data, annotating a chart, rehearse, the source, and the editor.
  - Each command's key comes from the commands' own list, the one the palette names, whether or not the command applies now.
  - A key no command runs by name is declared beside the code that answers it (`canvasKeys`, `typingKeys`, `stripKeys`, `layerKeys`, `cueKeys`, `findKeys`, `dataKeys`, `rehearsalKeys`): a gesture and the keys held with it, the keys that move what is selected, a panel's own. The source's keys for its findings come from its keymap.
  - On a Mac the keys read ⌘, ⌥, and ⇧; elsewhere Ctrl+, Alt+, and Shift+. `?` again, or Escape, closes the sheet, focus back where it was.
  - `web/keys.mjs` checks it in headless Chromium, audits it with axe-core, and presses keys as the sheet names them to see that they do what it says.
- **A chart annotated from its marks** (PLAN 2.67, ADR-0013). With a chart selected, a click on one of its marks picks it, outlined (`Player.markAt`). A right click on a mark selects its chart, as a right click selects what it is on, picks the mark, and offers what may be made of it (§3.7):
  - **Highlight** the mark, at its category and series, or its whole series. Where highlights pick the mark out, **take the highlight off** takes each of them away.
  - **Call it out.** A field over the mark asks for the words. Enter, or leaving the field, writes the callout on the mark: at its category and series, or at its category and value where those do not tell it from another. Escape writes nothing.
  - **Rule its value**, a `y` rule there.
  - **Band from it to another.** The next mark clicked ends the band: across the two marks' categories, or from one's value to the other's where they share a category. Escape stops it.
  - A donut has no axes, so a slice offers its highlight alone.
  - **An annotation** of the chart selected is selected by a click on what draws it: its text, its rule, or its band (`Player.noteAt`). It is outlined.
    - A callout dragged stands where it is let go (`Player.calloutAt`). On a mark, it stands on that mark. Elsewhere, it stands at the nearest category and at the value there, rounded to a tenth of the step five ticks would take across the value axis.
    - A double click asks for its words again, with what it says. Delete takes it away.
  - Escape lets go of the mark or the annotation, and then of the chart.
  - **Each** is one `annotate` patch (§7.3) by `user`: one change in the source, one step to undo. It is written where the chart's annotations live. With "Only in" checked in the inspector, or Alt held through a drag, it is kept to the state shown (`fork`).
  - `web/annotate.mjs` checks it in headless Chromium on the revenue example. It audits a mark picked, with its menu open, with axe-core.
- **A shape's points and corners** (PLAN 2.68, ADR-0013). With a line, an arrow, or a polygon selected, the canvas draws a handle on each of its points and a smaller one at each edge's middle; a line or an arrow that gives no points shows the two it draws (§3.3). A rect selected has a handle inside its top-left corner, as far in as its radius, and never on the corner's resize handle. The engine says what they stand on (`Scene::outline`, `Player.outline`): the shape's kind, its box at rest with the map its `transform` draws it through, its points as fractions of the box, the fewest its kind keeps, and a rect's radius as drawn with each of the theme's radius steps as that rect would draw it.
  - **A point dragged** moves within the box, to a hundredth of it, read back through the shape's transform. Let go where it was, it is picked, and Delete takes it away; a line or an arrow keeps two points and a polygon three, and the status says so.
  - **An edge's middle pressed** adds a point there, between its two; a drag from it places the new point where it is let go.
  - **The corner handle** dragged along its diagonal rounds the rect to the theme's radius step nearest the radius it reaches, `radius.N`, and the status names the step.
  - **Each** is one `choose` (§7.3) by `user` of `points` or `radius`, written where it lives: one change in the source, one step to undo. With Alt held through the drag, or "Only in" checked in the inspector, it is kept to the state shown (`fork`). Escape leaves the shape as it was, then lets go of a point picked.
  - `web/points.mjs` checks it in headless Chromium on the torture deck's `shapes` case, with axe-core.

#### 9.2.5 The panels

Tabs beside the inspector show the state's layers, and what the deck is made of besides its nodes: its data, its theme, the bundle's files, and the versions its history keeps.

- **The layers** (PLAN 2.50, ADR-0013). A tab beside the inspector lists the state shown's nodes, topmost first (a stack's in the order it lays them out), nested as their containers and groups hold them, as `inspect --layers` says (`Player.layers`, `web/src/layers.ts`). Those the state does not show are listed dimmed: those that leave in it, those another state of its slide shows, and those no state shows, a node hidden where it entered among them.
  - **Selecting.** A click on a name selects the node on the canvas, one under the rest too, and a node selected on the canvas is marked in the list.
  - **Showing and hiding.** The eye hides a node in the state shown, with what it holds (`hide_node`), or shows one hidden there (`show_node`). What the deck refuses, a node whose slot the state's layout lacks, the status says why.
  - **Renaming.** A double click on a name, or F2, renames the node everywhere the deck names it (`rename_node`).
  - **Reordering.** A node shown, dragged through the list, drops before or after the one under the pointer, by which half of it the pointer is over; Alt with ↑ or ↓ moves the one focused a place. It goes over or under it by `z`, or, in a stack, before or after it in its order (`Player.arranging`, as `inspect --arrange --before`).
  - **Into another container.** Dropped beside a node another container holds, or the canvas, a node goes there with it; dropped on a container's middle, into it, first among what it holds (`--into`). It is placed as that container places what it holds, where it stands where it can (§7.1, `--before`). Alt with ← takes the one focused out of its container, listed just before it; Alt with → puts it into the container listed just before it, last among what that holds. A container does not drop into what it holds.
  - **Each** is one patch, by `user`: one change in the source, one step to undo.
  - `web/layers.mjs` checks it in headless Chromium on the revenue example, and audits it with axe-core.
- **The data** (PLAN 2.55, ADR-0014). A tab beside the layers shows a data source as a table, as `scaena data` shows it (§3.10, `web/src/data.ts`): a picker of the deck's sources (`Player.dataSources`), the file the one shown is, or that its rows are written inline, and how many rows it has; then a column for each of its columns, its type under its name, and a row for each of its rows, its index first, each cell as written.
  - **Cells.** Each is a field. Enter, or leaving it changed, sets it (`data_edit` `set`), Enter going on to the cell below (Shift+Enter above); Escape puts back what it held, and ↑ and ↓ move between rows. A value its column does not read is refused, with why, as is one that would leave the deck invalid (E103 for a chart's marks no longer told apart): the status says so, and the cell holds what it held.
  - **What does not read.** A cell its column does not read is marked, with why, and listed under the table: a click there goes to it. The deck does not validate until it reads, and the edit that sets it is what fixes it, so a data edit needs a source that compiles, not a deck that validates. A bundle with such a cell opens all the same, each state at an instant until the deck can be laid out: the editor says why, and the panel mends it.
  - **Rows.** + Row adds a row after the row last focused, else at the end; − Row takes that row away.
  - **Each** change is one `data_edit` by `user`: one write of the file that keeps every other byte, or one patch of rows written inline. Every chart and table that reads the source is drawn again, and the bundle is changed, to save. The page's history records the file's bytes with the change (§8.1).
  - **Undo.** Undo and Redo, and ⌘Z and ⇧⌘Z in a field as it was, undo the last change to the data and make it again: a file as the page kept each version of it (`Player.dataUndo`, `dataRedo`), and rows written inline as the source's own undo does. The editor undoes its own changes (§8.2): the user's and its assistant's, as the source's undo does, and never a file dropped on the page, which leaves nothing before it to undo. An undo is a change by `user` (`undo data_edit q3: revenue of row 3`).
  - **Rows and what they draw** (PLAN 2.64, ADR-0013). The row of the cell focused is chosen: the canvas outlines what it draws in the state shown, in the format shown, each chart mark and table row made from it, and the line under the table says which (`Player.marksOf`). A click on a chart's mark, or a table's row, with the tab shown, chooses the rows it was made from, the first in view; a double click opens the tab on them (`Player.markAt`). A mark is made from rows through its node's `dataTransform`: a row kept, sorted, or derived draws as itself, an aggregated or pivoted row draws its group, and a folded row the row it unfolds (§3.10). A point is on a bar or a slice inside it, on a dot within its radius, on an area in its column, and on a line near a point it runs through. `web/rows.mjs` checks it in headless Chromium on the revenue example, and audits it with axe-core.
  - **Quote** (PLAN 2.72, ADR-0019). With characters selected in a text typed in, a cell focused, then Quote, makes them quote it: one `style_text` of `quote` (§7.3), by `user`, one step to undo, the figure set from the data in the same patch. Its row is found by a column of text whose value no other row has, else by those columns together, else by its index. Focus in the Data tab keeps the text typed in and its characters selected, as the inspector does. A data edit that sets figures again is an edit of the source too, which says so. `web/quotes.mjs` checks it in headless Chromium on the revenue example.
  - The assistant edits data by the same tool (`data_edit`, §9.2.9). `web/data.mjs` checks the panel in headless Chromium on the revenue example with a cell that does not read: the file it saves is the one `scaena data` writes for the same edits, byte for byte. It audits the panel with axe-core.
- **The theme** (PLAN 2.61, ADR-0016). A tab beside the data shows the theme the deck names, and where it lives: a file the bundle holds, or written in the deck (`Player.themeText`, `web/src/theme-panel.ts`).
  - **Its colors.** Each is its swatch and its value as the theme writes it, with the color roles that name it. The swatch picks a color; the value takes any the theme reads (`#rrggbb[aa]`, `oklch(…)`).
  - **Its type roles.** Each is its family, one of the theme's, and its size, weight, leading, and tracking.
  - **Its spacing:** the grid's gutter, margins, and baseline, and the space unit.
  - **Each change** is one `theme_edit` by `user` (§3.6, `Player.themeEdit`): every node that names what changed is drawn in it, in every state and format. It is refused, with why, where the deck would not validate in the theme it leaves (a value the theme's schema refuses is E106), and the tab shows the theme as it was.
  - **One step to undo.** The source does not change, so it takes the edit as a change that carries the theme's text before and after: ⌘Z writes the theme back, and ⇧⌘Z writes it again (`Player.writeFiles`), as with a restored version. The next save records the edit (§8.1).
  - **It follows the canvas.** It shows the theme frames are drawn in, so an undo, a theme chosen above the canvas, or the assistant's edit shows here as it does there. The assistant edits the theme by the same tool (§9.2.9).
  - `web/theme-edit.mjs` checks the tab in headless Chromium on the revenue example, saved with a history, and audits it with axe-core.
- **The bundle's files** (PLAN 2.59, ADR-0013). A tab beside the theme lists the bundle's images, fonts, and data, as `scaena files` lists them (§7.1, `Player.bundleFiles`, `web/src/files.ts`): how many of each, and how many nothing names; then each file under its kind, by its path, with its size, what names it, and each node drawn from it, with the states that show it so.
  - **A node** a file draws is a button. It puts the cursor in the first state that shows it so, as the strip does, and selects it there.
  - **An image** is dragged from the list. Over an image node on the canvas, the node shows it: one `choose` of `src`, written where it lives (§7.3). Anywhere else on the canvas, it is inserted there, as Insert inserts it. On the source, its path goes where it is dropped, quoted. Insert, beside it, inserts it from the keyboard where the pointer last pressed the canvas. Each is one patch by `user`, one step to undo. A PNG or a JPEG is dragged and inserted, since those are what the painters draw (§3.3).
  - **A file nothing names** says so, with Remove. Remove takes it out of the bundle (`Player.removeFile`), and the deck draws and reads the same; one something names is refused, with what names it. Undo and Redo, and ⌘Z and ⇧⌘Z in the panel, put it back and take it out again: they undo the last change to the bundle's files, the Data panel's edits of a file among them, from the bytes the page keeps of each. The next save takes it out where the bundle is kept. The deck does not change, so the page's history records nothing (§7.1).
  - `web/files.mjs` checks the tab in headless Chromium, on the torture deck with an image nothing names.
- **Versions** (PLAN 2.60, §8.2). A tab beside the files lists the versions a bundle's history keeps, as `scaena history` lists them (`web/src/versions.ts`), the newest first: each its number, what it did, by whom, and when. The history's own module reads them (`scaena-history`, PLAN 2.9), loaded the first time the tab is shown; the engine's module keeps no CRDT (SPEC §15). The versions are what the history holds: an edit is one once a save records it, and the tab lists it after the save. A bundle that keeps no history says how to begin one.
  - **A version chosen** shows the deck as it was, read only: a state of it at rest, picked from its own states, drawn by a session of its own on the bundle as it is with the version's deck and data files (`Player.viewVersion`, `versionPng`). Beside it, what changed from it to the deck as it is now, or to another version chosen, as `scaena history --diff` says it (`Player.compareVersions`). Each edit compares it again.
  - **Restore** makes the version the deck again, with the data files its sources named then, as one change by `user` (`Player.restoreVersion`). It is refused, as a patch is, where the deck would not validate in the bundle as it is (a file it names taken out since, E102), and the status says why. A source that does not compile is no bar: a version is a way back from one.
  - **One step to undo.** The source takes the restored deck as one change, which carries the data files the restore wrote. ⌘Z puts the deck back and writes each file back as it was; ⇧⌘Z restores them again (`Player.writeFiles`). The next save records the restore (`history --restore <id>`).
  - `web/versions.mjs` checks the tab in headless Chromium on the revenue example, saved with a history and changed by `scaena` since, and audits it with axe-core.

#### 9.2.6 Storage

Storage (PLAN 2.4): a bundle opens from a URL, a folder on disk (File System Access), a `.scaena` file, or the browser's own storage, the origin-private file system (OPFS), which keeps bundles under `bundles/`. `?bundle=opfs:NAME` opens one kept there, in the player and the editor alike.

- **The page holds the whole bundle**: the deck and theme, then every other file by its path in the bundle. The engine is built from the fonts and images the deck names, and again on the first frame after it names others, so a file can join at any time. From a URL, the page reads the files the deck names, and every file a saved bundle's manifest lists.
- **Save** (⌘S) writes the bundle as `scaena save` does (§3.1), canonical, its files named by their content, with a manifest, where it is kept: the folder it was opened from, or its place in the browser's storage. A bundle from a URL goes into the browser's storage under its name; a `.scaena` file is copied there when it is opened. The files go first, the deck and its manifest last, so the deck never names a file not yet written; then the files the save renamed go. A save keeps fonts whole, so an edit can still draw any character. The editor goes on from the save: the paths the save renamed are renamed in the source, and nothing else in it changes. A source that does not compile, or a deck that does not validate, is not saved.
- **Download** gives the bundle as a `.scaena` zip with its fonts subset to what the deck can draw, the bytes `scaena save` writes. The subsetter is a WASM module of its own (`scaena-subset`, 244 KB gzipped), loaded the first time: the engine's module leaves it out to stay within §15.
- **Export** (PLAN 2.54): Export…, the palette, or a state's menu in the strip makes in the worker what `scaena export` writes for the bundle (§10), and downloads it. Nothing is sent anywhere. A source that does not compile, or a deck that does not validate, is not exported.
  - **A PNG** of the state shown at rest, in the format shown, at the width asked (the canvas's to begin with), the height keeping its aspect, painted by the CPU painter. It is within §13.5 of `export --format png --size` of that state: the painter runs on WASM SIMD in the browser (ADR-0004 finding 15).
  - **A PDF** of the deck, each slide at its last state: the bytes `export --format pdf` writes. The engine's module lays the pages out as the CLI does (`Player.pdfLaidOut`) and hands them over, each page's frame with the fonts and images it names. The PDF writer (krilla) is a WASM module of its own (`scaena-pdf`, 807 KB gzipped), loaded the first time a PDF is asked for: in the engine's module it would take it past §15's 3.0 MB.
  - **One HTML file** that plays the deck offline: the bytes `export --format html` writes, its fonts subset by the subsetter's module. It is the single-file page (§9.2.8) filled in with the bundle. The build puts that page beside the editor, and `scaena serve` serves the one it carries for `export --format html`.
- **A file dropped** on the source joins the bundle where its kind goes: an image under `assets/`, named by its SHA-256 as a save names it, a font under `fonts/`, and a data file under `data/`. Its path, quoted, goes where it was dropped. A `.scaena` file dropped opens instead.
- **A file dropped on the canvas** (PLAN 2.76) joins the bundle as one dropped on the source does. A picture, a PNG or a JPEG, takes the place of the image node it is dropped on, and anywhere else is inserted there, as Insert inserts it. A CSV or JSON file becomes a data source, declared as `data_attach` declares it (§7.2): under an id made from its file's name and new to the deck's sources, each column typed as narrowly as its values allow (`Player.attaching`). That is one patch; a chart of it, as Insert offers one, is inserted where it was dropped, another. The same file dropped again is the source that reads it, and a chart of it is the only change; a file with other rows under a name the bundle holds goes beside it with a number after its name (`data/visits-2.csv`, `Player.placing`), so no source reads other rows than it did. A file that does not read as rows, or would leave the deck invalid, is refused with why. Anything else is not taken.
- **History** (PLAN 2.9). A bundle that keeps a history (§8) has each save recorded in it, by the CRDT as a WASM module of its own (`scaena-history`, 1.00 MB gzipped). The page loads it the first time it saves such a bundle, as it loads the subsetter to download one: in the engine's module, the CRDT would add a third again (§15). After what the history held, a save records, each change stamped when it was made:
  - `deck.json` as the bundle held it, by `fs`, if it was edited outside Scaena since the history last recorded it;
  - each edit the assistant made since the bundle was opened or saved, by `agent:` and the model's name, with what its tool did and the nodes and states it renamed, after the user's edits until then, by `user` (`edit`);
  - each gesture on the canvas, by `user`: a `place` patch, and a run of typing in place (`type`), its keystrokes one change while nothing else is made between them;
  - then the deck as saved, by `user` (`save`): the rest of the user's edits, and the files the save renamed.

  The status says the save was recorded. The history then holds the deck as saved, so the next command that records takes in nothing by `fs`. A download records as a save does, in the copy it gives. A bundle that keeps no history saves without the module and starts none, until the Versions tab's Keep a history: that save begins one as `scaena save --history` does, the deck as saved and the files it is drawn from its first version, `user`'s `history begins`, and each save after records as above (PLAN 2.87).
- A folder opened before is kept by name across reloads (`?bundle=folder:NAME`): the browser keeps its handle, and asks again for leave to write, which takes a click.
- **New** (PLAN 2.12) starts a deck as `deck_create` makes one (§7.2): a title and one of the themes that ship (Dusk, Daybreak, Ember, §3.6), the fonts it names, and one state with nothing on it, on a 1920 × 1080 canvas. The build carries the themes and their fonts beside the pages, and the page fetches them only to make a deck; a single file carries neither. The deck is kept nowhere, named for its title, until its first save puts it in the browser's storage under that name.
- **Save as** (PLAN 2.12) saves the bundle as Save does, somewhere new, and keeps it there: a folder on disk where the browser can open one, kept by name as an opened folder is, or the browser's storage under another name (`name-2`, … where that is taken). A served bundle saves to its folder alone.

#### 9.2.7 Served

**Served** (PLAN 2.11, ADR-0012): `scaena serve` (§7.1) gives the player and the editor a bundle's folder on disk, at `/bundle/`, with `?serve` in their addresses.

- **This machine only.** It listens on 127.0.0.1 and answers only a request that names `localhost` or `127.0.0.1` at its port, so another site's page cannot reach it through a name of its own. A write (`PUT`, `DELETE`) from any other origin is refused, and none goes outside the folder: no `..`, no name that starts with a dot, no link that leads out. Each file is written whole.
- **Each change shows as it is made.** The server scans the folder every 150 ms and announces each change at `/scaena/events` (server-sent events), once two scans agree. A changed `deck.scn` is compiled first, as `scaena compile` does. The player reads the bundle again, and shows it at the state it was on. A `deck.scn` that does not compile is said at its line over the slide, and in the terminal, until it does; the deck stays as it was.
- **The editor** opens the folder's `deck.scn` as its source, where it keeps one, rather than the deck decompiled. Its save writes the bundle back to the folder as `scaena save` does, in any browser, then `deck.scn` as the editor shows it. A change on disk comes into an editor that has nothing of its own not saved; over such changes, it is offered.
- **A page's own saves** carry its id (`X-Scaena-Client`), so it does not hear them back; every other page reads the bundle again. `web/live.mjs` checks it in headless Chromium.

#### 9.2.8 The single file

Export: **single-file HTML** (PLAN 2.5, §10): `scaena export --format html` writes the player, the engine, and the bundle as one file that plays from a disk, or a USB stick, with no network.

- **The page** is the player's (`web/standalone.html`): the same controls, holds, presenter view, and fullscreen. `just web` builds it with its code, its styles, and the engine inside it: the player's module alone, without the editor's operations, 2.15 MB gzipped (§15), in base64. A `scaena` built after it carries it.
- **The export fills it in** with the deck's title and language, every file the player reads (each gzipped, in base64, by its path in the bundle: what `scaena save` writes, fonts subset to what the deck draws, without the manifest or the history), the states it plays, and how each reads (§3.12).
- **A file's page starts no module worker, and no worker from its own address.** The page compiles the engine and hands it to the worker, which starts from the page's own code as a classic script. Nothing the page keeps goes into the browser's storage.
- **No network.** The page's content security policy lets nothing load, and nothing in it is anywhere else. Opened from its address on disk with the network off, the torture deck's file shows every golden frame within §13.5 of the goldens by the CPU painter, and WebGPU paints a file too. The engine it carries paints each frame byte for byte as the web player's engine does (`web/standalone.mjs`).

#### 9.2.9 The assistant

**Assistant** (BYOK, §11; PLAN 2.6), in the editor's Assistant tab, with the user's own key.

- **Who answers.** Anthropic's Messages API (with its header for a browser's direct calls; the system prompt cached), OpenAI's Chat Completions (or a server that speaks it, at an address the user gives), or Gemini's generateContent. The page lists the models the key can use and names none itself.
- **Keys** go from the page to their provider and nowhere else, and never into a bundle. A key is kept for the tab alone by default. Kept on the device, when the user asks, it is encrypted (AES-GCM) with a key the browser keeps in IndexedDB and never hands out; the panel says that a script the page runs could still use it. Forget key forgets it.
- **Its tools are the MCP server's (§7.2)**, on the bundle the page holds: the same names and arguments, less `bundle`, `out`, and `painter`. They are `deck_read`, `deck_patch`, `deck_find`, `deck_lint`, `deck_inspect`, `deck_diff`, `deck_render` (the CPU painter's frame, sent to the model as an image), `spine_read`, `spine_update`, `data_attach`, which declares a data file the bundle holds already, as one dropped on the page does, `data_edit`, which edits a source's rows as the Data panel does (§9.2.5), and `theme_edit`, which edits the theme the deck names as the Theme tab does, so a look asked of the whole deck is an edit of the theme, not a literal in each node (§3.6, ADR-0016). `resource_read` reads what the server serves as resources, a skill the bundle carries (`skills/NAME/SKILL.md`, `bundle://skills/NAME`), and the theme the deck is drawn in (`bundle://theme`). `deck_create`, `theme_apply`, `deck_export`, and `deck_history` are not among them: the page opens and downloads bundles itself, the Versions tab is where the user reads and restores the history, and the assistant asks the user when the work needs a new bundle, another theme, an export, or an earlier version. The worker runs each tool in the engine's module (`Player.tool`), by the operations of `scaena-ops` (ADR-0009). Each operation that writes has a twin that computes what to write and writes nothing, which is what the page calls, so the CRDT stays out of the module (ADR-0011). The session keeps each edit, with what the operation says it did, for the next save to record as the model's (PLAN 2.9).
- **The prompt** is the author-deck skill, the deck in a few facts, and the list of resources. The resources are a WASM module of their own (`scaena-resources`, 0.21 MB gzipped), which the worker loads with the assistant's code the first time the user asks something.
- **What it sees** (PLAN 2.52). A question begins with what the editor shows as it is asked, in brackets: the state shown (and the format, if another), the nodes selected, each with its type, and the characters selected in a text typed in, counted as `replace_text` and `style_text` count them, with the text they make (`[In the editor: state revenue shown; selected: title (text); in title, characters 0 to 7 selected: "Revenue".]`). The prompt says that "this", "these", and "shorter" mean what it names, in that state. The conversation keeps it with the question. Focus in the Assistant tab keeps a text typed in, and its characters selected, as the inspector does.
- **While it works** the source is read-only. Each edit it makes is compiled, shown in the state the editor shows, and linted in the worker before its next call, and comes into the source as an edit, which undoes as one; a theme edit comes in as the Theme tab's does, with the theme's text before and after. What the question has changed so far is selected once the canvas stands in that edit: the nodes whose own props, a state's delta for them, or the deck's overrides of them differ from before it was asked, and those it added, of those the state shown shows, held where the first is. Stop ends it after the call it is in. A question takes at most 32 rounds of calls. The model is sent the newest two frames it drew, and the older ones by name.
- The agent loop of PLAN 1.19 runs through the editor against a scripted server for each provider's wire format (`web/assistant.mjs`), and what it sees, and what is selected after it, against a scripted Anthropic (`web/seeing.mjs`).

#### 9.2.10 The canvas by keys

Every gesture the canvas takes has a key (PLAN 2.75, WCAG 2.1.1), which the keys sheet lists (§9.2.4).

- **Selecting.** Tab and Shift+Tab select the next node, or the one before, in reading order (§3.12: as they are painted, a container where the first it holds is) among those beside the one selected. Past either end nothing is selected and the focus leaves the canvas, so no key holds it there. Enter goes into a container or a group, selecting its first node, and Escape back out. ⌘A selects what is selected and everything beside it, or everything on the canvas.
- **A selection by keys** (PLAN 2.89). Space keys the node selected, and the selection is made a node at a time: Tab and Shift+Tab move the key to the next node beside it, or the one before, outlined and the selection unchanged, and Space puts the node keyed in the selection or takes it out, as Shift+click does. Escape stops, what is selected staying. Space taps to toggle; held with a drag it still pans (§9.2.4).
- **Handles.** Enter on a shape goes to its points and a rect's corner, on an image to its crop's sides and its focal point (§9.2.4, PLAN 2.68, 2.74), and on a chart to its marks, then its annotations. Tab and Shift+Tab step through them, the one keyed marked. An arrow moves a point, a crop's side, or the focal point a hundredth, a tenth with Shift, and a corner a radius step: each one `choose`, the patch the handle's drag makes, kept to the state shown with Alt. On a point, + adds a point after it and Delete takes it away. A mark reached is picked, so its menu (Shift+F10) annotates it; an annotation reached is selected, Enter changes what it says, and Delete takes it away. A callout is moved to another mark by calling that mark out. Escape leaves the handles.
- **Turning and moving.** `[` and `]` turn what is selected 15° back or on, 1° with Shift, as its round handle does; the arrows move it and, with Shift, resize it (PLAN 2.31).
- **Layouts.** In a layout shown (PLAN 2.71), Tab keys a slot, an arrow moves it a track, and Shift with one resizes it, each one theme edit.
- `web/bykeys.mjs` does each by keys alone in headless Chromium, on the torture deck and the revenue example.

### 9.3 Mac client (Phase 3)

- SwiftUI app; engine via `scaena-ffi` (C ABI, `cbindgen`), `vello` on Metal through `wgpu` with a `CAMetalLayer` surface.
- SwiftUI owns chrome only: document browser, state list, timeline scrubber, inspector, source pane. **No TextKit/CoreText in the render path.**
- Keychain for keys; Apple Foundation Models for on-device assistant tasks; Share/Quick Look via PDF export.

### 9.4 Presenter remote (later)

iPhone/iPad remote and notes view over local network; trivial once state is `(stateId, t)`.

---

## 10. Export projections

All exports consume the same resolved document. The **spine** is the contract for projections that are not the deck.

| Projection | Mechanism |
|---|---|
| PDF | `pdf` painter (krilla, PLAN 1.20): a page per slide at its last state, in spine order, or per state with `--states`; the canvas at 2 units to the point (1920 × 1080 is a 960 × 540 pt page); paths and gradients as vectors; text as text in subset fonts, each glyph saying its cluster (§6); images at their own resolution, a JPEG as its own file, without its metadata, turned by its EXIF orientation (ADR-0017); shaders as images of their CPU reference at 2× the canvas (§3.8), as JPEG at quality 90 where they are opaque, so a full-bleed mesh is about 1 MB and not 15, and kept whole where what is under them shows; tagged and outlined from the spine (§3.12). A font krilla cannot read or subset stops the export with an error. The editor exports the same bytes (§9.2.6) |
| PNG | each state at rest, by the CPU painter (§13), at any size in the canvas's aspect ratio. The editor exports the state shown (§9.2.6) |
| SVG | each state at rest (PLAN 1.21): the canvas is the `viewBox`, at any size. A layer is a group: its transform, clip, opacity, and blend (CSS `mix-blend-mode`). Paths fill and stroke in colors and linear and radial gradients, as the painters' sRGB stops spell out their Oklab blend (§6). Glyphs are outlines, a path each, unhinted as the painters draw them, so nothing lays the text out again; over each run lies its text, transparent and stretched across the run, so the SVG selects, copies, and searches as the deck reads. Images embed as PNG, filtered as the painters filter them. What SVG cannot draw is an image of the CPU painter's pixels at the SVG's size: shaders (their CPU reference, §3.8), color glyphs (COLR, bitmaps), and sweep gradients. Drawn by an SVG rasterizer, every torture state passes §13.5 against the CPU painter |
| Video (mp4/webm/ProRes) | the global timeline (§2.4), or the states `--states` names in that order, sampled at `fps` (default 60): frame *k* shows the moment *k*/`fps` seconds in. Each state plays its cue, then its `hold`, the dwell that makes a deck a video; a state with neither has no frame. Frames are the CPU painter's, so deterministic; or, with `--painter gpu` (§13.6), vello's on the GPU, each painted while the frames before it are read back, and within §13.5's tolerance of the CPU painter's. One that draws what the frame before it drew is painted once. They are piped to `ffmpeg` as RGB over black, converted to BT.709 video-range YUV and tagged so: H.264 (CRF 18, 4:2:0) in MP4, VP9 (CRF 30, 4:2:0) in WebM, or ProRes 422 HQ (10-bit 4:2:2) in QuickTime (`prores`). A sound track (`--audio`, any file ffmpeg reads) plays from the first frame, cut where the frames end or carried on in silence until they do. Each run of states one beat names is a chapter titled by its claim (a chapter track in MP4 and QuickTime, Matroska chapters in WebM); a state no beat names is a chapter of its slide, titled by the slide's id, and a deck without a spine has none. The video is written beside `--out`, under a name of its own, and renamed to it when whole, so two exports of one file never mix: the last to finish stays |
| Single-file HTML | one file that plays the deck in a browser, offline (PLAN 2.5, §9.2): the player's page, its code, and the engine (the player's module alone), which `just web` builds and `scaena` carries, filled in with the bundle as `scaena save` writes it (fonts subset; no manifest or history), each file gzipped in base64, and how each state reads (§3.12). `--states` picks the states it plays, in order. Its content security policy lets nothing load. The same bundle exports to the same bytes, from the CLI or the editor (§9.2.6) |
| **Spine JSON** (`export --format spine`, PLAN 1.22) | `spine.json` (`docs/schema/spine.schema.json`, generated from `scaena-core::spine`): the deck's title, language, canvas, and formats; the spine as the deck holds it (§3.11); every state with its slide, notes, and place on the global timeline (§2.4); and every beat with the state that shows it (the last of its states in deck order), its `start` and `end` on the timeline, and its renders. Each beat is drawn at rest into `renders/` beside the file: a thumbnail, `<beat>.png`, 480 px wide or `--size`, and the beat in each other format the deck lists at that format's canvas size, `<beat>@9x16.png` |
| Infographic | external: each beat's claim, evidence, and `media.infographic`, and its `9:16` render |
| Motion graphic | external: the video, with a chapter per beat |
| Podcast | external: each beat's `media.podcast.script`, or its claim and notes, in the spine's order and the deck's `lang` → TTS → `--audio` |

The existing export code integrates against `spine.json` + `scaena render`/`export`; nothing in it needs to know the document format. `docs/projections.md` is the integration note: what each pipeline reads, and how a narration is timed to the beats.

---

## 11. LLM integration (BYOK, no backend)

1. **Any agent via MCP** (Phase 1). No keys handled by Scaena.
2. **In-app assistant** (Phase 2/3). Provider adapters: Anthropic Messages API (direct browser calls with the explicit opt-in header), OpenAI (Chat Completions, or a server that speaks it), Gemini. Keys: Keychain (Mac); the tab's own storage, or the device's, encrypted, when the user asks (web). Keys are never written into a bundle. The assistant uses the same operations as the CLI through function calling: on the web, the MCP server's tools on the bundle the page holds (§9.2, ADR-0011). Prompts come from `skills/`: the author-deck skill, and the rest as resources the model reads.
3. **On-device** (Mac): Apple Foundation Models for lint explanations, notes drafting, copy tightening.

---

## 12. Repository layout

```
crates/
  scaena-core     document model (serde + schemars), ids, tracking resolution, timeline math (easing, springs), display list, shader kinds (CPU reference + WGSL), document-level lints, the spine projection
  scaena-engine   theme cascade, layout (taffy), text (parley/harfrust), charts→marks, shader nodes→ops, timeline resolution, sampling
  scaena-paint    painters: vello (gpu), vello_cpu (cpu); both run shader ops
  scaena-export   pdf (krilla), svg, png, video (ffmpeg driver, chapters), html
  scaena-ops      the operations every client exposes, over a bundle, with typed results (ADR-0009)
  scaena-cli      `scaena` binary over scaena-ops
  scaena-mcp      MCP server (rmcp) over scaena-ops
  scaena-serve    `scaena serve`: the web pages on a bundle's folder, on this machine (hyper), the folder watched and deck.scn compiled
  scaena-resources what agents read (SPEC by section, the schemas in parts, the lint catalog, the skills, examples): the MCP server's resources, and a WASM module of its own for the page's assistant
  scaena-wasm     wasm-bindgen bindings
  scaena-subset   the font subsetter as a WASM module of its own, which a page loads to download a bundle
  scaena-history  the CRDT as a WASM module of its own, which a page loads to save a bundle that keeps a history
  scaena-pdf      the PDF writer as a WASM module of its own, which a page loads to export a PDF
  scaena-ffi      C ABI (cbindgen) for Swift
  scaena-store    CRDT document (loro), ops, history, bundle I/O
web/              Vite + TS player/editor (Phase 2)
apps/mac/         SwiftUI client (Phase 3)
docs/             SPEC, PLAN, ADRs, schemas (generated; MCP tools' in schema/mcp/), examples
skills/           agent skills (SKILL.md)
tests/            golden display lists, golden rasters, lint fixtures, parity harness
```

---

## 13. Determinism contract

1. No wall clock in the engine. `t` is an input.
2. All randomness is seeded from the document (`seed` fields) and the node id.
3. Fonts come from the bundle only; system font discovery is not compiled into the engine. Shaping is `harfrust` via `parley`, font reading is `skrifa`; the same bytes produce the same glyph runs on every platform.
4. Floating point: layout and interpolation use `f32` with a fixed evaluation order; display lists are compared bit-for-bit after rounding lengths to 1/64 cu and transform linear parts to 2⁻¹⁶ (`quantize`; rounding a rotation's sine to 1/64 would erase it). (If platform `f32` drift appears in practice, the golden test rounds; the contract does not.)
5. GPU vs CPU raster parity is tested per fixture with a tolerance (ΔE in Oklab ≤ 1.0 on 99.9% of pixels; AA edges excluded by a 1-px dilation mask; and no pixel anywhere, edges included, differs by half the channel range (128/255) or more, so a hole or a misplaced glyph cannot hide in the mask).
6. Export frames are produced by the CPU painter unless the caller opts into GPU.
7. Transcendental math in the render path (`cbrt`, `pow`, `exp`, `sin`, …) goes through `libm`'s pure-Rust implementations, not `std`. The `std` float methods call the platform's math library, whose last bits differ between Linux, macOS, and WASM. Geometry avoids transcendentals where it can: rounded corners are arithmetic Béziers. Shader references compute theirs once per frame, and per pixel use only `+ − × ÷` and comparisons, in the same order as their WGSL twins; sRGB encoding compares against a table of 255 thresholds instead of calling `pow`. Springs (`scaena_core::timeline::Spring`) evaluate through `libm` too.
8. Images are decoded in Rust from the bundle's bytes, by code that takes no path by the processor it runs on. A PNG is lossless, so the `png` crate's pixels are the file's. A JPEG's pixels depend on its decoder's arithmetic, so `scaena_core::jpeg` decodes it in integers alone, from one source on every target (ADR-0017). `photos_decode_as_zune_jpeg_does` holds it to zune-jpeg byte for byte, and torture case 50 holds it to the goldens natively and in the browser.

---

## 14. Testing strategy

- **Schema conformance:** every example and fixture validates against `deck.schema.json` and `theme.schema.json` in CI (`jsonschema`).
- **DSL round-trip:** property tests — `compile(decompile(doc)) == doc` for every fixture and for generated documents; `decompile` is idempotent on its own output. (Source formatting is explicitly not preserved; see §4.)
- **Golden display lists:** fixtures → `DisplayList` JSON snapshots; any change is a reviewed diff.
- **Golden rasters:** `vello_cpu` PNGs per fixture with per-pixel tolerance (`kompari`-style diffs attached to CI).
- **Parity harness:** GPU vs CPU on every fixture (nightly or on demand where a GPU exists).
- **Shader parity:** CPU reference vs WGSL per kind, per seed.
- **Lint fixtures:** one deck per rule that must trigger it, one that must not.
- **Fuzz:** DSL parser and JSON loader (`cargo-fuzz`).
- **Benches:** layout, sample, paint at 1080p and 4K (`criterion`), tracked against §15 budgets.
- **Agent loop test:** an MCP smoke test that creates a deck, introduces an E100, lints, applies the fix, re-lints to zero errors, renders. It talks to `scaena mcp` on stdio, as an agent's client does (`crates/scaena-cli/tests/mcp.rs`), and runs in CI with the rest.

---

## 15. Performance budgets

Budgets are per stage, on named benchmark decks, on a reference machine (M-series Mac, 8 performance cores; Linux CI numbers are recorded but not gated). "Cold" includes bundle load and font parsing; "warm" means caches primed. Numbers are targets measured by `criterion` benches; a regression against the recorded baseline fails CI.

**Benchmark decks** (`tests/bench/`): **B1** text-heavy, 40 states, 4 fonts, no charts; **B2** chart-heavy, 12 states, 6 charts with key morphs; **B3** shader-heavy, 8 states, full-bleed mesh + grain on every state; **B4** the torture deck from PLAN 0.2. `scripts/build_bench_decks.py` writes B2 and B3 on B1's theme and fonts.

**How they are measured** (PLAN 1.24). `crates/scaena-cli/benches/stages.rs` times every stage in the table on B1–B4 (`just bench`). A bench is named `stage/deck`. A stage that goes over a deck's states, cues, or frames counts them, so its time is also given per state, cue, or frame. A `_one` stage times the deck's slowest state. CI's `bench` workflow times, on Linux, every push to a pull request that could change a number once it is ready for review; a draft is not timed. On Linux and on macOS on Apple Silicon, which also paints on Metal, it times `main` once a week and on a manual run, into each runner's history. macOS judges no pull request: a macOS minute bills as ten Linux ones, and that runner's noise hides any regression under about 30%. A pull request is timed beside its base on Linux, and `scripts/bench_gate.py` judges it bench by bench:

- the workflow builds the base too, the commit the pull request merges onto, and times both on the same machine in the same job;
- it builds both with each function starting on a 64-byte line (`-C llvm-args=-align-all-functions=6`, as `just bench` builds them), so code a change adds elsewhere in the binary cannot move a bench's loops onto other boundaries (ADR-0004 finding 18);
- both run on the base's decks: the pull request's build reads them from the base's worktree (`SCAENA_BENCH_DECKS`), so the two run the same work and the gate judges the code. A deck a pull request changes is timed on `main` once it lands, and judged from the next pull request on. Where a format change leaves the base's decks unreadable to the pull request's build, its benches fail;
- it builds both before it times either, then times them a group of benches at a time, the base first, so a spell of load on the machine falls on both;
- a bench is slower when the pull request takes longer than the base by more than the run's floor: 10%, or 2.5 times the run's noise if that is more. The noise is how far the benches stray from their base: a pull request changes few of them, so the spread of all their changes (1.4826 × the median absolute deviation of the log ratios) measures the machine. On Linux the floor is 10%. On the macOS runner, whose benches stray by about 12% beside an identical base, it is about 30%, so only a larger regression shows there;
- a slower bench is timed again beside the base, bench by bench, in three turns: the pull request first, then the base first, then the pull request first. Each turn sets the two side by side, so a spell of load falls on both;
- it **regresses** when it is slower than the base by more than the floor in every turn. One slow run on either side can sway a turn, not three.

On macOS the workflow turns Spotlight off first: it indexes the files a build writes while the benches run.

A pull request with a regression fails, unless it carries the label `bench-accept`. Runs of the same code on different CI machines spread too far to judge a change by. Of six runs of `main` on the macOS runner, 46% of the times were more than 10% from their bench's median and 13% more than 30%, and Linux runs land on different processors. So each runner's history of `main`'s runs, kept by the workflow, is shown beside each bench for context and does not judge. The run summary shows the budgets below for every deck, timed on a pull request by its code on the base's decks, then every bench against its base and `main`'s runs. A `probe` bench, a fixed sort, shows how fast the runner was, to tell a slow machine from a slow change. The `wasm` job checks the engine's WASM size, records the font subsetter's beside it (a module of its own, which a page loads only to download a bundle, §9.2), the history's (another, loaded only to save a bundle that keeps one), what the assistant reads (another, loaded the first time it is asked something), the PDF writer's (another, loaded the first time a PDF is exported), and the player's module (the engine without the editor's operations, which a single-file export carries, with the hyphenation patterns the editor's module is handed as a text needs them, §9.2.1), and records B1's cold start in headless Chromium. It builds them as `just wasm` does, by Cargo's `wasm` profile: release's, with the crates that run as a deck is read, edited, or saved, not each frame, built for size.

| Stage | Budget (warm unless noted) | Deck |
|---|---|---|
| WASM engine size (gzip, no fonts) | ≤ 3.0 MB | — |
| Cold start: WASM load → first frame at 1080p | ≤ 500 ms | B1 |
| Resolve + layout, one snapshot (text shaping included) | ≤ 15 ms | B1 |
| Resolve + layout, all snapshots | ≤ 400 ms | B1 |
| Sample one frame (interpolate resolved geometry) | ≤ 1 ms | B1, B2 |
| GPU paint, one frame at 1080p | ≤ 6 ms | B1, B2, B3 |
| CPU paint, one frame at 1080p (`vello_cpu`, 8 threads) | ≤ 12 ms (B1, B2) · ≤ 25 ms (B3) | B1, B2, B3 |
| Headless PNG render, **cold**, single frame | ≤ 300 ms | B1 |
| Lint, document-level | ≤ 100 ms | B1 |
| Lint, layout-level, all states | ≤ 1 s | B1 |
| MCP `deck_render` round trip (cold process, cached fonts) | ≤ 1 s | B1 |
| Video export, CPU path (sample + CPU paint, pipelined, raw frames to the encoder) | ≥ 1× realtime at 1080p60 (B1, B2) · ≥ 0.5× (B3) | B1–B3 |
| Video export, GPU path (sample + GPU paint + readback + encode) | ≥ 2× realtime at 1080p60 | B1–B3 |

Notes: the CPU paint stage paints a frame on one thread, but for its shaders, whose rows are worked out on every core up to the 8 its budget allows (§3.8). The video stage plays the deck's longest cue, laid out beforehand as the exporter lays out each cue, and paints its frames on every core into an ffmpeg that discards them: it times Scaena's side, not an encoder. The GPU stage includes reading the frame back, so the report records it but does not judge it against the paint budget. The GPU video stage (`video_gpu`) is the video stage with `--painter gpu`: each frame painted by vello while the frames before it are read back, into the same discarding ffmpeg. On a machine whose only adapter is a software one (Mesa's lavapipe on Linux CI), it times that, not a GPU. The cold PNG budget and the video budgets are different workloads and are deliberately not comparable — video frames never re-run layout (§5) and pipeline paint and encode across threads. Shader-heavy decks are allowed to cache shader tiles between frames when uniforms are unchanged. If the CPU video path cannot reach 1× realtime on B1 by gate 1, the video exporter defaults to GPU where available and the budget is revisited in an ADR rather than silently relaxed.

---

## 16. Open questions

1. Vertical text and complex script timing (phase?).
2. Video/audio nodes — decoding in the engine or in the client?
3. Font licensing: settled as proposed, warn and don't block. W230 reports a font whose embedding bits do not allow what a bundle does with it, and nothing refuses it (PLAN 2.14).
4. Live data sources and refresh semantics.
5. Direct-manipulation editing model and how it expresses patches in template-managed layouts.
6. Multiple formats: how much per-format override is allowed before it's a second deck? PLAN 1.13 settles the minimum: a format's template set moves slots and changes the grid, and nodes and states are shared. Open: per-format node props (a shorter headline in `9:16`). A node placed by `col`/`row` or `rect` in a deck with formats, which does not move with the slots, is W302's (PLAN 1.15).
7. Collaboration conflict UX when it arrives.
8. A very large font catalog (Jay, 2026-10-01). Proposal: a local, content-addressed font store with a searchable index (family, styles, axes, script coverage, license), filled on demand from catalog sources. Editors and agents pick from it (`scaena fonts search | add`, and the same over MCP), and saving subsets the chosen faces into the bundle. The render path is unchanged, bundle fonts only (§13), so the catalog's size never reaches layout or painting. Open: which sources, licensing (Q3), offline use, and drawing script fallback chains from the catalog.
9. Copy that quotes data. **Resolved** (ADR-0019, PLAN 2.72): a run's `quote` names a cell of a source, read through a `dataTransform` and a row found by its values or its index; the run's text is the figure as last set, which every patch and data edit sets again with the claims that hold it, and W427 finds one a file changed behind the deck's back (§3.5). Still open: figures in `alt` and notes, and claims about a trend. As first put: every figure in text, `alt`, claims, and notes is a literal, so a data update is a manual sweep, and nothing notices a claim the new data makes false. In the authorability spike (PLAN 0.13), rolling a chart forward was one field for the chart and 22 hand edits for the words. Proposal: a text run can bind a value, as in `{ "bind": "@revenue", "value": "sum(revenue) where quarter = last(quarter)", "format": "$,.1f" }`, written in `dataTransform`'s expression language (§3.10) and resolved per snapshot like any text, so a figure rolls forward with its data. Open: how big the expression language gets, how an editor shows a bound figure, the DSL spelling, and whether a claim about a trend ("growth accelerated") can be checked at all.
10. Fonts that follow the theme. A deck's `fonts` restates the files its theme's families name, and rendering registers only what `fonts` lists. In the authorability spike no deck could render for that reason (finding 5); `validate` now catches it (E102, with the fix). Proposal: rendering registers every theme family's file, and `fonts` lists only the fonts the theme does not name. Trade: one list fewer to keep in step, against a deck that no longer says by itself which files it needs, since another theme names others. Saving already handles both (PLAN 1.4).
11. Image formats and sizes. **JPEG is resolved** (ADR-0017, PLAN 2.66). `scaena_core::jpeg` decodes it in integers, with zune-jpeg's portable arithmetic, the same bytes on every target. It is held to zune-jpeg file by file and to the torture goldens. GIF and WebP stay out: a bundle may hold them (`scaena files` counts them as images, §7.1), but no painter draws them. Still open: vello keeps every image a frame draws in one 8192 px square atlas and silently skips an image that does not fit. Six 12-megapixel photos on one slide would lose some on the GPU while the CPU draws them all, and photos make that likelier. Proposal: painters receive each image already reduced to at most twice the device size it is drawn at, by one deterministic box filter in `scaena-paint` that both painters share. That bounds the atlas, removes minification aliasing, and keeps the painters on the same pixels. Open: whether saving should also store reduced copies, and what size an export at 4K needs.

---

## 17. Glossary

- **Node** — an object in the scene graph with a stable id.
- **State** — a named point on the timeline; a cue. Every click is a state.
- **Slide** — a navigation grouping of consecutive states.
- **Snapshot** — a state resolved to absolute property values (after tracking).
- **Tracking** — unchanged properties carrying forward from the previous state.
- **Transition** — interpolation between two snapshots, matched by id.
- **Choreography** — scheduled animations within a state (staggers, sequences).
- **Role** — a named typographic/semantic style from the theme.
- **Theme** — a design system: tokens, roles, layouts, motion, palettes.
- **Override** — an explicit literal that bypasses the theme; tracked and lintable.
- **Spine** — the narrative structure (sections → beats) beneath the states.
- **Projection** — any output rendered from the spine/document (deck, PDF, video, infographic, podcast).
- **Display list** — the painter-agnostic, serializable description of one frame.
- **Painter** — a backend that consumes a display list (vello, vello_cpu, pdf, svg).
- **Bundle** — the self-contained on-disk form of a deck.

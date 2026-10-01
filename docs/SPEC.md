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

**Tracking.** By default a state declares only *changes* relative to the previous state; unchanged properties carry forward ("track"). A state MAY instead declare `from` (branch from an arbitrary earlier state) or `mode: "absolute"` (no tracking; everything explicit). The state's `layout` template tracks the same way, so a build on the same slide need not restate it. Per-state-only properties (keyframe `anim` tracks) never track: an entrance must not replay on the next cue. The engine resolves every state to an **absolute snapshot** before layout. Authors edit deltas; the engine thinks in snapshots; tooling shows both.

**Slides.** A `slide` key groups consecutive states for navigation and export (a build sequence is one slide with several states). A state with no `slide` key is its own slide.

### 2.3 Transitions

A transition is the interpolation between two resolved snapshots over a duration with an easing. Nodes are matched by `id`:

- present in both → **morph** (geometry, color, opacity, text, data interpolate; per-node `transition` policy may force `crossfade` or `cut`)
- present only in the target → **enter** (preset or explicit keyframes)
- present only in the source → **exit**

Text morphs at word granularity (shared words move, others cross-fade). Charts morph at mark granularity (marks matched by data key; kind changes interpolate mark geometry). Shader nodes interpolate uniforms.

### 2.4 Time

- `t` is milliseconds since the start of the *current* transition. `t ≥ transition.duration` means the state is at rest.
- Within a state, nodes may own **keyframe tracks** and **choreography** (staggers, delays, sequences) that run after the transition completes or in parallel with it (`timing: "with" | "after"`).
- A state MAY declare `hold` (ms) for auto-advance; this is how a deck becomes a video. The **global timeline** is the concatenation of (transition + choreography + hold) across states.

### 2.5 The design system

A **theme** is a design system: tokens (color, type, space, motion, shader palettes, data palettes), **roles** (what a `display` text is), **layout templates** (what a `split` state looks like), and named springs. The document stores references, never resolved values. Re-theming is re-rendering. Theme values are properties, so a theme change is itself animatable.

### 2.6 The narrative spine

Beneath the states sits a **spine**: sections → beats, each beat carrying a claim, evidence references, speaker notes, and the states that express it. The deck is a projection of the spine; so are the infographic, the PDF, the motion piece, and the podcast (§10). The spine is also the accessibility reading order.

---

## 3. Document model

### 3.1 Bundle

A deck is a **bundle**: a directory `name.scaena/` or a zip of the same layout with extension `.scaena`.

```
name.scaena/
  manifest.json          # format version, deck.json hash, content hashes, created/modified
  deck.json              # CANONICAL INTERCHANGE form of the logical document (schema: docs/schema/deck.schema.json)
  deck.scn               # OPTIONAL authoring projection (DSL); regenerated on save
  theme.json             # theme used by this deck (copied in; decks are self-contained)
  spine.json             # OPTIONAL externalized spine (if absent, lives in deck.json)
  data/                  # CSV/JSON data sources referenced by @name
  assets/<sha256>.<ext>  # images, content-addressed
  fonts/<family>-<hash>.ttf|otf|woff2   # subsetted fonts, content-addressed
  history/deck.loro      # CRDT document: persistence authority + history while editing (absent in "flat" exports)
```

Rules:
- **Authority.** The *logical document* (the typed model in `scaena-core`) is the truth. `deck.json` is its canonical interchange representation: deterministic serialization, the thing git diffs and agents patch. While a bundle is open in an editor, the CRDT (`history/deck.loro`, §8) holds persistence authority and history; `deck.json` is regenerated from it on every save and the two never disagree. `deck.scn` is an authoring projection: edits to it are compiled into the logical document (through the CRDT when one is open) and it is regenerated on save. If files are found inconsistent on disk (hand edits while closed), the loader applies the newest file as a change authored `fs` and regenerates the others; `deck.json` is the tiebreaker.
- Assets and fonts are referenced by content hash; the bundle is self-contained and portable.
- Fonts are **subsetted** into the bundle at save time. The render path MUST NOT consult system fonts (§13). System fonts are only enumerated in editors for picking.
- `manifest.json` records `"scaena": "<format version>"` (semver; majors break) and the hash of `deck.json` it was last written against.

### 3.2 Top-level document

```jsonc
{
  "scaena": "0.1",
  "meta":   { "title": "...", "author": "...", "created": "...", "lang": "en-US" },
  "canvas": { "width": 1920, "height": 1080, "unit": "cu" },   // canvas units; 1 cu = 1 px at 1080p
  "formats": ["16:9", "9:16"],                               // additional projections (optional)
  "theme":  "theme.json",                                     // path in bundle, or inline object
  "fonts":  [ { "family": "...", "file": "fonts/...", "axes": {...} } ],
  "data":   { "q3": { "source": "data/q3-revenue.csv", "schema": { ... } } },
  "spine":  { "sections": [ ... ] },
  "nodes":  { "<id>": { "type": "...", ... } },               // the scene graph
  "states": [ { "id": "...", ... } ],                         // the cue list, ordered
  "overrides": { "<nodeId>": { "<prop>": value } }           // explicit, lintable pixel/theme overrides
}
```

**IDs.** Slugs: `^[a-z][a-z0-9_-]{0,63}$`, unique across `nodes`, across `states`, and across spine `beats`. Agents SHOULD choose meaningful IDs (`title`, `rev-chart`). The CRDT layer assigns internal IDs independently; slugs are for humans and agents.

### 3.3 Node types

Every node: `{ "type", "id" (implicit from key), "name"?, "alt"?, "semantic"?, "parent"?, "z"?, "tags"?: [...] }`.

**Two semantic axes.** `role` (on text nodes) and the theme describe *what something looks like*. `semantic` describes *what it does in the argument*: `claim | evidence | annotation | context | comparison | takeaway | source | navigation | decoration`. They are independent: a claim may be a `headline` on one state and a `caption` on another. `semantic` is optional in v1 and reserved for the narrative lint family (§7.5) — e.g. "this beat's claim has no visible expression" or "evidence outranks the claim in visual hierarchy". Agents SHOULD set it; nothing renders differently because of it.

| type | purpose | key properties |
|---|---|---|
| `text` | typographic text | `role`, `runs` or `text`, `fit`, `balance`, `maxLines`, `align`, `box`, `features`, `axes`, `lang` |
| `shape` | vector geometry | `path` (SVG path data) or `kind: rect\|ellipse\|line\|arrow\|polygon`, `fill`, `stroke`, `radius` |
| `image` | raster/vector image | `src` (asset ref), `fit: cover\|contain\|fill`, `focal: [x,y]`, `crop` |
| `chart` | data-bound visualization | §3.7 |
| `shader` | GPU/CPU parametric background or fill | §3.8 |
| `stack` | layout container (axis) | `axis: x\|y`, `gap`, `align`, `distribute`, `padding`, `children` |
| `grid` | layout container (grid) | `cols`, `rows`, `gap`, `padding`, `children` (+ child `area`) |
| `frame` | layout container (absolute) | `children` (+ child `rect`) |
| `group` | transform-only grouping | `children` |

Deferred node types (not in v1 schema): `video`, `audio`, `table`, `code`, `embed`.

Common animatable properties: `opacity`, `transform` (`translate`, `rotate`, `scale`, `skew`, `anchor`), `fill`, `stroke`, `blur`, `clip`, `blend`, `shadow`, plus type-specific ones (text content/style, chart data/encodings, shader uniforms).

### 3.4 Layout

Placement is declared with `at`, resolved against the state's **layout template** (from the theme) or an explicit container.

```jsonc
"at": { "col": [1, 7], "row": [1, 1] }            // grid cell range on the template grid
"at": { "in": "left", "align": "center" }          // slot in the layout template
"at": { "rect": [120, 80, 900, 420] }              // canvas units — an OVERRIDE (lint W301)
"at": { "parent": "stats", "index": 2 }            // child of a container node
```

Containers follow CSS flex/grid semantics as implemented by `taffy`: `stack` = flex (one axis), `grid` = CSS grid, `frame` = absolute. Sizing values: `fixed(cu)`, `fit`, `fill`, `fraction(n)`, `aspect(w:h)`, `min/max`.

**Alignment** in a cell is `x: start|center|end|stretch` and `y: start|center|end|stretch|cap|baseline|x-height`; a single keyword sets both axes. The slot's `align` is the default, the node's `align` overrides it, and `at.align` overrides both; `start` when nobody says. `start`/`center`/`end` align the text's box. The typographic anchors align text of any size to a shared line: `y: "cap"` and `y: "x-height"` put the first line's cap height or x-height on the cell's top edge, and `y: "baseline"` puts the last line's baseline on the cell's bottom edge. `box: "cap"` trims the text's box from the first line's cap height to the last line's baseline (CSS `text-box: trim-both cap alphabetic`), so a cap top can sit exactly on a grid line; `box: "line"` (default) uses line boxes. Cap height and x-height come from the font's OS/2 table, never from glyph bounds.

**Text fit policy** (`fit`): `wrap` (default) | `shrink` (down to role `minSize`) | `grow` (up to role `maxSize`) | `clip` | `error`. Overflow under `wrap`/`clip` is lint **E100**; under `shrink` it is **W203** once the minimum is reached.

**Baseline grid.** Themes MAY define a baseline grid; text roles snap leading to it; lint **W221** on violations.

### 3.5 Typography

Text nodes carry a `role` from the theme (`display`, `headline`, `title`, `body`, `caption`, `label`, `numeral`, `code`, …). The role supplies family, size, weight, leading, tracking, measure (max line length), case, numeric features, and variable axes (`wght`, `opsz`, `wdth`). Rich text is `runs: [{ "text", "role"?, "emphasis"?: "high"|"low", "style"?: {...} }]`.

The engine MUST implement:
- Shaping via `harfrust` (the HarfBuzz port) through `parley` (ligatures, kerning, contextual alternates, OpenType features, variable axes), with per-run `features` and `axes` overrides. Font tables and metrics are read with `skrifa` (ADR-0004).
- Line breaking: `wrap: "greedy" | "pretty" | "balance"`. `pretty` minimizes raggedness and avoids short last lines (Knuth–Plass or equivalent); `balance` equalizes line lengths (titles).
- Hyphenation by `lang` (optional, off by default for display roles).
- Widow/orphan control: `minLastLineWords` (default 2 for body, 1 for display).
- Hanging punctuation and optical margin alignment (`opticalMargins: true`), on by default for display roles.
- Numeric styles: `tabular`/`proportional`, `lining`/`oldstyle`; charts default to tabular lining.
- Vertical metrics by cap height and x-height from the font tables (not bounding boxes).
- Text splitting for animation: `split: "lines" | "words" | "glyphs"`, exposing units to choreography (§3.9).
- Bidi and script fallback via `fontique` restricted to bundle fonts.

### 3.6 Theme (design system) schema

See `docs/schema/theme.schema.json`. Shape:

```jsonc
{
  "scaena-theme": "0.1",
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
    "families": { "display": { "family": "...", "file": "fonts/...", "axes": {...} }, "body": {...}, "mono": {...} },
    "scale":    { "ratio": 1.25, "base": 32 },
    "roles": {
      "display":  { "family": "display", "size": 128, "weight": 650, "leading": 0.95, "tracking": -0.02, "opsz": 96, "wrap": "balance", "box": "cap", "minSize": 72, "measure": 18 },
      "headline": { "...": "..." }, "body": { "...": "..." }, "caption": { "...": "..." }, "numeral": { "numeric": "tabular-lining", "...": "..." }
    }
  },
  "grid":   { "columns": 12, "gutter": 24, "margin": 96, "baseline": 8 },
  "layouts": {
    "title":  { "slots": { "title": {...}, "subtitle": {...} } },
    "split":  { "slots": { "left": { "col": [1, 6] }, "right": { "col": [7, 12] } } },
    "full":   { "slots": { "main": { "col": [1, 12], "row": [1, 6] } } }
  },
  "motion": {
    "durations": { "fast": 180, "standard": 420, "slow": 800 },
    "easings":   { "standard": [0.2, 0, 0, 1], "out": [0, 0, 0, 1], "in": [0.4, 0, 1, 1] },
    "springs":   { "snappy": { "stiffness": 420, "damping": 34, "mass": 1 }, "gentle": { "stiffness": 170, "damping": 26, "mass": 1 } },
    "presets":   { "rise": { "from": { "opacity": 0, "translate": [0, 24] }, "ease": "out", "duration": "standard" }, "grow": {...}, "fade": {...} }
  },
  "shaders": { "palettes": { "dusk": ["#...", "#...", "#...", "#..."] }, "presets": { "mesh-soft": { "kind": "mesh", "params": {...} } } },
  "charts": { "axis": { "role": "label" }, "label": { "role": "numeral" }, "strokeWidth": "thin", "cornerRadius": 2 }
}
```

**Cascade** (later wins): theme role defaults → node `style` → state `props` → `overrides`. Only `overrides` may contain raw pixel/color literals; literals elsewhere are lint **W300**. Overrides are tracked per node so a client can show "3 overrides, not theme-safe."

### 3.7 Charts

A chart is a declarative spec compiled to **marks**; it never stores pixels.

```jsonc
{
  "type": "chart",
  "kind": "bar",                       // v1: bar | stackedBar | line | area | scatter | dot | donut
                                       // deferred (not v1, not scheduled): slope | waffle | range | heatmap
  "data": "@q3",                       // data source ref, optionally with a transform pipeline
  "transform": [ { "filter": "region == 'NA'" }, { "sort": "-revenue" }, { "limit": 8 } ],
  "x": { "field": "quarter", "type": "ordinal" },
  "y": { "field": "revenue", "type": "quantitative", "format": "$,.0f", "domain": [0, null] },
  "series": { "field": "product" },
  "color": { "field": "product", "scale": "categorical" },
  "key": "product",                    // identity for morphing marks across states
  "axes": { "x": { "show": true }, "y": { "show": false, "gridlines": true } },
  "labels": { "show": "ends", "role": "numeral" },
  "legend": "none",
  "annotations": [ { "kind": "callout", "at": { "x": "Q3", "y": 42 }, "text": "Launch" } ],
  "enter": { "preset": "grow", "stagger": 40, "spring": "snappy" }
}
```

Rules:
- **Kinds are normative here.** v1 ships exactly `bar`, `stackedBar`, `line`, `area`, `scatter`, `dot`, `donut` (PLAN 1.9); the schema enum matches. `slope`, `waffle`, `range`, `heatmap` are deferred and unscheduled; adding one is a schema change plus a PLAN task.
- All color, type, stroke, and radius come from the theme (`charts` section + tokens). Charts have no style literals.
- Marks are matched by `key` across states; a change of `kind` interpolates mark geometry (bar → point → line). Axis rescaling animates.
- Data updates interpolate values by key; added/removed keys enter/exit with the chart's presets.
- Label collision is lint **W310**; the engine MAY auto-resolve with `labels.collide: "hide" | "nudge"`.
- Number/date formatting uses a d3-format / ICU-compatible subset defined in `docs/spec/format.md` (Phase 1).

### 3.8 Shader nodes

```jsonc
{ "type": "shader", "kind": "mesh", "seed": 7, "palette": "dusk",
  "params": { "points": 5, "drift": 0.15, "softness": 0.8, "grain": 0.04 } }
```

Kinds in v1: `mesh` (mesh gradient), `gradient` (linear/radial/conic), `noise` (simplex/fbm), `grain`, `particles` (seeded field). Each kind has a **typed parameter schema**, a **seed**, and only depends on `t` for motion.

Requirements:
- Every kind has a CPU reference implementation in Rust **and** a WGSL implementation; the GPU is an optimization. Parity is tested per kind with a per-pixel tolerance (§14).
- No arbitrary shader source in documents. Ever.
- Uniforms are animatable properties like any other.
- For PDF/SVG export, shaders rasterize at a declared DPI (default 2× canvas) and embed as images.

### 3.9 Animation

**Properties and keyframes.** Any animatable property may carry a track inside a state:

```jsonc
"anim": {
  "opacity":   [ { "t": 0, "v": 0 }, { "t": 400, "v": 1, "ease": "out" } ],
  "translate": [ { "t": 0, "v": [0, 24] }, { "t": 400, "v": [0, 0], "spring": "snappy" } ]
}
```

**Presets** (`enter`, `exit`, `emphasis`) are theme-defined keyframe bundles with parameters. **Choreography** composes units:

```jsonc
"choreography": [
  { "target": "title", "split": "words", "enter": "rise", "stagger": 30, "timing": "after" },
  { "target": "rev",   "enter": "grow",  "stagger": 40, "spring": "snappy", "timing": "with" },
  { "sequence": [ { "target": "note", "enter": "fade" }, { "target": "arrow", "enter": "draw" } ], "delay": 200 }
]
```

- `timing: "with"` runs during the state transition; `"after"` runs once the transition rests.
- Springs are solved analytically (damped harmonic oscillator); they terminate at a settle threshold recorded in the resolved timeline so durations are known ahead of time (required for video export).
- Easing curves are cubic Béziers; named in the theme.
- Interpolation rules: numbers linear; colors in Oklab; transforms decomposed; paths with equal command structure interpolate point-wise, else cross-fade; text by word diff; chart marks by key.
- Lint **W320** flags more than N concurrent animated nodes (theme-tunable, default 12) and **W321** flags total choreography longer than the theme's `maxBuild` (default 2.5 s).

**State transition** (per state): `"transition": { "duration": "standard", "ease": "standard", "match": "id" }`. Per node: `"transition": "morph" | "crossfade" | "cut"`.

### 3.10 Data sources

`data.<name>` → `{ "source": "data/x.csv" | "data/x.json" | { "inline": [...] }, "schema": { "field": "number|string|date|boolean" }, "parse": { "date": "%Y-%m" } }`. Sources are read at resolve time and cached by hash. Live sources are deferred (§16). Charts reference `@name` and MAY apply a transform pipeline (`filter`, `sort`, `limit`, `derive`, `aggregate`, `pivot`) with a small, specified expression language (Phase 1).

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
- The spine order defines reading order for accessibility and the export order for every projection (§10).

### 3.12 Accessibility

- Every non-decorative node carries `alt` (text nodes default to their content). Images without `alt` are **W410**.
- Reading order = spine order, then z-order within a state.
- PDF export is tagged (structure from spine + nodes); HTML export carries ARIA from the same data.
- Lint **E110**/**E111** enforce contrast (4.5:1 body, 3:1 display) against the resolved background.

---

## 4. DSL (`.scn`)

The DSL is an authoring projection of the logical document. It is line-oriented and indentation-scoped, and agents may edit either it or `deck.json`.

**Round-trip guarantee is semantic, not textual.** The promise is

```
compile(decompile(document)) == document        (always)
decompile(compile(source))   == decompile(compile(decompile(compile(source))))   (decompilation is canonical)
```

and explicitly **not** `decompile(compile(source)) == source`. Whitespace, key order, shorthand choices, multi-line formatting, and other syntactic sugar are not preserved. The decompiler emits one canonical form (fixed indentation, props in schema order, one node per line with continuation lines when a line would exceed 100 columns). Line comments are attached as `_comment` to the state, node, or beat they precede and are re-emitted there; that is the whole extent of comment preservation. If you want your formatting kept, keep your file and regenerate `deck.json` from it — the compiler is the stable direction.

### 4.1 Example

```scn
deck "Q3 Review" theme:dusk canvas:16:9

data q3 "data/q3-revenue.csv"

section growth "Growth"
  beat doubled "Revenue doubled year over year." evidence:@q3 states:[revenue, mix]
    notes """
      Pause on the Q3 bar. Mention the launch.
    """

state revenue layout:full
  bg     shader:mesh seed:7 palette:dusk drift:0.15
  title  text role:display "Revenue doubled"       at:col(1-7) row(1)
  rev    chart:bar data:@q3 x:quarter y:revenue series:product key:product
         at:col(1-12) row(2-6)
         enter:grow stagger:40ms spring:snappy
  transition: standard

state mix slide:revenue            # a build on the same slide
  title  "…and the mix shifted"    # same id → word-level morph
  rev    chart:stackedBar          # marks matched by key, bars morph

state close from:intro             # branch: tracks from `intro`, not `mix`
  -rev                             # explicit exit
  title  "Thank you" role:headline at:in(center)
```

### 4.2 Grammar (sketch)

```
file       := header? decl*
header     := "deck" string prop* (NEWLINE INDENT (prop-line)* DEDENT)?
decl       := font | data | section | state | comment
font       := "font" id string prop*
data       := "data" id string prop*
section    := "section" id string NEWLINE INDENT beat+ DEDENT
beat       := "beat" id string prop* (NEWLINE INDENT (notes | media | prop-line)* DEDENT)?
state      := "state" id prop* NEWLINE INDENT line* DEDENT
line       := node-line | remove-line | prop-line | choreo-line | notes | comment
node-line  := id (type-spec)? value? prop* (NEWLINE INDENT prop-line* DEDENT)?   -- continuation lines are props
type-spec  := ("text" | "shape" | "image" | "stack" | "grid" | "frame" | "group")
            | ("chart" ":" kind) | ("shader" ":" kind)
remove-line:= "-" id
choreo-line:= "choreo" (id | "[" id ("," id)* "]") prop*          -- one choreography item
media      := "media" prop*
prop-line  := prop+
prop       := key ":" value
value      := string | number unit? | id | path | "@" id | list | map | call
list       := "[" (value ("," value)*)? "]"
map        := "{" (key ":" value ("," key ":" value)*)? "}"
call       := id "(" arg ("," arg)* ")"          -- col(1-7), in(left), rect(120,80,900,420)
notes      := "notes" (string | tripleString)
comment    := "#" .* EOL
```

Rules:
- A node line in a state sets that node's props for that state; the node is created in `nodes` on first mention with the given type; later mentions MUST NOT change the type (lint **E104**).
- A bare string after the id sets the primary value (`text` for text nodes, `src` for images).
- `-id` exits a node. Nodes not mentioned track forward (unless the state is `mode:absolute`). A node line with no props (`bg`) makes the node visible with its defaults (JSON: `"bg": {}`).
- `choreo` lines compile to the state's `choreography` array in order; `choreo [a, b]` targets several nodes.
- Reserved slots: `in(canvas)` is full-bleed (the whole canvas), `in(grid)` is the margin box; both exist in every layout template.
- Units: `ms`, `s`, `cu` (default for lengths), `%`. Durations may be theme names (`standard`).
- Decompilation is canonical (see the round-trip guarantee above); formatting is not preserved.

The compiler reports errors with line/column and a JSON-pointer into the compiled document.

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
- Layout is per-snapshot, not per-frame. Frames only sample.
- Multiple `formats` re-run layout with a different template set; the spine and nodes are shared.
- The engine exposes a **C ABI** (`scaena-ffi`, via `cbindgen`) and a **WASM API** (`scaena-wasm`, via `wasm-bindgen`) with the same surface: load bundle, list states, resolve timeline, `frame(state, t, viewport) -> DisplayList`, `lint`, `patch`.

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
    { "layer":  { "node": "title", "transform": [1, 0, 0, 1, 0, 0], "opacity": 1, "blend": "normal", "clip": null, "ops": [
      { "glyphs": { "font": 0, "size": 128, "coords": [0, 8192, -4096], "paint": { "solid": "#F2F0E9FF" },
                    "glyphs": [[38, 96, 300], [72, 131.5, 300]] } }
    ] } },
    { "image":  { "asset": "sha256:...", "src": [0, 0, 640, 480], "dst": [96, 96, 640, 480], "quality": "high" } },
    { "stroke": { "path": "M0 0L10 0", "paint": { "linear": { "start": [0, 0], "end": [10, 0], "stops": [[0, "#FF6A3DFF"], [1, "#FF6A3D00"]] } },
                  "width": 2, "cap": "round", "join": "round", "miterLimit": 4, "dash": [], "dashOffset": 0 } }
] }
```

Rules:
- Ops are stateless and in paint order: later ops draw over earlier ones. A `layer` scopes `transform`, `clip`, `opacity`, and `blend` for its children and names the scene `node` it draws; painters isolate it only when they must (opacity below 1, a blend other than normal, or a clip). Nodes are drawn in ascending `z`, ties in scene-graph order (`paint_order`), so op order is a pure function of the document.
- Coordinates are in canvas units; `viewport` is the canvas extent, and painters map it to their output pixels. Glyph positions are in their layer's coordinate space (a text node's layer translates to its box), so moving a node changes one transform, not every glyph.
- Fonts are referenced by index into `fonts` (bundle font ids, in first-use order); painters receive the subset bytes once. A variable instance is its normalized coordinates (F2Dot14, in the font's `fvar` axis order), the exact values `vello` and `vello_cpu` take.
- Glyph positions are final (post-shaping, post-kerning); painters never shape text. **This is the parity guarantee.**
- Shader ops carry parameters, not pixels; a painter either runs the WGSL or the CPU reference.
- In JSON, colors are `#RRGGBBAA` (sRGB, straight alpha) and paths are absolute SVG path data (`M L Q C Z`); in postcard they are four bytes and an element list. Every number is finite: the encoders refuse NaN and infinities rather than writing `null`.
- The display list is the unit of golden testing (§14). Goldens are written with `to_golden_json` (one op per line, one glyph per line, so a diff reads as "this op changed" or "this glyph moved") after `quantize` (§13).

**Painters:** `vello` (GPU via wgpu — WebGPU, Metal, Vulkan, DX12), `vello_cpu` (headless, CI, agents, export), `pdf` (krilla: vector paths, real text with embedded subsets, shaders as images), `svg` (static frames), `png` (via vello_cpu), `video` (PNG/raw frame sequence piped to ffmpeg; frame `n` at `t = n / fps` over the global timeline).

---

## 7. Agent surface

### 7.1 CLI (`scaena`)

```
scaena compile   <deck.scn> [-o deck.json]            # DSL → JSON
scaena decompile <deck.json> [-o deck.scn]            # JSON → DSL (canonical form)
scaena validate  <bundle>                             # schema + semantic validation
scaena lint      <bundle> [--state ID] [--json] [--fix] [--severity error|warning|info]
scaena inspect   <bundle> --state ID [--resolved]     # absolute snapshot, resolved styles, timeline
scaena render    <bundle> --state ID [--t MS] [--size WxH] [--out frame.png] [--display-list out.json] [--painter cpu|gpu]
scaena export    <bundle> --format pdf|png|svg|mp4|webm|html|spine [--states a,b] [--fps 60] [--out DIR|FILE]
scaena patch     <bundle> --ops ops.json [--dry-run]  # JSON Patch (RFC 6902) + semantic ops
scaena diff      <bundle> --from ID --to ID           # what changes between two states (resolved)
scaena theme     <bundle> --apply theme.json          # re-theme; prints lint delta
scaena serve     <bundle> [--port N]                  # dev server: live preview + watch + HTTP API
scaena mcp                                            # stdio MCP server exposing the same operations
```

All commands support `--json`; exit codes: `0` ok, `1` lint errors, `2` invalid input, `3` internal.

`render` without `--t` renders the state at rest. `--size` defaults to the canvas size and must keep the canvas's aspect ratio (to the nearest pixel): painters scale uniformly, never stretch. `--out` defaults to `<state>.png`. The display list it writes is unquantized (§13.4: only comparisons round).

### 7.2 MCP tools

Tool names mirror the CLI: `deck_create`, `deck_read`, `deck_patch`, `deck_lint`, `deck_inspect`, `deck_render` (returns image content + display-list digest), `deck_export`, `deck_diff`, `theme_apply`, `data_attach`, `spine_read`, `spine_update`. Each tool's input/output schema is generated from the Rust types (`schemars`) and shipped in `docs/schema/mcp/`. The MCP server also exposes **resources**: `scaena://schema/deck`, `scaena://schema/theme`, `scaena://lint/catalog`, and `scaena://examples/*`, so an agent can learn the format without docs.

### 7.3 Patch semantics

- **JSON Patch** (RFC 6902) against `deck.json` with JSON-pointer paths, plus **semantic ops** that compile to JSON Patch:
  `add_node`, `remove_node`, `set_prop {state, node, prop, value}`, `add_state {after, id, slide?}`, `move_state`, `set_text`, `bind_data`, `apply_preset`, `retheme`.
- Patches are validated before apply; a failed patch is atomic (nothing applied).
- Every patch becomes a CRDT change (§8); `--dry-run` returns the lint delta without committing.

### 7.4 Lint results

```jsonc
{ "code": "E100", "severity": "error", "message": "Text overflows its box by 2 lines",
  "path": "/states/3/props/title", "state": "revenue", "node": "title",
  "measure": { "lines": 5, "maxLines": 3 },
  "hint": "Shorten to ~18 words, or set fit:shrink (minSize 72).",
  "fix": [ { "op": "replace", "path": "/nodes/title/fit", "value": "shrink" } ] }
```

`fix` is optional and MUST be a valid patch; `lint --fix` applies all safe fixes (never content changes).

### 7.5 Lint catalog (initial)

Three families. **Mechanical** rules say "this cannot be shown" (1xx). **Design** rules say "this is shown badly" (2xx typography, 3xx theme/motion/charts). **Narrative** rules say "this does not make the argument" (4xx; most need `semantic` and the spine). The loop an agent runs is generate → critique → repair → look, and the narrative family is what turns the lint engine from a compiler into a critic.

| Code | Sev | Rule |
|---|---|---|
| E100 | error | text overflow (wrap/clip) |
| E101 | error | unintended collision between nodes in the same layer |
| E102 | error | reference to unknown node/state/data/font/asset |
| E103 | error | chart field missing in data / type mismatch |
| E104 | error | node type changed across states |
| E105 | error | duplicate id |
| E110 | error | body text contrast < 4.5:1 |
| E111 | error | display text contrast < 3:1 |
| E120 | error | font lacks glyphs for content (after fallback within bundle) |
| W200 | warn | widow/orphan |
| W201 | warn | line exceeds role measure |
| W202 | warn | display role exceeds `maxLines` |
| W203 | warn | shrink reached `minSize` |
| W210 | warn | density: words/slide above theme threshold |
| W220 | warn | mixed alignment within a state |
| W221 | warn | baseline-grid violation |
| W300 | warn | style literal outside `overrides` |
| W301 | warn | absolute `rect` placement in a template-managed state |
| W310 | warn | chart label collision |
| W320 | warn | too many concurrent animations |
| W321 | warn | build exceeds `maxBuild` |
| W401 | warn | state not referenced by any beat |
| W410 | warn | image without `alt` |
| W420 | warn | *(narrative, reserved)* beat's claim has no node with `semantic: claim` in its states |
| W421 | warn | *(narrative, reserved)* evidence shown with no claim in the same state |
| W422 | warn | *(narrative, reserved)* evidence outranks the claim in visual hierarchy (role size) |
| W423 | warn | *(narrative, reserved)* beat cites `evidence` that no state shows |
| W424 | warn | *(narrative, reserved)* claim density: more than N claims in one state |
| W425 | warn | *(narrative, reserved)* two consecutive beats with identical claims |
| I400 | info | state identical to previous (no-op cue) |
| I401 | info | node never visible |
| I402 | info | override makes node theme-unsafe (count) |

Reserved narrative rules are specified here so codes are stable; they are implemented in PLAN 1.15 only where `semantic` and the spine give them something to check, and are never errors in v1.

Rules live in `crates/scaena-core/src/lint/` (document-level) and `crates/scaena-engine/src/lint/` (layout-level); each rule is a struct with `code`, `severity`, `check(&Context) -> Vec<Finding>` and a fixture deck under `tests/lint/`.

### 7.6 Skills

Prompts and procedures for agents ship in-repo as skills (`skills/<name>/SKILL.md`): `author-deck` (interview → spine → states → lint loop), `retheme`, `tighten-copy`, `chart-from-data`, `motion-pass`. Skills call the CLI/MCP; they are versioned with the format.

---

## 8. Storage, versioning, collaboration

### 8.1 CRDT document

The document lives in a **Loro** document (fallback option: Automerge; ADR-0002). Containers:

- `meta` — map
- `nodes` — map of maps (`id → props`)
- `states` — movable list of maps (ordered cue list)
- `spine` — tree (sections → beats)
- `data`, `fonts`, `overrides` — maps
- long text fields (`notes`, `runs`) — rich text containers

`deck.json` is an **export** of the CRDT state (canonical for git, diffs, agents). `history/deck.loro` carries the full history. On load, if `deck.json` is newer than the CRDT snapshot (e.g., edited by hand or by an agent via files), the diff is imported as a change authored by `fs`.

### 8.2 Ops, undo, branches

- Every edit (UI, CLI, MCP) is a change with an author (`user`, `agent:<name>`, `fs`), a timestamp, and an optional message.
- Undo/redo via the CRDT's undo manager, per author.
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

- Static site (Vite + TypeScript). No backend.
- Engine as WASM in a **Web Worker**; rendering to an `OffscreenCanvas` via WebGPU (`vello`), falling back to `vello_cpu` → `ImageBitmap` where WebGPU is unavailable.
- Player: keyboard/remote navigation, state scrubber, presenter view in a second window (`BroadcastChannel`), auto-advance for `hold`.
- Editor v1: CodeMirror 6 with a `.scn` mode, live lint gutter, live preview; inspector shows resolved values and override counts; direct manipulation deferred (emits patches when it arrives).
- Storage: OPFS + File System Access API; bundle import/export; **single-file HTML export** (engine + bundle inlined; no network).
- Assistant (BYOK, §11) runs in the worker and uses the same operations as the CLI.

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
| PDF | `pdf` painter per slide (last state of each slide) or per state; tagged; vector text; shaders as images |
| PNG/SVG | per state at any size |
| Video (mp4/webm/ProRes) | global timeline sampled at `fps`; `hold` provides dwell; deterministic frames; audio track optional (external) |
| Single-file HTML | player + bundle, offline |
| **Spine JSON** (`export --format spine`) | sections, beats, claims, evidence, notes, per-beat rendered thumbnails and alt-format renders |
| Infographic | external: spine + `formats:["9:16"]` renders per beat → existing pipeline |
| Motion graphic | external: video export with per-beat chapters |
| Podcast | external: spine claims + notes → script → TTS |

The existing export code integrates against `spine.json` + `scaena render`/`export`; nothing in it needs to know the document format.

---

## 11. LLM integration (BYOK, no backend)

1. **Any agent via MCP** (Phase 1). No keys handled by Scaena.
2. **In-app assistant** (Phase 2/3). Provider adapters: Anthropic Messages API (direct browser calls with the explicit opt-in header), OpenAI, Gemini. Keys: Keychain (Mac), encrypted local storage with a session-only option (web). Keys are never written into a bundle. The assistant uses the same operations as the CLI through function calling; prompts come from `skills/`.
3. **On-device** (Mac): Apple Foundation Models for lint explanations, notes drafting, copy tightening.

---

## 12. Repository layout

```
crates/
  scaena-core     document model (serde + schemars), ids, tracking resolution, timeline math (easing, springs), document-level lints
  scaena-engine   theme cascade, layout (taffy), text (parley/harfrust), charts→marks, shaders (CPU ref + WGSL), timeline resolution, sampling, display list
  scaena-paint    painters: vello (gpu), vello_cpu (cpu)
  scaena-export   pdf (krilla), svg, png, video (ffmpeg driver), html, spine
  scaena-cli      `scaena` binary
  scaena-mcp      MCP server (rmcp) over the same operations
  scaena-wasm     wasm-bindgen bindings
  scaena-ffi      C ABI (cbindgen) for Swift
  scaena-store    CRDT document (loro), ops, history, bundle I/O
web/              Vite + TS player/editor (Phase 2)
apps/mac/         SwiftUI client (Phase 3)
docs/             SPEC, PLAN, ADRs, schemas, examples
skills/           agent skills (SKILL.md)
tests/            golden display lists, golden rasters, lint fixtures, parity harness
```

---

## 13. Determinism contract

1. No wall clock in the engine. `t` is an input.
2. All randomness is seeded from the document (`seed` fields) and the node id.
3. Fonts come from the bundle only; system font discovery is not compiled into the engine. Shaping is `harfrust` via `parley`, font reading is `skrifa`; the same bytes produce the same glyph runs on every platform.
4. Floating point: layout and interpolation use `f32` with a fixed evaluation order; display lists are compared bit-for-bit after rounding lengths to 1/64 cu and transform linear parts to 2⁻¹⁶ (`quantize`; rounding a rotation's sine to 1/64 would erase it). (If platform `f32` drift appears in practice, the golden test rounds; the contract does not.)
5. GPU vs CPU raster parity is tested per fixture with a tolerance (ΔE in Oklab ≤ 1.0 on 99.9% of pixels; AA edges excluded by a 1-px dilation mask).
6. Export frames are produced by the CPU painter unless the caller opts into GPU.

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
- **Agent loop test:** an MCP smoke test that creates a deck, introduces an E100, lints, applies the fix, re-lints to zero errors, renders.

---

## 15. Performance budgets

Budgets are per stage, on named benchmark decks, on a reference machine (M-series Mac, 8 performance cores; Linux CI numbers are recorded but not gated). "Cold" includes bundle load and font parsing; "warm" means caches primed. Numbers are targets measured by `criterion` benches from Phase 0 onward; a regression against the recorded baseline fails CI.

**Benchmark decks** (`tests/bench/`): **B1** text-heavy, 40 states, 4 fonts, no charts; **B2** chart-heavy, 12 states, 6 charts with key morphs; **B3** shader-heavy, 8 states, full-bleed mesh + grain on every state; **B4** the torture deck from PLAN 0.2.

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
| Video export, CPU path (sample + CPU paint + PNG encode, pipelined) | ≥ 1× realtime at 1080p60 (B1, B2) · ≥ 0.5× (B3) | B1–B3 |
| Video export, GPU path (sample + GPU paint + readback + encode) | ≥ 2× realtime at 1080p60 | B1–B3 |

Notes: the cold PNG budget and the video budgets are different workloads and are deliberately not comparable — video frames never re-run layout (§5) and pipeline paint and encode across threads. Shader-heavy decks are allowed to cache shader tiles between frames when uniforms are unchanged. If the CPU video path cannot reach 1× realtime on B1 by gate 1, the video exporter defaults to GPU where available and the budget is revisited in an ADR rather than silently relaxed.

---

## 16. Open questions

1. Vertical text and complex script timing (phase?).
2. Video/audio nodes — decoding in the engine or in the client?
3. Font licensing: enforce embedding permissions bits at bundle time? (Proposal: warn, don't block.)
4. Live data sources and refresh semantics.
5. Direct-manipulation editing model and how it expresses patches in template-managed layouts.
6. Multiple formats: how much per-format override is allowed before it's a second deck?
7. Collaboration conflict UX when it arrives.

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

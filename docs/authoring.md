# Writing a deck

A Scaena presentation is a **deck**: one set of objects, and a timeline of **states** that changes them, one click at a time. You write it as `.scn` source, a short indented text format. Before Scaena draws anything, it checks the deck three ways:
- **the grammar:** does it parse;
- **the schema:** is every property real;
- **lint:** can it be shown, and shown well.

This guide is for people. It covers writing a deck, checking it, seeing it, saving it, and presenting it. Agents follow `skills/author-deck/SKILL.md`, which covers the same operations.

## What you are editing

A deck lives in a **bundle**: a folder, or the same folder zipped as a `.scaena` file.

```
my-talk/
  deck.json              the deck: what Scaena reads and saves
  deck.scn               optional: the deck as source, if you keep one by hand
  themes/dusk.theme.json the theme it names
  fonts/  data/  assets/ the files it names
  manifest.json          written by a save
  history/deck.loro      optional: every change, and who made it (scaena save --history)
```

- **`deck.json` is the deck. `.scn` is the same deck written for people.**
  - `scaena compile` turns source into `deck.json`, and `scaena decompile` turns any deck back into source.
  - The round trip is exact for the deck, but not for your formatting. A comment on its own line, or at the end of one, is kept with the next deck header, node, override, state, or node line. Other comments, blank lines, and alignment are not kept.
- **Objects exist for the whole deck. A state lists only what changes.**
  - What a state does not mention stays as it was.
  - The same id in a later state is the same object, so it moves or morphs rather than cutting.
  - `-id` takes an object off.
- **The theme owns the look.** You write names, never sizes, colors, or coordinates:
  - a role: `role:headline`;
  - a layout and a slot in it: `layout:figure`, then `at:in(main)`;
  - a motion preset: `enter:rise`.

  [The theme's names](#the-themes-names) lists them. A literal size or color outside `overrides` is lint W300.

## Three ways to work

| | You need | Good for |
|---|---|---|
| [The editor](#the-editor-edit-save-see), in a browser | the site folder, a browser, and `python3` to serve it | writing, and seeing each change as you type |
| [The command line](#the-command-line-edit-check-look) | a clone of the repository, and Rust | your own text editor, git, scripts, CI, exports to PDF and video |
| [An assistant](#an-assistant) | an API key, or an MCP client | first drafts, "make the chart a stacked bar", repairs lint asks for |

All three run the same engine, and the same checks.

## The editor: edit, save, see

Serve the site folder and open `editor.html`. The folder is `target/site` from `just site`, or a copy of it. Its `serve.py` serves it on this machine only:

```sh
cd target/site && python3 serve.py      # then open http://localhost:8080/editor.html
```

- **Edit.** The deck is on the left, as `.scn`.
  - Each keystroke compiles. Source that does not compile says where, and the preview keeps the last deck that did.
  - The state picker jumps to a state's line.
  - The format picker shows the deck in each of its `formats`.
- **Move.** Click an object in the preview to select it, and drag it.
  - It snaps to the theme's grid and the layout's slots, and the move becomes a change to your source: one ⌘Z undoes it, in the preview or the source.
  - Before you let go, the status says in how many states the move shows. Hold Alt to keep it to the state shown, and Shift to place it freely, off the grid, which lint flags as an override (W301).
  - Handles resize it by the grid's tracks. The arrow keys move it a track at a time, and Shift with an arrow resizes it. Escape selects what holds it.
- **Type.** Double-click a text in the preview, or press Enter with it selected, and type where it stands.
  - Your typing goes where the text is set: in the state that sets it, or in the object itself, and the status says in how many states it shows. Double-click with Alt held to keep it to the state shown.
  - A text in several looks (`runs`) keeps them: what you type takes the look of the words around it.
  - Each burst of typing is one ⌘Z. The text reflows as you type, and lint says at once if it no longer fits. Escape, or a click outside the text, stops typing.
- **See.** The preview on the right shows the state the cursor is in, at rest.
  - **Play** opens the player in a new tab, with the motion, on the deck as last saved. A folder opened from disk has no Play: the player cannot open it. A new deck has none until its first save.
  - In the player, → and ← step, and F is fullscreen. **Presenter** opens your notes, the next state, and a clock in a second window, and **Edit** goes back to the editor.
- **Check.** Each edit lints the state shown, and every state once you stop typing.
  - Findings stand in the gutter at the line they are about, and in the list under the source. F8 goes to the next one.
  - A finding that has a fix offers it as one click. Only the lines the fix changes change.
  - The **Inspector** tab shows the state's timing, its objects, and each text's resolved look.
- **Save.** Where the deck goes depends on where it came from:

| | |
|---|---|
| **New…** | a new deck: a title and a theme (Dusk, Daybreak, or Ember), with its fonts and one empty state. It is kept nowhere until you save it. |
| **Open folder…** (Chrome, Edge) | a bundle folder on your disk. **Save** writes back into it. Use this to keep your work as files. |
| **Open .scaena…** | a `.scaena` file, copied into the browser's storage. |
| **Kept in this browser** | the bundles this browser keeps. A deck opened from the site's own folder saves here, under its name. |
| **Save** (⌘S) | writes the bundle where it is kept, as `scaena save` writes it, fonts whole. Files are renamed by their content (a hash in the name), and `manifest.json` is written; that is normal. Source that does not compile is not saved. |
| **Save as…** | the bundle saved somewhere new, and kept there from then on: under another name in the browser, or, in Chrome and Edge, in a folder on your disk. |
| **Download .scaena** | the bundle as one file, fonts subset, to keep, send, or open with the command line. |
| **Served** (`scaena serve`) | the folder on disk the server serves. **Save** writes back into it, in any browser, and into its `deck.scn` where it keeps one; the editor opens that `deck.scn` as its source. A change on disk comes in as it is made, or, over changes of yours not saved, is offered. |

A file dropped on the source joins the bundle, and its path, quoted, goes where you dropped it: a font under `fonts/` and a data file under `data/`, by name, and an image under `assets/`, named by its content. A `.scaena` file dropped opens instead. The page warns before it closes over changes not saved.

**Your own deck in the site.** Copy its folder into the site's `decks/`, then open `editor.html?bundle=decks/my-talk/`, or the same address on `index.html` to play it. **Open folder…** shows only where the browser can open one (Chrome and Edge, not Safari or Firefox). Elsewhere, saves stay in the browser until you **Download .scaena**, or until you serve the folder with `scaena serve` instead.

**Start a deck.** **New…** asks for a title and a theme. The deck it makes has the theme, the fonts the theme names, and one state with nothing on it, as an assistant's `deck_create` makes one. Its first **Save** keeps it in the browser under a name made from its title; **Save as…** puts it in a folder on your disk instead. To start from another deck, open that one and **Save as…** under a new name.

## The command line: edit, check, look

**Set up once** on a Mac: Xcode's command-line tools, Rust from [rustup.rs](https://rustup.rs), then the CLI.

```sh
git clone https://github.com/welshofer/Scaena.git && cd Scaena
cargo install --path crates/scaena-cli --locked    # puts `scaena` in ~/.cargo/bin
```

`scaena export --format html` needs the web pages built first: run `just web`, then install the CLI again (ADR-0010). That needs [`just`](https://github.com/casey/just), Node, the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` 0.2.129.

**The loop.** Start a deck, turn it into source, serve it, and write:

```sh
scaena new talk --theme dusk --title "Field notes"   # a new deck: dusk, daybreak, or ember
scaena decompile talk -o talk/deck.scn            # the deck as source
scaena serve talk                                 # then open http://localhost:4848/, and leave it running
```

- **Start from what you have.** `scaena new` makes a bundle as the editor's **New…** does, from a theme the CLI carries with its fonts; `--theme` also takes a theme file. A deck someone sent as a `.scaena` file is a zip: `unzip talk.scaena -d talk` makes it a folder.
- **Each save** of `talk/deck.scn`, in any text editor, compiles into `talk/deck.json`, and the player shows it at the state it was on.
- **A line that does not compile** is shown at its line, in the terminal as `compile` shows it and over the slide, and the deck stays as it was.
- **`http://localhost:4848/edit`** is the editor on the same folder. Its saves go back into the folder, `deck.scn` included, in any browser.
- **This machine only.** `serve` answers on 127.0.0.1 and writes only inside the folder. It needs a `scaena` built after `just web`.

Without the server, each step is a command of its own:

```sh
scaena compile talk/deck.scn -o talk/deck.json    # the grammar, the schema, every reference
scaena lint talk                                  # layout, contrast, motion, the argument
scaena render talk --state revenue --out revenue.png && open revenue.png
```

- **`compile`** writes `deck.json` only if the deck is valid. Otherwise it shows each error at your line, and writes nothing.
- **`lint`** exits 1 when it finds an error, so a script or CI can stop on it. `--fix` applies the fixes lint has checked by laying the state out again.
- **`render`** draws a state at rest. `--t 400` draws 400 ms into its cue, and `--format 9:16` another of the deck's formats.
- **To see it play without the server,** export one file and open it: `scaena export talk --format html --out talk.html`.
- **Other exports:** `scaena export talk --format pdf --out talk.pdf`. `png` and `svg` write an image of each state into a directory. `mp4`, `webm`, and `prores` need `ffmpeg`.
- **More commands:**
  - `scaena inspect talk --state revenue --resolved` shows what a state holds after the theme.
  - `scaena diff talk --from revenue --to mix` shows what a click changes.
  - `scaena patch` makes edits as JSON operations (SPEC §7.3).

**Pick one place to write.** If you keep `deck.scn`, it is your source: `scaena serve` compiles it as you save, or `compile` does when you ask. The editor on a served folder opens and saves that same `deck.scn`. The editor anywhere else writes `deck.json` alone, and leaves a `deck.scn` behind. If you edit only in the editor, `deck.json` is the source, and `scaena decompile` gives you its `.scn` whenever you want it.

## An assistant

The editor's **Assistant** tab works on the open deck with your own key: Anthropic, OpenAI, or Gemini. Ask in plain words, such as "put the headline 'Revenue doubled' on the second slide and fix what lint finds". It edits through the same operations, lints, renders, and shows you each step. Each of its edits shows in the source as it happens, and undoes as one.

Any MCP client can do the same from outside the browser. `scaena mcp` serves the operations as tools, and the format, SPEC, and lint catalog as resources (SPEC §7.2).

## How a deck is checked

### 1. The grammar

`.scn` has a grammar: SPEC §4.2 gives it rule by rule, and SPEC §4.3 gives the canonical form the decompiler writes. The compiler in `crates/scaena-core/src/dsl/` is its reference implementation. Tests hold `compile(decompile(deck)) == deck` for every deck in the repository and for thousands of random variations on them (`crates/scaena-core/tests/dsl.rs`), and hold each error to where it points.

Source that does not parse exits 2, at the line and column:

```
  × this string is not closed on its line
   ╭─[deck.scn:3:10]
 2 │ state a
 3 │   t text "unterminated
   ·          ─
   ╰────
```

### 2. The schema: the DTD

`deck.json`'s structure is a JSON Schema: `docs/schema/deck.schema.json`, with `theme.schema.json`, `patch.schema.json`, `manifest.schema.json`, and `spine.schema.json` beside it.
- **The schemas are generated from the typed model in the code, so they cannot drift from what the engine reads.** No one edits them by hand (ADR-0007).
- **`scaena validate`** checks a bundle against them. It then checks what a schema cannot say:
  - every reference: a node, a state, a file, a theme name (E102);
  - what charts read from their data (E103);
  - that no object changes type (E104);
  - ids (E105);
  - each state, resolved, against its objects' types (E106).
- **`compile` runs the same checks,** and shows each finding at the source that wrote it, with the JSON pointer where it lands in `deck.json`. A finding exits 1, and nothing is written:

```
E106

  × unknown property `sise`; did you mean `size`?
    ╭─[deck.scn:23:46]
 22 │   -claim
 23 │   title "Pro drove the growth" role:headline sise:12 semantic:claim at:in(header)
    ·                                              ───┬───
    ·                                                 ╰── /states/1/props/title/sise
 24 │   rev chart:bar data:@q3 x:{field: quarter, type: ordinal}
    ╰────
```

To check `deck.json` as you type in VS Code, map the schema in the workspace's `.vscode/settings.json`, by its path from the workspace's folder. `deck.json` takes no `$schema` key: its top level allows only the deck's own keys.

```json
{ "json.schemas": [{ "fileMatch": ["**/deck.json"], "url": "./docs/schema/deck.schema.json" }] }
```

### 3. Lint

A valid deck can still be shown badly. `scaena lint` lays out every state, in every format the deck lists, and judges what it would show. Each finding has:
- a stable code, a severity, and a message;
- where it is: a JSON pointer, a state, an object, and a format;
- what it measured, such as how far text overflows;
- a hint, and often a fix.

| Codes | Family | For example |
|---|---|---|
| E100–E120 | mechanical: it cannot be shown | text that does not fit its box (E100), objects that collide (E101), text that does not contrast with what is painted behind it (E110, E111), characters the font has no glyph for (E120) |
| W200–W230 | typography and fonts | a widow (W200), too many words on screen (W210), a font whose license forbids what a bundle does with it (W230) |
| W300–W322 | the theme, charts, motion | a literal where a theme name goes (W300), chart text too small to read (W312), too much moving at once (W320) |
| W401–W426 | the argument | a state no beat names (W401), a slide with evidence and no claim (W421), two claims on screen (W424) |
| I400–I402 | information | a state that changes nothing (I400) |

An error is something an audience would see go wrong: text cut off, objects on top of each other, words they cannot read. A warning is design advice: take it seriously, and decide. SPEC §7.5 lists every rule, and `tests/lint/` holds a deck that triggers each one and a deck that must not.

The narrative rules read two things you write: each object's `semantic` (`claim`, `evidence`, `navigation`, …), and the **spine**, the outline of sections and beats that says what each part of the talk claims.

## The language, in one deck

This deck has three states over two slides. It uses the Dusk theme, its fonts, and `data/q3-revenue.csv`, all in `docs/examples/`. It compiles and lints with no findings, and a test holds it to that.

```scn
deck "Q3 Review" theme:"themes/dusk.theme.json" canvas:1920x1080 lang:en-US

font Fraunces "fonts/Fraunces-VF.ttf" axes:{wght: [100, 900], opsz: [9, 144]}
font Inter "fonts/Inter-VF.ttf" axes:{wght: [100, 900], opsz: [14, 32]}
font "JetBrains Mono" "fonts/JetBrainsMono-VF.ttf" axes:{wght: [100, 800]}

data q3 "data/q3-revenue.csv"
  schema:{quarter: string, product: string, revenue: number, customers: number}

section open "Open"
  beat opening "Revenue doubled this year." states:[cover]

section growth "Growth"
  beat doubled "Pro drove the growth." evidence:[@q3] states:[revenue, mix]

state cover layout:title
  title text role:display "Q3 Review" semantic:navigation at:in(title)
  claim text role:title "Revenue doubled this year." semantic:claim at:in(subtitle)
  notes "Let the title sit before you speak."

state revenue layout:figure transition:standard
  -claim
  title "Pro drove the growth" role:headline semantic:claim at:in(header)
  rev chart:bar data:@q3 x:{field: quarter, type: ordinal}
    y:{field: revenue, type: quantitative, format: "$,.1f", domain: [0, null], title: "Revenue ($M)"}
    series:{field: product, type: nominal} color:{field: product, type: nominal, scale: categorical}
    alt:"Quarterly revenue by product." semantic:evidence at:in(main)
  choreo rev enter:grow

state mix slide:revenue
  rev kind:stackedBar
```

Line by line:
- **The header.** `deck "Q3 Review" theme:… canvas:1920x1080` names the theme and the canvas in canvas units, which the painters scale to any size. Add `formats:[9:16, 1:1]` to lay the deck out again in other shapes.
- **Fonts and data.** `font` lists each of the theme's font families, with its file in the bundle. `data q3 "…"` declares a data source and its columns' types. A chart reads it as `@q3`.
- **The spine.** `section` and `beat` say what each part claims, and which states make the claim. The PDF, the video's chapters, and the narrative lint read them.
- **`state cover layout:title`** is the first click, on the theme's `title` layout.
  - `title text role:display "Q3 Review" … at:in(title)` declares an object where it first appears: its id, its type, its text, its role, and its slot.
  - `notes` are the speaker's notes.
- **`state revenue layout:figure transition:standard`** is the next click, on another layout:
  - `-claim` takes the subtitle off;
  - `title "Pro drove the growth" role:headline … at:in(header)` is the same `title`, so its words morph and it moves to its new slot;
  - `rev chart:bar …` enters, and `choreo rev enter:grow` grows its bars in.
- **`state mix slide:revenue`** is a build on the same slide. `rev kind:stackedBar` turns the same bars, matched by key, into stacks.

What the props mean is in SPEC §3: the objects in §3.3, layout in §3.4, type in §3.5, charts in §3.7, and motion in §3.9. `docs/examples/revenue.deck.scn` is a fuller deck, and `docs/examples/trails.deck.json` (decompile it to read it as source) is fifteen slides that use most of what a deck can hold.

## The theme's names

The shipped themes, Dusk, Daybreak, and Ember, share these names, so a deck moves between them with no other change (`scaena theme --apply`). A name the theme does not have is E102. A test holds this section to the themes.

| Layout | Its slots |
|---|---|
| `title` | `kicker`, `title`, `subtitle`, `art` |
| `statement` | `kicker`, `statement`, `support` |
| `poster` | `kicker`, `statement`, `support`, `note`, `art` |
| `full` | `kicker`, `header`, `main`, `note` |
| `figure` | `kicker`, `header`, `main`, `note` |
| `narrow-figure` | `kicker`, `header`, `main`, `note` |
| `stat` | `kicker`, `number`, `claim`, `detail`, `under` |
| `split` | `body`, `main` |
| `art-left` | `kicker`, `header`, `art`, `body` |
| `art-right` | `kicker`, `header`, `body`, `art` |
| `quote` | `quote`, `who` |

Every layout also has `in(canvas)`, the whole canvas, and `in(grid)`, the margins' box. `at:col(1-6) row(2-8)` places by the theme's 12 × 12 grid instead.

| Names | |
|---|---|
| Text roles (`role:`) | `display`, `headline`, `title`, `body`, `caption`, `label`, `axis`, `value`, `lede`, `kicker`, `figure`, `quote`, `numeral`, `code` |
| Motion presets (`enter:`, `exit:`, `emphasis:`) | `fade`, `rise`, `grow`, `draw`, `words`, `pulse` |
| Durations (`transition:`, `duration:`) | `fast`, `standard`, `slow` |
| Easings (`ease:`) | `standard`, `in`, `out`, `linear` |
| Springs (`spring:`) | `snappy`, `gentle`, `heavy` |
| Colors | `ink`, `paper`, `paper-2`, `accent`, `accent-2`, `muted`, `line` |
| Color roles | `surface`, `surface-2`, `onSurface`, `onSurfaceMuted`, `accent`, `onAccent`, `line` |
| Shader presets (`preset:`) | `backdrop`, a mesh behind a title; `texture`, a slow noise field |

A color names a theme color or a color role: `style:{color: accent}` on text, `fill:surface-2` on a shape or a container. A shader takes a preset whole, its kind included: `bg shader:mesh preset:backdrop at:in(canvas) z:-100`.

## Recipes

Each recipe adds to the deck above, and each compiles; a test holds them to that. A state that no beat names is lint W401, so add a new state's id to a beat's `states:[…]`.

**A new slide, with its own beat.** Take the last slide's objects off, and put new ones on.

```scn
section close "Close"
  beat end "Thank you." states:[close]

state close layout:statement
  -title
  -rev
  thanks text role:display "Thank you" semantic:navigation at:in(statement)
  notes "Take questions."
```

**New words for the next click.** The same id with new text morphs, word by word.

```scn
state faster slide:revenue
  title "Pro grew three times faster"
```

**A line too long for its slot.** Lint says E100, with how much it overflows. Shorten it first. Otherwise, let it shrink to the role's smallest size; its fix offers this:

```scn
state long slide:revenue
  title "Pro drove the growth this year, and Enterprise is next in every region" fit:shrink
```

**Motion, and a self-running deck.** `transition` names how long the cue takes; `choreo` brings an object in with a preset, after a delay; `hold` moves on by itself after the cue, for a kiosk or a video.

```scn
state turn layout:statement transition:slow hold:4s
  -title
  -rev
  point text role:display "One more thing" semantic:claim at:in(statement)
  choreo point enter:rise delay:240ms
```

**A picture.** A PNG in the bundle's `assets/` folder, with `alt` text for a reader (W410 without it):

```scn
state ridge layout:art-right
  -rev
  title "The ridge trail, rebuilt" at:in(header)
  photo image "assets/trails-ridge.png" alt:"The rebuilt ridge trail at dawn." semantic:evidence
    at:in(art)
```

For tables, stats, rows of cards, other chart kinds, and shader backgrounds, see SPEC §3.3 and the trails example.

## Present and export

- **Present** from the player: → and ← step, F goes fullscreen, and **Presenter** opens the presenter view. `?fps` in the player's address shows its frame rate.
- **One file that plays anywhere, offline:** `scaena export talk --format html --out talk.html`. It is the player and the deck in one HTML file, for a USB stick or an email.
- **PDF:** `scaena export talk --format pdf --out talk.pdf`. It draws each slide at its last state, in spine order, with real text a reader can copy.
- **Images and video:** `--format png` or `--format svg` write each state into a directory. `--format mp4` plays the timeline, cues and holds, at `--fps`, with a chapter per beat.

## Reference

- SPEC §3: the document model, each object type and its props.
- SPEC §4: the `.scn` format and its grammar.
- SPEC §7.1: every command and flag.
- SPEC §7.5: every lint rule.
- `docs/schema/`: the JSON Schemas.
- `docs/examples/`: whole decks, themes, fonts, and data to start from.
- `skills/`: the procedures agents follow, which read as checklists for people too.
- `web/README.md`: the player and the editor in depth.

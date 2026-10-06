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
  - Press ⌘' (Ctrl+'), or Grid beside the zoom, to see the theme's grid over the slide: its columns and rows, the gutters and margins between them, and the baseline grid its text sits on. Press it again to hide it. As you drag or resize, a line shows wherever an edge or the middle of what you move meets another object's, or the slide's; with Shift, off the grid, it snaps onto one within a few pixels. Hold ⌘ (Ctrl) as well to place it exactly where you drag.
- **Several at once.** Shift-click to add an object beside the one selected, or take it away; they must sit in the same place, side by side on the slide or in the same container. Or drag across an empty part of the slide to select what the box encloses.
  - Drag them, or press an arrow key, and they move together, keeping where they stand about each other. Delete, ⌘D, ⌘C, and ⌘X act on all of them, and a paste puts them down as they stood.
  - The inspector aligns them by their left, right, top, or bottom edges or their middles, on the grid's tracks, and spreads three or more evenly. It offers what all of them have, and what you choose there is theirs.
  - ⌘] brings what is selected in front of the next thing it overlaps, and ⌘[ sends it behind; with Shift, in front of or behind everything beside it. Each is one ⌘Z.
  - ⌘G (Ctrl+G) puts what is selected in a group, which moves, fades, and enters as one; ⌘⇧G takes a group apart again. Nothing moves either way. The objects in a stack or a grid are already held together, and stay as they are.
- **Zoom.** ⌘+ and ⌘− (Ctrl on Windows and Linux), or the zoom's buttons beside the Insert menu, bring the slide closer, up to eight times, and ⌘0 shows it whole again. Pinch, or turn the wheel with ⌘ held, to zoom about the pointer. Zoomed in, the wheel moves what you see, and so does a drag with Space held. The slide is painted again at each size, so text and edges stay sharp, and everything you do zoomed in, a drag, a resize, typing, lands as it would at the whole slide.
- **Find and replace.** ⌘F (Ctrl+F), or Find, opens a bar above the slide that finds words in every slide's text. Enter goes to the next one and shows it, marked; Replace changes it, and Replace All changes every one at once, one step to undo. A text that several slides show is changed once, where it is written, so it reads the same in all of them. Match case and Whole words narrow what is found. On the command line, `scaena find deck "Q3" --replace "Q4"` does the same.
- **Time the build.** Under the preview, the state's cue is a timeline: the transition, then a bar for each thing that moves.
  - Drag a bar to start it later or sooner, and drag its end to make it longer or shorter. Each change is written where the motion is set, in the state's choreography or on the object itself, and one ⌘Z undoes it. A motion on a spring lasts as long as it takes to settle, so its end stays put.
  - Add a motion picks one of the theme's presets for the object selected: as it comes on, or for emphasis. Objects that leave in this state can be given one as they go.
  - Press or drag along the ruler to see the cue at that moment, and Play to watch it run. Click the slide to edit it at rest again.
- **Type.** Double-click a text in the preview, or press Enter with it selected, and type where it stands.
  - Your typing goes where the text is set: in the state that sets it, or in the object itself, and the status says in how many states it shows. Double-click with Alt held to keep it to the state shown.
  - A text in several looks (`runs`) keeps them: what you type takes the look of the words around it.
  - Each burst of typing is one ⌘Z. The text reflows as you type, and lint says at once if it no longer fits. Escape, or a click outside the text, stops typing.
  - **Links.** Select some words and press ⌘K (Ctrl+K). Type a web address (`https://…`, or `mailto:…` for an email) or the id of one of your states, and press Enter: the words are underlined and go there when the deck is played, in the single file, and in a PDF. Press ⌘K and Enter on nothing to take the link away. Each is one ⌘Z.
  - **Lists.** Press ⌘⇧8 (Ctrl+Shift+8) for bullets, or ⌘⇧7 for numbers, on the paragraphs your selection touches; press it again to take them out of the list. In a list, Enter starts the next item, and Enter in an empty item ends the list; Tab moves an item a level in, and Shift+Tab out. With a text selected and not typed in, the keys act on all of it. Your theme draws the markers and the indents. Each is one ⌘Z.
  - Select some words and press ⌘B (Ctrl+B) to make them bold, or, bold already, not. ⌘I (Ctrl+I) sets them in italic, or not: the italic is the family's own, and a theme whose family has none sets them upright and says so (W231). With words selected, the inspector shows their look instead of the text's: choose a role or a color there and only those words take it. Each is one ⌘Z.
- **Choose a look.** With an object selected, the inspector shows what your theme offers for it: a text's role, family, weight, color, and fit, a shape's fill and stroke, a chart's kind, an entrance and an exit, and more.
  - Each choice is one ⌘Z, and goes where the value is set: in the state that sets it, or in the object itself. The status says in how many states it shows. Tick "only in" to keep it to the state shown.
  - A color or a length you write out yourself, and any text size, is an override: it goes in the deck's `overrides`, holds in every state, and a new theme does not change it. The inspector marks it so. The × beside a value takes it away where it is set, so what is under it shows.
  - A chart shows what it reads: its data, the column on each axis (offered from the columns its data has, numbers for a value), the series, and how it reads them. Choose another source and the chart reads it: where that source lacks an axis's column, the axis reads another of the same kind, a name for a name and a number for a number, and the chart drops whatever else the source has no column for (a series, say). "Only in" makes it a data update: this state reads the new numbers, and the chart moves to them from the state before.
  - An image shows its fit, its focal point, and its crop. Press Pick, then click the image where its subject is: that point stays in view however the box is shaped, as cover crops it. Escape leaves it as it was. A crop is a part of the image, as fractions of it: x, y, width, and height. Drag an image file from your computer onto an image in the preview to put it in that image's place: it joins the bundle, and one ⌘Z puts the old one back.
  - Every object offers its description and its part in the story: what a reader hears. A description is what a screen reader says for it: what an image or a chart shows, or, for a text, what to say in place of its words (left empty, its words are read). An object marked decoration is not read at all. Under the choices, the inspector says how the object reads, as the player says it to a screen reader; with nothing selected, it reads the state shown part by part, in order, and a part selects its object. An image with no description is W410: its mark offers Describe it, which opens the inspector on the image, its description ready to type.
- **Add, copy, delete.** The Insert menu under the preview offers what your theme and bundle have: a text in each of the theme's roles, a rectangle, an ellipse, a line, an arrow, each image in the bundle, a chart and a table of each data source, and each of the theme's shader backgrounds. The chart is made from the columns the data has: a line over dates, bars over names, grouped where a name repeats.
  - Or draw it: press T (a text), R (a rectangle), O (an ellipse), L (a line), or A (an arrow) on the preview, and drag where it goes. It lands on the grid's cells you covered, or with Shift exactly where you dragged; a line runs the way you dragged. A click puts it where you clicked, a text you draw is ready to type into, and Escape stops drawing.
  - What you choose goes where you last clicked in the preview, or in its middle, snapped to the grid. A text or an image clicked into an empty slot of the layout fills the slot, so it follows the deck into its other formats. A shader fills the canvas, behind everything. It appears from the state shown on, selected, ready to drag or type in.
  - Delete (or Backspace) takes the object selected, and what it holds, out of the state shown and the states after it. Something that no state shows any more leaves the deck, so adding and deleting leaves nothing behind. Shift+Delete takes it out of the deck. ⌘D (Ctrl+D) puts a copy beside it, with copies of what it holds.
  - ⌥⌘C (Ctrl+Alt+C) copies the look of the object selected, as the state shows it: a text's role and style, a shape's fill, outline, and corners, an image's corners, a shader's preset and palette, a chart's labels, a stack's or a grid's gap. ⌥⌘V (Ctrl+Alt+V) gives that look to each object selected, written where each one's own look is set, as a choice in the inspector is: on the object, so it holds in every state, or in the step that sets it. Where the look takes the theme's value, the object's own setting goes, so the theme's shows. An object of another kind takes what it shares with the look, an image a shape's corners; the status says what took nothing, and why. `scaena inspect --state S --look NODE --onto A,B` gives the same patch on the command line.
  - ⌘C (Ctrl+C) copies the object selected, with what it holds, and ⌘X cuts it. ⌘V pastes it where you last clicked, in this deck or another, bringing the data and images it reads. What the other deck's theme lacks, a color or a role, is left out, and the status says what; a text in a role the theme lacks takes the nearest role it has. Text copied from anywhere else comes in as body text.
  - Each is one ⌘Z.
- **Layers.** The Layers tab beside the inspector lists the objects of the state shown, topmost first, inside what holds them. Click one to select it, even one under the rest. Those dimmed are not shown here: they leave in this state, another step of the slide shows them, or no state shows them any more.
  - The eye hides an object in this state, or shows one hidden here. A double click on a name, or F2, renames it everywhere the deck names it.
  - Drag an object up or down among those beside it to put it in front of or behind them; in a stack, to lay it out earlier or later. Alt with ↑ or ↓ moves the one focused a place.
  - Drag it beside an object another container holds, or out among the slide's own, and it goes there; drop it on a container's middle and it goes in, at the top. It keeps its place on the slide where it can: in the grid cells it stands in, inside a frame, at its spot in a stack's order, or after a grid's last cell. Alt with ← takes it out of its container, and Alt with → puts it into the container just above it.
  - Each is one ⌘Z.
- **Layouts.** Press **Layout** above the preview to see the layout your slide uses as its slots, each named, on your theme's grid. Drag a slot to move it, or a handle to resize it: it lands on the grid, and every slide that uses the layout changes with it, because the change is to your theme. In another format (Formats, or the format menu), a slot you move moves in that format only. **New layout from this one…** in ⌘K copies the layout under a name you give it, and the slide shown takes the copy, so you can change it without changing the others. Escape leaves the layout. Each is one ⌘Z.
- **Data.** The Data tab shows a data source your charts and tables read as a table: each column with its type, and each row as the file has it. A cell its column does not read, `n/a` in a column of numbers, is marked and listed under the table, with why. The deck does not check until it reads (E103), and setting the cell here is how you mend it.
  - Click a cell and type. Enter sets it and goes to the cell below, and Escape puts back what it held. A value its column does not read, or one that would leave the deck invalid, is refused, and the status says why. Every chart and table that reads the source shows the change.
  - **+ Row** adds an empty row after the row you are in, and **− Row** takes that row away.
  - The file keeps every byte you did not change, its quoting and line endings included, as `scaena data` writes it. Undo and Redo, or ⌘Z in a cell you have not changed, put it back as it was. A save records each version in the bundle's history.
  - Click a row's cell and the slide outlines what that row draws: its bar, its point, its slice, its row of a table. The line under the table says which, or that nothing on this slide draws it. With the Data tab open, click a bar on the slide and its row is chosen; double-click a bar to open the Data tab on it. A bar your `dataTransform` sums from several rows chooses them all.
- **Annotate a chart.** Select a chart, then click one of its bars, points, or slices, and it is outlined. Right-click it to:
  - highlight it, or its whole series;
  - call it out, with words you type over it;
  - rule a line at its value;
  - band from it to the next bar you click.
  - A donut's slice offers its highlight alone.
  - Click a callout, a rule, or a band to select it. Drag a callout to another bar, double-click it to change what it says, or press Delete to take it away.
  - Each is one edit, written where the chart's annotations are: on the chart, or in the state that changes them. Check **only in** in the inspector, or hold Alt as you drag, to keep it to this state. In the source, they are the chart's `annotations`.
- **A shape's points and corners.** Select a line, an arrow, or a polygon, and each of its points has a handle, with a smaller one at each edge's middle. A line that gives no points shows the two it draws, across its box.
  - Drag a point to move it. It stays inside the shape's box, at a hundredth of the box.
  - Click an edge's middle to add a point there, or drag from it to put the new point where you let go.
  - Click a point to pick it, and press Delete to take it away. A line or an arrow keeps two points, and a polygon three; the status says so.
  - A rectangle has a handle inside its top-left corner. Drag it in to round the corners, or out to square them. It snaps to the theme's radius steps, `radius.0`, `radius.1`, and so on, and the status names the step.
  - Each is one edit, written where the shape's `points` or `radius` are: on the shape, or in the state that changes them. Hold Alt as you drag, or check **only in**, to keep it to this state. Escape leaves the shape as it was.
- **Files.** The Files tab lists what your bundle holds besides the deck: its images, fonts, and data files, each with its size, what in the deck names it, and the objects drawn from it, slide by slide. Click an object there to go to the first slide that shows it, selected.
  - Drag an image from the list onto an image on the slide to show it there instead, or onto an empty spot to add it there. **Insert** adds it where you last clicked the slide. Drop it on the source and its path goes there. Each is one ⌘Z.
  - A file nothing names says so. **Remove** takes it out of the bundle, and the deck looks the same. Undo puts it back, and the next save takes it out where the bundle is kept. A file something names stays, and the status says what names it.
- **Versions.** A bundle that keeps a history (`scaena save --history` begins one) lists its versions in the Versions tab, the newest first, each by who made it and when; each save adds the edits since the last. Click one to see a slide of it as it was, and what changed since, or since another version. Restore makes it the deck again, with its data, as one edit: ⌘Z takes it back.
- **Rehearse.** **Rehearse** plays the deck from the start as you would present it: → or a click goes on, ← goes back, Escape stops. It counts how long you spend on each slide, then shows each one's time and the hold it would keep, its time less its animation. **Keep as holds** writes them all as one edit, so the deck plays by itself at your pace; ⌘Z takes them back.
- **Keys.** Press ? anywhere but the source or a field, or click **Keys**, to see every key the editor answers, grouped by what it does, as your machine writes them: ⌘, ⌥, and ⇧ on a Mac, Ctrl, Alt, and Shift elsewhere. ⌘K lists the same commands by name, and runs them.
- **Formats.** If your deck lists other formats (`formats:[9:16]`), **Formats** beside the format menu shows the slide in each of them side by side under the preview, its own canvas first, and plays them as the preview plays. Each says how many findings it has on the slide shown: a headline that fits the wide slide and runs long in the tall one says so on the tall one. Click one to edit the deck in that format.
- **Theme.** The theme menu beside the format menu lists the themes your bundle holds, the deck's own chosen, and the themes that ship. Choose one to see the deck in it: one ⌘Z takes it back. A theme that lacks a role or a color your deck uses is refused, and the status says which.
  - The Theme tab edits the theme itself: its colors, each with the roles that use it; its type roles, each one's family, size, weight, leading, and tracking; and its spacing. Change a value and press Enter, and every slide that uses it shows the change. A value the theme cannot read is refused, and the status says why. Each change is one ⌘Z, and a save records it in the bundle's history.
  - Your bundle holds its own copy of a theme that ships, and your edits change that copy, never the theme that ships. Choose the theme that ships again from the menu, and it comes in beside your edited copy (`dusk-2.theme.json`) rather than over it.
- **States.** The strip under the preview shows each state of the deck, small, and how long its cue runs. Click one, or use the arrow keys, to show it.
  - **+ Step** adds a state after the one shown that shows what it shows: change it, and the deck builds from one to the other. **+ Slide** adds an empty slide after the shown one's, in its layout, ready to fill from the Insert menu.
  - Drag a state to move it, or hold Alt and press an arrow key. F2, or a double click on its name, renames it, and Delete removes it. A state another builds on stays; the status says which.
  - Each is one ⌘Z.
- **A state's look.** With nothing selected (Escape, until the status says so), the inspector shows the state itself: its layout, how it comes in, how long it holds, and your notes.
  - The layouts offered are those with a place for each object on the slide. A layout goes where it is set, so the states of a build change together, and the status says how many; tick "layout only in" to keep it to the state shown.
  - How it comes in is a duration from the theme (or a cut), an ease, or a spring. A hold is in seconds: how long the state stays before the deck goes on by itself.
  - Each choice is one ⌘Z.
- **See.** The preview on the right shows the state the cursor is in, at rest.
  - **Play** opens the player in a new tab, with the motion, on the deck as last saved. A folder opened from disk has no Play: the player cannot open it. A new deck has none until its first save.
  - In the player, → and ← step, and F is fullscreen. **Presenter** opens your notes, the next state, and a clock in a second window, and **Edit** goes back to the editor.
- **Check.** Each edit lints the state shown, and every state once you stop typing.
  - Findings stand in the gutter at the line they are about, and in the list under the source. F8 goes to the next one.
  - On the preview, a mark at the corner of an object counts the findings about it, red for an error, yellow for a warning. A mark at the top left counts those about the state itself. The strip counts each state's. Click a mark to read its findings and take a fix; "In the source" goes to the line.
  - A finding that has a fix offers it as one click, one ⌘Z. Only the lines the fix changes change.
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
  - `scaena data talk q3` shows a data source's rows as written, each by its index, and any value its column does not read. `--edits` sets cells, adds rows, and takes them away, in one write of the file that keeps every other byte (SPEC §3.10).
  - `scaena files talk` lists the bundle's images, fonts, and data, what names each, and the objects drawn from each, in which states. `--remove` takes out files nothing names, and refuses the rest.
  - `scaena theme talk --edit ops.json` edits the theme the deck names by JSON Patch operations on it (`/tokens/color/accent`, `/type/roles/body/size`), refused where the deck would not check in the theme it leaves; `--apply theme.json` swaps in another theme (SPEC §3.6).
  - `scaena history talk` lists the versions its history keeps. `--at 3` prints version 3's deck, `--diff 3` says what changed from it to the deck now (`--diff 3,5` from one version to another), and `--restore 3` makes it the deck again, with its data files, as one change.

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
    ╭─[deck.scn:26:46]
 25 │   -claim
 26 │   title "Pro drove the growth" role:headline sise:12 semantic:claim at:in(header)
    ·                                              ───┬───
    ·                                                 ╰── /states/1/props/title/sise
 27 │   rev chart:bar data:@q3 x:{field: quarter, type: ordinal}
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
| W200–W231 | typography and fonts | a widow (W200), too many words on screen (W210), a font whose license forbids what a bundle does with it (W230), italic asked of a family with no italic face (W231) |
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
font Fraunces "fonts/Fraunces-Italic-VF.ttf" style:italic axes:{wght: [100, 900], opsz: [9, 144]}
font Inter "fonts/Inter-Italic-VF.ttf" style:italic axes:{wght: [100, 900], opsz: [14, 32]}
font "JetBrains Mono" "fonts/JetBrainsMono-Italic-VF.ttf" style:italic axes:{wght: [100, 800]}

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
- **Fonts and data.** `font` lists each of the theme's font families, with its file in the bundle, and each family's italic face (`style:italic`), which text in italic is set in. `data q3 "…"` declares a data source and its columns' types. A chart reads it as `@q3`.
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

**A tilt, and a turn on the next click.** `transform` draws an object turned (`rotate`, degrees clockwise), scaled, leaning (`skew`), or moved (`translate`), about its middle or its `anchor`, without laying it out again. Between two states it turns from one to the other.

```scn
state tilted slide:revenue
  title transform:{rotate: -4}

state level slide:revenue
  title transform:{rotate: 0}
```

**A line too long for its slot.** Lint says E100, with how much it overflows. Shorten it first. Otherwise, let it shrink to the role's smallest size; its fix offers this:

```scn
state long slide:revenue
  title "Pro drove the growth this year, and Enterprise is next in every region" fit:shrink
```

**Motion, and a self-running deck.** `transition` names how long the cue takes; `choreo` brings an object in with a preset, after a delay; `hold` moves on by itself after the cue, for a kiosk or a video. Once any state holds, the deck runs on its own, so a state with neither a cue nor a hold would show for 0 ms. Lint W323 names each one, here `cover` and `mix`, with a hold that fits its reading.

```scn
state turn layout:statement transition:slow hold:4s
  -title
  -rev
  point text role:display "One more thing" semantic:claim at:in(statement)
  choreo point enter:rise delay:240ms
```

**A picture.** A PNG or a JPEG in the bundle's `assets/` folder, with `alt` text for a reader (W410 without it). A photo goes in as the camera wrote it: one stored on its side shows upright, as its EXIF orientation says. A JPEG in CMYK or at 12 bits a sample is refused, saying why; save it as RGB at 8 bits first (SPEC §3.3):

```scn
state ridge layout:art-right
  -rev
  title "The ridge trail, rebuilt" at:in(header)
  photo image "assets/trails-ridge.png" alt:"The rebuilt ridge trail at dawn." semantic:evidence
    at:in(art)
```

**A link.** A run's `link` goes to a web address or to a state of the deck. The words are underlined, and a click on them while the deck plays goes there; renaming the state keeps the link (SPEC §3.5):

```scn
state sources slide:revenue
  title runs:[{text: "Revenue "}, {text: "doubled", link: {href: "https://example.com/method"}}, {text: " "}, {text: "again", link: {state: cover}}]
```

**A list.** A text's paragraphs are its lines, each ended by a line break (`\n`). `list` makes them a list's items, one entry for each: `{kind: bullet}` or `{kind: number}`, deeper with `level`, or `null` for a paragraph that is no item. The theme draws the markers, a bullet or a number for each level, and indents each level its own step, the item's wrapped lines starting under its words (SPEC §3.5):

```scn
state plan layout:statement
  -title
  -rev
  steps text role:body "Hire two engineers\nShip the API\nThe docs first\nThen the SDK" semantic:claim
    list:[{kind: number}, {kind: number}, {kind: bullet, level: 1}, {kind: bullet, level: 1}] at:in(statement)
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

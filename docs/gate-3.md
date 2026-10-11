# Gate 3

Phase 3's exit criteria (PLAN, "Exit criteria (gate 3)"), each with its evidence so far. Every task of Phase 3 has landed: the app (3.1–3.8), the editor as the browser's (3.9–3.17), and the window as a presentation app's, with what came after it (3.18–3.30). What the gate still needs is a real Mac: CI's macOS runner is a virtual machine with a paravirtual GPU, which compiles and tests the Swift but cannot read the gate's bars.

**Where it stands:**
- **Criterion 1** (same pixels as the web player) is met on CI's Mac, through the goldens both clients are held to. A real M-series Mac's GPU reads it again in the run below.
- **Criterion 2** (first frame under 300 ms) waits on that run. On CI's virtual Mac, the release build opens B1 to its first frame in 32–35 ms warm and 161 ms cold, once the GPU the app makes as it starts is made. That is not the gate's reading.
- **Criterion 3** (no text layout in SwiftUI) is met, and held there by a test. The source pane is TextKit in chrome, on Jay's call (below).

## 1. Same bundle, same frame, same pixels (within tolerance) as the web player: met on CI's Mac

- **Same frame.** The Mac edits and draws through the session the browser does (`scaena-session`, ADR-0021).
  - `scaena-ffi`'s `a_bundle_opened_from_its_files_draws_its_goldens` opens the torture deck from its files, as the app does. Every state's display list at rest digests to the goldens (`tests/golden/torture/raw.fnv1a`).
  - The WASM engine matches the same digests (PLAN 0.8, `just wasm-smoke`).
- **Same pixels.** The Mac paints with vello on Metal: `LayerPainter` builds the scene `GpuPainter` does, then blits it onto the layer.
  - On CI's macOS runner, vello on Metal paints every torture state within SPEC §13.5 of the CPU goldens (`cargo test -p scaena-paint --features gpu`, with `SCAENA_REQUIRE_GPU=1`).
  - ScaenaKit's `aSurfacePaintsOnMetalWhatTheCPUPainterPaints` holds a frame presented on a `CAMetalLayer` (B1's cover, read back from the layer) to the CPU painter's pixels.
  - The web player's frames, by WebGPU and by the CPU painter, are held to the same goldens (`just web-smoke`). The two clients meet there.
- **Still to do.** The run on a real Mac (below) shows the torture deck in the app beside the web player on the same Mac, by eye, and reads B1's cover through the layer on that Mac's GPU.

## 2. Open → first frame < 300 ms for a 40-state deck on an M-series Mac: needs a real Mac

`GateTests`' `aFortyStateDeckShowsItsFirstFrameSoonAfterOpening` times B1 (40 states, SPEC §15) as the app opens a deck, once the app has started:
- the bundle's files read;
- the session opened, and its timeline;
- the Metal surface made;
- the first state's first frame presented on a layer.

It prints the time, and where it went.

- **Where the time went.** The `mac-app` workflow first read the release build on CI's virtual Mac at 0.62 s warm, and 2.2 s cold:
  - files read and session opened, 2 ms; the timeline, 1 ms; the first frame, 31–50 ms;
  - the surface, 0.56–0.61 s: the first device and vello's pipelines. A second surface, made after it, took 65–90 ms.
- **So every layer paints on one GPU** (PLAN 3.2). The GPU is one device and one set of vello's pipelines for the process, which the app makes as it starts, off its main thread (`ScaenaSurface.warm`). A surface on it only configures its layer.
  - The test makes the GPU first, as the app does.
  - It prints the time that took beside the reading, and the reading from the app's start, both together.
- **The same run, with one GPU:**

  | | open → first frame | files and session | timeline | surface | first frame | the GPU, at the app's start | from the app's start |
  |---|---|---|---|---|---|---|---|
  | warm | 32–35 ms | 2–3 ms | 2 ms | 2 ms | 26–28 ms | 0.66–0.68 s | 0.70–0.71 s |
  | cold | 161 ms | 39 ms | 3 ms | 24 ms | 97 ms | 2.5 s | 2.7 s |

  A second surface, made right after the first frame, took 72–121 ms. That is most likely the first frame's work still finishing on the virtual GPU: wgpu waits for all of the device's work before it configures a layer. A real Mac reads it again.
- **The reading is open → first frame with the app running**, as SPEC §15 reads its budgets: warm unless noted. The cold reading, an app launched by opening the deck, is printed beside it. On Jay's call (2026-10-11) the gate reads the first, and the cold reading is logged beside it.
- **CI's readings are not the gate's.** CI's macOS job runs the test on every pull request under three conditions that are not the gate's:
  - the engine linked unoptimized (the debug library);
  - every test at once;
  - a virtual machine with a paravirtual GPU.

  It holds the test to five seconds, so that a gross regression shows.
- **The gate's reading** is the release build, on an M-series Mac. From the repository's root:

  ```
  cargo build --release -p scaena-ffi
  libs=$(cargo rustc -q --color never --release -p scaena-ffi --lib --crate-type staticlib -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
  cd apps/mac/ScaenaKit
  swift test -c release --filter aFortyStateDeck -Xlinker -L"$PWD/../../../target/release" $(for l in $libs; do printf -- '-Xlinker %s ' "$l"; done)
  ```

  The line `gate 3, criterion 2: B1 (40 states), open to first frame: …` is the reading, and the line after it says where the time went. Run it three times, and log the median.

  CI's `mac-app` workflow runs the same command three times on its virtual Mac, beside the app it builds, and puts the readings in the run's summary. They show the command works and how near the release build comes, and are not the gate's.

## 3. SwiftUI code contains zero text layout; all geometry comes from the engine: met

- **What draws a deck is the engine's.** Every frame, glyph, and box comes from the engine:
  - the canvas, the stage, and the presenter's view: vello on Metal;
  - the states drawn small and the next state: the CPU painter's pixels;
  - each outline, handle, and landing a drag shows: the engine's boxes (`boxes`, `hit`, `targets`, `snap`);
  - a caret and a selection in a text typed in on the canvas (PLAN 3.9): the engine's carets (`carets`). The view that takes the keys answers the text input system (`NSTextInputClient`) from them, and lays out and draws nothing.

  SwiftUI draws chrome: lists, the inspector, the findings, the assistant's conversation.
- **Held by a test.** `crates/scaena-ffi/tests/chrome.rs` reads every Swift file of the app, and fails on any API that lays text out, measures it, or draws it outside the engine. It names Core Text, TextKit's layout managers and containers, `NSTextField`, string measuring, and string drawing.

**The source pane, on Jay's call (2026-10-11).** The source pane is an `NSTextView` of the deck's `.scn`: TextKit, in chrome, as the browser's source pane is CodeMirror. SPEC §9.3 bars TextKit and Core Text from the render path, and CLAUDE.md's invariant 8 now says the same: no TextKit where a deck is drawn. It had said "No TextKit in the Mac app", which would have meant an editor the engine draws, weeks of work that helps no deck, or CodeMirror in a web view, HTML in chrome. The test allows `NSTextView` in `SourcePane.swift` alone.

## The run on a real Mac

About an hour on an M-series Mac with macOS 15 or later, Rust, and Xcode's Swift; or with the `mac-app` workflow's `Scaena.app` and a release checkout for criterion 2. The window numbers its slides and names no state, so a step names a slide by its number, the state's id from the deck's source in brackets.
1. `just mac`, then open `tests/fixtures/torture.scaena`, and the same deck in the web player (`just web-dev`). Look at a few slides side by side: one with text, 11 (`hanging`); one with a chart, 22 (`chart`); one with a shader, 23 (`mesh`).
2. Open `tests/bench/b1.scaena`. Play it (⌥⌘P), with a second display if there is one.
3. Run criterion 2's command three times. Log the median.
4. Drag the cover's title elsewhere on the slide, then undo. Double-click the title and type a few words, with an input method that composes too (Japanese, say), then undo once: the words go together. Ask the assistant something with a key from Settings. Export the PDF, and share it.
5. The editor, as the browser's (PLAN 3.10–3.17), on a copy of the torture deck (`cp -R tests/fixtures/torture.scaena /tmp`). Each edit undoes with one ⌘Z:
   - On slide 47 (`links`), select a few words, give them a color in the Format tab, and link them to another slide with ⌘K, by its number. On slide 46 (`lists`), make a point a numbered step with ⌘⇧7, and indent it with Tab.
   - From the Insert menu, insert a text, duplicate it with ⌘D, lock it with ⇧⌘L, and try to drag it.
   - Copy the chart on slide 22 (`chart`) with ⌘C, and paste it on slide 24 (`shapes`). Copy a picture in Preview and paste it on the canvas, then a few cells from Numbers.
   - On slide 26 (`containers`), Shift-click the 42% and the 2.4x stats, align them in the Format tab, and group them with ⌘G.
   - Add a slide with Slide › New Slide. Open the light table (⌥⌘L) and drag a slide elsewhere. Rehearse (⌥⌘R) a few slides, and keep the times.
   - In the Document tab, change a color of the theme, and a cell of `bars` in its data. Save, edit, save again, and restore the first version.
   - Turn a node by the handle above it, and crop the picture on slide 25 (`images`). Show the grid (⌘'), zoom with a pinch, show View › Show Other Sizes and click 9:16, and replace a word with Find in Deck (⇧⌘F).
   - Tab through the canvas, Return into a container, move a node with the arrows, and turn it with `]`. With VoiceOver on (⌘F5), move through the canvas.
6. The window as a presentation app's, and what came after it (PLAN 3.18–3.30), on a copy of B1 (`cp -R tests/bench/b1.scaena /tmp`), and on the trails example where a step names it (`just cli save docs/examples/trails.deck.json --to /tmp/trails.scaena`). Each edit undoes with one ⌘Z:
   - **In a person's words (3.18).** Slides are numbered, a step set in under its slide, and an object is called by its first words. A property and its value read as a presentation app's do: a color by what it is for, with its swatch; a duration in seconds; a hold as On Click or After 5.5 s. No id, property path, lint code, source line, or millisecond shows anywhere, nor in what an edit says it did. The toolbar holds View, Zoom, Add Slide, Play, Text, Shape, Image, Chart, Table, Share, the assistant, and Format, Animate, and Document; the timeline, the other sizes, the issues, and the source start hidden. A click on Shape inserts the deck's first shape, and a press held on it offers the rest.
   - **A drag lands where it is let go (3.19).** Drag the title, "Scaena", across the slide and let go: it stays there. Near a grid track, the slide's edge or middle, or the subtitle's edge, it is drawn in, a guide where they meet; with ⌘ held, nothing draws it in. With Shift held, it snaps into a slot or onto the grid's cells, as before. Nothing is outlined in orange.
   - **Issues where they are (3.20).** Drag the subtitle over the title: a red mark at the top right of one of them, and a click on it says "It overlaps another object." Undo. Type over the title until it no longer fits: its mark says "The text doesn't fit its box." and offers Shrink to Fit; take it, then undo twice. View › Show Issues lists the rest.
   - **A text inserted is typed in (3.21).** Click an empty part of the slide, then the toolbar's Text: a text goes in there with its words selected, and what is typed takes their place. Escape stops the typing and leaves the text selected.
   - **A file dropped (3.22).** From the Finder, drop a photo on an empty part of the slide: it goes in where it is let go. Drop another onto it: it takes its place. Drop `docs/examples/data/hed-tenure.csv` on the slide: a chart of it lands there, and the Document tab's data lists it.
   - **A right click (3.23).** Right-click the title, or Control-click it: it is selected, and the menu offers Cut, Copy, and Paste, Duplicate, Delete, Arrange, Copy Style and Paste Style, Lock, and Insert, which puts what is chosen where the click was. Double-click the title to type in it, then right-click: the menu is the text's own.
   - **What is inserted stays on its slide (3.24).** On slide 2 (`goal`), insert a text, then duplicate it, and copy and paste it: each shows on the slide's later steps, and on slide 3 none of them does.
   - **Alignment (3.25).** Select the subtitle, and in the Format tab click Center, then Right, then Left: its lines move with each, and it stays where it stood up and down.
   - **A slide's layouts (3.26).** On the trails example's cover, with nothing selected, the Format tab draws its layouts small, best first, the one it takes outlined: Title, then Title over Photo, Centered Title, and Title beside Photo, marked for what does not fit. The pointer over Title over Photo shows the slide in it on the canvas, nothing made; a click takes it. Above them, the Layout button names the slide's layout and opens the theme's layouts by their sections (3.31), those the slide may take, Title outlined.
   - **The window fits (3.27).** Opened on a laptop's screen, the window fits within it. Dragged narrower, it goes to about 890 points before it stops, and what the toolbar cannot hold goes into its » menu. Insert › Text from the menu bar inserts a text.
   - **Fields (3.28).** In the Document tab's Theme, type another hex in a color's field and press Return: the slides take it. Edit › Undo takes it back, and ⌘⇧Z makes it again. Type a size in Body's field and press Escape: it is put back. Click Body's size stepper a few times quickly: one ⌘Z takes them all back.
   - **The safe area (3.29).** View › Show Safe Area (⌥⌘') shades a strip 3/8 inch in from each edge of the slide, amber, dashed at its inner edge. Drag the subtitle into it: nothing stops it, and nothing is marked. ⌥⌘' hides it.
   - **The layout library (3.30, 3.31).** Add Slide opens a gallery of 122 layouts in 13 sections, Titles through Closing, and Blank, each under its name (SWOT, Q&A, Gantt Chart) and drawn as it is used: a picture where a picture goes, a chart or a table where a figure goes. Bullets adds a slide after the one shown, its words to type over: a double click on its headline selects them whole, and typing replaces them. Photo outlines where its picture goes; a press on those words asks for a picture, which lands there. File › New, in a theme, opens a deck on a title slide.
   - **The timeline (3.14).** On the trails example's slide 11 (`funding`), View › Show Timeline puts the cue under the slide, which keeps its size. A click on the chart makes its lane stand out and the others step back; a click on another lane selects its object. A bar's end dragged makes its motion longer.

What the run finds is logged here and in PLAN's gate log, and each of PLAN 3.2–3.30 it shows working is ticked there.

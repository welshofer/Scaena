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

Serve the site folder and open `editor.html`. Once it has opened, the site works with no network: the page says **Works offline**, and your browser can install it as an app. The folder is `target/site` from `just site`, or a copy of it. Its `serve.py` serves it on this machine only:

```sh
cd target/site && python3 serve.py      # then open http://localhost:8080/editor.html
```

- **Edit.** The slide comes first: the editor opens with the canvas in the middle and the inspector at the right. **Source** (or ⌘\) opens the deck at the left, as `.scn`, and folds it away again; the editor remembers which you left it.
  - Each keystroke compiles. Source that does not compile says where, and the preview keeps the last deck that did.
  - The state picker jumps to a state's line. On the slide, PageDown and PageUp show the next slide or step and the one before. ⌥⇧⌘C (Ctrl+Alt+Shift+C) puts the keys on the slide from anywhere on the page, and ⌥⇧⌘F on the slides under it.
  - The format picker shows the deck in each of its `formats`.
- **Move.** Click an object in the preview to select it, and drag it.
  - It snaps to the theme's grid and the layout's slots, and the move becomes a change to your source: one ⌘Z undoes it, in the preview or the source.
  - Before you let go, the status says in how many states the move shows. Hold Alt to keep it to the state shown, and Shift to place it freely, off the grid, which lint flags as an override (W301).
  - Handles resize it by the grid's tracks. A picture's corner keeps its proportions instead and lands where you let go; with Shift, its edges go to the tracks. The arrow keys move it a track at a time, and Shift with an arrow resizes it. Escape selects what holds it.
  - Press ⌘' (Ctrl+'), or Grid beside the zoom, to see the theme's grid over the slide: its columns and rows, the gutters and margins between them, and the baseline grid its text sits on. Press it again to hide it. As you drag or resize, a line shows wherever an edge or the middle of what you move meets another object's, or the slide's; with Shift, off the grid, it snaps onto one within a few pixels. Hold ⌘ (Ctrl) as well to place it exactly where you drag.
- **Several at once.** Shift-click to add an object beside the one selected, or take it away; they must sit in the same place, side by side on the slide or in the same container. Or drag across an empty part of the slide to select what the box encloses.
  - Drag them, or press an arrow key, and they move together, keeping where they stand about each other. Delete, ⌘D, ⌘C, and ⌘X act on all of them, and a paste puts them down as they stood.
  - The inspector aligns them by their left, right, top, or bottom edges or their middles, on the grid's tracks, and spreads three or more evenly. It offers what all of them have, and what you choose there is theirs.
  - ⌘] brings what is selected in front of the next thing it overlaps, and ⌘[ sends it behind; with Shift, in front of or behind everything beside it. Each is one ⌘Z.
  - ⌘G (Ctrl+G) puts what is selected in a group, which moves, fades, and enters as one; ⌘⇧G takes a group apart again. Nothing moves either way. The objects in a stack or a grid are already held together, and stay as they are.
- **Zoom.** ⌘+ and ⌘− (Ctrl on Windows and Linux), or the zoom's buttons beside the Insert menu, bring the slide closer, up to eight times, and ⌘0 shows it whole again. Pinch, or turn the wheel with ⌘ held, to zoom about the pointer. Zoomed in, the wheel moves what you see, and so does a drag with Space held, or, with nothing selected, an arrow key (Shift with it goes farther). Tab brings what it selects into view. The slide is painted again at each size, so text and edges stay sharp, and everything you do zoomed in, a drag, a resize, typing, lands as it would at the whole slide.
- **Find and replace.** ⌘F (Ctrl+F), or Find, opens a bar above the slide that finds words in every slide's text, in each picture's or chart's description, in your notes, in each beat's claim and notes, in a chart's or a table's titles and notes, and in your data. A value of your data is found and left as it is, since a filter, a quote, or a highlight may name it: rename it in the Data tab. Enter goes to the next one and shows it, marked in a slide's text, and the status line says where it is; Replace changes it, and Replace All changes every one at once, one step to undo. A text that several slides show is changed once, where it is written, so it reads the same in all of them. Match case and Whole words narrow what is found. On the command line, `scaena find deck "Q3" --replace "Q4"` does the same.
- **Time the build.** Under the preview, the state's cue is a timeline: the transition, then a bar for each thing that moves.
  - Drag a bar to start it later or sooner, and drag its end to make it longer or shorter. Each change is written where the motion is set, in the state's choreography or on the object itself, and one ⌘Z undoes it. A motion on a spring lasts as long as the spring takes to settle, or, for an object leaving, until it first reaches its look, so its end stays put.
  - Add a motion picks one of the theme's presets for the object selected: as it comes on, or for emphasis. Objects that leave in this state can be given one as they go, unless a slide transition comes into it: that moves the slide left whole, and none of its exits plays.
  - **By Bullet.** For a list, or any text of two or more paragraphs, Build In and Build Out in the inspector, and Add a motion, offer By Bullet: the text comes in one paragraph a click, or goes one a click. A click is a state, so it adds a step for each paragraph to the slide, each drawing one more, and the text stays laid out whole so nothing moves as the rest arrive. One ⌘Z takes the build and its steps back, and so does choosing another build, or none, later: the list comes in whole again, and the steps it added go (a step you have given more stays). A list can build in by bullet and out by bullet too. Type a new bullet where a step shows only the first and it shows there as you type; the later steps each show one more. In the source each step says which paragraphs it draws, `paragraphs:[1, 2]`, and you can write that yourself.
  - Press or drag along the ruler to see the cue at that moment, and Play to watch it run. Click the slide to edit it at rest again.
  - Select an object on the slide and its bars stand out, the other motions fading back; select a group and its members' do. Click a bar or its name, or press Enter on a bar, to select its object on the slide.
- **By keys alone.** Everything the mouse does on the slide, the keys do too, and `?` lists them. Tab and Shift+Tab select each thing on the slide in reading order, and past the last the focus moves on. Enter goes into a stack or a group, and Escape back out. Enter on a shape, an image, or a chart goes to its handles: its points or corner, its crop and focal point, its bars and annotations. Tab steps through them and the arrows move the one marked, a hundredth at a time, a tenth with Shift. `[` and `]` turn what is selected, and ⌘A selects everything beside it. To select some of them, press Space on the first, Tab to each of the others (they're outlined as you go), and press Space to add it, or Space again to take it out; Escape stops.
- **Type.** Double-click a text in the preview, or press Enter with it selected, and type where it stands.
  - Your typing goes where the text is set: in the state that sets it, or in the object itself, and the status says in how many states it shows. Double-click with Alt held to keep it to the state shown.
  - A text in several looks (`runs`) keeps them: what you type takes the look of the words around it.
  - Each burst of typing is one ⌘Z. The text reflows as you type, and lint says at once if it no longer fits. Escape, or a click outside the text, stops typing.
  - **Links.** Select some words and press ⌘K (Ctrl+K). In the field above them, type a web address (`https://…`, `mailto:…` for an email, or just `example.com/page`, which takes `https://`), a slide's number, or the id of one of your states, and press Enter: the words are underlined and go there when the deck is played, in the single file, and in a PDF. On words linked already, the field shows where they go; click away or press Escape to keep the link, or empty the field and press Enter to take it away. Each is one ⌘Z.
  - **Lists.** Press ⌘⇧8 (Ctrl+Shift+8) for bullets, or ⌘⇧7 for numbers, on the paragraphs your selection touches; press it again to take them out of the list. In a list, Enter starts the next item, and Enter in an empty item ends the list; Tab moves an item a level in, and Shift+Tab out. Tab in a line that is no item makes it one, a level in. Backspace at the start of an item takes its bullet off (or moves it a level out first), and a second Backspace joins it to the line above. With a text selected and not typed in, the keys act on all of it. Your theme draws the markers and the indents. Each is one ⌘Z.
  - Select some words and press ⌘B (Ctrl+B) to make them bold, or, bold already, not: a family of one weight, as Salon's serif is, sets them in its own weight and says so (W232). ⌘I (Ctrl+I) sets them in italic, or not: the italic is the family's own, and a theme whose family has none sets them upright and says so (W231). With words selected, the inspector shows their look instead of the text's: choose a role or a color there and only those words take it. Each is one ⌘Z.
- **Choose a look.** With an object selected, the inspector shows what your theme offers for it: a text's role, family, weight, color, and fit, a shape's fill and stroke, a chart's kind, an entrance and an exit, and more.
  - Each choice is one ⌘Z, and goes where the value is set: in the state that sets it, or in the object itself. The status says in how many states it shows. Tick "only in" to keep it to the state shown.
  - A color or a length you write out yourself, and any text size, is an override: it goes in the deck's `overrides`, holds in every state, and a new theme does not change it. The inspector marks it so. The × beside a value takes it away where it is set, so what is under it shows.
  - A chart shows what it reads: its data, the column on each axis (offered from the columns its data has, numbers for a value), the series, and how it reads them. Choose another source and the chart reads it: where that source lacks an axis's column, the axis reads another of the same kind, a name for a name and a number for a number, and the chart drops whatever else the source has no column for (a series, say). "Only in" makes it a data update: this state reads the new numbers, and the chart moves to them from the state before.
  - A filled rectangle or oval, a picture, or a container with a fill offers its Shadow: the theme's Soft, Raised, or Floating, or none. It falls under the object, the outline of what shows blurred, and moves, turns, and fades with it. A line, an outline, a shape from the library, a picture in a mask other than an oval, and a PNG that may let what is under it show cast none, so none is offered there; nor are Corners where they round nothing: on an oval, a library shape, or a masked picture.
  - A line or an arrow offers its Line Start and Line End beside its Border: none, an arrowhead, an open one, or a circle, a square, or a diamond on the end, each sized by the line's width. A connector, made by Connect with an arrow or a line, offers its Connection too: Straight, Corner (an elbow that turns at right angles), or Curved, which leaves one object and meets the other square to their sides. Each is one ⌘Z.
  - An image offers its Border and Border Width, the theme's colors and strokes, drawn around what shows: the rounded corners, the oval, or the shape it is masked by.
  - A picture's frame is your theme's: a mat of one of its colors around the picture, a thin keyline at its edge, or both, inside the picture's box, the picture cropped and focused within it. Every picture in the deck takes it, so the inspector says what it is and offers nothing to change; a theme edit changes it (`pictures.frame`, theme format 0.21, SPEC §3.6). The themes that ship frame nothing.
  - Insert › Map… puts a map of a place on the slide: type an address, a place, or a latitude and longitude, press Return, and pick one of the places it matches if there are several. It goes in the picture selected, the slide's empty place for a picture, or where you last clicked. It asks for a Geoapify key of your own the first time (geoapify.com; its Free plan will do), and keeps it as the assistant's keys are kept. The map's picture is kept in the deck, so it shows offline; the Format tab's Map section zooms it, centers it on its pin, or fetches it again, each one step to undo.
  - An image offers Adjust Image: Exposure, Contrast, Saturation, and Warmth, each a slider from −1 to 1 that sets when let go, the middle leaving the picture as it is. Saturation at its least is black and white. Between two steps, one adjustment fades into the next where the picture stands.
  - An image shows its fit, its focal point, and its crop. Or drag the handles on the image itself: one inside each side crops it from that side, the whole image outlined as you drag, and the round one is its focal point. Press Pick, then click the image where its subject is: that point stays in view however the box is shaped, as cover crops it. Escape leaves it as it was. A crop is a part of the image, as fractions of it: x, y, width, and height. Drag an image file from your computer onto an image in the preview to put it in that image's place: it joins the bundle, and one ⌘Z puts the old one back. Drag it onto an empty spot and it is added there, as Insert adds it.
  - Every object offers its description and its part in the story: what a reader hears. A description is what a screen reader says for it: what an image or a chart shows, or, for a text, what to say in place of its words (left empty, its words are read). An object marked decoration is not read at all. Under the choices, the inspector says how the object reads, as the player says it to a screen reader; with nothing selected, it reads the state shown part by part, in order, and a part selects its object. An image with no description is W410: its mark offers Describe it, which opens the inspector on the image, its description ready to type.
- **Add, copy, delete.** The Insert menu under the preview offers what your theme and bundle have: a text in each of the theme's roles, a rectangle, an ellipse, a line, an arrow, each image in the bundle, a chart and a table of each data source, and each of the theme's shader backgrounds. Under Draw, it lists the draw tools with their keys, Text (T), Rectangle (R), Oval (O), Line (L), Arrow (A), and Pen (P): choose one, or press its key on the slide, then drag where it goes. The chart is made from the columns the data has: a line over dates, bars over names, grouped where a name repeats. A deck with no data still has a chart and a table to insert, of a few sample rows you then edit in the Data tab. A picture comes in at its own proportions. **Icon…** opens the icons that ship, 1,434 of them: type what you need (`rocket`, `arrow up`, `money`), tick Solid for the filled ones, and click one, or press Return for the first. It lands where you last clicked on the slide, in the theme's accent, and keeps its size beside other icons; it is decoration a reader passes over, named by the icon in the layers. With an icon selected, Icon… replaces it: the one you choose takes its place, size, and color. The layouts Icon Columns, Icon Cards, Icon Rows, and Icon Grid start a slide with an icon over each item, for you to replace, and a deck made from a prompt sets each item under the icon its words name. On the command line, `scaena icons rocket` finds the same, each with the path and view `add_node` takes.
  - Or draw it: press T (a text), R (a rectangle), O (an ellipse), L (a line), or A (an arrow) on the preview, and drag where it goes. It lands on the grid's cells you covered, or with Shift exactly where you dragged; a line runs the way you dragged. A click puts it where you clicked, a text you draw is ready to type into, and Escape stops drawing.
  - What you choose goes where you last clicked in the preview, or in its middle, snapped to the grid. Where a text, an image, a chart, or a table there would overlap something, it goes to the nearest place on the grid that is clear, so lint has no overlap to report; where nothing is clear, it stays where you clicked. A text, an image, a chart, or a table clicked into an empty slot of the layout fills the slot where the slot takes it (words where words go, and a picture, a chart, or a table where the layout asks for one), so it follows the deck into its other formats and its other themes; a picture there lies under the layout's words. A picture dragged from your computer is named after its file. A shader fills the canvas, behind everything. It appears in the state shown, selected, ready to drag or type in, and stays on that slide: the steps after it on the slide show it, and the next slide does not. A copy or a paste stays on its slide too.
  - Delete (or Backspace) takes the object selected, and what it holds, out of the state shown and the states after it. Something that no state shows any more leaves the deck, so adding and deleting leaves nothing behind. Shift+Delete takes it out of the deck. ⌘D (Ctrl+D) puts a copy beside it, with copies of what it holds.
  - ⌥⌘C (Ctrl+Alt+C) copies the look of the object selected, as the state shows it: a text's role and style, a shape's fill, outline, and corners, an image's corners, mask, border, and adjustments, the shadow a shape, an image, or a container casts, a shader's preset and palette, a chart's labels, a stack's or a grid's gap. ⌥⌘V (Ctrl+Alt+V) gives that look to each object selected, written where each one's own look is set, as a choice in the inspector is: on the object, so it holds in every state, or in the step that sets it. Where the look takes the theme's value, the object's own setting goes, so the theme's shows. An object of another kind takes what it shares with the look, an image a shape's corners; the status says what took nothing, and why. `scaena inspect --state S --look NODE --onto A,B` gives the same patch on the command line.
  - ⌘C (Ctrl+C) copies the object selected, with what it holds, and ⌘X cuts it. ⌘V pastes it where you last clicked, in this deck or another, bringing the data and images it reads. What the other deck's theme lacks, a color or a role, is left out, and the status says what; a text in a role the theme lacks takes the nearest role it has. What you copy in another app pastes as what it is: a screenshot or a picture as an image (a GIF or a WebP as a PNG of it), cells from Numbers, Excel, or Google Sheets as a data source and the table they were, each figure as you copied it (`$1,234.50`, `12.5%`), and other text as body text.
  - Each is one ⌘Z.
- **Layers.** The Layers tab beside the inspector lists the objects of the state shown, topmost first, inside what holds them. Click one to select it, even one under the rest. Those dimmed are not shown here: they leave in this state, another step of the slide shows them, or no state shows them any more.
  - The eye hides an object in this state, or shows one hidden here. A double click on a name, or F2, renames it everywhere the deck names it.
  - Drag an object up or down among those beside it to put it in front of or behind them; in a stack, to lay it out earlier or later. Alt with ↑ or ↓ moves the one focused a place.
  - Drag it beside an object another container holds, or out among the slide's own, and it goes there; drop it on a container's middle and it goes in, at the top. It keeps its place on the slide where it can: in the grid cells it stands in, inside a frame, at its spot in a stack's order, or after a grid's last cell. Alt with ← takes it out of its container, and Alt with → puts it into the container just above it.
  - **Lock a background.** The lock beside an object's eye locks it, and ⇧⌘L locks what is selected. A locked photo or shape stays put: clicks, the marquee, and ⌘A reach what is over it, and keys and Delete leave it alone. Select it in the Layers tab to see it outlined, and unlock it there, with ⇧⌘L, or by a right click on it.
  - Each is one ⌘Z.
- **Layouts.** Press **Layout** above the preview to see the layout your slide uses as its slots, each named, on your theme's grid. Drag a slot to move it, or a handle to resize it: it lands on the grid, and every slide that uses the layout changes with it, because the change is to your theme. In another format (Formats, or the format menu), a slot you move moves in that format only. **New layout from this one…** in ⌘K copies the layout under a name you give it, and the slide shown takes the copy, so you can change it without changing the others. A copy of a layout that shows no slide number or footer, such as a title's, shows none either. Escape leaves the layout. Each is one ⌘Z.
- **Data.** Drag a CSV or JSON file from your computer onto the slide: it becomes one of the deck's data sources, named for its file, each column typed as its values allow, and a chart of it lands where you dropped it. Each is one ⌘Z. Drop the same file again for another chart of it. Cells copied from a spreadsheet and pasted on the slide become a data source too, with the table they were. A different file with the same name is kept beside the first, as `visits-2.csv`, so the charts you have keep their rows. The Data tab shows a data source your charts and tables read as a table: each column with its type, and each row as the file has it. A cell its column does not read, `n/a` in a column of numbers, is marked and listed under the table, with why. The deck does not check until it reads (E103), and setting the cell here is how you mend it.
  - Click a cell and type. Enter sets it and goes to the cell below, and Escape puts back what it held. A value its column does not read, or one that would leave the deck invalid, is refused, and the status says why. Every chart and table that reads the source shows the change.
  - **+ Row** adds an empty row after the row you are in, and **− Row** takes that row away.
  - The file keeps every byte you did not change, its quoting and line endings included, as `scaena data` writes it. Undo and Redo, or ⌘Z in a cell you have not changed, put it back as it was. A save records each version in the bundle's history.
  - Click a row's cell and the slide outlines what that row draws: its bar, its point, its slice, its row of a table. The line under the table says which, or that nothing on this slide draws it. With the Data tab open, click a bar on the slide and its row is chosen; double-click a bar to open the Data tab on it. A bar your `dataTransform` sums from several rows chooses them all.
  - **Quote a figure.** Double-click a text, select the words that should be a figure, then click a cell here and **Quote**: the words become what the cell holds, and when the cell changes, so do they, and so does a claim that says them. Typing over the figure makes it plain words again.
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
  - A file nothing in the deck uses says so. **Remove** takes it out of the bundle, and the deck looks the same. ⌘Z anywhere puts it back, as one step with your other edits, and the next save takes it out where the bundle is kept. A file something uses stays, and the tab says what uses it.
- **Versions.** A bundle that keeps a history (**Keep a history** in the tab begins one with a save, as `scaena save --history` does) lists its versions in the Versions tab, the newest first, each by who made it and when; each save adds the edits since the last. Click one to see a slide of it as it was, and what changed since, or since another version. Restore makes it the deck again, with its data, as one edit: ⌘Z takes it back.
- **Rehearse.** **Rehearse** plays the deck from the start as you would present it: → or a click goes on, ← goes back, Escape stops. It counts how long you spend on each slide, then shows each one's time and the hold it would keep, its time less its animation. **Keep as holds** writes them all as one edit, so the deck plays by itself at your pace; ⌘Z takes them back.
- **Keys.** Press ? anywhere but the source or a field, or click **Keys**, to see every key the editor answers, grouped by what it does, as your machine writes them: ⌘, ⌥, and ⇧ on a Mac, Ctrl, Alt, and Shift elsewhere. ⌘K lists the same commands by name, and runs them.
- **Formats.** If your deck lists other formats (`formats:[9:16]`), **Formats** beside the format menu shows the slide in each of them side by side under the preview, its own canvas first, and plays them as the preview plays. Each says how many findings it has on the slide shown: a headline that fits the wide slide and runs long in the tall one says so on the tall one. Click one to edit the deck in that format. There a node that has its own layout in that format (`formats`, below) moves there alone when you drag it, and stays where it was in the rest; one without moves in every format. Right-click it and choose **Place anew in this format** to give it one.
- **Theme.** The theme menu beside the format menu lists the themes your bundle holds, the deck's own chosen, and the themes that ship. Choose one to see the deck in it: one ⌘Z takes it back. A theme that lacks a role or a color your deck uses is refused, and the status says which.
  - The Theme tab edits the theme itself: its colors, each with the roles that use it; its type roles, each one's family, size, weight, leading, and tracking; and its spacing. Change a value and press Enter, and every slide that uses it shows the change. A value the theme cannot read is refused, and the status says why. Each change is one ⌘Z, and a save records it in the bundle's history.
  - From a photo, in the same tab, gives the theme a photo's colors. Click a photo the bundle holds, or right-click a photo on the slide and choose Theme colors from this photo. The accents take the photo's hues and the neutrals lean to its cast, each at the theme's own lightness and chroma. Text in each color still reads on the slide. On the command line it is `scaena theme --from-photo assets/photo.jpg`, with `--dry-run` to see the colors first.
  - Your bundle holds its own copy of a theme that ships, and your edits change that copy, never the theme that ships. Choose the theme that ships again from the menu, and it comes in beside your edited copy (`dusk-2.theme.json`) rather than over it.
- **States.** The strip under the preview shows each state of the deck, small, and how long its cue runs. Click one, or use the arrow keys, to show it.
  - **+ Step** adds a state after the one shown that shows what it shows: change it, and the deck builds from one to the other. **+ Slide** adds an empty slide after the shown one's, in its layout, ready to fill from the Insert menu.
  - Drag a state to move it, or hold Alt and press an arrow key. F2, or a double click on its name, renames it, and Delete removes it, and each object only it showed. A state another builds on stays; the status says which.
  - Each is one ⌘Z.
  - **Slides** shows the light table: every slide, large, in place of the preview. Select slides as you would files, then drag them, or press Alt with an arrow, to move them; ⌘D copies them and Delete takes them out. Each slide still shows what it showed, and a copy is its own: change it, and the slide it copies stays as it was. Enter shows a slide; Escape closes the table.
- **A state's look.** With nothing selected (Escape, until the status says so), the inspector shows the state itself: its layout, how it comes in, how long it holds, and your notes.
  - The layouts offered are those with a place for each object on the slide. A layout goes where it is set, so the states of a build change together, and the status says how many; tick "layout only in" to keep it to the state shown.
  - Above them, the inspector draws the state in each layout it may take, best first: fewest errors, then fewest warnings, then those the slide fills, its figures set as figures, each saying what lint finds there. Layouts that would show it alike are one picture, which names the others. Point at one, or Tab to it, and the slide shows in it, nothing changed until you click it or press Enter. `scaena inspect BUNDLE --state S --layouts` lists the same. A slide whose objects stand on the grid, in no layout's places, has no pictures: every layout would show it alike.
  - How it comes in is a duration from the theme (or a cut), an ease, or a spring. A hold is in seconds: how long the state stays before the deck goes on by itself.
  - **Slide Number and Footer** numbers every slide and sets the words you type at the foot of each, where the theme places them, in its type: the number at the right, the footer from the left. A slide's number is its place in the deck, so it is right after every move, copy, and removal. The theme leaves both off its title, divider, and full-bleed layouts; **On this slide** shows them there anyway, or hides them on any slide.
  - Each choice is one ⌘Z.
- **See.** The preview on the right shows the state the cursor is in, at rest.
  - **Play** opens the player in a new tab, with the motion, on the deck as last saved. A folder opened from disk has no Play: the player cannot open it. A new deck has none until its first save.
  - In the player, → and ← step, and F is fullscreen. Past the last slide, a black screen says the show has ended, and stays until a click, Escape, or ← leaves it. **Presenter** opens your notes, the next state, a timer you can pause and reset, and the time of day in a second window, and **Edit** goes back to the editor.
- **Check.** Each edit lints the state shown, and every state once you stop typing.
  - Findings stand in the gutter at the line they are about, and in the list under the source. F8 goes to the next one.
  - On the preview, a mark at the corner of an object counts the findings about it, red for an error, yellow for a warning. A mark at the top left counts those about the state itself. The strip counts each state's. Click a mark to read its findings and take a fix; "In the source" goes to the line.
  - A finding that has a fix offers it as one click, one ⌘Z. Only the lines the fix changes change.
  - The **Inspector** tab shows the state's timing, its objects, and each text's resolved look.
- **Save.** Where the deck goes depends on where it came from. The toolbar's **File** menu holds the rest:

| | |
|---|---|
| **New…** | a new deck: a title and a theme (Dusk, Daybreak, Ember, or Salon), with its fonts, on a title slide whose words you type over. It is kept nowhere until you save it, and says "Not saved yet" under the slide: leaving the page, or opening another deck, asks first. |
| **Open folder…** (Chrome, Edge) | a bundle folder on your disk. **Save** writes back into it. Use this to keep your work as files. |
| **Open .scaena…** | a `.scaena` file, copied into the browser's storage. |
| **Kept in this browser…** | the bundles this browser keeps. A deck opened from the site's own folder saves here, under its name. Each is opened by its name, renamed with **Rename…**, or deleted with **Delete…**, which asks first; the deck open is renamed where it is and saves there from then on, and is not deleted. |
| **Save** (⌘S) | writes the bundle where it is kept, as `scaena save` writes it, fonts whole. Files are renamed by their content (a hash in the name), and `manifest.json` is written; that is normal. Source that does not compile is not saved. ⌘S works anywhere on the page, while you type on the slide or in a field of the inspector too: what you typed is in the save. The tab names the deck and says when it is edited; undo back to what you saved and it is not. |
| **Save as…** | the bundle saved somewhere new, and kept there from then on: under another name in the browser, or, in Chrome and Edge, in a folder on your disk. |
| **Download .scaena** | the bundle as one file, fonts subset, to keep, send, or open with the command line. |
| **Served** (`scaena serve`) | the folder on disk the server serves. **Save** writes back into it, in any browser, and into its `deck.scn` where it keeps one; the editor opens that `deck.scn` as its source. A change on disk comes in as it is made, or, over changes of yours not saved, is offered. |

A file dropped on the source joins the bundle, and its path, quoted, goes where you dropped it: a font under `fonts/` and a data file under `data/`, by name, and an image under `assets/`, named by its content. A `.scaena` file dropped opens instead. The page warns before it closes over changes not saved.

**Your own deck in the site.** Copy its folder into the site's `decks/`, then open `editor.html?bundle=decks/my-talk/`, or the same address on `index.html` to play it. **Open folder…** shows only where the browser can open one (Chrome and Edge, not Safari or Firefox). Elsewhere, saves stay in the browser until you **Download .scaena**, or until you serve the folder with `scaena serve` instead.

**Start a deck.** **New…** asks for a title and a theme. The deck it makes has the theme and the fonts the theme names, as an assistant's `deck_create` makes one, and opens on a title slide in the theme's `title` layout: its title, the line under it, and its kicker are words to type over, and the place for a picture is outlined. Its first **Save** keeps it in the browser under a name made from its title; **Save as…** puts it in a folder on your disk instead. To start from another deck, open that one and **Save as…** under a new name.

## The command line: edit, check, look

**Set up once** on a Mac: Xcode's command-line tools, Rust from [rustup.rs](https://rustup.rs), then the CLI.

```sh
git clone https://github.com/welshofer/Scaena.git && cd Scaena
cargo install --path crates/scaena-cli --locked    # puts `scaena` in ~/.cargo/bin
```

`scaena export --format html` needs the web pages built first: run `just web`, then install the CLI again (ADR-0010). That needs [`just`](https://github.com/casey/just), Node, the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` 0.2.129.

**The loop.** Start a deck, turn it into source, serve it, and write:

```sh
scaena new talk --theme dusk --title "Field notes"   # a new deck: dusk, daybreak, ember, or salon
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
  - `scaena start talk revenue --layout bullets` adds a slide after the slide `revenue` is on, in one of the theme's layouts, as the editor's Add Slide does: its words to write over. `scaena inspect talk --state revenue --layouts` lists the layouts a slide may take.
  - `scaena background talk revenue backdrop` gives a slide one of the theme's backgrounds, `--none` takes it away, and with neither it lists what the slide may take.
  - `scaena data talk q3` shows a data source's rows as written, each by its index, and any value its column does not read. `--edits` sets cells, adds rows, and takes them away, in one write of the file that keeps every other byte (SPEC §3.10).
  - `scaena files talk` lists the bundle's images, fonts, and data, what names each, and the objects drawn from each, in which states. `--remove` takes out files nothing names, and refuses the rest.
  - `scaena theme talk --edit ops.json` edits the theme the deck names by JSON Patch operations on it (`/tokens/color/accent`, `/type/roles/body/size`), refused where the deck would not check in the theme it leaves; `--apply theme.json` swaps in another theme, and `--apply salon` one that ships, by its name, with its fonts (SPEC §3.6).
  - `scaena map talk "Ferry Building, San Francisco"` puts a map on a slide: a picture from Geoapify, with your own key in `GEOAPIFY_API_KEY` (from geoapify.com; its Free plan will do), in the slide's empty place for a picture, or the image `--into` names, or a new one `--at X,Y,W,H`. The place's words go to Geoapify, and the map's middle and size. `--show city` takes in more, `--places` lists what a name finds, and `--pick 2` takes the second. `--refresh map --zoom 16` fetches it again. The picture stays in the bundle, so the deck draws offline.
  - `scaena generate talk "Why the trail needs a bridge" --audience "the county board" --goal Persuade` writes a deck from a prompt with Vzestup's Narrato model and your own key (`ANTHROPIC_API_KEY`, `OPENAI_API_KEY`, or `GEMINI_API_KEY`; `--models` lists what the key can use), grounded in what `--source` gives it: a file, a web page, `sec:TICKER`, or `github:OWNER/REPO`. `--slides slides.json` makes the deck from slides written elsewhere, with no call.
  - `scaena health talk` reads the deck as a talk (SPEC §7.7): pacing against the length its brief asked, whether each slide makes two to five points and its notes give the speaker enough to say, how each slide follows from the one before, and how the slides' intensity follows the arc, with a finding on each slide to look at and, where one is safe, a fix. `--json` gives the editor's Story tab's numbers.
  - `scaena icons arrow right --json` finds the icons that ship by their words, each its path and the view it is drawn on, for a shape on a slide.
  - `scaena history talk` lists the versions its history keeps. `--at 3` prints version 3's deck, `--diff 3` says what changed from it to the deck now (`--diff 3,5` from one version to another), and `--restore 3` makes it the deck again, with its data files, as one change.

**Pick one place to write.** If you keep `deck.scn`, it is your source: `scaena serve` compiles it as you save, or `compile` does when you ask. The editor on a served folder opens and saves that same `deck.scn`. The editor anywhere else writes `deck.json` alone, and leaves a `deck.scn` behind. If you edit only in the editor, `deck.json` is the source, and `scaena decompile` gives you its `.scn` whenever you want it.

## An assistant

The editor's **Assistant** tab works on the open deck with your own key: Anthropic, OpenAI, or Gemini. Ask in plain words, such as "put the headline 'Revenue doubled' on the second slide and fix what lint finds". It edits through the same operations, lints, renders, and shows you each step. Each of its edits shows in the source as it happens, and undoes as one. Under each step that changed a slide, the tab shows that slide before and after; click it to go there.

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
| W200–W232 | typography and fonts | a widow (W200), too many words on screen (W210), a font whose license forbids what a bundle does with it (W230), italic asked of a family with no italic face (W231), a weight heavier than a family of one weight draws (W232) |
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

state cover layout:title-center
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
- **The header.** `deck "Q3 Review" theme:… canvas:1920x1080` names the theme and the canvas in canvas units, which the painters scale to any size. Add `formats:[9:16, 1:1]` to lay the deck out again in other shapes. A node placed in a slot moves with the slot; one that needs to stand elsewhere in a format says so on the node, `formats:{"9:16": {at: col(1-12) row(3-12), cols: 1}}`: where it goes, how big, how it is turned, a container's tracks and spacing, and a text's lines there, its own everywhere else.
- **Slide numbers and a footer.** Add `furniture:{number: true, footer: "Q3 Review · Board"}` to the header to number each slide and set those words at its foot, where the theme places them. The number is the slide's place in the deck, every step of a slide shows it, and the theme leaves both off its title slides. A slide's first state says `furniture:false` to show neither there, or `furniture:true` to show them on a layout the theme leaves them off; a later step that says either is an error.
- **Fonts and data.** `font` lists each of the theme's font families, with its file in the bundle, and each family's italic face (`style:italic`), which text in italic is set in. `data q3 "…"` declares a data source and its columns' types. A chart reads it as `@q3`.
- **Fonts a theme keeps as they are.** A theme family may say `upright: true`: it has no italic by design, because its script sets none, as Japanese and Korean do not. Text in italic is set upright in it, and lint still says so (W231). A family may also say `whole: true`, and then no save subsets its files, in the editor, on the Mac and the iPad, or with `scaena save`: a font whose license reserves its name, as Pretendard's does, may not keep that name once it is cut down. Both are the theme's to say, not the deck's (SPEC §3.6).
- **The spine.** `section` and `beat` say what each part claims, and which states make the claim. The PDF, the video's chapters, and the narrative lint read them.
- **`state cover layout:title-center`** is the first click, on the theme's `title-center` layout: a title and the line under it, centered. The theme's `title` layout keeps a place for a picture beside them, which lint (W304) says is empty until one fills it.
  - `title text role:display "Q3 Review" … at:in(title)` declares an object where it first appears: its id, its type, its text, its role, and its slot.
  - `notes` are the speaker's notes.
- **`state revenue layout:figure transition:standard`** is the next click, on another layout:
  - `-claim` takes the subtitle off;
  - `title "Pro drove the growth" role:headline … at:in(header)` is the same `title`, so its words morph and it moves to its new slot;
  - `rev chart:bar …` enters, and `choreo rev enter:grow` grows its bars in.
- **`state mix slide:revenue`** is a build on the same slide. `rev kind:stackedBar` turns the same bars, matched by key, into stacks. `rev orient:horizontal` would run them across instead, the quarters down the side: the way a ranking, or categories with long names, reads best. `rev kind:slope`, its quarters filtered to two, would compare the first with the last, each product a line that says its change. `rev facet:{field: product}` would draw a panel for each product instead, on one scale.

What the props mean is in SPEC §3: the objects in §3.3, layout in §3.4, type in §3.5, charts in §3.7, and motion in §3.9. `docs/examples/revenue.deck.scn` is a fuller deck, and `docs/examples/trails.deck.json` (decompile it to read it as source) is fifteen slides that use most of what a deck can hold.

## The theme's names

The shipped themes, Dusk, Daybreak, Ember, and Salon, share these names, so a deck moves between them with no other change (`scaena theme --apply`). A name the theme does not have is E102. A test holds this section to the themes.

Each layout's slots say what goes in them (`prompt`, SPEC §3.6): words to type over, a picture, a chart, or a card, a rule, an arrow, or a chevron. Add Slide in the editor offers every layout by the gallery's sections, Titles, Statements, Lists, Comparison, Frameworks, Process, Time, Numbers, Charts, Pictures, People, Editorial, Diagrams, Briefing, Classroom, and Closing, each under its name and drawn as its picture: the slide it starts, a picture in each picture's place, a chart or a table in each figure's, and the theme's picture with a pin over it in each map's, as its slots' `example`s say (PLAN 3.30, 3.31, O.1e); Insert › Map… fills a map's place. A new slide in one has the words in its slots and its cards and rules; its pictures and figures wait for you. Its search finds a layout by its name, what it is for, its section, or another name people call it: Kanban finds Roadmap, and Mind Map finds Hub and Spoke. A slot that `bleed`s reaches the canvas's edge: `photo` is a picture edge to edge, and `photo-left` one off the left edge. `scripts/build_layouts.py` writes the library into each theme.

| Layout | Its slots | In the gallery |
|---|---|---|
| `title` | `kicker`, `title`, `subtitle`, `art` | Title, in Titles |
| `section` | `kicker`, `number`, `title`, `support` | Section, in Titles |
| `photo-title` | `art`, `title`, `subtitle` | Title over Photo, in Titles |
| `title-only` | `kicker`, `header` | Title Only, in Titles |
| `title-center` | `kicker`, `title`, `subtitle` | Centered Title, in Titles |
| `title-split` | `art`, `title`, `subtitle`, `kicker` | Title beside Photo, in Titles |
| `section-photo` | `art`, `number`, `title`, `support` | Section and Photo, in Titles |
| `section-number` | `kicker`, `number`, `title`, `support` | Section Number, in Titles |
| `paper-title` | `title`, `authors`, `affiliations`, `venue`, `first-logo`, `second-logo` | Paper Title, in Titles |
| `statement` | `kicker`, `statement`, `support` | Statement, in Statements |
| `poster` | `kicker`, `statement`, `support`, `note`, `art` | Poster, in Statements |
| `quote` | `quote`, `who` | Quote, in Statements |
| `thesis` | `kicker`, `claim`, `support` | Thesis, in Statements |
| `big-question` | `kicker`, `question`, `support` | Big Question, in Statements |
| `manifesto` | `kicker`, `header`, `body` | Manifesto, in Statements |
| `key-point` | `kicker`, `card`, `point`, `follow` | Key Point, in Statements |
| `haiku` | `poem`, `rule`, `attribution` | Haiku, in Statements: the poem in `verse`, shrunk to fit, a short rule, and who wrote it or what it is on, which waits empty |
| `lyrics` | `art`, `words`, `credit` | Lyrics, in Statements |
| `bullets` | `kicker`, `header`, `body` | Bullets, in Lists |
| `numbered` | `kicker`, `header`, `body` | Numbered List, in Lists |
| `agenda` | `kicker`, `header`, `body` | Agenda, in Lists |
| `three-rows` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Three Rows, in Lists |
| `bullets-photo` | `art`, `header`, `body` | Bullets and Photo, in Lists |
| `checklist` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first`, `second`, `third`, `fourth`, `first-state`, `second-state`, `third-state`, `fourth-state` | Checklist, in Lists |
| `agenda-photo` | `art`, `header`, `body`, `kicker` | Agenda and Photo, in Lists |
| `four-points` | `kicker`, `header`, `first-number`, `second-number`, `third-number`, `fourth-number`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Four Points, in Lists |
| `numbered-cards` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-number`, `second-number`, `third-number`, `fourth-number`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Numbered Cards, in Lists |
| `icon-columns` | `kicker`, `header`, `first-icon`, `second-icon`, `third-icon`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Icon Columns, in Lists |
| `icon-cards` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-icon`, `second-icon`, `third-icon`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Icon Cards, in Lists |
| `icon-rows` | `kicker`, `header`, `first-icon`, `second-icon`, `third-icon`, `fourth-icon`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Icon Rows, in Lists |
| `icon-grid` | `kicker`, `header`, `first-icon`, `second-icon`, `third-icon`, `fourth-icon`, `fifth-icon`, `sixth-icon`, `first-title`, `second-title`, `third-title`, `fourth-title`, `fifth-title`, `sixth-title`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth` | Icon Grid, in Lists |
| `two-columns` | `kicker`, `header`, `first`, `second` | Two Columns, in Comparison |
| `comparison` | `kicker`, `header`, `first-title`, `first`, `second-title`, `second` | Comparison, in Comparison |
| `pros-cons` | `kicker`, `header`, `first-title`, `first`, `second-title`, `second` | Pros and Cons, in Comparison |
| `three-columns` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Three Columns, in Comparison |
| `cards` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Cards, in Comparison |
| `before-after` | `kicker`, `header`, `first-art`, `arrow`, `second-art`, `first`, `second` | Before and After, in Comparison |
| `head-to-head` | `kicker`, `header`, `middle-card`, `first-title`, `second-title`, `first-criterion`, `second-criterion`, `third-criterion`, `first-left`, `second-left`, `third-left`, `first-right`, `second-right`, `third-right` | Head to Head, in Comparison |
| `scenarios` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-title`, `second-title`, `third-title`, `first-figure`, `second-figure`, `third-figure`, `first`, `second`, `third` | Scenarios, in Comparison |
| `myth-fact` | `kicker`, `header`, `first-card`, `second-card`, `first-title`, `second-title`, `first`, `second` | Myth and Fact, in Comparison |
| `pricing` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `mark`, `first-title`, `second-title`, `third-title`, `first-price`, `second-price`, `third-price`, `first`, `second`, `third` | Pricing, in Comparison |
| `feature-grid` | `header`, `first-card`, `third-card`, `first-product`, `second-product`, `third-product`, `fourth-product`, `first`, `second`, `third`, `fourth`, `first-first`, `first-second`, `first-third`, `first-fourth`, `second-first`, `second-second`, `second-third`, `second-fourth`, `third-first`, `third-second`, `third-third`, `third-fourth`, `fourth-first`, `fourth-second`, `fourth-third`, `fourth-fourth` | Feature Grid, in Comparison |
| `quadrants` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Quadrants, in Frameworks |
| `funnel` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first`, `second`, `third`, `fourth` | Funnel, in Frameworks |
| `pyramid` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first`, `second`, `third`, `fourth` | Pyramid, in Frameworks |
| `matrix` | `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `y-high`, `y-low`, `first`, `second`, `third`, `fourth`, `x-low`, `x-high` | Two-by-Two Matrix, in Frameworks |
| `swot` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | SWOT, in Frameworks |
| `okrs` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first`, `second`, `third`, `fourth`, `first-target`, `second-target`, `third-target`, `fourth-target` | OKRs, in Frameworks |
| `raci` | `kicker`, `header`, `first-card`, `third-card`, `first-role`, `second-role`, `third-role`, `first-task`, `second-task`, `third-task`, `first-lead`, `first-team`, `first-sponsor`, `second-lead`, `second-team`, `second-sponsor`, `third-lead`, `third-team`, `third-sponsor` | RACI, in Frameworks |
| `three-horizons` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Three Horizons, in Frameworks |
| `venn` | `kicker`, `header`, `first-circle`, `second-circle`, `third-circle`, `first`, `second`, `third`, `note` | Venn Diagram, in Frameworks |
| `cycle` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-arrow`, `second-arrow`, `third-arrow`, `fourth-arrow`, `first`, `second`, `third`, `fourth` | Cycle, in Frameworks |
| `nine-box` | `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `fifth-card`, `sixth-card`, `seventh-card`, `eighth-card`, `ninth-card`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth`, `seventh`, `eighth`, `ninth` | Nine-Box Grid, in Frameworks |
| `stack` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Stack, in Frameworks |
| `hub-spoke` | `kicker`, `header`, `hub-circle`, `first-spoke`, `second-spoke`, `third-spoke`, `fourth-spoke`, `hub`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Hub and Spoke, in Frameworks |
| `risk-matrix` | `kicker`, `header`, `cell-5-1`, `cell-5-2`, `cell-5-3`, `cell-5-4`, `cell-5-5`, `cell-4-1`, `cell-4-2`, `cell-4-3`, `cell-4-4`, `cell-4-5`, `cell-3-1`, `cell-3-2`, `cell-3-3`, `cell-3-4`, `cell-3-5`, `cell-2-1`, `cell-2-2`, `cell-2-3`, `cell-2-4`, `cell-2-5`, `cell-1-1`, `cell-1-2`, `cell-1-3`, `cell-1-4`, `cell-1-5`, `likely-title`, `likely-5`, `likely-4`, `likely-3`, `likely-2`, `likely-1`, `impact-1`, `impact-2`, `impact-3`, `impact-4`, `impact-5`, `impact-title`, `first`, `second`, `third`, `takeaway`, `note` | Risk Matrix, in Frameworks |
| `business-model-canvas` | `kicker`, `header`, `partners-card`, `activities-card`, `resources-card`, `value-card`, `relationships-card`, `channels-card`, `segments-card`, `costs-card`, `revenue-card`, `partners`, `activities`, `resources`, `value`, `relationships`, `channels`, `segments`, `costs`, `revenue`, `value-words` | Business Model Canvas, in Frameworks |
| `maturity-model` | `kicker`, `header`, `first-step`, `second-step`, `third-step`, `fourth-step`, `fifth-step`, `here-arrow`, `here`, `takeaway`, `first`, `second`, `third`, `fourth`, `fifth` | Maturity Model, in Frameworks |
| `steps` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Steps, in Process |
| `process` | `kicker`, `header`, `first-chevron`, `second-chevron`, `third-chevron`, `fourth-chevron`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Process, in Process |
| `how-it-works` | `kicker`, `header`, `first-number`, `second-number`, `third-number`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | How It Works, in Process |
| `pipeline` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first-figure`, `second-figure`, `third-figure`, `fourth-figure`, `first`, `second`, `third`, `fourth` | Pipeline, in Process |
| `journey` | `kicker`, `header`, `second-lane`, `first-phase`, `second-phase`, `third-phase`, `first-lane`, `third-lane`, `does-first`, `does-second`, `does-third`, `thinks-first`, `thinks-second`, `thinks-third`, `feels-first`, `feels-second`, `feels-third` | Journey, in Process |
| `recipe` | `kicker`, `header`, `needs-title`, `needs`, `method-title`, `method` | Recipe, in Process |
| `roadmap` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Roadmap, in Time |
| `timeline` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `fourth-title`, `rule`, `first`, `second`, `third`, `fourth` | Timeline, in Time |
| `milestones` | `kicker`, `header`, `rule`, `first-date`, `second-date`, `third-date`, `fourth-date`, `first`, `second`, `third`, `fourth` | Milestones, in Time |
| `quarters` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `fourth-title`, `rule`, `first`, `second`, `third`, `fourth` | Quarters, in Time |
| `gantt` | `kicker`, `header`, `first-month`, `second-month`, `third-month`, `first-bar`, `second-bar`, `third-bar`, `fourth-bar`, `first`, `second`, `third`, `fourth` | Gantt Chart, in Time |
| `year` | `kicker`, `header`, `month-1`, `month-2`, `month-3`, `month-4`, `month-5`, `month-6`, `month-7`, `month-8`, `month-9`, `month-10`, `month-11`, `month-12`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Year at a Glance, in Time |
| `timeline-photos` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art`, `first-date`, `second-date`, `third-date`, `fourth-date`, `first`, `second`, `third`, `fourth` | Timeline with Photos, in Time |
| `month` | `kicker`, `header`, `main` | Month, in Time |
| `event` | `art`, `tag`, `date`, `what`, `where` | Event, in Time |
| `stat` | `kicker`, `number`, `claim`, `detail`, `under` | Stat, in Numbers |
| `big-number` | `number`, `claim`, `under` | Big Number, in Numbers |
| `two-numbers` | `kicker`, `first-title`, `second-title`, `first`, `second` | Two Numbers, in Numbers |
| `numbers` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Three Numbers, in Numbers |
| `number-grid` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `fifth-card`, `sixth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `fifth-title`, `sixth-title`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth` | Six Numbers, in Numbers |
| `two-figures` | `kicker`, `header`, `first-figure`, `second-figure`, `first`, `second`, `note` | Two Figures, in Numbers |
| `four-numbers` | `kicker`, `header`, `first-figure`, `second-figure`, `third-figure`, `fourth-figure`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Four Numbers, in Numbers |
| `scorecard` | `kicker`, `header`, `rule`, `first-figure`, `second-figure`, `third-figure`, `fourth-figure`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Scorecard, in Numbers |
| `ranking` | `kicker`, `header`, `first-card`, `third-card`, `first-rank`, `second-rank`, `third-rank`, `fourth-rank`, `first`, `second`, `third`, `fourth`, `first-figure`, `second-figure`, `third-figure`, `fourth-figure` | Ranking, in Numbers |
| `progress` | `kicker`, `header`, `first-track`, `second-track`, `third-track`, `first-bar`, `second-bar`, `third-bar`, `first`, `second`, `third`, `first-figure`, `second-figure`, `third-figure` | Progress, in Numbers |
| `number-photo` | `art`, `number`, `claim`, `under` | Number and Photo, in Numbers |
| `metric-trend` | `kicker`, `header`, `number`, `claim`, `main`, `note`, `source` | Number and Trend, in Numbers |
| `kpi-dashboard` | `kicker`, `header`, `first-figure`, `second-figure`, `third-figure`, `fourth-figure`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth`, `first-chart`, `second-chart` | KPI Dashboard, in Numbers |
| `status-report` | `kicker`, `header`, `first-card`, `third-card`, `stream-label`, `status-label`, `next-label`, `risk-label`, `first-icon`, `second-icon`, `third-icon`, `first`, `second`, `third`, `first-status`, `second-status`, `third-status`, `first-next`, `second-next`, `third-next`, `first-risk`, `second-risk`, `third-risk` | Status Report, in Numbers |
| `market-size` | `header`, `first-circle`, `second-circle`, `third-circle`, `first-figure`, `second-figure`, `third-figure`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Market Size, in Numbers |
| `pictograph` | `kicker`, `header`, `figure-1`, `figure-2`, `figure-3`, `figure-4`, `figure-5`, `figure-6`, `figure-7`, `figure-8`, `figure-9`, `figure-10`, `number`, `claim`, `note` | Pictograph, in Numbers |
| `figure` | `kicker`, `header`, `main`, `note` | Chart, in Charts |
| `narrow-figure` | `kicker`, `header`, `main`, `note` | Narrow Chart, in Charts |
| `split` | `body`, `main` | Chart and Words, in Charts |
| `figure-numbers` | `kicker`, `header`, `main`, `first-title`, `first`, `second-title`, `second` | Chart and Numbers, in Charts |
| `bar-chart` | `kicker`, `header`, `main`, `takeaway`, `note` | Bar Chart, in Charts |
| `line-chart` | `kicker`, `header`, `main`, `callout`, `note`, `source` | Line Chart, in Charts |
| `donut-chart` | `kicker`, `header`, `main`, `first-title`, `first`, `second-title`, `second` | Donut Chart, in Charts |
| `table` | `kicker`, `header`, `main`, `note` | Table, in Charts |
| `chart-bullets` | `kicker`, `header`, `main`, `body` | Chart and Bullets, in Charts |
| `two-charts` | `kicker`, `header`, `first-title`, `second-title`, `first-chart`, `second-chart` | Two Charts, in Charts |
| `positioning-map` | `kicker`, `header`, `main`, `takeaway`, `note`, `source` | Positioning Map, in Charts |
| `full` | `kicker`, `header`, `main`, `note` | Full Bleed, in Pictures |
| `art-left` | `kicker`, `header`, `art`, `body` | Art Left, in Pictures |
| `art-right` | `kicker`, `header`, `body`, `art` | Art Right, in Pictures |
| `photo` | `art` | Photo, in Pictures |
| `photo-caption` | `art`, `note` | Photo and Caption, in Pictures |
| `photo-left` | `art`, `header`, `body` | Photo Left, in Pictures |
| `photo-right` | `header`, `body`, `art` | Photo Right, in Pictures |
| `two-up` | `kicker`, `header`, `first-art`, `second-art`, `first`, `second` | Two Up, in Pictures |
| `three-up` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `first`, `second`, `third` | Three Up, in Pictures |
| `four-up` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art` | Four Up, in Pictures |
| `mosaic` | `first-art`, `second-art`, `third-art` | Mosaic, in Pictures |
| `six-up` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art`, `fifth-art`, `sixth-art` | Six Up, in Pictures |
| `panorama` | `kicker`, `header`, `art`, `note` | Panorama, in Pictures |
| `photo-quote` | `art`, `quote`, `who` | Quote over Photo, in Pictures |
| `four-in-a-row` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art`, `first`, `second`, `third`, `fourth` | Four in a Row, in Pictures |
| `callout` | `kicker`, `header`, `art`, `first-line`, `second-line`, `third-line`, `fourth-line`, `first`, `second`, `third`, `fourth` | Callouts, in Pictures |
| `photo-grid-text` | `first-art`, `second-art`, `third-art`, `fourth-art`, `header`, `body` | Photo Grid and Words, in Pictures |
| `logo-wall` | `kicker`, `header`, `logo-1`, `logo-2`, `logo-3`, `logo-4`, `logo-5`, `logo-6`, `logo-7`, `logo-8`, `logo-9`, `logo-10`, `logo-11`, `logo-12` | Logo Wall, in Pictures |
| `device-mockup` | `kicker`, `header`, `screen`, `frame`, `base`, `title`, `body` | Device Mockup, in Pictures |
| `map` | `kicker`, `header`, `art`, `note` | Map, in Pictures |
| `map-words` | `art`, `header`, `body` | Map and Words, in Pictures |
| `map-full` | `art`, `header` | Full-Bleed Map, in Pictures |
| `property` | `art`, `price`, `address`, `beds`, `baths`, `area`, `second-art` | Property, in Pictures |
| `exhibit-callout` | `kicker`, `header`, `page`, `ring`, `pointer`, `passage-card`, `passage`, `source` | Exhibit Callout, in Pictures |
| `team` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `first-title`, `second-title`, `third-title`, `first`, `second`, `third` | Team, in People |
| `testimonials` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first`, `second`, `third`, `first-note`, `second-note`, `third-note` | Testimonials, in People |
| `quote-portrait` | `art`, `quote`, `who` | Quote and Portrait, in People |
| `bio` | `kicker`, `art`, `name`, `role`, `story` | Bio, in People |
| `team-four` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Team of Four, in People |
| `team-six` | `kicker`, `header`, `first-art`, `second-art`, `third-art`, `fourth-art`, `fifth-art`, `sixth-art`, `first-title`, `second-title`, `third-title`, `fourth-title`, `fifth-title`, `sixth-title`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth` | Team of Six, in People |
| `team-names` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `fifth-card`, `sixth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `fifth-title`, `sixth-title`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth` | Team by Name, in People |
| `org-chart` | `kicker`, `header`, `top-card`, `first-card`, `second-card`, `third-card`, `trunk`, `branch`, `first-drop`, `second-drop`, `third-drop`, `top`, `first`, `second`, `third` | Org Chart, in People |
| `speaker` | `art`, `name`, `role`, `talk` | Speaker, in People |
| `two-quotes` | `kicker`, `header`, `first-card`, `second-card`, `first`, `second`, `first-note`, `second-note` | Two Quotes, in People |
| `buyer-persona` | `kicker`, `art`, `name`, `role`, `goals-title`, `goals`, `pains-title`, `pains` | Buyer Persona, in People |
| `definition` | `kicker`, `term`, `meaning`, `example` | Definition, in Editorial |
| `letter` | `kicker`, `from`, `date`, `to`, `body`, `sign` | Letter, in Editorial |
| `memo` | `kicker`, `header`, `to-label`, `from-label`, `about-label`, `date-label`, `to`, `from`, `about`, `date`, `rule`, `body` | Memo, in Editorial |
| `receipt` | `kicker`, `slip`, `title`, `first-item`, `second-item`, `third-item`, `fourth-item`, `first-amount`, `second-amount`, `third-amount`, `fourth-amount`, `rule`, `total-label`, `total` | Receipt, in Editorial |
| `chyron` | `art`, `band`, `tag`, `story` | Lower Third, in Editorial |
| `headline` | `tag`, `headline`, `dek`, `byline`, `art` | Front Page, in Editorial |
| `faq` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | FAQ, in Editorial |
| `glossary` | `kicker`, `header`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Glossary, in Editorial |
| `magazine` | `kicker`, `header`, `first`, `pull`, `second` | Magazine Spread, in Editorial |
| `case-study` | `kicker`, `header`, `art`, `challenge-title`, `challenge`, `done-title`, `done`, `result-figure`, `result` | Case Study, in Editorial |
| `certificate` | `border`, `kind`, `presented`, `name`, `for`, `first-line`, `second-line`, `first-sign`, `second-sign` | Certificate, in Editorial |
| `references` | `kicker`, `header`, `body` | References, in Editorial |
| `patient-case` | `kicker`, `header`, `art`, `presentation-title`, `history-title`, `findings-title`, `presentation`, `history`, `findings` | Patient Case, in Editorial |
| `brand-board` | `kicker`, `header`, `logo`, `logo-frame`, `logo-use`, `first-swatch`, `second-swatch`, `third-swatch`, `first`, `second`, `third`, `type-sample`, `type-title`, `type` | Brand Board, in Editorial |
| `fine-print` | `kicker`, `header`, `body` | Fine Print, in Editorial |
| `flowchart` | `kicker`, `header`, `first-card`, `first-arrow`, `decision-shape`, `yes-arrow`, `second-card`, `no-arrow`, `third-card`, `back-arrow`, `first`, `decision`, `yes`, `second`, `no`, `third` | Flowchart, in Diagrams |
| `swimlane` | `kicker`, `header`, `first-lane`, `second-lane`, `third-lane`, `first-step`, `second-step`, `third-step`, `fourth-step`, `fifth-step`, `first-arrow`, `second-arrow`, `third-arrow`, `fourth-arrow`, `first-role`, `second-role`, `third-role`, `first`, `second`, `third`, `fourth`, `fifth` | Swimlane, in Diagrams |
| `decision-tree` | `kicker`, `header`, `choice-card`, `first-chance-shape`, `second-chance-shape`, `first-branch`, `second-branch`, `first-first-branch`, `first-second-branch`, `second-first-branch`, `second-second-branch`, `choice`, `first`, `second`, `first-first-odds`, `first-second-odds`, `second-first-odds`, `second-second-odds`, `first-first`, `first-second`, `second-first`, `second-second` | Decision Tree, in Diagrams |
| `fishbone` | `kicker`, `header`, `spine`, `first-bone`, `second-bone`, `third-bone`, `fourth-bone`, `head-card`, `head`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Fishbone, in Diagrams |
| `architecture` | `kicker`, `header`, `client-first-card`, `client-second-card`, `client-third-card`, `service-first-card`, `service-second-card`, `service-third-card`, `data-card`, `first-call`, `second-call`, `third-call`, `first-query`, `second-query`, `third-query`, `client-tier`, `service-tier`, `data-tier`, `client-first`, `client-second`, `client-third`, `service-first`, `service-second`, `service-third`, `data` | Architecture, in Diagrams |
| `issue-tree` | `kicker`, `header`, `root-card`, `first-card`, `second-card`, `first-branch`, `second-branch`, `first-first-branch`, `first-second-branch`, `second-first-branch`, `second-second-branch`, `root`, `first`, `second`, `first-first`, `first-second`, `second-first`, `second-second` | Issue Tree, in Diagrams |
| `seating-chart` | `kicker`, `header`, `first-table`, `second-table`, `third-table`, `fourth-table`, `fifth-table`, `sixth-table`, `first`, `second`, `third`, `fourth`, `fifth`, `sixth`, `first-left`, `second-left`, `third-left`, `fourth-left`, `fifth-left`, `sixth-left`, `first-right`, `second-right`, `third-right`, `fourth-right`, `fifth-right`, `sixth-right` | Seating Chart, in Diagrams |
| `quad-chart` | `kicker`, `header`, `art`, `objectives-card`, `team-card`, `milestones-card`, `dates-title`, `dates`, `objectives-title`, `team-title`, `milestones-title`, `objectives`, `team`, `milestones` | Quad Chart, in Briefing |
| `decision-brief` | `kicker`, `header`, `bottom-card`, `bottom-title`, `bottom`, `first-card`, `second-card`, `third-card`, `fourth-card`, `mark`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Decision Brief, in Briefing |
| `incident-review` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-title`, `second-title`, `third-title`, `fourth-title`, `first`, `second`, `third`, `fourth` | Incident Review, in Briefing |
| `options-compared` | `kicker`, `header`, `first-card`, `third-card`, `chosen`, `option-label`, `first-criterion`, `second-criterion`, `third-criterion`, `total-label`, `first`, `second`, `third`, `first-first`, `first-second`, `first-third`, `second-first`, `second-second`, `second-third`, `third-first`, `third-second`, `third-third`, `first-total`, `second-total`, `third-total` | Options Compared, in Briefing |
| `concept-of-operations` | `kicker`, `header`, `first-phase-shape`, `second-phase-shape`, `third-phase-shape`, `first-first-card`, `first-second-card`, `first-third-card`, `second-first-card`, `second-second-card`, `second-third-card`, `third-first-card`, `third-second-card`, `third-third-card`, `first-phase`, `second-phase`, `third-phase`, `first-effort`, `second-effort`, `third-effort`, `first-first`, `first-second`, `first-third`, `second-first`, `second-second`, `second-third`, `third-first`, `third-second`, `third-third` | Concept of Operations, in Briefing |
| `quiz` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `fourth-card`, `first-letter`, `second-letter`, `third-letter`, `fourth-letter`, `first`, `second`, `third`, `fourth` | Quiz, in Classroom |
| `game-board` | `kicker`, `header`, `first-first-card`, `first-second-card`, `first-third-card`, `second-first-card`, `second-second-card`, `second-third-card`, `third-first-card`, `third-second-card`, `third-third-card`, `fourth-first-card`, `fourth-second-card`, `fourth-third-card`, `first-category`, `second-category`, `third-category`, `fourth-category`, `first-first`, `first-second`, `first-third`, `second-first`, `second-second`, `second-third`, `third-first`, `third-second`, `third-third`, `fourth-first`, `fourth-second`, `fourth-third` | Game Board, in Classroom |
| `closing` | `kicker`, `statement`, `support` | Closing, in Closing |
| `next-steps` | `kicker`, `header`, `action-label`, `owner-label`, `when-label`, `first-card`, `second-card`, `first`, `second`, `third`, `first-owner`, `second-owner`, `third-owner`, `first-when`, `second-when`, `third-when` | Next Steps, in Closing |
| `the-ask` | `kicker`, `ask`, `first`, `second`, `third` | The Ask, in Closing |
| `qa` | `kicker`, `question`, `support` | Q&A, in Closing |
| `contact` | `art`, `thanks`, `name`, `details` | Contact, in Closing |
| `takeaways` | `kicker`, `header`, `first-card`, `second-card`, `third-card`, `first-number`, `second-number`, `third-number`, `first`, `second`, `third` | Takeaways, in Closing |
| `call-to-action` | `kicker`, `statement`, `button-card`, `button` | Call to Action, in Closing |
| `acknowledgments` | `kicker`, `header`, `thanks`, `names`, `logo-1`, `logo-2`, `logo-3`, `logo-4`, `logo-5`, `logo-6` | Acknowledgments, in Closing |
| `join-or-scan` | `kicker`, `code`, `ask`, `link`, `how` | Join or Scan, in Closing |

Every layout also has `in(canvas)`, the whole canvas, and `in(grid)`, the margins' box. `at:col(1-6) row(2-8)` places by the theme's 12 × 12 grid instead.

| Names | |
|---|---|
| Text roles (`role:`) | `display`, `headline`, `title`, `body`, `caption`, `label`, `axis`, `value`, `lede`, `kicker`, `figure`, `quote`, `verse`, `numeral`, `hero`, `code` |
| Motion presets (`enter:`, `exit:`, `emphasis:`) | `appear`, `fade`, `rise`, `words`, `bullets`, `typewriter`, `pop`, `scale`, `fadeScale`, `scaleBig`, `drop`, `moveInLeft`, `moveInRight`, `moveInTop`, `moveInBottom`, `skid`, `spin`, `pivot`, `swing`, `flip`, `grow`, `draw`, `pulse`, `blink`, `jiggle` |
| Durations (`transition:`, `duration:`) | `fast`, `standard`, `slow` |
| Easings (`ease:`) | `standard`, `in`, `out`, `linear` |
| Springs (`spring:`) | `snappy`, `gentle`, `heavy`, `bouncy` |
| Colors | `ink`, `paper`, `paper-2`, `accent`, `accent-2`, `muted`, `line` |
| Color roles | `surface`, `surface-2`, `onSurface`, `onSurfaceMuted`, `accent`, `onAccent`, `line` |
| Shader presets (`preset:`) | `backdrop` and `aurora`, meshes behind a title; `texture` and `haze`, slow noise fields; `lagoon`, light on water (a caustics shader); `sweep`, a conic gradient turning slowly; `dust`, drifting particles |

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

**Text that does not read.** Lint says E110 for body text under 4.5:1 against what lies behind it, or E111 for display text under 3:1, with the colors it measured. Its fix chooses the theme's color that reads best there, as the inspector's color does, written where the text's color lives. Lint offers it only where the text reads in that color in every state and format the choice reaches. Re-themed in Daybreak, the trails deck's storm slide would set dark text on a dark photo; the fix sets it in `surface`, and a re-theme takes that fix itself, in the editor as on the command line, and says which texts it recolored.

**Motion, and a self-running deck.** `transition` names how long the cue takes; `choreo` brings an object in with a preset, after a delay; `hold` moves on by itself after the cue, for a kiosk or a video. Once any state holds, the deck runs on its own, so a state with neither a cue nor a hold would show for 0 ms. Lint W323 names each one, here `cover` and `mix`, with a hold that fits its reading.

```scn
state turn layout:statement transition:slow hold:4s
  -title
  -rev
  point text role:display "One more thing" semantic:claim at:in(statement)
  choreo point enter:rise delay:240ms
```

**A picture.** A PNG or a JPEG in the bundle's `assets/` folder, with `alt` text for a reader (W410 without it). A photo goes in as the camera wrote it: one stored on its side shows upright, as its EXIF orientation says. A print workflow's CMYK JPEG goes in as it is, its inks shown as they print. A JPEG at 12 bits a sample is refused, saying why; save it at 8 bits first (SPEC §3.3):

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

**A pair, kerned.** A run's `tracking` is space after each of its characters, in em, on top of the font's own kerning: on the first letter of a pair alone, it closes or opens that pair from where the font set it. A run's own `features` turn an OpenType feature on or off for its words alone, after the text's: here `Today` sets its letters loose (SPEC §3.5):

```scn
state kerned slide:revenue
  title runs:[{text: "A", style: {tracking: -0.02}}, {text: "V Labs, "}, {text: "Today", features: {kern: false}}]
```

**A figure from the data.** A run's `quote` makes its words a value of a data source: a row, found by its values or its index (`-1` is the last), and a column, written by a `format`. Write any text for it: every patch and data edit sets it from the data, and, where they say the figure as a word, the claim and notes of the beat that shows it, the notes of the slide that shows it, and the descriptions of what is shown with it. If the data file changes some other way, lint says so (W427), and `scaena lint --fix` writes the figure (SPEC §3.5):

```scn
state pro slide:revenue
  title runs:[{text: "Pro reached "}, {text: "$19.4", quote: {data: @q3, row: {quarter: "2026-Q3", product: Pro}, column: revenue, format: "$.1f"}}]
```

**A list.** A text's paragraphs are its lines, each ended by a line break (`\n`). `list` makes them a list's items, one entry for each: `{kind: bullet}` or `{kind: number}`, deeper with `level`, or `null` for a paragraph that is no item. The theme draws the markers, a bullet or a number for each level, and indents each level its own step, the item's wrapped lines starting under its words (SPEC §3.5):

```scn
state plan layout:statement
  -title
  -rev
  steps text role:body "Hire two engineers\nShip the API\nThe docs first\nThen the SDK" semantic:claim
    list:[{kind: number}, {kind: number}, {kind: bullet, level: 1}, {kind: bullet, level: 1}] at:in(statement)
```

**Verse.** A poem goes in the theme's `verse` role, which sets `wrap: none`: its lines break only where you break them, each aligned on its own, so a centered haiku stands each line in the middle. Give it `fit: shrink` and a line too wide shrinks the whole poem, never under the role's `minSize` (W203 past it). Any text may take `wrap: none`. `docs/examples/haiku.deck.json` is a haiku on a slide (SPEC §3.5).

**Code.** A text with `code` is code, set as written; where it names no role, it is set in the theme's `code` role, JetBrains Mono in the themes that ship. Its lines break only where it breaks them: nothing wraps or hangs, each tab stands to the next stop of four columns (the theme's `code.tab` sets another), and every line runs left to right. Name the language it is written in, or `plain`. The theme colors its words by kind, keywords, types, strings, comments, and the rest, each color held to read on the slide; a language Scaena has no scanner for has only its strings, numbers, comments, and brackets colored, which lint notes (I403). A theme picks the colors in its `code` section, `"code": { "syntax": { "keyword": { "color": "accent", "weight": 700 }, "comment": { "color": "muted", "italic": true } } }`, and every kind it leaves out takes a color derived from its accent. Its words are its `text`, so it takes no `runs` or `list`, and none of what sets prose, such as `measure` or `fit:wrap` (E106 names each). Typed into, its box grows down first, as a presentation app's text box does; then the code shrinks to fit, to 12 pt at presentation size (24 cu on a 1920 × 1080 canvas) and no further unless a `minSize` lets it; code drawn smaller, by a `minSize`, a size of its own, or a `transform`, is W205. Code that still does not fit is W203, with how many of its lines fit. `docs/examples/code.deck.json` is a slide of it and a step that changes it (SPEC §3.5), and `docs/examples/code.deck.scn` its source.

Write code's lines as a block: three quotes, `"""`, at the end of its line, the lines as they read, then the same three quotes alone on a line, whose indentation comes off every line above it. A block closes only on a line that is its own run of quotes, so code with a line of `"""` alone, as a Python docstring may open on, takes a fence of four, `""""`, and so on. `scaena decompile` writes code with a tab, a carriage return, or white space at a line's end as a JSON string, `\t` and `\n` written out, since an editor may change those (SPEC §4.2, §4.3):

```scn
state rank layout:figure
  -rev
  title "Most votes first" at:in(header)
  sort text role:code code:{language: rust} semantic:evidence at:in(main) """
    fn rank(talks: &mut [Talk]) {
        talks.sort_by(|a, b| b.votes.cmp(&a.votes));
    }
    """
```

**A map.** A map is a picture a service drew, fetched once into the bundle when it is made, and an image that says so: its `fetched` record names the service, when its answer came, what was asked, and the credit its terms ask shown with it, as runs with their links (`fetched:{from: geoapify, at: "2026-10-15T20:12:44Z", asked: "Ferry Building", credit: […]}`). The engine draws that credit with the picture, in the theme's caption role and never under 12 pt, on a card in its bottom-left corner, or under it with `credit:below`; no deck or theme can hide it, and W314 says where it cannot be read. `pin:{at: [0.5, 0.5], label: "Ferry Building"}` puts Phosphor's map pin on the point of the picture `at` names, in fractions of the whole image as `crop` is, in the accent unless its `color` names another of the theme's, its label on a card beside it. Any image may take a pin. A map with no `alt` reads as the map of its place (SPEC §3.3).

**A forecast.** A forecast is a data source fetched once into the deck, its rows written inline, with the record a map takes: `fetched:{from: open-meteo, at: …, asked: "Boston", credit: […], forecast: {place: {name: Boston, zone: America/New_York, offset: -14400, …}, period: day, units: metric}}`. Each chart or table that reads it carries the credit at its foot, after the place and the forecast's day there, and "edited" once `data_edit` has changed a row. Its days pass, so `scaena lint` judges them at `--now` (an instant or a day; else `SOURCE_DATE_EPOCH`, else the clock): W428 says a forecast's first day has passed at its place, and names the source to fetch again in `refresh`. `kept: true` keeps it as of its date, and lint judges no date in it (SPEC §3.10, §7.4).

**A forecast's chart.** `kind: weather` draws a forecast as the weather on a phone draws it: a column for each day or hour across its box (a row down a box taller than wide), each its name, its sky's icon, its high, its low, and its chance of rain, with a drop where it is 10% or more. `weather:{show: [period, sky, words, high, low, chance, wind]}` picks the parts and their order, `units: us` or `metric` converts at layout, and `every: 3` sets the hours between columns; a `highlight` annotation at a day's date, or an hour's time, sets it on a band (SPEC §3.7).

For tables, stats, rows of cards, other chart kinds, and shader backgrounds, see SPEC §3.3 and the trails example.

## Present and export

- **Present** from the player: → and ← step, F goes fullscreen, and **Presenter** opens the presenter view, its timer paused and reset by its Pause and Reset, the time of day beside it. B blacks the screen until a key brings the slide back, a slide's number then Return goes there, L is the laser, and ? lists the keys. Past the last slide, a black screen says the show has ended: → or Escape leaves it, out of fullscreen, and ← shows the last slide. `?fps` in the player's address shows its frame rate.
- **One file that plays anywhere, offline:** `scaena export talk --format html --out talk.html`. It is the player and the deck in one HTML file, for a USB stick or an email.
- **PDF:** `scaena export talk --format pdf --out talk.pdf`. It draws each slide at its last state, in spine order, with real text a reader can copy.
- **Images and video:** `--format png` or `--format svg` write each state into a directory, named by its slide and step (`Slide 03.png`, or `Slide 03, step 1.png`, `Slide 03, step 2.png` for a slide with steps) so the files sort in the deck's order; each SVG is titled by its slide's headline. `--format mp4` plays the timeline, cues and holds, at `--fps`, with a chapter per beat.

## Reference

- SPEC §3: the document model, each object type and its props.
- SPEC §4: the `.scn` format and its grammar.
- SPEC §7.1: every command and flag.
- SPEC §7.5: every lint rule.
- `docs/schema/`: the JSON Schemas.
- `docs/examples/`: whole decks, themes, fonts, and data to start from.
- `skills/`: the procedures agents follow, which read as checklists for people too.
- `web/README.md`: the player and the editor in depth.

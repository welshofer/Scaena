# apps/mac — SwiftUI client (Phase 3)

SwiftUI owns chrome only (document browser, state list, timeline scrubber, inspector, source pane). All geometry comes from the Rust engine through `crates/scaena-ffi` (C ABI via cbindgen); painting is `vello` on Metal through a `CAMetalLayer`-backed `wgpu` surface, every layer on one GPU that the app makes as it starts, off its main thread (`ScaenaSurface.warm`), so a deck opens to its first frame without waiting for it. **No TextKit/CoreText in the render path.** Keys in Keychain; Apple Foundation Models for on-device assistant tasks. See SPEC §9.3 and PLAN §Phase 3. Phase 3 began on 2026-10-08 on Jay's call, with gate 2's criterion 1 still open. The engine reaches Swift through the session the browser edits (ADR-0021).

## ScaenaKit

`ScaenaKit/` is the Swift package over the engine's C ABI (`crates/scaena-ffi`, PLAN 3.1): `ScaenaSession` opens a bundle from a folder, its files, or a `.scaena` zip, and answers as the browser's editor does. To build and test it on a Mac, from the repository's root:

```
cargo build -p scaena-ffi
libs=$(cargo rustc -q --color never -p scaena-ffi --lib --crate-type staticlib -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
cd apps/mac/ScaenaKit && swift test -Xlinker -L"$PWD/../../../target/debug" $(for l in $libs; do printf -- '-Xlinker %s ' "$l"; done)
```

CI's macOS runner does the same on every pull request ready for review that touches the engine or the app.

## The app

`just mac` builds `Scaena.app` (`apps/mac/build-app.sh`) and opens it: New starts a deck from Dusk, and Open takes a `.scaena` bundle or zip, such as `tests/bench/b1.scaena`. Without Rust on the Mac, run the `mac-app` workflow (Actions → mac-app → Run workflow), or open a pull request that changes how the app is made, and download the run's `Scaena.app` artifact; macOS asks once whether to open an app signed ad hoc (right-click → Open).

The window (PLAN 3.4): the states down the side, each drawn small; the state chosen on the canvas, a click selecting what draws there; its cue under it, to play and scrub; the findings lint makes, each with its fix; the deck's `.scn` beside the canvas (the Source toggle); and the inspector, which edits the node selected, or the state with none. Every edit is one step to undo, and what a save writes is the deck the source compiles to.

Play (⌥⌘P, PLAN 3.5) presents the deck from the state shown: on the external display if there is one, with the presenter's window (the state, the next, its notes, the time) here. The keys are the web player's; Escape ends.

The assistant (⌥⌘A, PLAN 3.6) is the browser's, with your own key: put an Anthropic, OpenAI, or Gemini key in Settings (⌘,), where it is kept in your Keychain, then choose a model in the panel, which lists the ones the key can use. A question begins with the state shown and the node selected, so "this" and "shorter" mean them. Each edit shows as it is made, and Undo takes it back. On a Mac with Apple Intelligence (macOS 26), the sparkle beside a finding explains it, and the inspector drafts a state's notes and tightens a text's words, answered on the Mac and offered to take or leave.

On the canvas (PLAN 3.7), drag a node to move it and a handle to resize it: it snaps to the theme's grid or into a slot, as in the browser, and Option keeps the change to the state shown. Shift takes a node off the grid, where it is outlined in orange, as lint flags it (W301), or puts one back on it. Each drag is one step to undo.

Double-click a text to type in it where you clicked (PLAN 3.9), with Option to keep what you type to the state shown. The keys are a text editor's: Shift extends the selection, Option moves by words, ⌘ to a line's ends; a second click selects a word, a third its paragraph; an input method composes in place; ⌘B and ⌘I make the characters selected bold or italic. A burst of typing is one step to undo. Escape, Tab, or a click outside the text stops typing.

With characters selected, the inspector offers their look: a role, emphasis, family, weight, italic, or color for them alone (PLAN 3.10). ⌘K links them to a web address or a state, which a click follows in Play. ⌘⇧8 and ⌘⇧7 make paragraphs a bulleted or a numbered list. Tab and Shift+Tab move items a level in or out, and Return in an empty item ends the list.

The Node menu (PLAN 3.11) inserts, by kind, what the theme and the bundle offer. It lands where you last clicked on the canvas, or in the room nearest there. The menu also duplicates the node selected (⌘D), deletes it from the state shown on or from the whole deck, and locks it (⇧⌘L) so the canvas passes over it. On the canvas, Delete and Shift+Delete do the same as the menu's two deletes, and Escape selects what holds the node. The lock beside each layer in the inspector locks or unlocks it.

With the canvas focused, Copy, Cut, and Paste (PLAN 3.12) copy, cut, and paste the node selected, in this deck or another, or in the browser's editor. Paste also takes what other apps copied. A screenshot or a picture copied in the Finder comes in as an image. A CSV comes in as a data source with a chart of it. Cells copied from Numbers or Excel come in as a data source and the table they were. Other words come in as a text. ⌥⌘C and ⌥⌘V copy one node's look and paste it on another.

Shift-click selects several nodes beside each other, and a drag from an empty spot draws a marquee (PLAN 3.13). Drag one of them to move them all. The inspector then shows what they share, and buttons to align, spread, order, and group them. ⌘G groups them, ⌘⇧G takes a group apart, and ⌘] and ⌘[ bring them forward or send them back.

A state's menu in the list down the side (PLAN 3.14) adds a step or a slide after it, renames it, moves it up or down, and removes it; drag a state to move it, and Add, under the list, adds after the state shown. Slides (⌥⌘L) shows every slide in place of the canvas: drag the slides selected onto another to move them, ⌥← and ⌥→ move them a place, ⌘D copies them, Delete takes them out, and Return or a double click shows a slide. Rehearse (⌥⌘R) plays the deck in the window as you would present it: → or a click goes on, ← goes back, and Escape stops. At the end it shows what each state took, and Keep makes those times the states' holds. Under the canvas, drag a motion's bar to change when it starts and its end to change how long it lasts; the plus beside Play adds one of the theme's motions to the node selected.

The inspector's column holds the panels (PLAN 3.15), chosen by the buttons above it. Theme puts the deck in another theme and edits this one's colors, type roles, and spacing, or gives it a photo's colors. Data shows a data source as a table: type in a cell and press Return, or add and remove rows. Files lists the bundle's images, fonts, and data, and takes out one that nothing names. Versions lists what the bundle's history keeps: choose one to see it and what changed since, and Restore to make it the deck again. ⌘Z undoes each.

The node selected has handles (PLAN 3.16). Drag the round handle above it to turn it, with Shift by 15°. On a line, an arrow, or a polygon, drag a point to move it, press the dot at an edge's middle to add one, and click a point and press Delete to take it away. Drag a rect's yellow corner handle to round its corners to the theme's steps. On an image, drag a bar inside a side to crop it from that side, and the ring to move its focal point. Option keeps each to the state shown. A node you turned still moves and resizes by its handles. ⌘' draws the theme's grid over the canvas.

Export (PLAN 3.8), in the toolbar, shares the deck as a PDF, the same bytes `scaena export --format pdf` writes, or the state shown as a PNG, through the Share sheet; shows the PDF in Quick Look; or saves either where you say.

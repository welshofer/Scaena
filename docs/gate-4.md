# Gate 4

Phase 4's exit criteria (PLAN, "Exit criteria (gate 4)"), each with its evidence so far. Every task of Phase 4 has landed (4.1–4.10). What the gate still needs is a real iPad. CI's iPad is a simulator on a virtual Mac: its GPU lacks what vello needs, so it paints with the CPU painter, and its timings are a virtual machine's.

**Where it stands:**
- **Criterion 1** (same pixels as the web player, on an iPad's GPU). The same frame is met on CI's simulator: the torture deck's display lists there are the goldens'. The same pixels on an iPad's GPU wait on an iPad, since vello never runs on the simulator.
- **Criterion 2** (first frame under 300 ms on an M-series iPad) waits on an iPad. `just ipad-gate UDID` reads it there, in release, with one command.
- **Criterion 3** (no text layout where a deck is drawn) is met, held by the test that holds gate 3's. The source pane's `UITextView` falls under gate 3's open call on TextKit in chrome.
- **Criterion 4** (each edit by touch the Mac's patch, and one step of undo) is met in part:
  - Each gesture makes the Mac's patch through the Mac's ScaenaKit functions, and the iPad's UI tests make each edit by touch on B1.
  - Undo is held step by step for keys and for the inspector's panels, but for no touch on the canvas.
  - The Pencil waits on an iPad.

## 1. Same bundle, same frame, same pixels (within tolerance) as the web player, painted on an iPad's GPU: the frame met, the pixels need an iPad

- **Same frame.** The iPad edits and draws through the session the Mac and the browser do (`scaena-session`, ADR-0021, ADR-0023), compiled for the iPad.
  - ScaenaKit's `theTortureDeckDrawsTheGoldensDisplayLists` opens the torture deck from its files, as the app does. Every state at rest digests to the goldens (`tests/golden/torture/raw.fnv1a`), as the web player's engine does (PLAN 0.8).
  - CI runs it on the simulator. Gate 4's own tests carry the deck and the digests (below), so an iPad runs the same test on its own engine, built in release.
  - On CI's simulator, all 60 of the torture deck's states at rest draw the goldens' display lists (`gate 4, criterion 1: 60 of the torture deck's 60 states …`).
- **Same pixels.** The iPad paints with vello on Metal, as the Mac does (`LayerPainter`).
  - On the simulator, Metal is Apple's second GPU family alone. It has no indirect dispatch, which vello asks for. So a layer there is painted by the CPU painter and shown through the same blit (ADR-0004 finding 23).
  - CI's `aSurfacePaintsOnMetalWhatTheCPUPainterPaints` therefore checks the blit on the simulator, not vello. There the layer's pixels are the CPU painter's own, 0.000 of a level off on average.
  - On an iPad, the same test asks for vello. It holds B1's cover on a layer to the CPU painter's pixels: a channel under one level off on average (SPEC §13.5). It prints how far off.
  - The simulator caught one thing an iPad would have refused. Metal on iPadOS passes 15 variables between a shader's stages, not 16, so a painter now asks the adapter for what it holds (ADR-0004 finding 22).
  - The web player's frames, by WebGPU and by the CPU painter, are held to the same goldens (`just web-smoke`). The clients meet there.
- **Still to do.** The run on an iPad (below): the tests on its GPU, and the torture deck in the app beside the web player, by eye.

## 2. Open → first frame < 300 ms for a 40-state deck on an M-series iPad: needs an iPad

`GateTests`' `aFortyStateDeckShowsItsFirstFrameSoonAfterOpening` times B1 (40 states, SPEC §15) as on the Mac (`docs/gate-3.md`):
- the GPU made, as the app makes it when it starts;
- the bundle's files read and the session opened, and its timeline;
- the Metal surface made;
- the first state's first frame presented on a layer.

On the iPad it prints `gate 4, criterion 2: …`, and where the time went.

- **An iPad reaches none of the Mac's files, and runs no test bundle without an app.** So gate 4's tests are a target of their own, `ScaenaGateTests`, hosted by an app that does nothing (`GateHost`), so that the tests time the engine alone. Its bundle carries B1, the torture deck, and the goldens' digests (`apps/ipad/project.yml`). CI runs the same target on the simulator, reading the same copies.
- **CI's readings are not the gate's.** CI's `ipad` job runs the test on the simulator under three conditions that are not the gate's:
  - the engine is the debug library;
  - the CPU painter paints;
  - the machine is a virtual Mac.

  It holds the test to five seconds, so that a gross regression shows. The job runs the gate's own command too (`apps/ipad/gate.sh`, below), so that the command keeps working, and prints the simulator's readings.
- **The simulator's reading**, from the first run of `gate.sh` on CI: B1 open to first frame in 26 ms, painted by the CPU painter.

  | | open → first frame | files and session | timeline | surface | first frame | a second surface |
  |---|---|---|---|---|---|---|
  | CI's iPad simulator, debug | 26.4 ms | 5.7 ms | 3.1 ms | 0.4 ms | 17.2 ms | 12.3 ms |

  The GPU was already made by the tests before it, so its time reads 0.0 ms. That is not the gate's reading.
- **The gate's reading** is the release build, on an M-series iPad joined to a Mac with Xcode and XcodeGen:
  1. Put a team that signs for the iPad in `apps/ipad/Team.xcconfig`, kept out of the repository: `DEVELOPMENT_TEAM = <your team's ID>`.
  2. Turn on the iPad's Developer Mode, unlock it, and find its UDID: `xcrun xctrace list devices`.
  3. Run `just ipad-gate UDID` (`apps/ipad/gate.sh UDID`). It builds the engine and the tests for the iPad in release, and runs `ScaenaGateTests` there: this criterion's test and criterion 1's two. It prints their readings.

  The line `gate 4, criterion 2: B1 (40 states), open to first frame: …` is the reading, and the line after it says where the time went. Run it three times, and log the median.

## 3. Neither SwiftUI nor UIKit lays out text where a deck is drawn; all geometry comes from the engine: met

- **What draws a deck is the engine's**, on the iPad as on the Mac:
  - the canvas, the stage, and the presenter's view: vello on Metal through `ScaenaView`'s `CAMetalLayer`, or the CPU painter on the simulator;
  - the states drawn small and the next state: the CPU painter's pixels;
  - each outline, handle, and landing a drag shows: the engine's boxes;
  - a caret and a selection in a text typed in on the canvas: the engine's carets.

  `CanvasKeys` answers `UITextInput` from those carets, and lays out and draws nothing (PLAN 4.4). ScaenaKit's `theIPadsKeysAndAnInputMethodTypeThroughUITextInput` holds the caret it reports to the engine's caret box.

  SwiftUI draws chrome: lists, the inspector, the findings, the assistant's conversation.
- **Held by gate 3's test.** `crates/scaena-ffi/tests/chrome.rs` reads every Swift file of ScaenaKit and of the app, their iPad code included, and the iPad's own (`apps/ipad`). It fails on any API that lays text out, measures it, or draws it outside the engine: Core Text, TextKit, `UITextView`, `UITextField`, `UILabel`, and string measuring and drawing.
- **The source pane** is a `UITextView` of the deck's `.scn`. That is TextKit in chrome, the iPad's side of Jay's open call on gate 3 (SPEC §9.3 against invariant 8, `docs/gate-3.md`). The test allows it in `SourcePane.swift` alone, as it allows the Mac's `NSTextView`.

## 4. Each edit by touch or Pencil is the patch the Mac's pointer makes for it, and one step of undo: met in part

- **The Mac's patch.** A touch is the Mac's gesture, never a new edit (ADR-0023). The canvas's press, drag, handles, typing, and menu go through the ScaenaKit functions the Mac's pointer goes through: `Targets.snap`, `Typing`, `CanvasOffer`, and the session's calls. Only the events that come in differ. ScaenaKit's tests of those functions run on the iPad simulator as on the Mac.
- **By touch, on B1.** The iPad's `TouchTests` make each edit by touch:
  - a tap selects the title;
  - a pinch zooms, and another takes the whole slide back;
  - a double tap types in the title;
  - a drag moves it;
  - a long press offers its menu, whose Duplicate copies it;
  - in the light table, a slide dragged onto another moves after it.
- **One step of undo**, held for keys and the inspector's panels:
  - `KeysTests`: eight nudges by the arrows, each taken back by one ⌘Z, and ⌘D's copy taken back by one ⌘Z;
  - `ThemeTests`: a theme value typed, and a stepper's tap, each one step (PLAN 3.28);
  - ScaenaKit's tests: a drag's patch keeps the source it replaced, for the window's undo, and a burst of typing is one step.
- **Not yet held: undo after a touch on the canvas.** No test undoes a drag or a long press's Duplicate. Without a keyboard, someone on an iPad undoes by iPadOS's three-finger swipe or the menu bar's Edit menu. The window's toolbar has no Undo, as Keynote's on the iPad has.
  - *Proposed:* Undo and Redo in the iPad's toolbar, which `TouchTests` then tap after each edit on the canvas.
- **The Pencil.** Its hover outlines what a press would take, read from the boxes the pointer's hover reads (PLAN 4.5). On the simulator, a pointer stands in for it. The Pencil itself waits on an iPad, as do Scribble, the software keyboard, and dictation (PLAN 4.4).

## The run on an iPad

About half an hour, with an M-series iPad on iPadOS 26 and a Mac with Xcode and XcodeGen.
1. Run `just ipad-gate UDID` three times. Log criterion 2's median, and criterion 1's lines.
2. Run `just ipad`, and run `ScaenaApp` on the iPad from Xcode. Copy `tests/fixtures/torture.scaena` and `tests/bench/b1.scaena` to the iPad's Files, in On My iPad › Scaena.
3. Open the torture deck, and the same deck in the web player on the Mac beside it (`just web-dev`). Look at a few states side by side: one with text, `hanging`; one with a chart, `chart`; one with a shader, `mesh`.
4. On B1, by touch:
   - Tap the title, and drag it.
   - Double-tap it, and type with the software keyboard, then with dictation, then write with the Pencil (Scribble).
   - Long-press it, and choose Duplicate.
   - Pinch to zoom.
   - In the light table, drag a slide.

   After each, undo with a three-finger swipe to the left, iPadOS's undo, and note whether the edit goes back in one step.
5. With the Pencil, hover over the canvas: what a press would take is outlined.
6. With a keyboard:
   - Tab through the canvas, move a node with the arrows, and undo with ⌘Z.
   - In the inspector's Document tab, type a theme value, press Return, and undo with ⌘Z.
7. Play B1 with an external display or AirPlay: the stage goes there, and the presenter's view stays on the iPad. Join from an iPhone's Remote with the code the presenter shows. Ask the assistant something with a key from its sheet.

What the run finds is logged here and in PLAN's gate log.

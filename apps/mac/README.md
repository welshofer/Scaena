# apps/mac — SwiftUI client (Phase 3)

SwiftUI owns chrome only (document browser, state list, timeline scrubber, inspector, source pane). All geometry comes from the Rust engine through `crates/scaena-ffi` (C ABI via cbindgen); painting is `vello` on Metal through a `CAMetalLayer`-backed `wgpu` surface. **No TextKit/CoreText in the render path.** Keys in Keychain; Apple Foundation Models for on-device assistant tasks. See SPEC §9.3 and PLAN §Phase 3. Phase 3 began on 2026-10-08 on Jay's call, with gate 2's criterion 1 still open. The engine reaches Swift through the session the browser edits (ADR-0021).

## ScaenaKit

`ScaenaKit/` is the Swift package over the engine's C ABI (`crates/scaena-ffi`, PLAN 3.1): `ScaenaSession` opens a bundle from a folder, its files, or a `.scaena` zip, and answers as the browser's editor does. To build and test it on a Mac, from the repository's root:

```
cargo build -p scaena-ffi
libs=$(cargo rustc -q --color never -p scaena-ffi --lib --crate-type staticlib -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
cd apps/mac/ScaenaKit && swift test -Xlinker -L"$PWD/../../../target/debug" $(for l in $libs; do printf -- '-Xlinker %s ' "$l"; done)
```

CI's macOS runner does the same on every pull request ready for review that touches the engine or the app.

## The app

`just mac` builds `Scaena.app` (`apps/mac/build-app.sh`) and opens it: New starts a deck from Dusk, and Open takes a `.scaena` bundle or zip, such as `tests/bench/b1.scaena`. Without Rust on the Mac, run the `mac-app` workflow (Actions → mac-app → Run workflow) and download its `Scaena.app` artifact; macOS asks once whether to open an app signed ad hoc (right-click → Open).

The window (PLAN 3.4): the states down the side, each drawn small; the state chosen on the canvas, a click selecting what draws there; its cue under it, to play and scrub; the findings lint makes, each with its fix; the deck's `.scn` beside the canvas (the Source toggle); and the inspector, which edits the node selected, or the state with none. Every edit is one step to undo, and what a save writes is the deck the source compiles to.

Play (⌥⌘P, PLAN 3.5) presents the deck from the state shown: on the external display if there is one, with the presenter's window (the state, the next, its notes, the time) here. The keys are the web player's; Escape ends.

The assistant (⌥⌘A, PLAN 3.6) is the browser's, with your own key: put an Anthropic, OpenAI, or Gemini key in Settings (⌘,), where it is kept in your Keychain, then choose a model in the panel, which lists the ones the key can use. A question begins with the state shown and the node selected, so "this" and "shorter" mean them. Each edit shows as it is made, and Undo takes it back. On a Mac with Apple Intelligence (macOS 26), the sparkle beside a finding explains it, and the inspector drafts a state's notes and tightens a text's words, answered on the Mac and offered to take or leave.

On the canvas (PLAN 3.7), drag a node to move it and a handle to resize it: it snaps to the theme's grid or into a slot, as in the browser, and Option keeps the change to the state shown. Shift takes a node off the grid, where it is outlined in orange, as lint flags it (W301), or puts one back on it. Each drag is one step to undo.

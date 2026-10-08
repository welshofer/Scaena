# apps/mac — SwiftUI client (Phase 3)

SwiftUI owns chrome only (document browser, state list, timeline scrubber, inspector, source pane). All geometry comes from the Rust engine through `crates/scaena-ffi` (C ABI via cbindgen); painting is `vello` on Metal through a `CAMetalLayer`-backed `wgpu` surface. **No TextKit/CoreText in the render path.** Keys in Keychain; Apple Foundation Models for on-device assistant tasks. See SPEC §9.3 and PLAN §Phase 3. Phase 3 began on 2026-10-08 on Jay's call, with gate 2's criterion 1 still open. The engine reaches Swift through the session the browser edits (ADR-0021).

## ScaenaKit

`ScaenaKit/` is the Swift package over the engine's C ABI (`crates/scaena-ffi`, PLAN 3.1): `ScaenaSession` opens a bundle from a folder, its files, or a `.scaena` zip, and answers as the browser's editor does. To build and test it on a Mac, from the repository's root:

```
cargo build -p scaena-ffi
libs=$(cargo rustc -q -p scaena-ffi --lib --crate-type staticlib -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
cd apps/mac/ScaenaKit && swift test -Xlinker -L"$PWD/../../../target/debug" $(for l in $libs; do printf -- '-Xlinker %s ' "$l"; done)
```

CI's macOS runner does the same on every pull request ready for review that touches the engine or the app.

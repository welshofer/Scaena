# apps/mac — SwiftUI client (Phase 3)

SwiftUI owns chrome only (document browser, state list, timeline scrubber, inspector, source pane). All geometry comes from the Rust engine through `crates/scaena-ffi` (C ABI via cbindgen); painting is `vello` on Metal through a `CAMetalLayer`-backed `wgpu` surface. **No TextKit/CoreText in the render path.** Keys in Keychain; Apple Foundation Models for on-device assistant tasks. See SPEC §9.3 and PLAN §Phase 3. Nothing here until gate 2 is logged.

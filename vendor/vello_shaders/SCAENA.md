# vello_shaders 0.10.0, patched for Scaena

This is vello_shaders 0.10.0 as crates.io publishes it, with one change, carried while it saves
the browser's editor module its size under SPEC §15's 3.0 MB. The workspace takes it through
`[patch.crates-io]` and leaves it out of its members, so Scaena's lints, formatting, and tests do
not run over it. Its `Cargo.lock`, its `Cargo.toml.orig`, and its `.cargo_vcs_info.json` are left
out.

**The WGSL it embeds has nothing a compiler reads taken out.** Its build script preprocesses each
shader (its imports inlined, so each carries the shared modules' licence headers and comments
again) and writes the text into `SHADERS`, which vello hands WebGPU, or wgpu's naga natively, to
compile. Here `build.rs`'s `minified` takes out each comment, line and block, each line's
indentation and trailing space, and the blank lines, before the text is written: WGSL has no
strings, so a comment starts wherever `//` or `/*` stands, and no token is parted by anything but
whitespace, which a newline still gives. The build parses each shader as written and as minified
with naga and holds the two modules alike whole, all but their spans (`spanless`: each module's
`Debug`, its spans said as `Span`), so a shader that reads otherwise, a token merged or a value
changed, fails the build. The MSL a `msl` build writes is translated from the
shader as written, as before.

The shaders compile to what they did, so every frame vello paints is as it was. The 23 shaders'
text is 235,751 bytes, from about 406,000, and the editor's module is 2,959,512 bytes gzipped
with it, from 2,996,232: 36.7 KB less.

On a vello bump: vendor vello_shaders again at the version vello names, carry `minified` and
`spanless` over into its `build.rs`, and build; the check fails a shader that reads otherwise
(ADR-0004 finding 29).

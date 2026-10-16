# skrifa 0.44.0, patched for Scaena

This is skrifa 0.44.0 as crates.io publishes it, with one change, carried while it saves the
browser's editor module its size (PLAN 2.146). The workspace takes it through `[patch.crates-io]`
and leaves it out of its members, so Scaena's lints, formatting, and tests do not run over it. Its
`Cargo.lock`, its `Cargo.toml.orig`, and its scripts are left out.

**Every pen draws through one instance of the scalers.** `OutlineGlyph::draw` is generic over its
pen, and each scaler under it (glyf, CFF and its charstring evaluator, the hinters, VARC) is generic
over it too, so a build compiles all of them once for each pen it passes. The browser's editor
module passes six: vello's path encoder and its two glyph caches' outlines, vello_cpu's (glifo's),
VARC's own, and the engine's `NoPen`. Here `draw` wraps its pen in `DynPen`, a pen behind a `&mut
dyn OutlinePen`, and calls `draw_dyn`, the body it had, so each scaler is compiled once
(`src/outline/mod.rs`). Each pen receives the same calls in the same order with the same values, so
every outline, and every frame, is as it was. The pen is called through a pointer for each segment:
in V8, B1's states paint on the CPU in 13.0 ms, from 12.4, the best of three runs each, within the
noise of the machine they ran on.

The editor's module is 2,595,567 bytes gzipped with it, from 2,674,394: 78.8 KB less.

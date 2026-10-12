# skera 0.7.0, patched for Scaena

This is skera 0.7.0 as crates.io publishes it, with one change, carried until skera makes it
upstream (ADR-0004 finding 24). skera 0.8.0 has the same line. The workspace takes it through
`[patch.crates-io]` and leaves it out of its members, so Scaena's lints, formatting, and tests do
not run over it. The `cli` binary, the tests, and the benches are left out: the published crate
carries none of the fonts its tests read.

**The repacker weighs a 32-bit offset's child the same on every target.** When a subset table's
16-bit offsets overflow, the repacker orders the table's objects by their distance from its root.
Each step's weight is the child's size plus `1 << (8 × the offset's width in bytes)`, times the
child's space plus one, which keeps what a 32-bit offset points at last. skera computed it in
`usize`. On wasm32 a `usize` is 32 bits wide, and a shift by 32 wraps to a shift by none, so a
32-bit offset's child weighed 1 instead of 2³². The browser's subsetter ordered the table
otherwise than the CLI's, and gave up where the CLI resolved it: a download of a bundle set in the
shipped Inter, Source Serif 4, or Roboto Serif failed at GPOS ("Subsetting table 'GPOS' failed").
hb-repacker weighs it in `int64_t`; so does this, in `u64` (`Graph::update_distances`, in
`src/graph.rs`).

On a 64-bit target nothing changes: every font in the repository and each script font a deck
fetches (PLAN G.22) subsets to the bytes it did. On wasm32 each now subsets to the native bytes.

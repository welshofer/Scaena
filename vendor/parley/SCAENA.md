# parley 0.11.1, patched for Scaena

This is parley 0.11.1 as crates.io publishes it, with two changes, carried until parley makes them
upstream (ADR-0004 findings 21 and 25). The workspace takes it through `[patch.crates-io]` and leaves it
out of its members, so Scaena's lints, formatting, and tests do not run over it.

**A change of spacing no longer splits a run of text into runs shaped apart.** parley shaped a
run of text in one style of spacing at a time: where letter or word spacing changed, it ended
one shaping item and began another, so the font's kerning, ligatures, and contextual forms
stopped at the change. Tracking on one letter, to close up a pair, cut the pair's kerning
instead of adding to it. Now an item ends only where the font, its size, its features, its
variations, its locale, the script, or the direction change, as before, and not where the
spacing does; each cluster takes the spacing its own style asks for (`LayoutData::finish`).

- `shape/mod.rs`: an `Item` no longer carries spacing, and a change of it breaks no run.
- `layout/data.rs`: a `RunData` no longer carries spacing; `finish` spaces each cluster by its
  style's `letter_spacing`, and each space by its `word_spacing` too.
- `layout/mod.rs`, `resolve/mod.rs`: the layout's `Style` keeps the two spacings it resolved.

Text whose spacing does not change within a run of one font lays out exactly as before.

**A no-break space no longer breaks a line it overflows** (finding 25). parley hung an
overflowing space or no-break space and broke the line after it, so a no-break space, UAX #14's
glue, held nothing together at a line's end. Now only a space hangs so; a no-break space
overflows as a letter does, and the line breaks at the last opportunity before it.

- `layout/line_break.rs`: the overflow case that hangs whitespace asks for `Whitespace::Space`.

Its manifest allows `deprecated`: parley 0.11.1 calls a method icu_properties 2.x deprecates, and as
a path dependency the warning would show in every build.

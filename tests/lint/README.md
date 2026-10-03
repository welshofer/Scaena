# Lint fixtures

One directory per rule, named by its code (SPEC §7.5), each with two decks:

- `trigger.deck.json`, which the rule finds something in, at the paths its test names;
- `clean.deck.json`, the nearest deck it finds nothing in. Most clean decks lint fully clean.

Each deck's `_comment` says what it shows.

**Two harnesses.**

- `E102`–`E106` are validation. `crates/scaena-core/tests/validate.rs` checks them against `theme.json` here, in a bundle that holds the files the clean decks name.
- Every other code is checked end to end by `crates/scaena-cli/tests/lint.rs`, as `scaena --json lint` runs it: validation, the document rules, then the layout rules with contrast painted by the CPU painter. Each deck is copied into a scratch bundle made of `bundle/` plus two of the torture deck's fonts, `RobotoSerif-VF.ttf` and `EBGaramond-VF.ttf`. That avoids a second copy of the font files.

`bundle/theme.json` sets its limits low so small decks reach them:
- density: 30 words a state;
- motion: three nodes at once, a 1.5 s build;
- body text: a 40-character measure and two words on a last line;
- display text: two lines at most.

Its charts set their text in the `chart` role, 24 cu: the smallest W312 lets pass.

`bundle/data/sales.csv` and `bundle/assets/photo.png` are what the decks that need data or an image name.

The torture deck's findings are its cases', pinned in `tests/golden/lint/torture.txt`.

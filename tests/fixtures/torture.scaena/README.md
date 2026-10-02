# Typography torture deck (PLAN 0.2, benchmark B4)

Phase 0 is adversarial: this bundle exists to make `parley`/`harfrust` and the painters fail now, not in month five. Each case is its own state, so the parity harness (PLAN 0.9) reports per case.

```
torture.scaena/
  deck.json        28 states, mode: absolute (but `chart` and `chart-next`, which build on `chart-intro`), layout: specimen
  theme.json       "Torture": every case-specific setting is a role, so the deck references roles only
  data/bars.csv    six quarters for the bar chart; bars-next.csv, the window a quarter later
  fonts/           five subset fonts + OFL texts; provenance in fonts/SOURCES.md
  assets/          test-card.png, 480 × 240, made by scripts/build_torture_images.py
```

## Fonts

| Family key | Font | Covers |
|---|---|---|
| `serif` | Roboto Serif (variable: `wdth` 50–150, `opsz` 8–144, `wght` 100–900, `GRAD`) | PLAN role: variable `wght`+`opsz`+`wdth`. Also `liga`, `kern`, `tnum`/`pnum`/`lnum`/`onum`, combining marks. |
| `garamond` | EB Garamond (variable `wght` 400–800) | PLAN role: discretionary ligatures (`dlig`: Th st ct ch ck ft tt). Also Greek, so it is the first fallback for Roboto Serif. |
| `hebrew` | Noto Sans Hebrew | PLAN role: fallback for a script the first two lack. |
| `arabic` | Noto Sans Arabic | Catalogue only: the Arabic bidi line. |
| `emoji` | Noto Color Emoji, COLRv1 only (`SVG ` table stripped) | Catalogue only: emoji. |

Fallback order (theme `type.families.*.fallback`): `serif` → `garamond` → `hebrew` → `arabic` → `emoji`. All five carry OS/2 v4 cap-height and x-height (ADR-0004 finding 3). Fonts are subsets built by `just torture-fonts` from google/fonts at a pinned commit, sha256-verified, deterministic; `just schema` checks that every character of every text node resolves through its role's chain and that each case uses only the fonts it is meant to test.

## Cases

Class: **kill** = gate 0 criterion 1 (bit-identical display lists macOS/Linux, rasters within tolerance across painters). **catalogue** = recorded with pass/fail and a note, not gating. **recorded** = listed in PLAN 0.2 without a class; reported like catalogue (the morph is gate 0 criterion 3; mesh is gated by PLAN 0.11).

| # | State | Case | Class | Pass when |
|---|---|---|---|---|
| 01 | `axes` | Variable axes | kill | wght 100/900, wdth 50/150, opsz 8/144 each change glyph shapes at fixed size (gate 0 criterion 2). |
| 02 | `liga` | Standard ligatures | kill | ffi ffl fj fl ligate; the `liga: false` control line does not. |
| 03 | `dlig` | Discretionary ligatures | catalogue | Th st ct tt ch ck ft ligate with `dlig`; the control line does not. |
| 04 | `kern` | Kerning-sensitive pairs | kill | AV To LT Wa Yo close up; the `kern: false` control line does not. |
| 05 | `numerals` | Numeral styles | kill | Tabular lines: 1111 = 0000 in width. Oldstyle lines: 3 4 5 7 9 descend. |
| 06 | `accents` | Accented Latin | kill | Every precomposed letter from Roboto Serif; no fallback, no `.notdef`. |
| 07 | `combining` | Combining marks (NFD) | catalogue | Same look as precomposed; stacked marks (ế ệ) do not collide; q̃ x́ positioned by GPOS. |
| 08 | `punctuation` | Quotes, dashes, ellipsis | recorded | Curly quotes, em/en dashes, ellipsis from the font. The opening “ hangs (SPEC §3.5). |
| 09 | `mixed` | Mixed weights and sizes, one line | kill | Five runs (100/400/900 at 56, then 112 and 28) on one baseline. |
| 10 | `tracking` | Tracking −0.08 em / +0.40 em | kill | Spacing applied per glyph without breaking shaping. |
| 11 | `hanging` | Hanging quotes, optical margins | kill (quotes; added on review after PLAN 0.9) · recorded (optical margins) | Every quote that opens a line hangs outside the text edge (the “ of line 1, the ‘ of line 3), with the next letter on the edge. Line 1 fits only because its quote is not measured (985 cu inside 998; 1009 with it). Recorded: T V W are not yet optically aligned, and line-end commas and periods do not hang (PLAN 1.8). |
| 12 | `balance` | Balanced two-line headline | kill | Two lines of similar length. Greedy gives 1642 + 521 cu at this width. |
| 13 | `pretty` | Pretty paragraph, bait widow | kill | Last line has ≥ 2 words. Greedy strands "mistake." alone, even with ±15 cu of measurement drift. |
| 14 | `wrap-longword` | One unbreakable word | recorded | Overflow is reported (E100) or handled by a declared policy; nothing escapes its box silently. |
| 15 | `wrap-url` | A URL | recorded | Breaks only at `/ ? & #`-style opportunities. |
| 16 | `wrap-narrow` | Narrow measure (268 cu) | recorded | Raggedness and any overflow recorded. |
| 17 | `emoji` | Emoji: COLRv1, ZWJ, skin tone, flag, VS16 | catalogue | One glyph per sequence, in color. Trap: EB Garamond maps the regional indicators, so per-codepoint fallback draws 🇺🇸 as two monochrome letters; fallback must be per cluster and honour default emoji presentation (UTS #51). |
| 18 | `bidi-hebrew` | Hebrew with inline Latin | catalogue | RTL paragraph; "Scaena" and "0.1" stay LTR; period at the left end. |
| 19 | `bidi-arabic` | Arabic with inline Latin | catalogue | Joined letterforms, lam-alef ligature, LTR islands. |
| 20 | `fallback` | Fallback within one run | kill | One run, three fonts: α ω from EB Garamond, Hebrew from Noto Sans Hebrew; no system font. |
| 21 | `anchors` | Cap, baseline, x-height alignment across 112 / 64 / 28 cu | recorded (added with PLAN 0.5) | Cap tops share the cell top (210 cu), baselines the cell bottom (642 cu), x-height tops of `axe` 780 cu. One line per specimen; the rows do not touch. |
| 22 | `chart-intro` → `chart` → `chart-next` | Bar chart: values in, then the next quarter | recorded (gate 0 criterion 3) | At rest, six bars keyed by label in the first categorical color, square on a hairline baseline and rounded at the top; value labels above, quarter labels below, tabular lining figures. Into `chart` (420 ms), the bars grow from the baseline and their labels count up. Into `chart-next`, the window scrolls a quarter by key: 2025-Q1 rides out under the left edge, 2026-Q3 rides in from the right, the rest slide and rescale. Each transition is its two states at rest exactly at t = 0 and 420 ms; the frames at 0.25 and 0.5 have goldens (`chart@0.25`, `chart-next@0.5`, …), and no frame lays anything out. |
| 23 | `mesh` | Mesh background | recorded (gate 0 criterion 4) | A seeded mesh (`torture` palette, 5 points, drift 0.12, softness 0.85, grain 0.035) fills the canvas under the case label, at 0.84 s on the global timeline (the two chart transitions before it). cpu, gpu, and web paint it within SPEC §13.5, and the CPU reference and the WGSL agree within one step (`shader_parity`). |
| 24 | `shapes` | Shapes | recorded (added with PLAN 1.7) | A rounded panel with a hairline, a pill, a ring, an arrow ending in its head, a rule and a dashed rule, a triangle, a star fitted from SVG path data, and an oval, each in its grid cell in theme colors. cpu, gpu, and web draw them within SPEC §13.5. |
| 25 | `images` | Images | recorded (added with PLAN 1.7) | The test card seven ways. Top row: `cover` keeps the middle bands and the disc; `cover` with `focal: [0, 0.5]` keeps the left bands and the yellow corner mark; `contain` shows all four corner marks, centered; `fill` with `crop: [0.25, 0, 0.5, 1]` stretches the middle four bands. Bottom row: `cover` with `focal: [0.5, 1]` and `radius.4` keeps the bottom, its half-transparent strip blending with the page (straight alpha, premultiplied once), corners rounded; `contain` in a narrow box rounds the drawn image's corners, not the box's; `crop: [0, 0, 0.125, 0.125]` magnifies the 2 px checkerboard about ten times, bilinear. cpu, gpu, and web draw them within SPEC §13.5. |
| 26 | `containers` | Containers | recorded (added with PLAN 1.7) | Rows 2–4: a row stack of three cards sharing its width, each a padded, rounded column stack whose figure and caption sit at its foot (`distribute: end`). Row 5: a row stack whose 72 cu figure and 40 cu label share one baseline on the row's bottom edge (`align: { y: baseline }`), and at its right end a group of a ring and a dot at 0.6 opacity. Rows 6–8: a grid container whose photo spans two rows of its `fraction(2)` column, its note and its 48 cu centered dot in the named areas beside it; and a framed card (fill, hairline, radius) whose photo fills its padding, with a pill and its label placed by `rect`. Every box comes from `taffy`, laid out once for the state; cpu, gpu, and web draw it within SPEC §13.5. |

Each state's `notes` repeats its pass condition in full, so a rendered PNG and its notes are enough to judge it.

## How the baits were made

`balance` and `pretty` only test anything if greedy breaking actually fails on them. Both strings were chosen by shaping with HarfBuzz 14.5 (uharfbuzz) on these exact fonts, axes, and tracking, then breaking greedily at the slot width (1728 cu, columns 1–12) and at ±8 and ±15 cu around it. Every offset reproduces the failure, so small width differences between HarfBuzz and `harfrust`/`parley` cannot quietly defuse a kill case.

## Not yet

- No `manifest.json`, and fonts use readable names rather than `fonts/<family>-<hash>` (SPEC §3.1): bundle I/O arrives in PLAN 1.4. The example deck follows the same convention.
- The deck has no spine; W401 does not apply.

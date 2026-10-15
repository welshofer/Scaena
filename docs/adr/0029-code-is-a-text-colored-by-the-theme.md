# ADR-0029: Code is a text, colored by its language in the theme's colors

**Status:** proposed · **Date:** 2026-10-15

## Context

Jay asked for code on slides on 2026-10-14: "User can paste source code into a text box and we detect (or user selects) language and we format and color code the code - great for tech conferences." A speaker at a conference needs all of these:
- the code in a monospace face with its indentation, at a size the back row reads;
- colored by its language, which is detected or chosen, and shown so it can be changed;
- on a card that shows on a projector;
- line numbers, on or off;
- lines in focus with the rest set back, changing from step to step;
- the code changing between steps token by token, as Keynote's Magic Move does in code talks;
- the block fitted to its box by growing and then shrinking, never by wrapping;
- all of it readable in the PDF handout and to a screen reader, in the browser and on the Mac and the iPad, through the same Rust.

Line numbers below are at 1d531e5. These facts frame the choice.

**A text is most of a code block already.** What these work on is a text: typing in place, carets, find and replace, the morph word by word, the reader, the PDF's tags, Insert, and the clipboard. `Session::carets` is `None` for a node that is no text (`crates/scaena-session/src/lib.rs:1178-1183`). Every shipped theme has a `mono` family, JetBrains Mono VF with its italic (`docs/examples/themes/dusk.theme.json:42`), and a `code` role (`:61`), so Insert offers "Text · code" already (`crates/scaena-core/src/inserts.rs:150-157`). SPEC §3.3 defers a `code` node type (`docs/SPEC.md:171`). A node's type cannot change across states (E104), so prose that became code would have to become a new node.

**The text engine keeps spaces, knows no tab, hangs quotes, and takes its direction from the text.**
- parley's ranged builder keeps spaces as written (`crates/scaena-engine/src/text.rs:588`).
- Nothing sets a tab stop, and JetBrains Mono maps no U+0009, so a tab draws `.notdef`: E120 (`crates/scaena-engine/src/lint/text.rs:248`).
- A quotation mark that opens a line hangs, in every role (`text.rs:620`, `docs/SPEC.md:299`).
- A layout runs in its first strong character's direction (`rtl = layout.is_rtl()`, `text.rs:619`).
- `measure` breaks the `code` role's lines at 72 characters.

**Color splits runs, not shaping.** The vendored parley shapes a run of one font whole across a change of color (ADR-0004 finding 21), and `read_layout` makes one `GlyphRun` per style (`crates/scaena-engine/src/sample.rs:2637-2647`). So every painter, the PDF, the SVG, and video draw a color per token with no new op. But parley joins neighbors of equal style (`vendor/parley/src/resolve/range.rs:138-154`), and a morph keeps a word's drawing only where its runs match one to one (`same_glyphs`, `sample.rs:1470-1483`); otherwise the two drawings cross-fade (`sample.rs:1455-1460`).

**The look has to carry in a room.** The canvas is 2 units to the point (`docs/SPEC.md:1859`), so the shipped `code` role, 26 cu (24 in Ember), is 13 pt, under body's 16 pt (32 cu, `dusk.theme.json:50`). W312 holds chart text to 12 pt, 24 cu (`docs/SPEC.md:1360`). Insert's box for a text is twice the role's line and at least a twelfth of the canvas: 90 cu for code (`inserts.rs:150-156`). By WCAG's formula:
- `surface-2` stands 1.06–1.11:1 off `surface` in all four shipped themes, and Dusk's `line` 1.34:1. A card in `surface-2` vanishes on a projector.
- A card a visible 1.3:1 off Dusk's slide (#29292E) takes its accent to 5.09:1 and its muted to 4.24:1. A palette has to be designed with its card.
- Colors designed for reading leave no room to dim by one color. On `paper-2`, Dusk's muted reads 5.23:1 against its accent's 6.27:1, Daybreak's 4.86 against 4.72, and Ember's 9.29 against 4.96, so dimming a line to muted brightens Ember's keywords.

**Lint and the editor read `fit` as written; layout does not.** Layout takes a text's own `fit`, else its slot's, else `shrink` for a shape's words, else `wrap` (`crates/scaena-engine/src/render.rs:686-694`). Lint's `fit()` (`lint/text.rs:23-25`) and `Engine::grown` (`render.rs:356`) read only the node's own, defaulting to `wrap`. So a slot's shrinking text that overflows at its floor is E100 today, not W203.

**Edits write where a value lives.** `replace_text` writes where the text lives (`crates/scaena-core/src/tracking.rs:94-125`), and `fork` writes the state's own delta, which the states after it track (`crates/scaena-core/src/patch/ops.rs:918-926`). `add_state` inserts and rewrites nothing after it (`ops.rs:192-210`). `tracking::keep_looks` (`tracking.rs:383`) keeps later states looking as they did, as the light table's moves do.

**What the design must keep.**
- A deck names roles and theme colors; a literal outside `overrides` is W300 (ADR-0005).
- Nothing render-derived enters the CRDT (SPEC §8.3). Layout runs once per snapshot; frames only sample.
- `scaena-core` takes no heavy dependency.
- CI holds the editor's module to 3,000,000 bytes gzipped (`.github/workflows/ci.yml:286-303`). At the head of PR #345 (main and the inserts batch), CI measured it on 2026-10-14 at 2,849,788 bytes gzipped, 8,480,387 raw: 150,212 bytes of room. The player's module, which a single-file export carries, was 2,209,576 bytes gzipped, 6,019,100 raw.
- `just check`'s random edits open revenue, charts, and B1 alone (`crates/scaena-session/tests/edits.rs:75-108`).

**An unmerged neighbor.** The gap branch (`origin/gap/builds-connectors-cmyk`, PLAN 2.127–2.129) takes deck format 0.27 and theme format 0.17. A click is a state (SPEC §2.2), so its By Bullet makes a step per paragraph through a text's `paragraphs`: a `Range` counted from 1, kept in step with typing by `lists::redrawn`, and morphed by `Track::Paragraphs`.

## Decision

1. **A code text is a text with `code`.**
   - The property is `code: { language, numbers?, start?, card?, focus? }`, added over deck formats 0.28 to 0.30.
   - `language` is required: a lowercase name, resolved through `scaena_core::code`'s table of names and aliases.
     - A name it knows (`rust`, `rs`, `c++`) colors by that language's scanner.
     - `plain` colors nothing.
     - Any other name colors by the generic scanner (strings, numbers, comments, brackets), and lint I403 says so.
     - So a language added later changes how code draws, a reviewed golden diff, never a format change.
   - A code text's words are its `text`. Beside `code`, these are E106: `runs`, `list`, `fit: "wrap"`, and the properties code does not use (`wrap`, `measure`, `hyphenate`, `hangingPunctuation`, `opticalMargins`, `minLastLineWords`, `style.case`).
   - `style.color` colors its plain tokens.
   - `code` lives where any property does and merges one level deep, so a state's delta can say `{"code": {"focus": [[3, 5]]}}`.
2. **The engine colors code at layout, from `text` and `language`.**
   - `scaena_core::code::highlight` is written by hand from tables, with no dependency and no regular expression.
   - It classifies by ASCII bytes and is linear and total. Interpolations and embedded languages nest at most 8 deep. What never closes runs to the end in its class; a string that cannot run past its line ends at the line's end.
   - It colors a block's first 1,000 lines; the rest draws plain.
   - Its tokens are of 16 kinds, which are the theme's to color: keyword, type, function, string, number, constant, comment, operator, punctuation, property, attribute, tag, variable, meta, inserted, and deleted. The deck stores no token and no color.
3. **The theme colors code, and every color reads where it is drawn.**
   - `code.syntax` gives each kind a color, a weight, and an italic. The color is a theme color's name or a literal, which a theme may hold.
   - A kind the theme names no color for is derived from the accent by `scaena_core::palette::syntax`.
   - At layout, every color code is drawn in is held to 4.5:1 on the ground it sits on, by the least step of lightness. The ground is its card's fill where that is one color (the node's own `fill` over the theme's), else the surface.
   - Shipped and derived palettes aim at 7:1 for keywords and plain text.
   - Syntax colors are not in `tokens.color`, so they stay out of every color picker. A theme from a photo holds them to 4.5:1, as it holds a role's text colors.
4. **Code grows, then shrinks, and never wraps.**
   - Its lines break only where it breaks them. Tabs stand to the theme's stops, nothing hangs, and every line runs left to right. There is no case, hyphen, or measure.
   - Typed or pasted, its box grows down first, as a presentation app's text box does. Then it shrinks, worked out from one layout.
   - The shrink stops at its own `minSize`, else its role's, else 24 cu at presentation size: 12 pt, W312's floor. W205 flags code drawn smaller.
   - Layout, lint, `Engine::grown`, and the inspector read one effective fit: the node's own, else its slot's, else `shrink` for a shape's words and for code, else `wrap`.
   - The shipped `code` role takes the body role's size (32 cu; Ember's 30), with `minSize: 24`.
5. **The card shows, and it hugs the code.**
   - The card wraps the laid-out code and its gutter at the theme's inset, and the node's `align` places it in the box.
   - The shipped cards stand at least 1.3:1 off the slide, with the theme's `soft` shadow. A theme that names no card gets `surface-2` stepped away from `surface` to 1.3:1.
   - Line numbers stand in a gutter that no caret, word, or reader takes.
6. **A walkthrough is states.**
   - `code.focus` names lines as `paragraphs` names paragraphs: ranges counted from 1, kept in step by typing. `start` only labels the gutter.
   - Lines out of focus mix each token's own color toward the ground by the theme's `code.focus.dim` (0.55), keeping hue, weight, and italic. They never go under 3:1, and lint holds them to it; SPEC §7.5 gains the exception.
   - The light themes' `band` marks the lines in focus.
   - A change of focus changes only color, so the morph glides it: `same_glyphs` compares glyph by glyph across run boundaries.
   - Walk Through Lines… makes a step for each range, starting and ending on the whole block.
7. **Magic Move is the morph, done by token.**
   - Between two states, a code text pairs its tokens, not UAX #14's words. Comments and strings pair word by word.
   - A token both states share moves to where it now stands; a token that was edited fades on its own.
   - Line numbers pair by number, and the card's box moves.
   - A code text set by `shrink` keeps one size across every text the deck gives it, so a step that adds a line does not rescale every token.
   - The code steps the editors make run at `slow` (800 ms in the shipped themes).
8. **Every code command leaves the deck valid and the states after it as they were.**
   - Make Code folds `runs` into `text` and takes away what E106 refuses beside `code`.
   - Not Code takes `code` away from the node and from every delta.
   - Walk Through, Next Code Step, and Paste as Next Step keep later states through `tracking::keep_looks`.
9. **Detection and tidying belong to the editor**, at a paste and at a paste into a code text, never at layout.
   - `scaena_core::code::detect` reads fences, shebangs, file names, `text/html` classes, and VS Code's `vscode-editor-data` `mode`.
   - Then it scores signals in integers, ties going by the table's order. Only a named language, or a strictly likely one, turns a paste into code.
   - `tidy`:
     - makes every line end a line feed, and U+00A0 a space;
     - takes a fence off, and turns tabs into spaces;
     - drops trailing spaces;
     - takes off the indentation the lines share, reading a first line cut at its first character as such;
     - reindents JSON only where it is minified.
   - The language found is written into the deck and shown on the block as a chip, one click to change.
10. **A code text reads as code, and the handout keeps every version.**
    - The live region and a single file get `<pre><code data-language>`, and the Mac's canvas a `code` part.
    - The PDF gets a `Code` element placed as a block, with the gutter and the card as artifacts.
    - The PDF prints each slide at its last state and, before it, each state whose code words change in the next. Export… offers each step.

## Calls taken pending Jay

Six questions are Jay's. The work goes on with the default each states, and an answer that differs changes only the item named.

1. **What "format" means.** Typesetting plus tidying: line ends made one, tabs turned into spaces, fences and the indentation the lines share taken off, non-breaking spaces turned into spaces, and minified JSON reindented. Never a language's own formatter (rustfmt, black, prettier): each is megabytes, its output changes from version to version, and it rewrites the speaker's code. *(Decision 9; PLAN 2.133.)*
2. **Ligatures.** Off in the shipped code role (`features: {calt: false}`), so `->` and `!=` draw as typed. A block's `features` may turn them on. *(PLAN 2.137.)*
3. **Lines out of focus.** Held at 3:1, a stated exception to WCAG's 4.5:1 for text a walkthrough sets back on purpose, which SPEC §7.5 will name. Holding them to 4.5:1 barely dims them, and a kind already at 4.5:1 could not dim at all. *(Decision 6; PLAN 2.140.)*
4. **The light themes' card.** Light: a step darker than the slide (Daybreak #DFDDD5, Gallery #DFDFDB, each 1.3:1 off it), its palette designed with it. A dark card, an editor's dark theme on a light slide, is the other way. *(PLAN 2.137.)*
5. **Code in other scripts.** Code keeps ADR-0027's exclusion from a deck's script fallbacks for now, so a Japanese comment or an emoji in a string draws as a box (E120), and the paste names the line that draws one. Falling back would cost those lines their column alignment and amend ADR-0027. *(PLAN 2.133.)*
6. **The languages on day one**, in the order the plan lands them: the C family and JSON first (PLAN 2.130); then Python, Ruby, Shell and Console, PowerShell, and SQL (2.134); then data, config, and markup (2.135); then the embedded languages (2.145). Every other name colors by the generic scanner until its scanner lands.

## Consequences

- **+** A code block is one object to type in, find in, select, move, build, walk through, and Magic Move, on every client, through the session they share. The Mac's and the iPad's Swift gain no layout and no highlighter.
- **+** It is semantic. A deck says `rust`, and a theme says how Rust looks. A re-theme or a theme from a photo recolors every block, and contrast holds by construction wherever code sits on one color.
- **+** Every painter, every export, and the remote draw it with no new display-list op.
- **−** The highlighter is the engine's, so a change to it changes how code draws. Each code case's golden changes and is reviewed. A token read wrongly colors wrongly until the scanner is fixed, but it never fails.
- **−** The highlighter is estimated at 40–80 KB gzipped, in the editor's module and in the player's, which every single-file export carries. The editor's module had 150,212 bytes of room when 2.130 measured it: just over the 150 KB below which the language tables would be handed over from the start, and about 70 KB once the estimate's top is spent. So 2.132 measures what the highlighter costs, and where it would leave less than 100 KB, the language tables are handed to the engine the first time a code text lays out, as the hyphenation patterns are (ADR-0015), before 2.132 lands.
- **−** A color the theme names may be drawn a step lighter or darker where its ground needs it, so the Theme tab shows the color as drawn.
- **−** Lines out of focus are held to 3:1, not 4.5:1: a deliberate exception to WCAG's ratio for body text, for text set back on purpose. A kind at 4.5:1 dims only 1.5×.
- **−** Nothing wraps. A block too big for its box at 12 pt overflows (W203), and the finding suggests fewer lines a step.
- **−** It takes three deck formats and three theme formats. Each deck bump rewrites every deck that names the format (100 files); each theme bump rewrites the 14 themes.

## Alternatives

- **A `code` node type** (the one SPEC §3.3 defers). Every path a text takes would need a twin: carets, typing, find, the morph, reading, the inspector, the DSL, and the Mac's `Words.kind`. And prose could not become code without becoming a new node (E104).
- **Colors written as runs at paste.** That puts what is drawn into the deck and its history. The runs go stale at every keystroke, and a re-theme leaves them behind.
- **syntect or tree-sitter.** Each brings a regular-expression engine or a C runtime to every target, at 0.7 MB to several MB gzipped against the module's 3 MB. tree-sitter also needs clang for wasm32, which CI does not install (`.github/workflows/ci.yml:252-269`).
- **A JavaScript highlighter in the page** (Shiki, or the editor's Lezer grammars). Only the browser would color code; the CLI, the PDF, the Mac, and video would not match it.
- **A closed list of languages in the schema.** Each new language would be a format bump, and every language not on the list would draw gray.
- **Syntax colors in `tokens.color`.** That puts sixteen more names in every color picker and every contrast fix.
- **Dimming to one muted color.** It passes lint, but it barely dims Dusk and Daybreak, and it brightens Ember's keywords.
- **Dimming by opacity with no floor**, as Slidev dims to 0.3. The spotlight is stronger, but nothing lint could hold.
- **A card that fills the node's box.** Six lines float at the top of an empty panel.
- **Bringing the shipped code look into every older copy of a theme.** Only the editors run `library()`, so the CLI's PDF and the editor's PDF of one bundle would differ.
- **A paste dialog or sheet.** A SwiftUI `TextEditor` keeps the system's smart quotes and dashes, and a dialog duplicates the canvas, which is the code editor already.
- **Reformatting with each language's formatter.** Each is megabytes, its output depends on its version, and it changes the speaker's code.

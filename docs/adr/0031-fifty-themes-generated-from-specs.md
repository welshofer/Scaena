# ADR-0031: Fifty themes, generated from specs and shipped in batches

**Status:** proposed · **Date:** 2026-10-15

## Context

On 2026-10-15 Jay asked for fifty themes, set in many more typefaces than the shipped themes use, and for an analysis of the decks a wide range of people make and of the themes that would serve them. Jay also holds the layout library to at least 75 slide types.

The analysis was made the same day:
- **The research pass** read 433 pages on 164 sites: who makes decks, how often, of what kinds, and what each kind needs.
- **The review pass** opened every claim it could, measured every font file a theme would carry, recomputed every color's contrast, and held the design against the repository at faf96d7.

Both are in "Themes for every deck", a claude.ai document shared with Jay (https://claude.ai/artifact/Jweqy5x8qFA1uY6NEzzSH6). A gallery beside it shows a sample slide in each of the fifty (https://claude.ai/artifact/TkyTznTgPbGfGA7yaQVVL2). This ADR keeps what the work needs, so a session without the document can carry it out.

**What the analysis found.**
- **Who makes decks.** People make decks for strategy and sales, boards and investors, all-hands and weekly status, classrooms, lecture halls and clinics, courts, councils and the military, launches and keynotes, listings and portfolios, worship, weddings and memorials.
  - Business, pitch, and report decks lead every template catalog read.
  - Education is the largest category in both big free catalogs: Slidesgo's 9,806 templates and SlidesCarnival's 1,140, read 2026-10-15.
- **Eleven kinds of deck.** Pitch, Report, Plan, Briefing, Update, Keynote, Lecture, Workshop, Research talk, Portfolio, and Ceremony.
- **Nine things a theme decides.** Tone (formal to playful), energy (calm to bold), density (one idea a slide to read as a document), light or dark, the type's personality, color (neutral with one accent to many hues), imagery (type-led to photo-led), decoration (bare to textured), and motion (still to choreographed).
  - On the first two, half the families sit formal and calm, and none sits playful and calm.
- **Twelve families of themes** serve those decks. Each family is a section of the theme chooser, never a theme's name.

**What this design stands on**, in the repository at 6e9ef33:
- **Four themes ship:** Dusk, Daybreak, Ember, and Gallery (`scaena_ops::shipped::THEMES`, from `docs/examples/themes/` and `docs/examples/authorability/themes/daybreak.theme.json`).
  - They share one vocabulary of layouts, slots, roles, and presets on one 12 × 12 grid (SPEC §3.6, PLAN 1.34, 2.102), so a deck moves between them with no E102.
  - `web/src/themes.ts`, the Mac's New from a Prompt (`PromptWindow.swift`), and the docs name the four.
  - The MCP server's `theme_apply` takes a theme file's path, not a name.
- **Their fonts** are eight files, 2,017,048 bytes (1,216,522 gzip-9): Fraunces, Inter, JetBrains Mono, and Instrument Serif, each upright and italic.
  - `scripts/build_bundle_fonts.py` subsets each from google/fonts at commit 9710da1, held by sha256.
  - They are compiled into the CLI, the MCP server, and the C ABI's library, and carried beside the web pages.
- **The layout library** is 128 layouts in 13 sections, which `scripts/build_layouts.py` writes into each shipped theme (theme formats 0.11–0.15).
  - `scaena_ops::theme::library` brings it into a bundle's older copy of a shipped theme.
- **Tests that name the four:**
  - `crates/scaena-ops/src/shipped.rs` demands an italic file for every family, and holds the four's motion presets and springs equal (`every_theme_that_ships_offers_the_same_builds`).
  - `crates/scaena-cli/tests/theme.rs` holds one vocabulary (`the_shipped_themes_name_the_same_things`) and moves every example deck between the four (`every_example_deck_moves_between_the_shipped_themes`).
  - `starting.rs` lints a slide started in every layout of every shipped theme: 512 starts.
- **Fonts and scripts.** ADR-0027 gives a deck in another script its font, fetched by a pinned URL and sha256. Lint E120 finds a character no face draws, W230 a font whose license forbids embedding it, W231 a family without its italic, and W232 a weight a family cannot draw.
- **The MANIFESTO** says: "We would rather ship four exceptional themes than four hundred mediocre ones." ADR-0005 says Scaena ships "a small number of excellent ones".
- **Formats.** The theme format is 0.17. Code on a slide takes theme formats 0.18–0.20 (PLAN 2.131–2.140), and ADR-0030's objects 0.21–0.24.

**What must hold** (CLAUDE.md's invariants):
- A deck names roles, slots, tokens, and presets, never a theme's values (ADR-0005). Those names are what a change of theme keeps, so every theme names the same ones.
- No system fonts in the render path. Every font a deck uses is a file in its bundle (SPEC §13).
- The editor's module stays under SPEC §15's 3,000,000 bytes gzipped. It was 2,849,788 bytes on 2026-10-14, so themes and fonts stay data, never compiled into it.
- Render never fetches: a theme's fonts are fetched before a deck is made or re-themed in it, never while drawing.

## Decision

Proposed. Every part below that rests on one of Jay's twelve calls names it, and the calls are under Calls for Jay.

### 1. Fifty themes, each held to the four's bar

Scaena ships fifty themes: the four and 46 more. Each is generated from a short spec, and each passes, before it lands, every check the four pass (decision 8). A theme that misses one does not ship, and no check is lowered to let it.

The MANIFESTO's sentence and ADR-0005's are amended to say so (call 1): what the MANIFESTO guards against is the mediocre theme, and generation with these checks is how fifty stay exceptional. A theme is a design system, as ADR-0005 has it, and every one of the fifty carries the whole layout library.

### 2. Twelve families, fifty themes

| Family | What it is | Themes |
| --- | --- | --- |
| Executive | Corporate neutral: a quiet sans or serif-led voice, one accent, cards and charts that survive projectors and brand kits. | Daybreak, Harbor, Meridian, Keystone, Equator |
| Analyst | Dense and exact: action titles, tabular figures, grays with one signal color; built to be read as well as presented. | Ember, Ledger, Quant, Sterling |
| Civic | Public service and access: plain type set large, AAA contrast where asked, briefs that put the bottom line first. | Garrison, Council, Docket, Beacon, Signpost |
| Launch | Startup bold: big geometric type, saturated color or gradients, numbers as heroes. | Voltage, Liftoff, Sorbet, Hanji |
| Stage | Cinematic dark: one point a slide, photographs edge to edge, purposeful motion for big rooms. | Dusk, Noir, Marquee, Nocturne |
| Editorial | Type-led magazine and Swiss pages, the serif revival over grotesks, and the script-native editions. | Gallery, Broadsheet, Basel, Washi, Qalam |
| Scholar | Academic and clinical: figure-first, Greek in the text faces, references and intervals first-class; Lyceum and Seminar are one light-and-dark pair in Literata. | Lyceum, Margin, Remedy, Seminar |
| Classroom | Friendly and legible for learners of every age, with a dyslexia-friendly cream theme. | Recess, Kite, Sunbeam, Haven |
| Lens | Photo-led: quiet type framing pictures for portfolios, listings, travel and family slideshows; Viewfinder's slate and Darkroom's brass leave the color to the photographs. | Darkroom, Viewfinder, Postcard, Threshold |
| Ceremony | Worship, weddings and memorials: reverent, legible at a distance, slow fades. | Vespers, Hymnal, Vow, Elegy |
| Craft | Tactile and human: grain, paper, slab and rounded type, warm palettes for community work. | Kindred, Fieldwork, Halftone, Saffron |
| Systems | Code and tech: mono titles, code cards legible at the back of a hall, light and dark. | Kernel, Lambda, Runtime |

The fifty are listed at the end, under The fifty, with their colors, faces, and signatures. 36 are light and 14 dark. In every new theme:
- the ink reads at 7:1 or more on both backgrounds, and the quieter text and the accent at 4.5:1 or more;
- Beacon and Signpost, the high-contrast pair, hold every text color to 7:1;
- lines read at 1.3:1 or more on the page, and cards sit 1.2:1 or more off it;
- 19 second accents are for large text and marks only, at 3:1 or more (WCAG 1.4.11). With Daybreak's and Gallery's, 21 of the fifty are.

### 3. Generated, never drawn by hand

- **The generator.** `scripts/build_themes.py`, beside `build_layouts.py`, writes every key of a theme file but its layouts and furniture, which `build_layouts.py` keeps writing. Its `--check` runs in `just schema`, so in `just check`, and as its own step in CI's `schema` job.
- **The first gate.** Before any new theme lands, the generator reproduces the four's files byte for byte. Dusk's two copies are first rewritten in the form the other three files take, every value kept: their hand-aligned columns are not a form a generator writes.
- **A spec** holds only what differs from a shared base: the name and description, the mode, the seven colors, the families with their italics, axes, features and fallback, a type voice with each role's deltas, geometry and margin, shader palettes, chart and table deltas, the data palettes (`tokens.data`), any stroke weight unlike the base's (Ember's thickest is 5), and any shader preset's parameters unlike the base's (Gallery's backdrop). Later it holds the code card and syntax colors (theme formats 0.18–0.20). Outside its layouts, Dusk's file is 9.4–9.9 KB of compact JSON today, and about 5.1 KB without what the shared base holds.
- **The shared base** is motion (25 presets, 4 springs, 4 easings, 3 durations), shadows, density, space, radius, stroke, the color roles, and the shader presets' and palettes' names. It writes no list defaults, since none of the four sets `type.lists`, and furniture is `build_layouts.py`'s.
- **Colors** are worked in Oklab with `scaena_core::palette`'s math. The generator refuses a new theme's spec that misses one of these bars:
  - the ink at 7:1, and every other text color at 4.5:1 (7:1 in Beacon and Signpost), on paper and on paper-2;
  - the paper on the accent at 4.5:1;
  - the second accent at 3:1 on either background (WCAG 1.4.11);
  - the line at 1.3:1 on paper, and paper-2 at 1.2:1 off it.

  The four are held to their files as they ship until call 10's separate change: their cards sit 1.06–1.11:1 off the page, and Gallery's line 1.29:1.
- **Type comes from three voices:** a variable serif display (Dusk's), tighter sizes (Ember's), and a display of one weight (Gallery's).
  - Display measures are scaled by the face's own width (PLAN 2.102).
  - Snapped leadings fall on whole 8-unit lines (W221), and chart roles are 24 units or more (W312).
  - Every weight a role asks for is one its family draws (W232). So no body face is static, since a static family draws one weight and would set every bold in it.
- **Layouts are keyed by geometry family,** not by theme name: `build_layouts.py`'s `THEMES`, `OLD_SLOTS_IN`, and `ember_slots()` name themes today. Most new themes take Dusk's geometry, and a new geometry is linted whole.
- **A knob a signature needs** that the format lacks is added library-wide, for every theme, before the batch that needs it: a picture's frame or mat (Darkroom, Viewfinder, Postcard, Vow), before batch 3.
- **Motion needs no knob** (call 9). Every theme file already carries its own motion. If each theme takes its own values, the builds test holds the presets' names, not their values.

### 4. One registry, names that are words

- **The registry.** One registry lists each theme: its slug, its name, its family, its geometry family, its fonts' keys, and its file or files. Its description, mode, colors, and type voice are its spec's: the generator copies them into the registry for the clients that list themes, and its `--check` holds the copy. These read the registry, not four literals:
  - `scaena_ops::shipped`'s table and `web/src/themes.ts`;
  - the MCP tools' descriptions and the Mac's and the iPad's theme lists;
  - `scripts/validate_schema.py`, the docs, and every test that names the four.
- **By name everywhere.** `theme --apply` and the MCP server's `theme_apply` take a shipped theme by name as well as a file, as `scaena new --theme`, `scaena generate --theme`, and `deck_create` already do.
- **Names.** A theme's name is one word, equal to its slug ignoring case, since `shipped::theme` and the C ABI's `library()` match names that way (call 4).
  - No name is a theme's name in PowerPoint, Keynote, Google Slides, Gamma, or Pitch, as read on 2026-10-15.
  - Gallery is one: a PowerPoint theme of Office 2013 and 2016. It is renamed Salon: its file becomes `salon.theme.json`, named "Salon". `gallery` stays an alias in `shipped::theme`, `web/src/themes.ts`, and `scaena_ops::theme::library`, which takes a copy named Gallery as Salon's, so every deck and bundle that names it opens as before.
  - Executive, Civic, Editorial, and Craft are also theme names there, and they stay family names here, section labels only.

### 5. Fonts

**46 families, 42 of them new.** Every one is under the SIL Open Font License 1.1, read in its own `OFL.txt` at google/fonts 9710da1 (Pretendard, in its own `LICENSE`). Every one of the 94 files read may be embedded (fsType 0, W230). The files the fifty carry, each family's upright and italic, are listed at the end, under The fonts:

| What | Bytes | gzip-9 |
| --- | ---: | ---: |
| The 46 families | 80,684,328 | 42,856,320 |
| Without Noto Sans JP, Noto Serif JP, and Pretendard | 50,780,740 | 25,958,300 |
| The 42 new families | 77,605,444 | 41,098,136 |

Measured on 2026-10-15, before subsetting. A saved deck carries only the subset of each font it uses, as today.

- **Where the files live** (call 2). The four's eight files stay in git. `build_bundle_fonts.py`'s pinned table grows to every family: its google/fonts path at 9710da1, the upstream file's sha256, its license file's sha256, its italic, and what it covers.
  - Pretendard comes from orioncactus/pretendard at its v1.3.9 tag, the one family google/fonts does not hold.
  - The build fetches the rest by sha256 into a cache CI keeps.
  - Its `--check` also compares each `SOURCES.md` with the files beside it, which it does not do today.
- **What each client carries** (call 3):

  | Client | Carries | Fetches when first needed |
  | --- | --- | --- |
  | CLI and MCP server | Every theme's JSON, about 8.2 MB raw and 1.24 MB gzip-9 for fifty, and the four's fonts | Any other theme's fonts, by a pinned URL, sha256, and size, kept by digest in its cache. Offline, the four, Basel, and Kernel work, and any other theme is refused with why |
  | Browser | A thumbnail of each theme, drawn at build by the CPU painter, and the four's fonts in the service worker's install | A theme's JSON and its own fonts when New, New from a Prompt, or the theme picker needs them, held to sha256 in Cache Storage |
  | Mac | The four's fonts in the static library; `scaena_create` and `retheme` hand over only the chosen theme's fonts, not all eight | Other fonts once, kept by digest in Application Support, as ADR-0027's path keeps a script's |
  | iPad | The Mac's library | The rest into the app's container, kept by digest. A first deck in a theme not yet fetched is refused offline, with why |

- **What a theme's faces lack** is drawn by Inter, which every client embeds (call 7). A fallback names a family of the theme, not a file, so Inter and its italic become a family of every theme whose text families lack it, under the key `inter`. That key is named last in every text family's fallback, before ADR-0027's script family. The four need none, since their body is Inter, and the vocabulary test leaves the key out of the families it compares. Read from each face's character map, Inter covers:
  - → in the 8 themes where no face draws it (Quant, Garrison, Remedy, Kite, Sunbeam, Threshold, Vespers, Lambda);
  - Greek in 6 bodies (Beacon, Haven, Recess, Kite, Qalam, Sunbeam), and Cyrillic in 2 (Beacon, Haven);
  - Washi's Latin Extended-A, of which its Japanese faces draw 3 of 19 letters sampled.
- **Reserved Font Names** (call 5). A bundle keeps its fonts as files beside the deck, so a saved subset is a Modified Version (OFL FAQ 1.11, 1.12, 2.6). A Modified Version may not use a Reserved Font Name as its primary name (OFL 1.1, condition 3).
  - Pretendard reserves its own name and three more: "Source", "Inter", and "M PLUS 1" (its `LICENSE`). A subset renamed under call 5's (c) avoids all four.
  - Noto Sans JP, and ADR-0027's Noto Sans SC and KR, reserve "Source", which their names do not use, so their subsets are fine.
  - The bench bundles' Source Serif 4 subset (`tests/bench/b1.scaena` to `b3.scaena`, made by `build_bundle_fonts.py`) keeps "Source Serif 4" as its name, so it uses its reserved name today.
  - The review dropped three families for their reserved names: Carlito, Merriweather Sans, and Source Serif 4. So batches 1 to 4 wait on no reserved name.
- **Italics** (call 6). Noto Serif JP, Noto Sans JP, and Pretendard have no italic files. `shipped.rs`'s rule that every family ships an italic would keep Washi and Hanji out.
- **What a theme's first use may download** (call 12):
  - The six themes on the four's fonts download nothing: Dusk, Daybreak, Ember, Gallery, Basel, and Kernel. The other 44 download a median of 0.98 MB gzip-9, each family once, kept by digest, so a second theme in the same family costs nothing.
  - Latin themes run to 4.55 MB (Elegy). Equator, multi-script by design, downloads 5.46 MB; Washi 13.95 MB, Saffron 4.85 MB, and Hanji 2.95 MB.

### 6. Batches and their gates

The 46 land in five batches, after a batch of rails, each only when its checks pass. Within a batch, each theme may land alone.

| Batch | Themes | Why this order | Gate before it |
| --- | --- | --- | --- |
| 0. Rails | — | Registry, generator, font table and fetch path, thumbnails, tests that read the registry. No new theme yet. | Jay's calls 2 to 4 and 12, each before the rail it decides; T.0a waits on none |
| 1. Business core and access | Harbor, Ledger, Basel, Kernel, Meridian, Keystone, Voltage, Quant, Beacon, Signpost | Business, pitch and report decks lead every catalog; Basel and Kernel prove the path with no new font; the high-contrast pair sets the bar the later batches are judged against. | The generator reproduces the four byte for byte; Jay's calls 1, 7, and 9 |
| 2. Teach and research | Lyceum, Margin, Seminar, Remedy, Recess, Kite, Sunbeam, Haven, Equator, Council | Education is the largest category in both big free catalogs (Slidesgo 9,806; SlidesCarnival 1,140, read 2026-10-15). | The first 16 slide types land (call 11) |
| 3. Stage, story and pictures | Liftoff, Sorbet, Noir, Marquee, Nocturne, Broadsheet, Darkroom, Viewfinder, Threshold | Keynotes, launches, portfolios and listings; needs the picture-frame knob. Nocturne's Cormorant Garamond replaces a face with a reserved name, so nothing here waits on call 5. | A picture frame or mat in the theme format |
| 4. Civic, craft and ceremony | Garrison, Docket, Sterling, Kindred, Fieldwork, Halftone, Vespers, Hymnal, Vow, Elegy | Briefings, courts, boards, community, worship and life events. | — |
| 5. Scripts and the rest | Postcard, Lambda, Runtime, Saffron, Qalam, Washi, Hanji | Reserved Font Names, italics for Japanese and Korean, script faces, right-to-left layout, and the rest. | Jay's calls 5, 6, and 8 |

Batch 1 also waits on call 7: Inter, last in every fallback, draws the → Quant's faces lack and the Greek and Cyrillic Beacon's body lacks. Call 12 comes with the rails: its (b) changes what they pin and fetch, and its (c) would move Keystone's and Signpost's bodies to Inter. Before batch 5, three of its themes wait on calls:
- Hanji on Pretendard's reserved name and its italics (calls 5 and 6);
- Washi on its italics (call 6), with Inter drawing the Latin its faces lack (call 7);
- Qalam on right-to-left layout (call 8).

Saffron, Postcard, Lambda, and Runtime close the fifty with them.

### 7. Slide types

Every theme carries the whole library: 128 layouts in 13 sections today, against Jay's bar of 75. Layouts are what a change of theme keeps, so a deck keeps every slide in any of the fifty. Each addition is drawn once for each geometry family and linted in every theme.

The decks people make still lack 53 slide types. With them, the library is 181 layouts in 17 sections, and 186 with the five more ADR-0030 adds: Map and Words, Full-Bleed Map, Forecast, Price Chart, and Haiku. The four new sections are Briefing, Classroom, Code, and Diagrams. The 53 come in three batches:
- **First, 16**, before theme batch 2 (call 11), so each new theme is drawn and reviewed once.
- **Next, 23.**
- **Last, 14.** Most of these wait on something the format lacks.

A slot today holds words, a shape, a picture, a chart of a v1 kind, or a table of 2 to 6 columns. None holds a video, a web page, an equation, or a waterfall, Mekko, or word-cloud chart.

| Batch | Slide | Section | What it shows | Note |
| --- | --- | --- | --- | --- |
| 1 | KPI Dashboard | Numbers | Four measures, each with its figure, its change, and a trend under them | Scorecard's four measures with Two Charts' lines under them |
| 1 | Flowchart | Diagrams | Steps and a yes-or-no decision joined by arrows | A decision diamond is a polygon |
| 1 | Swimlane | Diagrams | Steps across lanes, one lane for each role |  |
| 1 | Positioning Map | Charts | Players placed by two measures, ours marked | A scatter where Two-by-Two Matrix places words in quarters |
| 1 | Feature Grid | Comparison | Features down the side, a check or a Harvey ball for each product or plan | A table's cells hold text, so a check or a ball is a character the theme's faces draw (✓ ● ○). No shipped face draws ◐, Inter included, so a half ball waits for a face that does |
| 1 | Risk Matrix | Frameworks | Risks on a likelihood-by-impact grid | A 5 × 5 grid of cards shaded by the theme's color roles toward the accent, since a slot names a color token or role, not a palette entry: Nine-Box Grid is the 3 × 3 unshaded one |
| 1 | Case Study | Editorial | A customer's challenge, what was done, and the results |  |
| 1 | Logo Wall | Pictures | Twelve customer or partner logos | Six Up's pictures are photographs under captions; a logo wall is twelve contained at one height |
| 1 | Device Mockup | Pictures | A screenshot in a laptop or phone frame | The frame is a polygon or a rect |
| 1 | Business Model Canvas | Frameworks | The nine blocks of a business model |  |
| 1 | Status Report | Numbers | Workstreams with status, next milestone and risks | A table's cells hold text, so each status icon is a slot beside its row |
| 1 | Quad Chart | Briefing | Picture and dates, team, objectives and milestones in four quarters | Variant of Quadrants, one quarter a picture: named for the people who search for it |
| 1 | Decision Brief | Briefing | The bottom line, then issue, options, recommendation and the decision sought |  |
| 1 | Quiz | Classroom | A question and four answers, the right one revealed by a build | The layout gives each of the four answers a card; the reveal is a build a person adds after the slide starts (PLAN 2.112), as Game Board's links are |
| 1 | Market Size | Numbers | Total, serviceable and obtainable market as nested circles | Nested circles, where Venn Diagram's overlap |
| 1 | Buyer Persona | People | A customer's portrait, role, goals and pains | Bio with goals and pains |
| 2 | Decision Tree | Diagrams | Choices and chances with their odds and outcomes |  |
| 2 | Fishbone | Diagrams | An effect and its causes along the bones |  |
| 2 | Architecture | Diagrams | Systems as labelled boxes in tiers, joined by arrows | Stack's layers rest on each other; tiers of boxes join by arrows |
| 2 | Issue Tree | Diagrams | A question split into sub-questions, left to right | Left to right, two levels, where Org Chart runs top down one level |
| 2 | Maturity Model | Frameworks | Levels as rising steps and where we stand | Steps run level; these rise |
| 2 | Pictograph | Numbers | A share shown as ten figures, some filled |  |
| 2 | Incident Review | Briefing | What happened, its impact, the root cause and the actions |  |
| 2 | Options Compared | Briefing | Courses of action scored against criteria, totals, and the one chosen | Variant of Table with a total row and the choice marked |
| 2 | Paper Title | Titles | Title, authors, affiliations, venue and logos |  |
| 2 | References | Editorial | Works cited in small type | In the caption role; past 40 words W210 warns, as for Fine Print |
| 2 | Acknowledgments | Closing | Funders and collaborators with their logos |  |
| 2 | Join or Scan | Closing | A QR code and the link to join, give or download | The code is a picture the user supplies; Scaena makes no QR code |
| 2 | Certificate | Editorial | Recipient, achievement, date and signatures inside a border |  |
| 2 | Game Board | Classroom | A grid of points, each linking to its question | Each card links to its state by a run's link (PLAN 2.70), added after the slide starts |
| 2 | Map | Pictures | A map with pins and their labels | Callouts point at a picture's parts; a map holds pins and labels beside it (ADR-0030's map object can fill it) |
| 2 | Lyrics | Statements | Two lines of a song, centered over a quiet backdrop, the credit small: Quote over Photo set for singing | Variant of Quote over Photo, centered, two lines, the credit small |
| 2 | Property | Pictures | Hero photograph, price, beds, baths and area | Number and Photo with three specifications and a second photograph |
| 2 | Patient Case | Editorial | Presentation, history, findings and an image |  |
| 2 | Month | Time | A month as a calendar with what falls on each day | Needs a format change: the table example holds 2 to 6 columns and a week has 7; else 7 labels and 35 rects |
| 2 | Title and Code | Code | Source in the theme's code card, key lines called out | PLAN 2.138's: a text in the code role; the card comes with PLAN 2.136 |
| 2 | Code beside Notes | Code | Code beside the words that explain it | PLAN 2.138's: a text in the code role beside words |
| 2 | Two Code Blocks | Code | Before and after in two panes | PLAN 2.138's: two texts in the code role |
| 2 | Event | Time | A date set large, what and where, and a picture: a save-the-date, an announcement, a webinar | No layout holds a date with its place and time; this review adds it |
| 3 | Waterfall | Charts | A start value stepping to an end value | Needs a chart kind, which SPEC §3.7 lacks |
| 3 | Mekko | Charts | Segment sizes as widths, shares as heights | Needs a chart kind |
| 3 | Forest Plot | Charts | Effects with intervals running across, by study | Needs range to run across: orient is bar's and stackedBar's alone (E106 on any other kind) |
| 3 | Video | Pictures | A video full slide or beside words | Needs the deferred video node (SPEC §1.3, §3.3) |
| 3 | Web Page | Pictures | A live web page on the slide | Needs the deferred embed node |
| 3 | Word Cloud | Statements | The audience's words sized by frequency | Needs a word-cloud drawing |
| 3 | Live Poll | Charts | A question and its votes as they come in | Needs live data from outside the deck (ADR-0030's kind of object) |
| 3 | Countdown | Numbers | Minutes left to the start or the end of a break | Needs a timer, or a run of one-second states |
| 3 | Seating Chart | Diagrams | Tables as circles with guests' names |  |
| 3 | Exhibit Callout | Pictures | A document page with its key passage enlarged | Callouts' notes point at one picture; this enlarges a passage of a page beside it |
| 3 | Concept of Operations | Briefing | Phases across, lines of effort down | Variant of RACI: phases across, lines of effort down, a card in each cell |
| 3 | Brand Board | Editorial | Palette swatches, type and logo use |  |
| 3 | Fine Print | Editorial | A safe-harbor or disclaimer page in small type | In the caption role; W210's 40-word density warns on it by design. Add Slide's search also finds it as Disclosures |
| 3 | Equation | Statements | One equation set large, its terms defined under it | Needs a math node: today an equation is a text, linear (E = mc²), with no fractions, roots or matrices; this review adds it |

Some slides people search for are already in the library under another name, so Add Slide's search finds them there: Kanban opens Roadmap; Learning Objectives opens Numbered List; Mind Map opens Hub and Spoke; Scripture opens Quote; In Memoriam opens Bio; Order of Service opens Agenda; Problem and Solution opens Comparison; Exit Ticket opens Big Question; Rubric opens Table; Thank You opens Closing; Lower Third Lyrics opens Lower Third. Disclosures will open Fine Print once the last batch adds it.

### 8. Checks every theme passes

- **The generators.** `build_themes.py --check` reproduces the four byte for byte; `build_layouts.py --check` and `build_bundle_fonts.py --check` check theirs.
- **The schema.** `just schema` validates every theme at the current theme format.
- **One vocabulary.** `the_shipped_themes_name_the_same_things` runs over the registry, not four literals.
- **Every example deck re-themes onto it:** revenue, charts, trails, higher-ed, ridgeline, code, and authorability, 86 states. There is no refusal and no E102, and no contrast error is left after recoloring.
- **Every layout starts clean.** Every layout of every theme starts a slide that lint finds nothing in, with its examples drawn (`starting.rs`).
- **Fonts.** Each font passes these checks:
  - Every file resolves by sha256 and may be embedded (W230).
  - Every family has an italic (W231) unless call 6 exempts it.
  - Every role's weight is drawable (W232), so no body is static.
  - Every role's stack but code's, with Inter last, draws `narrative::writable`'s letters for its ten languages written in Latin or Cyrillic letters, English to Ukrainian; its other five take ADR-0027's script family. It also draws Latin Extended-A, ș ț, Greek, and − → ≈ ≤ ≥ ₂ ° ± × ✓ ● ○ (E120).
- **Contrast.** The matrix of each theme's role colors on paper and paper-2, paper on the accent, furniture, words on cards, and the second accent at 3:1. Then E110 and E111 on the started layouts and the example decks.
- **Baselines and motion.** W221 on snapped roles; the builds and shadows tests over every theme.
- **From a Photo,** on every theme, leaves every text color reading and adds no lint error.
- **Names.** Unique single words, each equal to its slug ignoring case, and no vendor's theme name.
- **Seen by a person.** A contact sheet of each theme's layout pictures is reviewed once. Each start's display-list digest is kept to catch drift after that, never blessed blindly.

### 9. Budgets

- **The editor's module** stays at or under 3,000,000 bytes gzip-9, the CI gate: themes and fonts stay data.
- **Embedded fonts** stay the four's 2,017,048 bytes unless Jay chooses a larger set that works offline (call 3).
- **Fifty embedded theme files** come to about 8.2 MB raw and 1.24 MB gzip-9. Each is 24.5–24.8 KB gzip-9, and its layouts are 126–129 KB of its 161–168 KB raw.
- **SPEC §15 holds with the heaviest Latin theme:** B1's cold start at most 500 ms, and MCP `deck_render` at most 1 s with its fonts cached.
- **Test time grows.** Slide starts go from 512 to 6,400 at 128 layouts, 9,050 at 181, and 9,300 at 186. The re-theme matrix goes from 28 to 350 CLI runs over its seven decks. They are sharded by theme, with the full matrix on main and a sample on each pull request.

## Calls for Jay

Each call gives its options and the recommendation. When each is needed:
- Calls 2 to 4 and 12 come before the rails they decide (PLAN T.0b–T.0d), and calls 1, 7, and 9 before batch 1.
- Call 11 comes before batch 2, and calls 5, 6, and 8 before batch 5. Call 10 can come any time.
- Call 5 also touches what ships today: the bench bundles' Source Serif 4 (PLAN T.0f).

The generator and the registry for the four (PLAN T.0a) wait on no call: they change no value in the four's files and nothing a person sees. The first slide types (PLAN S.1) wait on none either, though they add 16 layouts to the four, unless Jay takes call 11's (b), which puts every slide type after the fifty.

**1. Ship fifty? MANIFESTO 10 prefers four exceptional themes to four hundred mediocre ones, and ADR-0005 a small number of excellent ones.**
- (a) Amend both by an ADR: fifty, generated from specs, each held to the four's lint bar.
- (b) Ship twelve, one per family, and fetch the other 38.
- (c) Stay at four.
- *Recommended:* (a), in batches, each landing only when its checks pass.

**2. Fonts in git, or fetched at build?**
- (a) Commit every theme's subsets: the files the themes carry are 80.7 MB (42.9 MB gzip-9) before subsetting, 29.9 MB of it Japanese and Korean.
- (b) Keep the four's 8 files (2,017,048 B) in git; fetch the rest at build from google/fonts at commit 9710da1 by sha256, into a cache CI keeps.
- (c) Git LFS.
- *Recommended:* (b): clones stay small, and every build is reproducible by sha256.

**3. What do the CLI, the MCP server, the Mac and the iPad carry, and what works offline?**
- (a) Every theme's JSON and the four's fonts, the rest fetched on first use; offline, the four, Basel and Kernel.
- (b) Everything embedded: tens of MB more in each binary and in the site's install.
- (c) Only the four; the rest by name over the network.
- *Recommended:* (a); an embedded core of twelve only if users ask.

**4. Names: one word equal to its slug, or free names? Gallery is also a PowerPoint theme's name (Office 2013 and 2016); no other name of the fifty is a theme's name in Office, Keynote, Google Slides, Gamma or Pitch.**
- (a) One word equal to its slug, as the shipped table and the Mac's library match names ignoring case.
- (b) Free names with a slug field and one normalized comparison.
- *Recommended:* (a), as all 46 are; rename Gallery to Salon, keeping gallery as an alias so every deck that names it opens as before.

**5. Reserved Font Names. A bundle keeps its fonts as files beside the deck, so a saved subset is a Modified Version (OFL FAQ 1.11, 1.12, 2.6), which may not use a reserved name as its primary name (OFL 1.1, condition 3). Pretendard reserves its own name. ADR-0027's Noto Sans JP, SC and KR reserve "Source", which their names do not use, so their subsets are fine. The bench bundles' Source Serif 4 subset does use its reserved name.**
- (a) Choose families without one, as all but Noto Sans JP and Pretendard do.
- (b) Keep such families whole, never subset.
- (c) Rename the subsets in their name tables.
- *Recommended:* (b) now for Pretendard, and rename or drop the bench bundles' Source Serif 4 subset; whole files also admit IBM Plex, Source Sans 3, Lora, Playfair Display, Libre Baskerville, STIX Two, Bitter, Lato, Raleway and Andika later; have counsel say whether a bundle is a document or a distribution.

**6. Italics: shipped.rs demands an italic file for every family. Noto Serif JP, Noto Sans JP and Pretendard have none, nor do many display faces (Bebas Neue, Anton, Oswald, Fredoka, Pacifico and more).**
- (a) Keep the rule; Washi and Hanji wait.
- (b) Exempt a family whose script has no italic convention, W231 still warning.
- (c) Also exempt display families that no shipped role sets in italic.
- *Recommended:* (b) now; (c) before a later batch of louder display faces.

**7. What draws the characters a theme's own faces lack? In eight themes no face draws →; six bodies lack Greek and two Cyrillic; Washi's Japanese faces miss most of Latin Extended-A.**
- (a) The embedded Inter, named last in every text family's fallback, before ADR-0027's script family.
- (b) Per-script face slots in the theme format, ADR-0027's Noto faces by default.
- (c) Leave every script to ADR-0027.
- *Recommended:* (a) now: no download, and Greek, Cyrillic, Latin Extended-A and the symbols draw in every theme; per-script slots for Japanese, Chinese, Korean, Arabic and Devanagari later.

**8. Right-to-left layout, for Qalam and any Arabic or Hebrew deck.**
- (a) A mirrored geometry family.
- (b) Mirror at layout time when the deck's language runs right to left, in every theme.
- (c) Ship Qalam left to right with right-aligned roles.
- *Recommended:* (b), leaving photographs, logos and digits unmirrored, as Apple's guidelines do.

**9. Motion and density per theme?**
- (a) Keep motion identical and density at 40 everywhere.
- (b) Same names, different values within W320 and W321: Stage slower, Classroom bouncier, Analyst cutting; density from 25 (Stage) to 70 (Analyst). The test that holds the four's motion equal would hold the names instead.
- *Recommended:* (b); the names stay what a change of theme keeps.

**10. Hold cards to 1.2:1 off the page, and chart colors to 3:1 (WCAG 1.4.11)?**
- (a) New themes only; all 46 pass.
- (b) Retrofit the four too: their cards sit 1.06–1.11:1 off the page, Gallery's lines 1.29:1 (to #DEDED6, 1.32:1), its fourth chart gray 1.92:1 on the page and its third 2.91:1 on cards.
- (c) No guard.
- *Recommended:* (a) now; the four in a separate reviewed change, since it moves every existing deck's look.

**11. Slide types before or after the themes?**
- (a) The first 16 before batch 2, so each theme is drawn and reviewed once.
- (b) After all fifty.
- *Recommended:* (a): each new layout is drawn for each geometry and linted in every theme.

**12. What may a theme's first use download? Measured: the six themes on the four's fonts download nothing, and the other 44 a median of 0.98 MB gzip-9.**
- (a) At most 3 MB gzip-9 for a Latin theme: Lyceum (3.86 MB), Elegy (4.55) and Fieldwork (3.11) exceed it through Noto Sans and take Open Sans as body (2.02, 2.71, 1.26); Equator and the script themes exempt.
- (b) Carry subsets, as call 2's (a) does, so a theme downloads what a save keeps (not measured).
- (c) Keep 1.2 MB: 17 themes exceed it, so most bodies move to Inter.
- *Recommended:* (a): each family downloads once and is kept by digest, so a second theme in the same family costs nothing.

## Consequences

- **The theme chooser grows from four to fifty,** sectioned by family. Every deck keeps every slide through a change of theme, since every theme carries the same layouts, slots, roles, and presets.
- **The four become rows of the registry.** Their files come out of the generator byte for byte, after Dusk's one rewrite that keeps every value, and no deck that names one draws differently. Gallery is called Salon, and `gallery` still opens it (call 4). The four's cards and Gallery's line and chart grays are not retrofitted to the new bars except by a separate, reviewed change (call 10).
- **A theme's fonts are fetched once on first use,** outside the six that need none, so choosing a theme or re-theming a deck can reach the network. The CLI, the Mac, and the iPad fetch them by ADR-0027's path, which already fetches a script's font. The MCP server, which makes no network call today, makes its first through the CLI's transport, as ADR-0030's `map_fetch` does, so `scaena-mcp` links no HTTP client. Offline, a theme not yet fetched is refused with why, never drawn in another face.
- **The repository does not grow by the fonts:** clones stay the size they are, and builds are reproducible by sha256.
- **CI grows** by the starts and the re-theme matrix, sharded, with the full matrix on main.
- **The bench bundles' Source Serif 4 subset is renamed or dropped** (call 5), whatever else is decided, since its name is reserved.
- **The MANIFESTO and ADR-0005 are amended** on call 1. The bar each theme is held to does not move.

## Alternatives

- **Stay at four.** The MANIFESTO's sentence, kept. The decks the analysis found (classrooms, courts, worship, weddings, clinics) would keep reaching for one of four looks made for strategy and keynotes.
- **Twelve shipped, one for each family, and 38 in a library fetched by name.** Fewer to keep, but a family of one theme leaves most segments with one look, and the checks are the same work for twelve or fifty.
- **Theme files drawn by hand.** Fifty files of 161–168 KB each, kept to one vocabulary and one shared base by hand, would drift from one another with every change to the library.
- **Every font in git, or in Git LFS.** 80.7 MB in every clone, or a second store to keep; fetching by sha256 gives the same reproducibility.
- **Every font embedded in every client.** Tens of MB more in each binary and in the site's install, for themes most people never open.

## Sources

The document's Sources lists all 166 the passes read. These decide the design:

| Source | What it gave |
| --- | --- |
| https://openfontlicense.org/ofl-faq/ | Embedding in a document, whole or subset (1.11, 1.12); a font file of its own is distributed, and subsetting it is modification (2.6) |
| https://raw.githubusercontent.com/google/fonts/9710da1eacb3be272583c3224dcb70f9da6eadbb/ | Every family's `OFL.txt` and `METADATA.pb` at the commit the shipped fonts come from; each file measured, raw and gzip-9 |
| https://raw.githubusercontent.com/orioncactus/pretendard/main/ | Pretendard's `LICENSE` (OFL 1.1, four names reserved, its own among them), README, and variable file |
| https://www.w3.org/ | WCAG 2.1 and 2.2 Understanding: contrast (1.4.3, 1.4.6) and non-text contrast (1.4.11) |
| https://developer.apple.com/design/human-interface-guidelines/right-to-left | What mirrors right to left, and what does not: photographs, logos, and digits |
| https://www.indezine.com/products/powerpoint/learn/themes/which-themes-are-installed.html and https://notes.indezine.com/2018/05/gallery-powerpoint-theme-and-template.html | The themes installed with each Office version; Gallery among them |
| https://slidesgo.com/ and https://www.slidescarnival.com/tag/ | Template counts by category: education the largest in both |
| https://www.empowersuite.com/hubfs/Marketing/Downloads/PowerPoint%20Studie%202020/Englisch/The%20Ultimate%20Global%20PowerPoint%20Study%20-%20empower.pdf | Nielsen for empower, 2020: who makes decks, and that 56% start from templates and 39% from old decks |

The review's own measurements are its, not a source's: the fonts' bytes, the contrast ratios, what each face's character map lacks, and the downloads each theme's first use costs.

## The fifty

Batch S is the four as they ship today; call 10's (b) would move Gallery's line to `#DEDED6`. Colors are paper, paper-2, ink, muted, accent, accent-2, and line.

| Theme | Family | Batch | Mode | Titles / text / code | Colors | Signature |
| --- | --- | --- | --- | --- | --- | --- |
| Daybreak | Executive | S | Light | Fraunces / Inter / JetBrains Mono | `#FBFAF6` `#F1EEE6` `#16140F` `#6B675E` `#0F766E` `#C2410C` `#DAD5C9` | Dusk's light twin: warm paper, near-black ink and one deep teal, with Dusk's families, roles and layouts |
| Harbor | Executive | 1 | Light | Geist / Inter / JetBrains Mono | `#FFFFFF` `#E5E9EF` `#0F172A` `#4A5568` `#1D4ED8` `#B45309` `#CBD2DC` | Geist titles set tight and flush left over Inter, a navy kicker, cool-gray cards that still show on a projector |
| Meridian | Executive | 1 | Dark | Plus Jakarta Sans / Open Sans / Roboto Mono | `#0B1424` `#1A2740` `#EEF2F7` `#A3B1C6` `#F2B544` `#6CB4FF` `#2C3B57` | Gold figures: number and hero roles in gold Plus Jakarta Sans ExtraBold on deep navy, never on black |
| Keystone | Executive | 1 | Light | Montserrat / Roboto / Roboto Mono | `#FBFAF7` `#E8E5DC` `#1D1D1B` `#57554E` `#146C43` `#0E7490` `#D6D2C6` | Rounded cards at the largest radius step in warm gray, green icons, Montserrat titles in sentence case |
| Equator | Executive | 2 | Light | Noto Serif / Noto Sans / Cousine | `#FFFFFF` `#E5E8EE` `#1B1F24` `#50585F` `#3346A8` `#0A7E6E` `#CDD3DA` | One type system across scripts: Noto Serif over Noto Sans (Devanagari, Greek, Cyrillic), the family ADR-0027 already reaches for Japanese, Chinese, Korean and Arabic |
| Ember | Analyst | S | Dark | Fraunces / Inter / JetBrains Mono | `#0E0E12` `#1A1A1F` `#F6F4EF` `#BEBDC3` `#FF4100` `#FFB27A` `#3A3A40` | Serif claims and figures over a neutral sans; data in quiet grays with the orange spent on the point; its own geometry, kicker along the top |
| Ledger | Analyst | 1 | Light | Inter Tight / Inter / JetBrains Mono | `#FFFFFF` `#E6E9EE` `#101114` `#4D545E` `#0B57A4` `#C2410C` `#C8CED6` | Action titles: a full sentence at text weight in Inter Tight, source line bottom left and page number bottom right on every slide |
| Quant | Analyst | 1 | Light | Barlow Condensed / Roboto / Roboto Mono | `#FBFBFA` `#E4E4E0` `#161616` `#4F4F4F` `#A3154F` `#0F766E` `#CFCFCA` | Small multiples: condensed headers, four charts to a slide on one shared grid, the same colors and fonts week after week |
| Sterling | Analyst | 4 | Light | Gelasio / Open Sans / Cousine | `#FBF9F4` `#E6E1D5` `#14130F` `#534F46` `#1E3A5F` `#9A6B12` `#D4CCBC` | Gelasio's Georgia-metric figures in navy over Open Sans, so a deck sits beside a Word-built board pack; a thin gold rule under the kicker |
| Garrison | Civic | 4 | Light | Roboto Condensed / Roboto / Roboto Mono | `#FFFFFF` `#E9EAE3` `#111111` `#4A4C43` `#4B5A1E` `#B7791F` `#C9CBC0` | Titles in condensed capitals, the bottom line in the olive kicker, tables ruled row by row |
| Council | Civic | 2 | Light | Arimo / Arimo / Cousine | `#FFFFFF` `#E7E9EC` `#121417` `#42464D` `#0B4F8A` `#8A5A00` `#BFC4CB` | Arial-metric Arimo at the largest body the layouts allow, every slide numbered and the presenter named in the footer, as council rules ask |
| Docket | Civic | 4 | Light | Literata / Open Sans / Cousine | `#FCFBF8` `#E6E2D8` `#171512` `#524E46` `#7A1F2B` `#2F5D50` `#D2CCBE` | Literata titles with oxblood kickers; exhibit passages quoted in italic beside their page |
| Beacon | Civic | 1 | Dark | B612 / Atkinson Hyperlegible Next / JetBrains Mono | `#0A0A0A` `#222222` `#FFFFFF` `#D6D6D6` `#FFD60A` `#7DD3FC` `#8A8A8A` | Every text color at 7:1 or more on both surfaces (WCAG AAA), body at weight 500, no hairlines; B612 titles drawn for cockpit screens |
| Signpost | Civic | 1 | Light | Inclusive Sans / Noto Sans / Atkinson Hyperlegible Mono | `#FFFFFF` `#E3E3E3` `#000000` `#333333` `#0033A0` `#8B1E15` `#6B6B6B` | AAA contrast on white and on its cards: every text color 7:1 or more on both surfaces; cards outlined in ink as well as tinted, so nothing rests on a projector's contrast |
| Voltage | Launch | 1 | Light | Poppins / Inter / JetBrains Mono | `#FFFFFF` `#EAE4FB` `#0D0B14` `#4F4A63` `#5B21B6` `#C0266D` `#D5CDEE` | Huge Poppins numbers reversed out of full-bleed violet slabs |
| Liftoff | Launch | 3 | Dark | Red Hat Display / Inter / JetBrains Mono | `#0C0A1F` `#231D4A` `#F5F3FF` `#B4AED6` `#22D3EE` `#F472B6` `#332C5E` | A slow mesh backdrop in indigo, cyan and pink, text kept on solid ground |
| Sorbet | Launch | 3 | Light | DM Sans / Inter / JetBrains Mono | `#FFFBF7` `#F6DFCF` `#2A1E1B` `#634F49` `#A51F52` `#6B4FA0` `#EBD4C4` | A pastel gradient backdrop of peach, lilac and mint behind rounded cards; DM Sans titles in sentence case; raspberry and deep lilac ink |
| Hanji | Launch | 5 | Light | Pretendard / Pretendard / JetBrains Mono | `#FAF8F3` `#E6E0D3` `#1A1A18` `#535049` `#0B6E4F` `#C0392B` `#D5CEBF` | Pretendard for Hangul and Latin alike on warm paper white, a jade accent, Korean leading opened |
| Dusk | Stage | S | Dark | Fraunces / Inter / JetBrains Mono | `#101014` `#17171D` `#F2F0E9` `#8A8A96` `#FF6A3D` `#FFC857` `#2A2A33` | Fraunces display set tight with optical sizing over loosely set Inter; warm paper-white ink on near-black; one orange accent |
| Noir | Stage | 3 | Dark | Bodoni Moda / Inter / JetBrains Mono | `#070707` `#221E1E` `#F7F3F0` `#ABA5A1` `#FF3B30` `#FF8FA3` `#332D2D` | Bodoni titles at poster size, one red, a fine grain shader over black |
| Marquee | Stage | 3 | Dark | Archivo / Inter / JetBrains Mono | `#120B1C` `#2A1E3A` `#FFF8EE` `#BDB0CB` `#FFB000` `#FF5D8F` `#3A2C49` | Marquee capitals: Archivo narrowed to wdth 62 at weight 800, figures in amber at hero size |
| Nocturne | Stage | 3 | Dark | Cormorant Garamond / Inter / JetBrains Mono | `#0D1117` `#212A36` `#E8EDF2` `#A1ACB8` `#B8B2F0` `#E8A5B8` `#2E3947` | Full-bleed photographs with one line of light Cormorant Garamond at display size set in their dark space |
| Gallery | Editorial | S | Light | Instrument Serif / Inter / JetBrains Mono | `#FCFCFA` `#F1F1EC` `#151515` `#66665F` `#2340D9` `#D9481F` `#E0E0D8` | Instrument Serif headlines set very large over Inter on gallery white; one cobalt accent |
| Broadsheet | Editorial | 3 | Light | Playfair / Literata / JetBrains Mono | `#F7F4EC` `#E3DCCB` `#141414` `#4F4C45` `#B3261E` `#1F4E79` `#CBC3B1` | Red capitals for kickers and Playfair headlines at their optical size over Literata set like a newspaper column |
| Basel | Editorial | 1 | Light | Inter / Inter / JetBrains Mono | `#F5F5F2` `#DEDED9` `#111111` `#474747` `#B80F2A` `#1F3FBF` `#C9C9C3` | Swiss: flush-left Inter at huge sizes, red the only color, hairlines on the grid's lines; needs no font beyond the four's |
| Washi | Editorial | 5 | Light | Noto Serif JP / Noto Sans JP / JetBrains Mono | `#F8F6F0` `#E4DED1` `#1C1B19` `#55534D` `#5B3A78` `#B5322F` `#D3CDBF` | Mincho titles over Gothic text, Japanese leading opened to 1.5 or more, murasaki ink with a vermilion seal |
| Qalam | Editorial | 5 | Light | Amiri / Rubik / JetBrains Mono | `#FBF8F1` `#E7DFCE` `#1B1712` `#554E43` `#0F5E5C` `#A8541E` `#D6CCB8` | Naskh titles in Amiri over Rubik, zero tracking and opened leading for Arabic, layouts mirrored right to left |
| Lyceum | Scholar | 2 | Light | Literata / Noto Sans / Roboto Mono | `#FFFFFF` `#E5E8ED` `#121417` `#4C535C` `#482878` `#C25E00` `#CDD3DA` | Figures first with numbered captions; Literata titles over Noto Sans, Greek set as the paper sets it; equations only as text until a math node exists; the light half of a pair with Seminar |
| Margin | Scholar | 2 | Light | Alegreya / Alegreya / Roboto Mono | `#FAF7F0` `#E9E2D2` `#1E1A14` `#574F43` `#8C2F39` `#3B6E5A` `#D6CCB6` | Alegreya throughout, old-style figures in text and lining figures in tables; quotations set large in italic |
| Remedy | Scholar | 2 | Light | Atkinson Hyperlegible Next / Open Sans / Roboto Mono | `#FFFFFF` `#DEEAEC` `#0F1A1C` `#455659` `#0B6E78` `#B4451F` `#C7D6D9` | Clinical teal kickers and Atkinson Hyperlegible Next titles, large x-height and unmistakable letters that hold in auditoriums; references in caption type |
| Seminar | Scholar | 2 | Dark | Literata / Inter / JetBrains Mono | `#14161A` `#2A2F37` `#ECEEF1` `#AAB1BB` `#8FB8FF` `#F0B86E` `#343A43` | For dimmed lecture halls: off-white text, never pure white, under Literata titles; the dark half of a pair with Lyceum |
| Recess | Classroom | 2 | Light | Shantell Sans / Nunito / JetBrains Mono | `#FFFDF5` `#FBE6A8` `#1F1B2D` `#4E4860` `#C2271D` `#1E6FD9` `#EDD9A0` | Marker-drawn Shantell titles with the bounce axis on, sunny cards, an icon on every list |
| Kite | Classroom | 2 | Dark | Comic Neue / Nunito / Roboto Mono | `#1E3A31` `#2E5045` `#F4F1E8` `#CBD5CE` `#FFD45C` `#8FD3FF` `#436357` | Chalkboard green with chalk-white rounded type and a chalk-dust grain shader |
| Sunbeam | Classroom | 2 | Light | Rubik / Rubik / Roboto Mono | `#FFFFFF` `#FCE7C8` `#1B1B1F` `#4F4F57` `#B93C0B` `#6D28D9` `#EBD7B8` | Warm orange and Rubik's softened corners for middle and high school; quiz and game slides ready |
| Haven | Classroom | 2 | Light | Atkinson Hyperlegible Next / Atkinson Hyperlegible Next / JetBrains Mono | `#FBF7EC` `#EAE0C6` `#24221E` `#4C483F` `#1F4E8C` `#9C4A00` `#D9CEB3` | Dyslexia-friendly: cream ground and dark-gray text, no italics or capitals in any role, letter and line spacing opened as WCAG 1.4.12 allows |
| Darkroom | Lens | 3 | Dark | Montserrat / Inter / JetBrains Mono | `#0B0B0B` `#222222` `#F2F2F2` `#A8A8A8` `#D9B26F` `#7FA7C9` `#333333` | Black mats: pictures inset on the slide's black, captions in small spaced capitals |
| Viewfinder | Lens | 3 | Light | Kanit / Inter / JetBrains Mono | `#FFFFFF` `#EAEAEA` `#111111` `#505050` `#2F4858` `#A3412A` `#D0D0D0` | White-framed photographs with a thin keyline and Kanit captions (Latin and Thai); slate ink, so the photographs carry the color |
| Postcard | Lens | 5 | Light | Fraunces / Noto Sans / JetBrains Mono | `#FBF5EA` `#EEDFC6` `#2B2118` `#5E5244` `#A8401B` `#2E6F8E` `#E0CFB2` | Scrapbook: Fraunces with SOFT 100 and WONK on, photographs with white borders on a paper-grain shader |
| Threshold | Lens | 3 | Light | Bodoni Moda / Roboto / Roboto Mono | `#FFFFFF` `#EDEAE4` `#1A1A1A` `#55524D` `#1F4D3A` `#957038` `#D9D4CB` | Hero photographs bled to the edge; price and specifications as big Bodoni numbers |
| Vespers | Ceremony | 4 | Dark | Barlow / Roboto / Roboto Mono | `#0E0E10` `#222228` `#FFFFFF` `#CACAD0` `#F4C27A` `#8EC5FF` `#34343C` | Lyrics in white capitals, two lines at most, over a slow particles or gradient backdrop; a lower-third variant for the stream |
| Hymnal | Ceremony | 4 | Light | EB Garamond / Literata / JetBrains Mono | `#FBF7EE` `#EBE2CE` `#1F1A14` `#574F43` `#6B2737` `#8A6A1F` `#D9CDB4` | Scripture set like a lectern Bible: Garamond verses (Greek included), the reference in small capitals |
| Vow | Ceremony | 4 | Light | Cormorant Garamond / Open Sans / JetBrains Mono | `#FFFBF8` `#F3E2DA` `#2A2224` `#5E4F52` `#933A52` `#5E7D66` `#E6D2C9` | Names in Cormorant Garamond italic at hero size over blush; photographs with soft rounded corners |
| Elegy | Ceremony | 4 | Dark | Playfair / Noto Sans / JetBrains Mono | `#1A1A1C` `#2C2C30` `#EFEDE8` `#B4B1AA` `#C9B37E` `#9FB4C7` `#3A3A3F` | Portraits on charcoal; names and dates in light Playfair; nothing louder than a whisper of gold |
| Kindred | Craft | 4 | Light | Aleo / Open Sans / JetBrains Mono | `#FCF8F2` `#EBDFCC` `#231C15` `#5A4E41` `#9E3F17` `#2F6B5A` `#DDCFBA` | Aleo slab titles over photographs of people; outcome numbers in terracotta |
| Fieldwork | Craft | 4 | Light | Alegreya Sans / Noto Sans / Roboto Mono | `#F6F3EA` `#E4DFCE` `#1D211A` `#4E5446` `#3F6B2A` `#9C5B1B` `#D0CAB5` | A fine paper-grain shader, maps and field photographs, humanist titles in moss |
| Halftone | Craft | 4 | Light | Archivo / Inter / JetBrains Mono | `#FFFDF5` `#FADBE6` `#141414` `#474747` `#C2005E` `#0B57D0` `#E8CDD8` | Risograph print: two spot inks, a grain shader, Archivo widened to wdth 125 at weight 900 |
| Saffron | Craft | 5 | Light | Poppins / Google Sans / Google Sans Code | `#FFF9F0` `#F6E3C4` `#231A12` `#5C4A36` `#7A1F5C` `#A84300` `#E8D2AE` | Devanagari and Latin in one voice (Poppins over Google Sans, which also sets Bengali, Tamil, Telugu, Thai and Hebrew), marigold cards and plum ink |
| Kernel | Systems | 1 | Dark | JetBrains Mono / Inter / JetBrains Mono | `#0F1218` `#212734` `#E8ECF1` `#A0AAB8` `#9ADE7B` `#82AAFF` `#2D3442` | Mono titles in JetBrains Mono and code cards that read at the back of a hall; needs no font beyond the four's |
| Lambda | Systems | 5 | Light | Roboto Mono / Roboto / Roboto Mono | `#FFFFFF` `#E7E9ED` `#16181C` `#4B5059` `#6E40C9` `#0E7C66` `#D2D6DC` | Docs-like: mono titles over Roboto, code and diagrams on cool-gray cards |
| Runtime | Systems | 5 | Light | Google Sans Code / Inter / Google Sans Code | `#FFFDF2` `#FFE45C` `#000000` `#333333` `#2E2BD9` `#E8420B` `#000000` | Neobrutalist: yellow cards with thick black outlines, oversized mono titles, flat fills |

## The fonts

The files a theme carries: each family's upright and its italic, both variable where the family is variable, one weight and its italic where it is static. Each is a family of google/fonts at 9710da1 but Pretendard.

| Family | Kind | Sets | Scripts | Bytes | gzip-9 | Italic | Reserved name | Themes |
| --- | --- | --- | --- | ---: | ---: | --- | --- | ---: |
| JetBrains Mono | monospace | display, mono | Latin, Cyrillic, Greek | 378,764 | 187,596 | Yes | No | 30 |
| Inter | neo-grotesque | body, display | Latin, Cyrillic, Greek | 1,783,172 | 956,816 | Yes | No | 19 |
| Roboto Mono | monospace | display, mono | Latin, Cyrillic, Greek | 380,492 | 268,481 | Yes | No | 13 |
| Noto Sans | humanist sans | body | Latin, Cyrillic, Greek, Devanagari | 4,371,736 | 2,580,943 | Yes | No | 6 |
| Open Sans | humanist sans | body | Latin, Cyrillic, Greek, Hebrew | 1,116,628 | 736,669 | Yes | No | 6 |
| Roboto | neo-grotesque | body | Latin, Cyrillic, Greek | 1,019,528 | 604,312 | Yes | No | 6 |
| Literata | editorial serif | body, display | Latin, Cyrillic, Greek | 1,857,860 | 1,012,726 | Yes | No | 5 |
| Cousine | monospace | mono | Latin, Cyrillic, Greek, Hebrew | 603,736 | 335,899 | Yes | No | 4 |
| Fraunces | editorial serif | display | Latin | 775,344 | 542,256 | Yes | No | 4 |
| Atkinson Hyperlegible Next | accessibility | body, display | Latin | 238,468 | 122,249 | Yes | No | 3 |
| Archivo | grotesque | display | Latin | 1,399,964 | 540,484 | Yes | No | 2 |
| Bodoni Moda | didone | display | Latin | 338,404 | 190,112 | Yes | No | 2 |
| Cormorant Garamond | old-style serif | display | Latin, Cyrillic | 1,911,204 | 536,893 | Yes | No | 2 |
| Google Sans Code | monospace | display, mono | Latin | 259,192 | 152,783 | Yes | No | 2 |
| Montserrat | geometric sans | display | Latin, Cyrillic | 1,507,312 | 612,038 | Yes | No | 2 |
| Nunito | rounded | body | Latin, Cyrillic | 558,764 | 265,412 | Yes | No | 2 |
| Playfair | editorial serif | display | Latin, Cyrillic | 3,560,456 | 1,972,546 | Yes | No | 2 |
| Poppins | geometric sans | display | Latin, Devanagari | 335,104 | 152,223 | Yes | No | 2 |
| Rubik | grotesque | body, display | Latin, Cyrillic, Arabic, Hebrew | 714,788 | 296,665 | Yes | No | 2 |
| Alegreya | old-style serif | body, display | Latin, Cyrillic, Greek | 851,140 | 429,956 | Yes | No | 1 |
| Alegreya Sans | humanist sans | display | Latin, Cyrillic, Greek | 541,408 | 258,667 | Yes | No | 1 |
| Aleo | slab serif | display | Latin | 303,860 | 162,843 | Yes | No | 1 |
| Amiri | Arabic naskh | display | Latin, Arabic | 823,172 | 398,717 | Yes | No | 1 |
| Arimo | neo-grotesque | body, display | Latin, Cyrillic, Greek, Hebrew | 1,039,464 | 638,224 | Yes | No | 1 |
| Atkinson Hyperlegible Mono | monospace | mono | Latin | 110,324 | 65,582 | Yes | No | 1 |
| B612 | accessibility | display | Latin, Greek | 210,104 | 107,925 | Yes | No | 1 |
| Barlow | grotesque | display | Latin | 220,168 | 108,284 | Yes | No | 1 |
| Barlow Condensed | condensed display | display | Latin | 221,700 | 106,893 | Yes | No | 1 |
| Comic Neue | handwritten | display | Latin | 111,644 | 61,122 | Yes | No | 1 |
| DM Sans | geometric sans | display | Latin | 525,204 | 248,928 | Yes | No | 1 |
| EB Garamond | old-style serif | display | Latin, Cyrillic, Greek | 1,605,644 | 785,284 | Yes | No | 1 |
| Geist | neo-grotesque | display | Latin, Cyrillic | 343,640 | 178,186 | Yes | No | 1 |
| Gelasio | transitional serif | display | Latin | 342,768 | 193,120 | Yes | No | 1 |
| Google Sans | humanist sans | body | Latin, Cyrillic, Greek, Devanagari, Bengali, Tamil, Telugu, Hebrew, Thai | 10,054,364 | 4,540,531 | Yes | No | 1 |
| Inclusive Sans | accessibility | display | Latin | 226,892 | 118,694 | Yes | No | 1 |
| Instrument Serif | editorial serif | display | Latin | 141,604 | 71,516 | Yes | No | 1 |
| Inter Tight | neo-grotesque | display | Latin, Cyrillic, Greek | 1,175,432 | 628,803 | Yes | No | 1 |
| Kanit | Thai and Latin | display | Latin, Thai | 350,072 | 141,533 | Yes | No | 1 |
| Noto Sans JP | Japanese | body | Latin, Cyrillic (most), Greek, kana, Han (most) | 9,589,900 | 5,852,116 | No | "Source", not its name | 1 |
| Noto Serif | editorial serif | display | Latin, Cyrillic, Greek | 4,335,688 | 2,544,473 | Yes | No | 1 |
| Noto Serif JP | Japanese | display | Latin, Cyrillic (most), Greek, kana, Han (most) | 13,574,352 | 8,094,449 | No | No | 1 |
| Plus Jakarta Sans | geometric sans | display | Latin | 359,476 | 165,693 | Yes | No | 1 |
| Pretendard | Korean | body, display | Latin, Cyrillic, Greek, Hangul, kana | 6,739,336 | 2,951,455 | No | Its own, "Source", "Inter", and "M PLUS 1" | 1 |
| Red Hat Display | geometric sans | display | Latin | 199,148 | 103,678 | Yes | No | 1 |
| Roboto Condensed | condensed display | display | Latin, Cyrillic, Greek | 759,740 | 452,666 | Yes | No | 1 |
| Shantell Sans | playful | display | Latin, Cyrillic | 2,817,168 | 1,379,879 | Yes | No | 1 |

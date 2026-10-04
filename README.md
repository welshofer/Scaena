# Scaena

*A presentation engine from the other direction: a timeline of states over one persistent scene graph, rendered deterministically by one Rust core to the browser, the Mac, PDF, and video. Agents are first-class authors.*

**Status (2026-10-04):**

- **Phases 0 and 1 are done.** Their gates are logged in [`docs/spike-report.md`](docs/spike-report.md) and [`docs/gate-1.md`](docs/gate-1.md).
- **Phase 2, the browser, is built.** That means the player, the source editor, the assistant, and `scaena serve`.
- **Gate 2 is open on two criteria that need a real machine:** 60 fps in shipping browsers, and an assistant run with a key ([`docs/gate-2.md`](docs/gate-2.md)).
- **Phase 3, the Mac app, comes next.**

## What it does

- **Write** a deck as `.scn` source or as `deck.json`. `compile` and `decompile` turn each into the other with nothing lost.
- **Check** it.
  - `validate` checks the JSON Schema and every reference.
  - `lint` finds overflow, collisions, contrast against what is painted behind the text, missing glyphs, and motion. It also checks the argument itself: a beat with no claim, or evidence larger than its claim.
  - `lint --fix` applies the fixes it has checked by laying the state out again.
- **See** it. `render` draws any state at rest or mid-cue, with the CPU painter or the GPU. Between states, text morphs word by word, shapes point by point, and charts by their marks.
- **Theme** it. Dusk, Daybreak, and Ember ship with their fonts and share one vocabulary, so `theme --apply` moves a deck between them.
- **Export** it:
  - a PDF, tagged, with text that copies;
  - PNG and SVG;
  - MP4, WebM, or ProRes, with a chapter per beat;
  - the spine, for the pipelines that read it;
  - one HTML file that plays with no network.
- **Edit it in a browser.**
  - The player has a presenter view.
  - The source editor previews and lints as you type.
  - The assistant runs on your own key.
  - `scaena serve` puts both on a folder on disk.
- **Hand it to an agent.** `scaena mcp` serves every operation as an MCP tool, and the format, the SPEC, and the lint catalog as resources.

| Read | For |
|---|---|
| [`docs/authoring.md`](docs/authoring.md) | how to write a deck: edit, check, see, save, present |
| [`docs/MANIFESTO.md`](docs/MANIFESTO.md) | why: the goal and the ten beliefs |
| [`docs/SPEC.md`](docs/SPEC.md) | what: the document model, engine, agent surface, clients, and exports |
| [`docs/PLAN.md`](docs/PLAN.md) | when: phases, tasks, exit criteria, and the gate log |
| [`docs/adr/`](docs/adr/) | why *this way*: twelve decisions and their trade-offs |
| [`CLAUDE.md`](CLAUDE.md) | how to work in this repo, for people and agents |

## Try it

```sh
cargo install --path crates/scaena-cli --locked       # puts `scaena` in ~/.cargo/bin
scaena new talk --theme dusk --title "Field notes"    # a bundle: the theme, its fonts, one empty state
scaena decompile talk -o talk/deck.scn                # the deck as source; `scaena compile` goes back
scaena lint docs/examples/revenue.deck.json
scaena render docs/examples/revenue.deck.json --state revenue --out revenue.png
scaena export docs/examples/trails.deck.json --format pdf --out trails.pdf
```

**In a browser:** `just site`, then `cd target/site && python3 serve.py`, and open <http://localhost:8080/editor.html>.

**On your own folder:** `scaena serve talk` compiles `deck.scn` each time you save it, and the player shows the change. `scaena export --format html` writes a deck as one file.

- Both need a `scaena` built after `just web`.
- `just web` needs Node, the `wasm32-unknown-unknown` target, and `wasm-bindgen-cli` 0.2.129.

[`docs/authoring.md`](docs/authoring.md) walks the whole loop. With [`just`](https://github.com/casey/just), `just check` runs what CI runs.

## The model in a few lines

```scn
state revenue layout:figure hold:6s
  title text role:headline "Revenue doubled" semantic:claim at:in(header)
  rev chart:bar data:@q3 x:{field: quarter, type: ordinal} y:{field: revenue, type: quantitative}
    series:{field: product, type: nominal} semantic:evidence at:in(main)

state mix slide:revenue transition:slow hold:6s
  title "…and the mix shifted"   # the same node: its words morph
  rev kind:stackedBar            # the same marks: the bars restack
```

- **Nodes exist for the whole deck.** States are cues, and a property a state does not change carries forward.
- **Transitions interpolate by identity.**
- **The theme owns typography, layout templates, and motion.** A document names roles, slots, and presets, never pixels.
- **`semantic:` says what a node is for.** Lint checks the argument by `claim` and `evidence`, and a screen reader skips `decoration`.

The whole deck is [`docs/examples/revenue.deck.scn`](docs/examples/revenue.deck.scn).

## Layout

```
crates/   core (model, tracking, timeline, lint) · engine (theme, layout, text, charts) · paint (CPU, GPU)
          export (PDF, PNG, SVG, video, spine, HTML) · store (bundles, history) · ops (every operation)
          cli · mcp · serve · wasm · subset · history · resources (the browser's modules) · ffi (Swift)
docs/     SPEC, PLAN, MANIFESTO, authoring.md, adr/, schema/ (generated), examples/, gate logs
skills/   agent skills that drive the CLI and MCP
tests/    golden display lists and rasters, lint fixtures, the parity harness, bench decks
web/      the player and the editor (Vite + TypeScript)
apps/mac/ the SwiftUI client (Phase 3)
```

## Working name

*Scaena* — Latin, stage. Rename at will; only the crate prefix cares.

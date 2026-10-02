# Scaena

*A presentation engine from the other direction: a timeline of states over one persistent scene graph, rendered deterministically by one Rust core to the browser, the Mac, PDF, and video. Agents are first-class authors.*

**Status:** pre-Phase-0 scaffold (2026-10-01). The document model, state tracking, timeline math, display-list types, document-level lints, and a working `scaena validate | lint | inspect | diff | export --format spine` exist. Rendering does not exist yet — that is Phase 0, and it is designed to be able to kill the project cheaply.

| Read | For |
|---|---|
| [`docs/MANIFESTO.md`](docs/MANIFESTO.md) | why — the goal and the ten beliefs |
| [`docs/SPEC.md`](docs/SPEC.md) | what — document model, engine, agent surface, clients, exports |
| [`docs/PLAN.md`](docs/PLAN.md) | when — phases, tasks, exit criteria, gate log |
| [`docs/adr/`](docs/adr/) | why *this way* — six decisions and their trade-offs |
| [`CLAUDE.md`](CLAUDE.md) | how to work in this repo (humans and agents) |

## Try it

```sh
cargo build -p scaena-cli
./target/debug/scaena validate docs/examples/revenue.deck.json
./target/debug/scaena lint     docs/examples/revenue.deck.json
./target/debug/scaena inspect  docs/examples/revenue.deck.json --state mix
./target/debug/scaena diff     docs/examples/revenue.deck.json --from revenue --to mix
./target/debug/scaena export   docs/examples/revenue.deck.json --format spine
```

With [`just`](https://github.com/casey/just): `just check` runs fmt, clippy (`-D warnings`), tests, and schema validation.

## The model in six lines

```scn
state revenue layout:full
  title  text role:display "Revenue doubled" semantic:claim  at:in(header)
  rev    chart:bar data:@q3 key:product semantic:evidence     at:col(1-12) row(2-6)

state mix slide:revenue
  title  "…and the mix shifted"     # same id → the words morph
  rev    kind:stackedBar            # same marks → the bars morph
```

Nodes exist for the whole deck. States are cues; unchanged properties track forward. Transitions interpolate by identity. The theme owns typography, layout templates, and motion; documents reference roles, never pixels.

## Layout

```
crates/   scaena-core · scaena-engine · scaena-paint · scaena-export · scaena-store · scaena-ops · scaena-cli · scaena-mcp · scaena-wasm · scaena-ffi
docs/     SPEC, PLAN, MANIFESTO, adr/, schema/ (JSON Schema), examples/ (deck + theme + data), reviews/
skills/   agent skills driving the CLI/MCP
tests/    golden display lists, golden rasters, lint fixtures, parity harness, benchmark decks
web/      Vite + TS player/editor (Phase 2)
apps/mac/ SwiftUI client (Phase 3)
```

## Working name

*Scaena* — Latin, stage. Rename at will; only the crate prefix cares.

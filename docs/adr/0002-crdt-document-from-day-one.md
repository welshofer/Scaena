# ADR-0002: The document is a CRDT from day one; sync is deferred

**Status:** accepted · **Date:** 2026-10-01

## Context

Multi-user collaboration is not a v1 goal and the cost of a backend is to be avoided for now. But retrofitting a document model for collaboration is the expensive part, not the server. We also want undo/redo, history, branching, offline edits, and concurrent human + agent edits immediately.

## Decision

The in-memory document is a **Loro** document (`loro` crate; Rust core; WASM build) with the container layout in SPEC §8.1. `deck.json` is an export of the CRDT state and remains canonical for git and agents. `history/deck.loro` carries history. No sync server, identity, or presence until Phase 4.

**Fallback:** Automerge (`automerge` crate, `automerge-repo` sync ecosystem) if Loro's tree/rich-text APIs fight the model. Decide by the end of week 2 of Phase 1 (PLAN 1.23).

## Consequences

- **+** Undo, history, branches, offline, and multi-author edits with no server.
- **+** Phase 4 collaboration becomes "add a sync endpoint," not a rewrite.
- **−** Schema discipline: nodes/states/spine must map onto maps, movable lists, trees, and rich text containers; derived state (layouts, display lists, lint results) stays out.
- **−** Two representations (`deck.json`, `deck.loro`) to keep consistent; the `fs` author path handles out-of-band edits.

## Decision (PLAN 1.23, 2026-10-03): Loro stays

Loro's tree and rich text fit the model; Automerge is not needed.
- **Containers.** The tree holds the spine: sections, their beats ordered under them, a beat moving between sections as one move. Movable lists hold the cue list and paint order, so a state moved on one branch and edited on another is one state. Rich text holds `runs`, a mark per run.
- **What Loro does not do, and the work-around.** In `scaena-store::crdt`:
  - A map keeps no order of its keys, and a map value keeps none either: both are hash maps. So values are held as JSON text, and each map keeps its key order beside it.
  - A map key cannot be renamed. So nodes are keyed by keys of the CRDT's own, and everything that names a node names it by key: a rename is one operation.
- **Evidence.** Every deck in the repository goes in and comes out as `deck.json` byte for byte. Concurrent edits merge as SPEC §8.1 says (`crates/scaena-store/tests/crdt.rs`).
- **Rich text's one ambiguity.** Text typed exactly at the edge of a run while the run beside it is restyled may land in either run. Every side settles it the same way.

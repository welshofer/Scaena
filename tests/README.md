# tests/

| dir | what | since |
|---|---|---|
| `lint/<CODE>/` | `trigger.deck.json` must raise exactly that code; `clean.deck.json` must not | PLAN 1.15 (document-level rules may add theirs earlier) |
| `fixtures/torture.scaena/` | the typography torture deck (PLAN 0.2) | Phase 0 |
| `golden/` | display-list JSON snapshots and raster PNGs per fixture state; reviewed diffs only | PLAN 0.3 |
| `bench/` | B1–B4 benchmark decks and criterion benches (SPEC §15) | Phase 0 |

Parity harness (CPU vs GPU vs WASM): `crates/scaena-paint/tests/parity.rs`, PLAN 0.9.

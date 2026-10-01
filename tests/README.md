# tests/

| dir | what | since |
|---|---|---|
| `lint/<CODE>/` | `trigger.deck.json` must raise exactly that code; `clean.deck.json` must not | PLAN 1.15 (document-level rules may add theirs earlier) |
| `fixtures/torture.scaena/` | the typography torture deck (PLAN 0.2) | Phase 0 |
| `golden/` | display-list JSON snapshots and raster PNGs per fixture state; reviewed diffs only. A failing comparison writes the new output to `golden/**/actual/` (git-ignored); after reviewing it, re-bless with `SCAENA_BLESS=1 just test` | PLAN 0.3 (format), 0.4 (torture text) |
| `bench/` | B1–B4 benchmark decks and criterion benches (SPEC §15) | Phase 0 |

Parity harness (CPU vs GPU vs WASM): `crates/scaena-paint/tests/parity.rs`, PLAN 0.9.

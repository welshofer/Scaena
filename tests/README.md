# tests/

| dir | what | since |
|---|---|---|
| `lint/<CODE>/` | `trigger.deck.json` must raise exactly that code; `clean.deck.json` must not. The validation codes (E102, E104–E106) are here since PLAN 1.2, checked in `crates/scaena-core/tests/validate.rs` against the theme in `lint/theme.json` | PLAN 1.2 (validation), 1.15 (rules) |
| `fixtures/torture.scaena/` | the typography torture deck (PLAN 0.2) | Phase 0 |
| `golden/` | display-list JSON snapshots and raster PNGs per fixture state; reviewed diffs only. A failing comparison writes the new output to `golden/**/actual/` (git-ignored); after reviewing it, re-bless with `SCAENA_BLESS=1 just test` | PLAN 0.3 (format), 0.4 (torture text) |
| `bench/` | SPEC §15's benchmark decks. `b1.scaena` is B1, text-heavy: the manifesto as 40 states in 4 fonts (fonts from `scripts/build_bundle_fonts.py`). B4 is `fixtures/torture.scaena`; B2 and B3 arrive with charts and shaders (PLAN 1.9, 1.10). `just bench` times every stage on them (`crates/scaena-cli/examples/stages.rs`) | PLAN 0.14 |

Parity harness (CPU vs GPU vs WASM): `crates/scaena-paint/tests/parity.rs`, PLAN 0.9.

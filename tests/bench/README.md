# Benchmark decks (SPEC §15)

| Deck | What it times | Made by |
|---|---|---|
| `b1.scaena` | **B1**, text: the manifesto as 40 text states in four fonts, no charts | hand (PLAN 0.14) |
| `b2.scaena` | **B2**, charts: six charts (bar, stacked bar, line, area, donut, scatter), each shown and then updated by key: 12 states | `scripts/build_bench_decks.py` |
| `b3.scaena` | **B3**, shaders: a full-bleed mesh with grain over it on all 8 states, its seed and palette changing from state to state | `scripts/build_bench_decks.py` |
| `../fixtures/torture.scaena` | **B4**, the typography torture deck | hand (PLAN 0.2) |

B2 and B3 use B1's theme and fonts. `scripts/build_bundle_fonts.py` subsets the fonts of all three and checks that every family sets text. Don't edit B2 or B3 by hand: change the generator and run it again. `just schema` checks that the committed decks match what it writes.

    python3 scripts/build_bench_decks.py                                # B2 and B3
    python3 scripts/build_bundle_fonts.py --only b2 --only b3           # their fonts (network)

`just bench` times every SPEC §15 stage on all four decks (`crates/scaena-cli/benches/stages.rs`), and `just bench layout/b2` times one. CI times every pull request ready for review (not a draft) beside its base on the same Linux machine, both built with each function on a 64-byte line as `just bench` builds them, and `main` on Linux and macOS once a week (`scripts/bench_gate.py`; SPEC §15).

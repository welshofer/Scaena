# Hyphenation patterns

The tries the engine hyphenates by (SPEC §3.5, ADR-0015): TeX's hyph-utf8 patterns for seventeen
languages, compiled to tries by [`hypher`](https://github.com/typst/hypher) 0.1.8. Each file is that
crate's `tries/<code>.bin`, byte for byte, and `scaena_engine::hyphen` walks them as `hypher` does
(its walker is cut from that crate, MIT, `LICENSE-MIT-hypher`).

- Native builds, and the player's WASM module, compile them in (`scaena-engine`'s `hyphenation`
  feature).
- The editor's module leaves them out: its page hands a language's file over the first time a
  text hyphenates in it, and the engine refuses one whose SHA-256 is not the one below.
- `every_language_breaks_words_where_hypher_does` holds the walk to `hypher`'s, compiled in and
  handed over.

The patterns are hyph-utf8's ([tex-hyphen](https://github.com/hyphenation/tex-hyphen)), each
under its own license (LPPL, MPL, MIT, or BSD-3, as that project lists them); `hypher` takes only
patterns whose license allows them in a binary. A language is added with its file from the same
`hypher` version, its bounds and digest in `LANGUAGES`, and a line in the test.

| Code | Language | Fewest characters before, after a break | Bytes | SHA-256 |
|---|---|---|---|---|
| `en` | English | 2, 3 | 26,947 | `be2bcb386cb777a191813f68b995d0c22fbf30783ac52a0ad2489c8ebd7fd354` |
| `de` | German | 2, 2 | 206,263 | `dc057dc24e655a602c6a26ba0bd1532fd143d5d5057c4aec58ce4fe7b30bf96e` |
| `fr` | French | 2, 2 | 6,988 | `64359f17aeb80371018144eaa1afb638da546dfeb594e17a7e819eb8a39c06c5` |
| `es` | Spanish | 2, 2 | 13,649 | `fdca79005c290d9de18d753df630e5accd78333a3c73f418b9eff4a78499fbff` |
| `it` | Italian | 2, 2 | 1,555 | `a9dc16dbaa585c43b6db52e345c8d49bedf3c71df234a487e5d318adea0c24e7` |
| `pt` | Portuguese | 2, 3 | 1,065 | `4d55b1babb909cd40dd6f359442ddc77350bbca705246b8b020ce81be6e043a3` |
| `nl` | Dutch | 2, 2 | 64,293 | `1177ef126b31bbb30f080633680fc14cc4cd6f56d1597981c975389b861d978b` |
| `sv` | Swedish | 2, 2 | 23,595 | `eb9a3be6acbcba9df573a3b1065b0cf8ea2b08da7c2897a965f7280cd1639419` |
| `da` | Danish | 2, 2 | 5,759 | `c69e3d5d43abe2697b2220848a95225a14b292cc4611bf86864a031150084f42` |
| `fi` | Finnish | 2, 2 | 636 | `365d87c66398e2cdae80eee5114d9fcbf589131cea1b5701c63acb7306011f14` |
| `pl` | Polish | 2, 2 | 15,530 | `cf4e14e2cc9b030b8ef8a1cff037c82bc06ac104d30a3f5f5c58809b3cf30cbd` |
| `cs` | Czech | 2, 2 | 40,726 | `1f7cb32555583658b8bc6daa35d213e856f28b69d47f44322eae2e11c2037557` |
| `ru` | Russian | 2, 2 | 33,344 | `0961ecad3a031ab9bd3f3d3e88fbda330dcaab7c3d2bbc0213d742c95b201818` |
| `uk` | Ukrainian | 2, 2 | 21,312 | `606c09f88c8bb713e18feaa53c231666b20e55c8f2032c780ce282c5d0a8cb6f` |
| `tr` | Turkish | 2, 2 | 526 | `1661448a091060bdee0d923c5489f6b717dc3dd11f00e2eaed364c8e07889c13` |
| `el` | Greek | 1, 1 | 2,032 | `a3138a9bc257b374bcd918fb193592af8a02a841025f68c919f0d1138683a48c` |
| `ca` | Catalan | 2, 2 | 1,727 | `27a70e94fb14d5c2df252a6df211f69e7df9f4d0de9a014e0466a2ca731ddf5a` |

465,947 bytes in all.

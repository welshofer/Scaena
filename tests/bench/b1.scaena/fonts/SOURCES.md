# Benchmark B1 fonts: provenance

Built by `scripts/build_bench_fonts.py` (fontTools 4.66.1) from [google/fonts](https://github.com/google/fonts) at commit `9710da1eacb3be272583c3224dcb70f9da6eadbb`. Do not edit by hand; rerun the script.

| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | Theme family |
|---|---|---|---|---|---|
| `Fraunces-VF.ttf` | `ofl/fraunces/Fraunces[SOFT,WONK,opsz,wght].ttf` | `177ff6c0f14e5550…` | `93f51dff0b002288…` | 351 KB → 244 KB | `display` |
| `Inter-VF.ttf` | `ofl/inter/Inter[opsz,wght].ttf` | `29160a80ff49ddca…` | `744a5c4c70c482c0…` | 856 KB → 256 KB | `body` |
| `JetBrainsMono-VF.ttf` | `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | `48715a42ec242c21…` | `79d234941e622c63…` | 182 KB → 103 KB | `mono` |
| `SourceSerif4-VF.ttf` | `ofl/sourceserif4/SourceSerif4[opsz,wght].ttf` | `97b2d4da6e3cb494…` | `59632d4d51374145…` | 1181 KB → 491 KB | `text` |

**Subset:** every character in `deck.json` ∪ U+0020–007E ∪ U+00A0–017F ∪ U+2010–203A ∪ U+20AC. All OpenType layout features, all name records, variations and hinting kept.

**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file (sha256-pinned in the script).

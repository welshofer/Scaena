# Torture-deck fonts: provenance

Built by `scripts/build_torture_fonts.py` (fontTools 4.66.1) from [google/fonts](https://github.com/google/fonts) at commit `9710da1eacb3be272583c3224dcb70f9da6eadbb`. Do not edit by hand; rerun the script.

| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | PLAN 0.2 role |
|---|---|---|---|---|---|
| `RobotoSerif-VF.ttf` | `ofl/robotoserif/RobotoSerif[GRAD,opsz,wdth,wght].ttf` | `351ced75f3851806…` | `64adade3f89dd8e2…` | 3852 KB → 1790 KB | variable wght + opsz + wdth (kill cases) |
| `EBGaramond-VF.ttf` | `ofl/ebgaramond/EBGaramond[wght].ttf` | `ef9512f92f6d579e…` | `cbb935deec70722a…` | 831 KB → 291 KB | discretionary ligatures; Greek fallback |
| `NotoSansHebrew-VF.ttf` | `ofl/notosanshebrew/NotoSansHebrew[wdth,wght].ttf` | `7ef36a2c3593758c…` | `7002a46fb7c393f7…` | 110 KB → 71 KB | fallback for a script the first two lack |
| `NotoSansArabic-VF.ttf` | `ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf` | `63111b5b2e074dd4…` | `6240ae32d944a419…` | 824 KB → 164 KB | catalogue only: Arabic bidi line |
| `NotoColorEmoji-COLRv1.ttf` | `ofl/notocoloremoji/NotoColorEmoji-Regular.ttf` | `4d82a18d8d95f60b…` | `24f1031339499bdd…` | 24739 KB → 28 KB | catalogue only: emoji |
| `JetBrainsMono-VF.ttf` | `ofl/jetbrainsmono/JetBrainsMono[wght].ttf` | `48715a42ec242c21…` | `448ca6a6f221712e…` | 182 KB → 107 KB | code, set as written (PLAN 2.131) |
| `NotoSansJP-VF.ttf` | `ofl/notosansjp/NotoSansJP[wght].ttf` | `c2f3b4d463500a2d…` | `0a3fa848b29bf3bc…` | 9365 KB → 28 KB | Japanese line breaking: phrases, particles, counters (case 71) |
| `NotoSansSC-VF.ttf` | `ofl/notosanssc/NotoSansSC[wght].ttf` | `a3041811a78c361b…` | `1c9055bf1fc73bc6…` | 17355 KB → 9 KB | Chinese: a balanced headline (case 71) |
| `NotoSansKR-VF.ttf` | `ofl/notosanskr/NotoSansKR[wght].ttf` | `194018e6b2b293a7…` | `18e0ef49c6ec0da0…` | 10170 KB → 6 KB | Korean: breaks between words (case 71) |

**Subset:** every character in `deck.json` ∪ U+0020–007E ∪ U+00A0–017F ∪ U+2010–203A ∪ U+20AC; the Noto Sans JP, SC, and KR subsets hold only the characters of the text set in them (case 71). All OpenType layout features, all name records, variations and hinting kept. Noto Color Emoji additionally drops its `SVG ` table, so COLRv1 is the only color format any painter (vello, vello_cpu, PDF) can choose.

**Licenses:** SIL Open Font License 1.1, no Reserved Font Names; each `OFL-*.txt` beside the fonts is the upstream license file (sha256-pinned in the script).

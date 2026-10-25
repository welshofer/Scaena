# Script fonts for the tests: provenance

Built by `scripts/build_scripts.py` (fontTools 4.66.1) from [google/fonts](https://github.com/google/fonts) at commit `9710da1eacb3be272583c3224dcb70f9da6eadbb`. Do not edit by hand; rerun the script.

| File | Upstream path | sha256 | What |
|---|---|---|---|
| `NotoSansDevanagari-VF.ttf` | `ofl/notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf` | `14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e` | whole, the upstream bytes |
| `NotoSansJP-subset.ttf` | `ofl/notosansjp/NotoSansJP[wght].ttf` | `e41ec229b2ebe3f62e36f91617604d20757fc517b7ff08fafd6db6b0d1199cb7` | subset to `japanese.json`, the engine tests' Japanese, CJK punctuation, and full-width forms |

`japanese.json` and `hindi.json` are slides in Narrato's form, written in Japanese and Hindi, which the tests make decks of.

**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file (sha256-pinned in the script).

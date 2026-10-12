#!/usr/bin/env python3
"""Build what a deck in another script takes (PLAN G.22, ADR-0027), and the fonts its tests use.

1. The characters a save keeps of a CJK font beside a deck's own: the first level of each
   script's national standard, JIS X 0208's 2,965 kanji, GB 2312's 3,755 hanzi, and KS X 1001's
   2,350 Hangul syllables. Each is read from Python's codecs, which encode exactly the standard's
   characters, and written as a Rust string, in code point order
   (crates/scaena-store/src/common.rs).
2. The tests' script fonts (tests/fixtures/scripts/), from the same pinned google/fonts commit
   as every other font here, sha256-verified:
   - Noto Sans Devanagari whole, the upstream bytes, so a test serves the very font a deck in
     Hindi fetches, its digest the one `scaena_ops::scripts` pins;
   - Noto Sans JP subset to the characters of `japanese.json` beside it, a stand-in a test
     lays a deck in Japanese out in (a deck takes the whole font).

Check (no network; CI runs this): common.rs is as this writes it, and the fixtures' digests are
those recorded in SOURCES.md, Devanagari's the upstream's.

    pip install fonttools==4.66.1
    python3 scripts/build_scripts.py           # write common.rs, fetch and build the fixtures
    python3 scripts/build_scripts.py --cache ~/.cache/scaena-fonts
    python3 scripts/build_scripts.py --check
"""

import argparse
import io
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_torture_fonts as torture  # noqa: E402

ROOT = torture.ROOT
COMMON = ROOT / "crates/scaena-store/src/common.rs"
FIXTURES = ROOT / "tests/fixtures/scripts"

# Each set: its name, what it is, the codec that encodes the standard, the lead bytes of its
# first level, the code points to look among, and how many the standard has.
SETS = [
    ("KANJI", "JIS X 0208's first level: the 2,965 kanji Japanese writes most", "euc_jp", (0xB0, 0xCF), (0x4E00, 0x9FFF), 2965),
    ("HANZI", "GB 2312's first level: the 3,755 hanzi Chinese writes most", "gb2312", (0xB0, 0xD7), (0x4E00, 0x9FFF), 3755),
    ("HANGUL", "KS X 1001's 2,350 Hangul syllables, those Korean writes", "euc_kr", (0xB0, 0xC8), (0xAC00, 0xD7A3), 2350),
]

DEVANAGARI = (
    "ofl/notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf",
    "14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e",
)
JP = ("ofl/notosansjp/NotoSansJP[wght].ttf", "c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f")
LICENSES = {
    "OFL-NotoSansDevanagari.txt": ("ofl/notosansdevanagari/OFL.txt", "a216f6f8d85c7228093e0ee5e258d9d377e6671f68acb4db1930b29583d0f331"),
    "OFL-NotoSansJP.txt": ("ofl/notosansjp/OFL.txt", "1c05c68c34f9708415aada51f17e1b0092d2cea709bf4a94cd38114f9e73d7d9"),
}


def level(codec: str, lead: tuple[int, int], span: tuple[int, int]) -> str:
    out = []
    for cp in range(span[0], span[1] + 1):
        try:
            b = chr(cp).encode(codec)
        except UnicodeEncodeError:
            continue
        if len(b) == 2 and lead[0] <= b[0] <= lead[1]:
            out.append(chr(cp))
    return "".join(out)


def common() -> str:
    lines = [
        "//! The characters a save keeps of a CJK font beside a deck's own (PLAN G.22, ADR-0027):",
        "//! the first level of each script's national standard, so a deck saved can be edited in its",
        "//! language without the original font. Written by `scripts/build_scripts.py` from Python's",
        "//! codecs; do not edit by hand.",
        "",
    ]
    for name, what, codec, lead, span, count in SETS:
        chars = level(codec, lead, span)
        if len(chars) != count:
            sys.exit(f"{name}: {len(chars)} characters, where the standard has {count}")
        lines.append(f"/// {what}.")
        lines.append(f'pub const {name}: &str = "\\')
        for i in range(0, len(chars), 40):
            lines.append(chars[i : i + 40] + "\\")
        lines[-1] = lines[-1][:-1] + '";'
        lines.append("")
    return "\n".join(lines)


def japanese_chars() -> set[int]:
    """Every character of `japanese.json`, the letters a theme is asked for before a deck is
    written in Japanese (`LETTERS` in scaena_ops::narrative), CJK punctuation, and full-width
    forms."""
    import re

    text = json.dumps(json.loads((FIXTURES / "japanese.json").read_text(encoding="utf-8")), ensure_ascii=False)
    narrative = (ROOT / "crates/scaena-ops/src/narrative.rs").read_text(encoding="utf-8")
    letters = re.search(r'\("Japanese", "([^"]*)"\)', narrative)
    if not letters:
        sys.exit("scaena_ops::narrative::LETTERS names no letters for Japanese")
    chars = {ord(c) for c in text + letters[1]}
    chars.update(range(0x3000, 0x3040))
    chars.update(range(0xFF01, 0xFF5F))
    return chars


def build(cache: Path | None) -> None:
    from fontTools import subset
    from fontTools.ttLib import TTFont

    FIXTURES.mkdir(parents=True, exist_ok=True)
    devanagari = torture.fetch(DEVANAGARI[0], DEVANAGARI[1], cache)
    (FIXTURES / "NotoSansDevanagari-VF.ttf").write_bytes(devanagari)
    jp = torture.fetch(JP[0], JP[1], cache)
    opts = subset.Options()
    opts.layout_features = ["*"]
    opts.name_IDs = ["*"]
    opts.name_languages = ["*"]
    opts.notdef_outline = True
    opts.hinting = True
    font = TTFont(io.BytesIO(jp), recalcTimestamp=False)
    sub = subset.Subsetter(opts)
    sub.populate(unicodes=japanese_chars())
    sub.subset(font)
    out = io.BytesIO()
    font.save(out)
    (FIXTURES / "NotoSansJP-subset.ttf").write_bytes(out.getvalue())
    for file, (path, pinned) in LICENSES.items():
        (FIXTURES / file).write_bytes(torture.fetch(path, pinned, cache))
    rows = [
        ("NotoSansDevanagari-VF.ttf", DEVANAGARI[0], "whole, the upstream bytes"),
        ("NotoSansJP-subset.ttf", JP[0], "subset to `japanese.json`, CJK punctuation, and full-width forms"),
    ]
    lines = [
        "# Script fonts for the tests: provenance",
        "",
        f"Built by `scripts/build_scripts.py` (fontTools {torture.FONTTOOLS}) from "
        f"[google/fonts](https://github.com/google/fonts) at commit `{torture.COMMIT}`. Do not edit by hand; rerun the script.",
        "",
        "| File | Upstream path | sha256 | What |",
        "|---|---|---|---|",
    ]
    for file, path, what in rows:
        lines.append(f"| `{file}` | `{path}` | `{torture.sha256((FIXTURES / file).read_bytes())}` | {what} |")
    lines += [
        "",
        "`japanese.json` and `hindi.json` are slides in Narrato's form, written in Japanese and Hindi, which the tests make decks of.",
        "",
        "**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file (sha256-pinned in the script).",
        "",
    ]
    (FIXTURES / "SOURCES.md").write_text("\n".join(lines), encoding="utf-8")


def check() -> bool:
    ok = True
    if not COMMON.exists() or COMMON.read_text(encoding="utf-8") != common():
        ok = False
        print(f"error: {COMMON.relative_to(ROOT)} is not as scripts/build_scripts.py writes it: run it")
    sources = (FIXTURES / "SOURCES.md").read_text(encoding="utf-8") if (FIXTURES / "SOURCES.md").exists() else ""
    for file in ["NotoSansDevanagari-VF.ttf", "NotoSansJP-subset.ttf"]:
        path = FIXTURES / file
        digest = torture.sha256(path.read_bytes()) if path.exists() else "missing"
        if f"`{digest}`" not in sources:
            ok = False
            print(f"error: tests/fixtures/scripts/{file} ({digest}) is not the one SOURCES.md records")
    devanagari = FIXTURES / "NotoSansDevanagari-VF.ttf"
    if devanagari.exists() and torture.sha256(devanagari.read_bytes()) != DEVANAGARI[1]:
        ok = False
        print("error: tests/fixtures/scripts/NotoSansDevanagari-VF.ttf is not the upstream font")
    print(f"scripts {'ok' if ok else 'FAILED'}")
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check common.rs and the fixtures; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    args = ap.parse_args()
    if not args.check:
        COMMON.write_text(common(), encoding="utf-8")
        build(args.cache)
    sys.exit(0 if check() else 1)


if __name__ == "__main__":
    main()

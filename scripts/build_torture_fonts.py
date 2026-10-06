#!/usr/bin/env python3
"""Build and check the typography torture deck's bundle fonts (PLAN 0.2).

Build: download five SIL OFL fonts from google/fonts at a pinned commit, verify
their sha256, subset them to every character the deck uses plus ASCII, Latin-1,
Latin Extended-A and common punctuation (so small text edits need no rebuild),
and write fonts/*.ttf, the OFL texts, and fonts/SOURCES.md. Subsetting keeps all
OpenType layout features, variations, and hinting. Noto Color Emoji also drops
its `SVG ` table so COLRv1 is the only color format any painter can pick.

Check (no network; CI runs this): every character of every text node resolves
through its role's family fallback chain to a bundle font, and each case uses
exactly the fonts it is meant to test (a kill case that silently fell back would
be testing the wrong thing).

    pip install fonttools==4.66.1
    python3 scripts/build_torture_fonts.py           # rebuild fonts, then check
    python3 scripts/build_torture_fonts.py --cache ~/.cache/scaena-fonts   # reuse verified downloads
    python3 scripts/build_torture_fonts.py --check   # check only
"""

import argparse
import hashlib
import http.client
import json
import sys
import time
import unicodedata
import urllib.parse
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BUNDLE = ROOT / "tests/fixtures/torture.scaena"
FONTS = BUNDLE / "fonts"
COMMIT = "9710da1eacb3be272583c3224dcb70f9da6eadbb"
FONTTOOLS = "4.66.1"

# bundle file -> (google/fonts path, upstream sha256, PLAN 0.2 role)
SOURCES = {
    "RobotoSerif-VF.ttf": (
        "ofl/robotoserif/RobotoSerif[GRAD,opsz,wdth,wght].ttf",
        "351ced75f3851806aa6d846b669361521eb1925cfc530396df9c1a1b77061ddb",
        "variable wght + opsz + wdth (kill cases)",
    ),
    "EBGaramond-VF.ttf": (
        "ofl/ebgaramond/EBGaramond[wght].ttf",
        "ef9512f92f6d579e5dc75af59a5a4b1b8b47d2eda89e00b954d44520e5369027",
        "discretionary ligatures; Greek fallback",
    ),
    "NotoSansHebrew-VF.ttf": (
        "ofl/notosanshebrew/NotoSansHebrew[wdth,wght].ttf",
        "7ef36a2c3593758cdb622e1bdef4f84523e92fbc3ccc667438dd80ff54c2de88",
        "fallback for a script the first two lack",
    ),
    "NotoSansArabic-VF.ttf": (
        "ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf",
        "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e",
        "catalogue only: Arabic bidi line",
    ),
    "NotoColorEmoji-COLRv1.ttf": (
        "ofl/notocoloremoji/NotoColorEmoji-Regular.ttf",
        "4d82a18d8d95f60ba883ce242bbadbf84a576e987745dd7ba38d71c67cff2d73",
        "catalogue only: emoji",
    ),
}
LICENSES = {  # OFL text shipped beside each font (OFL 1.1 requires it to travel with the font)
    "OFL-RobotoSerif.txt": ("ofl/robotoserif/OFL.txt", "34dbfbb43e0b4fdeef445d77b9ac0b988e5ad7a9bbf16808c97b66c66d51f553"),
    "OFL-EBGaramond.txt": ("ofl/ebgaramond/OFL.txt", "0985066662eb755ed3683ae5482a81a9195b49ce3f7e165cc2388b3dbece7dd7"),
    "OFL-NotoSansHebrew.txt": ("ofl/notosanshebrew/OFL.txt", "9b9fe028b5ba74d231659a1bbaf0ed09b11e759d1ca6a070999e16d151616b47"),
    "OFL-NotoSansArabic.txt": ("ofl/notosansarabic/OFL.txt", "07fc70bfeb985cc1a87a8587d0a0c80bab11c86c9dc3fd95b6f0cb332f983e96"),
    "OFL-NotoColorEmoji.txt": ("ofl/notocoloremoji/OFL.txt", "ac564676d10054a8445923dfc2dfb13c042d97888bd27c1b6ec6dfe89a9d8d62"),
}
EXTRA_RANGES = [(0x20, 0x7E), (0xA0, 0x17F), (0x2010, 0x203A), (0x20AC, 0x20AC)]
# Characters that select or join rather than draw; shaping consumes them.
# Hard line breaks end a line or a paragraph and set no glyph (SPEC §3.5): a list's items are
# paragraphs (PLAN 2.69).
IGNORABLE = {0x200C, 0x200D, 0x2060, 0xFE0E, 0xFE0F, 0x00AD, 0x000A, 0x000D, 0x2028, 0x2029}

# Fonts each text node must end up using. Unlisted nodes must use only their role's own family:
# a kill case that fell back would be testing fallback instead of its feature.
EXPECTED_FAMILIES = {
    "fallback-run": {"serif", "garamond", "hebrew"},
    "emoji-line": {"serif", "emoji"},
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fetch(path: str, expected: str, cache: Path | None) -> bytes:
    """Upstream bytes for `path`, verified against the pinned sha256 (from `cache` when it holds them)."""
    cached = cache / expected if cache else None
    if cached and cached.exists() and sha256(cached.read_bytes()) == expected:
        return cached.read_bytes()
    url = f"https://raw.githubusercontent.com/google/fonts/{COMMIT}/{urllib.parse.quote(path)}"
    for attempt in range(5):  # large files are sometimes cut off in transit; the hash catches the rest
        try:
            with urllib.request.urlopen(url, timeout=300) as r:
                data = r.read()
            break
        except (OSError, http.client.HTTPException) as e:
            if attempt == 4:
                sys.exit(f"download failed for {path}: {e}")
            time.sleep(2**attempt)
    if sha256(data) != expected:
        sys.exit(f"sha256 mismatch for {path}: got {sha256(data)}, pinned {expected}")
    if cached:
        cache.mkdir(parents=True, exist_ok=True)
        cached.write_bytes(data)
    return data


def deck_strings(value):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for v in value.values():
            yield from deck_strings(v)
    elif isinstance(value, list):
        for v in value:
            yield from deck_strings(v)


def build(deck: dict, cache: Path | None) -> None:
    import io

    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    if fontTools.version != FONTTOOLS:
        print(f"warning: fontTools {fontTools.version}, pinned {FONTTOOLS}; subset bytes may differ", file=sys.stderr)
    unicodes = {ord(c) for s in deck_strings(deck) for c in s}
    for lo, hi in EXTRA_RANGES:
        unicodes.update(range(lo, hi + 1))
    FONTS.mkdir(parents=True, exist_ok=True)
    rows = []
    for name, (path, pinned, role) in SOURCES.items():
        data = fetch(path, pinned, cache)
        opts = subset.Options()
        opts.layout_features = ["*"]
        opts.name_IDs = ["*"]
        opts.name_languages = ["*"]
        opts.notdef_outline = True
        opts.hinting = True
        if name.startswith("NotoColorEmoji"):
            opts.drop_tables = opts.drop_tables + ["SVG"]  # fontTools matches stripped tags
        font = TTFont(io.BytesIO(data), recalcTimestamp=False)
        sub = subset.Subsetter(opts)
        sub.populate(unicodes=unicodes)
        sub.subset(font)
        out = io.BytesIO()
        font.save(out)
        (FONTS / name).write_bytes(out.getvalue())
        rows.append((name, path, pinned, sha256(out.getvalue()), len(data), len(out.getvalue()), role))
    for name, (path, pinned) in LICENSES.items():
        (FONTS / name).write_bytes(fetch(path, pinned, cache))
    lines = [
        "# Torture-deck fonts: provenance",
        "",
        f"Built by `scripts/build_torture_fonts.py` (fontTools {FONTTOOLS}) from "
        f"[google/fonts](https://github.com/google/fonts) at commit `{COMMIT}`. Do not edit by hand; rerun the script.",
        "",
        "| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | PLAN 0.2 role |",
        "|---|---|---|---|---|---|",
    ]
    for name, path, up, sub_hash, n_up, n_sub, role in rows:
        lines.append(f"| `{name}` | `{path}` | `{up[:16]}…` | `{sub_hash[:16]}…` | {n_up // 1024} KB → {n_sub // 1024} KB | {role} |")
    ranges = " ∪ ".join(f"U+{lo:04X}–{hi:04X}" if lo != hi else f"U+{lo:04X}" for lo, hi in EXTRA_RANGES)
    lines += [
        "",
        f"**Subset:** every character in `deck.json` ∪ {ranges}. All OpenType layout features, all name records, "
        "variations and hinting kept. Noto Color Emoji additionally drops its `SVG ` table, so COLRv1 is the only "
        "color format any painter (vello, vello_cpu, PDF) can choose.",
        "",
        "**Licenses:** SIL Open Font License 1.1, no Reserved Font Names; each `OFL-*.txt` beside the fonts is the "
        "upstream license file (sha256-pinned in the script).",
        "",
    ]
    (FONTS / "SOURCES.md").write_text("\n".join(lines), encoding="utf-8")
    print(f"built {len(rows)} fonts into {FONTS.relative_to(ROOT)}")


def check(deck: dict, theme: dict) -> bool:
    from fontTools.ttLib import TTFont

    families = theme["type"]["families"]
    roles = theme["type"]["roles"]
    cmaps = {key: TTFont(BUNDLE / fam["file"], lazy=True).getBestCmap() for key, fam in families.items()}

    def chain(role):
        primary = roles[role]["family"]
        return [primary] + families[primary].get("fallback", [])

    texts = []  # (node id, text, role)
    for nid, node in deck["nodes"].items():
        if node["type"] != "text":
            continue
        if "runs" in node:
            texts += [(nid, run["text"], run.get("role", node["role"])) for run in node["runs"]]
        else:
            texts.append((nid, node["text"], node["role"]))
        for state in deck["states"]:
            delta = state.get("props", {}).get(nid, {})
            if "text" in delta:
                texts.append((nid, delta["text"], delta.get("role", node["role"])))

    ok, used = True, {}
    for nid, text, role in texts:
        for i, ch in enumerate(text):
            if ord(ch) in IGNORABLE:
                continue
            fams = chain(role)
            # UTS #51: default-emoji-presentation codepoints (approximated as U+1F000 and up, which covers
            # every emoji in this deck) and any base + VS16 must come from the color font, even when a text
            # font earlier in the chain maps them: EB Garamond maps the regional indicators U+1F1E6-1F1FF.
            if (ord(ch) >= 0x1F000 or text[i + 1 : i + 2] == "\ufe0f") and "emoji" in fams:
                fams = ["emoji"]
            fam = next((f for f in fams if ord(ch) in cmaps[f]), None)
            if fam is None:
                ok = False
                print(f"error: {nid}: U+{ord(ch):04X} {unicodedata.name(ch, '?')} is not in any font of role `{role}`")
            else:
                used.setdefault(nid, set()).add(fam)
    for nid, fams in used.items():
        node = deck["nodes"][nid]
        expected = EXPECTED_FAMILIES.get(nid) or {roles[r]["family"] for r in [node["role"]] + [run.get("role", node["role"]) for run in node.get("runs", [])]}
        if fams != expected:
            ok = False
            print(f"error: {nid} uses fonts {sorted(fams)}, expected {sorted(expected)}")
    print(f"coverage: {len(texts)} strings across {len(used)} text nodes {'ok' if ok else 'FAILED'}")
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check coverage only; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    args = ap.parse_args()
    deck = json.loads((BUNDLE / "deck.json").read_text(encoding="utf-8"))
    theme = json.loads((BUNDLE / "theme.json").read_text(encoding="utf-8"))
    if not args.check:
        build(deck, args.cache)
    sys.exit(0 if check(deck, theme) else 1)


if __name__ == "__main__":
    main()

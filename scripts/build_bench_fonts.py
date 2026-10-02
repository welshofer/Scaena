#!/usr/bin/env python3
"""Build and check the benchmark decks' bundle fonts (SPEC §15, PLAN 0.14).

The same pinned google/fonts commit, sha256 verification, and subset options as the
torture deck's fonts (scripts/build_torture_fonts.py, whose helpers this reuses).
Each font is subset to every character its deck uses plus ASCII, Latin-1, Latin
Extended-A, and common punctuation, which is what saving a bundle will do (SPEC §3.1).

Check (no network; CI runs this): every character of every text node's text, in
every state, maps in its role's own family. A benchmark that silently fell back
would time the wrong font.

    pip install fonttools==4.66.1
    python3 scripts/build_bench_fonts.py           # rebuild fonts, then check
    python3 scripts/build_bench_fonts.py --cache ~/.cache/scaena-fonts
    python3 scripts/build_bench_fonts.py --check   # check only
"""

import argparse
import io
import json
import sys
import unicodedata
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_torture_fonts as torture  # noqa: E402

ROOT = torture.ROOT
BENCH = {
    "b1": {
        "bundle": ROOT / "tests/bench/b1.scaena",
        "fonts": {  # bundle file -> (google/fonts path, upstream sha256, theme family)
            "Fraunces-VF.ttf": (
                "ofl/fraunces/Fraunces[SOFT,WONK,opsz,wght].ttf",
                "177ff6c0f14e5550a3c624247cd1189611d4eb65d000b14944c63d967958abbb",
                "display",
            ),
            "Inter-VF.ttf": (
                "ofl/inter/Inter[opsz,wght].ttf",
                "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031",
                "body",
            ),
            "JetBrainsMono-VF.ttf": (
                "ofl/jetbrainsmono/JetBrainsMono[wght].ttf",
                "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
                "mono",
            ),
            "SourceSerif4-VF.ttf": (
                "ofl/sourceserif4/SourceSerif4[opsz,wght].ttf",
                "97b2d4da6e3cb494b5a1e66ae176914d852ccabef49e0c02c0df25f3e39aca0b",
                "text",
            ),
        },
        "licenses": {
            "OFL-Fraunces.txt": ("ofl/fraunces/OFL.txt", "bdf4c22802eaf804f998195871c6b8938aac2ac14b2d78a8bd66a6f1eced833b"),
            "OFL-Inter.txt": ("ofl/inter/OFL.txt", "5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57"),
            "OFL-JetBrainsMono.txt": ("ofl/jetbrainsmono/OFL.txt", "b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d"),
            "OFL-SourceSerif4.txt": ("ofl/sourceserif4/OFL.txt", "5f94c3fd3a23131a417ab5a0c8452de57e70c3cfb9f604d88241f7065ebf9fd9"),
        },
    },
}


def build(name: str, deck: dict, cache: Path | None) -> None:
    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    if fontTools.version != torture.FONTTOOLS:
        print(f"warning: fontTools {fontTools.version}, pinned {torture.FONTTOOLS}; subset bytes may differ", file=sys.stderr)
    cfg = BENCH[name]
    fonts = cfg["bundle"] / "fonts"
    unicodes = {ord(c) for s in torture.deck_strings(deck) for c in s}
    for lo, hi in torture.EXTRA_RANGES:
        unicodes.update(range(lo, hi + 1))
    fonts.mkdir(parents=True, exist_ok=True)
    rows = []
    for file, (path, pinned, family) in cfg["fonts"].items():
        data = torture.fetch(path, pinned, cache)
        opts = subset.Options()  # the torture deck's options, minus its emoji special case
        opts.layout_features = ["*"]
        opts.name_IDs = ["*"]
        opts.name_languages = ["*"]
        opts.notdef_outline = True
        opts.hinting = True
        font = TTFont(io.BytesIO(data), recalcTimestamp=False)
        sub = subset.Subsetter(opts)
        sub.populate(unicodes=unicodes)
        sub.subset(font)
        out = io.BytesIO()
        font.save(out)
        (fonts / file).write_bytes(out.getvalue())
        rows.append((file, path, pinned, torture.sha256(out.getvalue()), len(data), len(out.getvalue()), family))
    for file, (path, pinned) in cfg["licenses"].items():
        (fonts / file).write_bytes(torture.fetch(path, pinned, cache))
    ranges = " ∪ ".join(f"U+{lo:04X}–{hi:04X}" if lo != hi else f"U+{lo:04X}" for lo, hi in torture.EXTRA_RANGES)
    lines = [
        f"# Benchmark {name.upper()} fonts: provenance",
        "",
        f"Built by `scripts/build_bench_fonts.py` (fontTools {torture.FONTTOOLS}) from "
        f"[google/fonts](https://github.com/google/fonts) at commit `{torture.COMMIT}`. Do not edit by hand; rerun the script.",
        "",
        "| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | Theme family |",
        "|---|---|---|---|---|---|",
    ]
    for file, path, up, sub_hash, n_up, n_sub, family in rows:
        lines.append(f"| `{file}` | `{path}` | `{up[:16]}…` | `{sub_hash[:16]}…` | {n_up // 1024} KB → {n_sub // 1024} KB | `{family}` |")
    lines += [
        "",
        f"**Subset:** every character in `deck.json` ∪ {ranges}. All OpenType layout features, all name records, "
        "variations and hinting kept.",
        "",
        "**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file "
        "(sha256-pinned in the script).",
        "",
    ]
    (fonts / "SOURCES.md").write_text("\n".join(lines), encoding="utf-8")
    print(f"built {len(rows)} fonts into {fonts.relative_to(ROOT)}")


def check(name: str, deck: dict, theme: dict) -> bool:
    from fontTools.ttLib import TTFont

    bundle = BENCH[name]["bundle"]
    families = theme["type"]["families"]
    roles = theme["type"]["roles"]
    cmaps = {key: TTFont(bundle / fam["file"], lazy=True).getBestCmap() for key, fam in families.items()}
    texts = []  # (node id, text, role)
    for nid, node in deck["nodes"].items():
        if node["type"] != "text":
            continue
        texts.append((nid, node["text"], node["role"]))
        for state in deck["states"]:
            delta = state.get("props", {}).get(nid, {})
            if "text" in delta:
                texts.append((nid, delta["text"], delta.get("role", node["role"])))
    ok, used = True, set()
    for nid, text, role in texts:
        family = roles[role]["family"]
        used.add(family)
        for ch in text:
            if ch == "\n" or ord(ch) in torture.IGNORABLE:
                continue
            if ord(ch) not in cmaps[family]:
                ok = False
                print(f"error: {nid}: U+{ord(ch):04X} {unicodedata.name(ch, '?')} is not in `{family}` (role `{role}`)")
    if used != set(families):
        ok = False
        print(f"error: the deck sets text in {sorted(used)}, but its theme has {sorted(families)}")
    print(f"{name} coverage: {len(texts)} strings in {len(used)} families {'ok' if ok else 'FAILED'}")
    return ok


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check coverage only; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    args = ap.parse_args()
    ok = True
    for name, cfg in BENCH.items():
        deck = json.loads((cfg["bundle"] / "deck.json").read_text(encoding="utf-8"))
        theme = json.loads((cfg["bundle"] / "theme.json").read_text(encoding="utf-8"))
        if not args.check:
            build(name, deck, args.cache)
        ok &= check(name, deck, theme)
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

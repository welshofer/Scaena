#!/usr/bin/env python3
"""Build and check the fonts of the benchmark and example bundles (SPEC §15, PLAN 0.14, 1.2).

The same pinned google/fonts commit, sha256 verification, and subset options as the
torture deck's fonts (scripts/build_torture_fonts.py, whose helpers this reuses).
Each font is subset to every character its deck uses plus ASCII, Latin-1, Latin
Extended-A, and common punctuation, which is what saving a bundle will do (SPEC §3.1).
The example decks are bundles like any other: `scaena validate` and `scaena render`
work on them as they stand.

Check (no network; CI runs this): every character of every text node's text, in
every state, maps in its role's own family, under each theme the bundle uses. A
benchmark must also set text in every family it ships: one that silently fell back
would time the wrong font.

    pip install fonttools==4.66.1
    python3 scripts/build_bundle_fonts.py           # rebuild fonts, then check
    python3 scripts/build_bundle_fonts.py --cache ~/.cache/scaena-fonts
    python3 scripts/build_bundle_fonts.py --check   # check only
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
FRAUNCES = (
    "ofl/fraunces/Fraunces[SOFT,WONK,opsz,wght].ttf",
    "177ff6c0f14e5550a3c624247cd1189611d4eb65d000b14944c63d967958abbb",
    "display",
)
INTER = ("ofl/inter/Inter[opsz,wght].ttf", "29160a80ff49ddcab2c97711247e08b1fab27a484a329ce8b813d820dc559031", "body")
JETBRAINS_MONO = (
    "ofl/jetbrainsmono/JetBrainsMono[wght].ttf",
    "48715a42ec242c21e9f02692891e147d022299a52e48d5e413e1a942193ffeda",
    "mono",
)
LICENSES = {
    "OFL-Fraunces.txt": ("ofl/fraunces/OFL.txt", "bdf4c22802eaf804f998195871c6b8938aac2ac14b2d78a8bd66a6f1eced833b"),
    "OFL-Inter.txt": ("ofl/inter/OFL.txt", "5b9321a4298cfeb6b34354164a1c3afc3db114569984c502b9b35d988fd58c57"),
    "OFL-JetBrainsMono.txt": ("ofl/jetbrainsmono/OFL.txt", "b2fe5e8987594e9ffd1d2ca52a2f5d73eb8335243893c5d6254b5ad69269591d"),
}
# The example decks' themes (Dusk, Daybreak) set display, body, and mono in these three.
EXAMPLE_FONTS = {"Fraunces-VF.ttf": FRAUNCES, "Inter-VF.ttf": INTER, "JetBrainsMono-VF.ttf": JETBRAINS_MONO}
BUNDLES = {
    "b1": {
        "title": "Benchmark B1",
        "bundle": ROOT / "tests/bench/b1.scaena",
        "deck": "deck.json",
        "every_family": True,
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
            **LICENSES,
            "OFL-SourceSerif4.txt": ("ofl/sourceserif4/OFL.txt", "5f94c3fd3a23131a417ab5a0c8452de57e70c3cfb9f604d88241f7065ebf9fd9"),
        },
    },
    "revenue": {
        "title": "Example deck",
        "bundle": ROOT / "docs/examples",
        "deck": "revenue.deck.json",
        "every_family": False,
        "fonts": EXAMPLE_FONTS,
        "licenses": LICENSES,
    },
    "authorability": {
        "title": "Authorability spike deck",
        "bundle": ROOT / "docs/examples/authorability",
        "deck": "deck.json",
        # Edit (e) moved the deck from Dusk to Daybreak; both must set its text.
        "themes": ["themes/daybreak.theme.json", "themes/dusk.theme.json"],
        "every_family": False,
        "fonts": EXAMPLE_FONTS,
        "licenses": LICENSES,
    },
}


def build(name: str, deck: dict, cache: Path | None) -> None:
    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    if fontTools.version != torture.FONTTOOLS:
        print(f"warning: fontTools {fontTools.version}, pinned {torture.FONTTOOLS}; subset bytes may differ", file=sys.stderr)
    cfg = BUNDLES[name]
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
        f"# {cfg['title']} fonts: provenance",
        "",
        f"Built by `scripts/build_bundle_fonts.py` (fontTools {torture.FONTTOOLS}) from "
        f"[google/fonts](https://github.com/google/fonts) at commit `{torture.COMMIT}`. Do not edit by hand; rerun the script.",
        "",
        "| Bundle file | Upstream path | Upstream sha256 | Subset sha256 | Upstream → subset | Theme family |",
        "|---|---|---|---|---|---|",
    ]
    for file, path, up, sub_hash, n_up, n_sub, family in rows:
        lines.append(f"| `{file}` | `{path}` | `{up[:16]}…` | `{sub_hash[:16]}…` | {n_up // 1024} KB → {n_sub // 1024} KB | `{family}` |")
    lines += [
        "",
        f"**Subset:** every character in `{cfg['deck']}` ∪ {ranges}. All OpenType layout features, all name records, "
        "variations and hinting kept.",
        "",
        "**Licenses:** SIL Open Font License 1.1; each `OFL-*.txt` beside the fonts is the upstream license file "
        "(sha256-pinned in the script).",
        "",
    ]
    (fonts / "SOURCES.md").write_text("\n".join(lines), encoding="utf-8")
    print(f"built {len(rows)} fonts into {fonts.relative_to(ROOT)}")


def check(name: str, deck: dict, theme: dict, label: str) -> bool:
    from fontTools.ttLib import TTFont

    cfg = BUNDLES[name]
    bundle = cfg["bundle"]
    families = theme["type"]["families"]
    roles = theme["type"]["roles"]
    cmaps = {key: TTFont(bundle / fam["file"], lazy=True).getBestCmap() for key, fam in families.items()}
    texts = []  # (node id, text, role)
    for nid, node in deck["nodes"].items():
        if node["type"] != "text":
            continue
        role = node.get("role", "body")
        texts += [(nid, text, run_role or role) for text, run_role in strings(node)]
        for state in deck["states"]:
            delta = state.get("props", {}).get(nid, {})
            role = delta.get("role", role)
            texts += [(nid, text, run_role or role) for text, run_role in strings(delta)]
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
    if cfg["every_family"] and used != set(families):
        ok = False
        print(f"error: the deck sets text in {sorted(used)}, but its theme has {sorted(families)}")
    print(f"{label} coverage: {len(texts)} strings in {len(used)} families {'ok' if ok else 'FAILED'}")
    return ok


def strings(props: dict):
    """A text node's (or delta's) strings, with the role a run sets, if any."""
    if "text" in props:
        yield props["text"], None
    for run in props.get("runs", []):
        yield run["text"], run.get("role")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check coverage only; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    args = ap.parse_args()
    ok = True
    for name, cfg in BUNDLES.items():
        deck = json.loads((cfg["bundle"] / cfg["deck"]).read_text(encoding="utf-8"))
        if not args.check:
            build(name, deck, args.cache)
        for theme_path in cfg.get("themes", [deck["theme"]]):
            theme = json.loads((cfg["bundle"] / theme_path).read_text(encoding="utf-8"))
            ok &= check(name, deck, theme, f"{name} ({theme_path})")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

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
    python3 scripts/build_bundle_fonts.py --only b2 --only b3   # rebuild two, check all
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
# Each family's italic face (PLAN 2.40), which the themes that ship name.
FRAUNCES_ITALIC = (
    "ofl/fraunces/Fraunces-Italic[SOFT,WONK,opsz,wght].ttf",
    "b24448c43702fac4ee856781d461a0dfba8d8e594b6e8e190234b75fed2c0e01",
    "display, italic",
)
INTER_ITALIC = (
    "ofl/inter/Inter-Italic[opsz,wght].ttf",
    "acd98e64795781b2058f07b18475e0ecee2a0fe2b42a49e2f9e37d0d6bf66ce6",
    "body, italic",
)
JETBRAINS_MONO_ITALIC = (
    "ofl/jetbrainsmono/JetBrainsMono-Italic[wght].ttf",
    "85ae2a5cd3f56baf1ce1c21a851322c58e3d8fbe8e8ad4a4d090a820dd7fe558",
    "mono, italic",
)
ROMANS = {"Fraunces-VF.ttf": FRAUNCES, "Inter-VF.ttf": INTER, "JetBrainsMono-VF.ttf": JETBRAINS_MONO}
# The example decks' themes (Dusk, Daybreak, Ember) set display, body, and mono in these three,
# each with its italic.
EXAMPLE_FONTS = {
    **ROMANS,
    "Fraunces-Italic-VF.ttf": FRAUNCES_ITALIC,
    "Inter-Italic-VF.ttf": INTER_ITALIC,
    "JetBrainsMono-Italic-VF.ttf": JETBRAINS_MONO_ITALIC,
}
# The benchmark decks' theme sets the three romans, and adds Source Serif 4, for quotations.
B1_FONTS = {  # bundle file -> (google/fonts path, upstream sha256, theme family)
    **ROMANS,
    "SourceSerif4-VF.ttf": (
        "ofl/sourceserif4/SourceSerif4[opsz,wght].ttf",
        "97b2d4da6e3cb494b5a1e66ae176914d852ccabef49e0c02c0df25f3e39aca0b",
        "text",
    ),
}
B1_LICENSES = {
    **LICENSES,
    "OFL-SourceSerif4.txt": ("ofl/sourceserif4/OFL.txt", "5f94c3fd3a23131a417ab5a0c8452de57e70c3cfb9f604d88241f7065ebf9fd9"),
}
BUNDLES = {
    "b1": {
        "title": "Benchmark B1",
        "bundle": ROOT / "tests/bench/b1.scaena",
        "deck": "deck.json",
        "every_family": True,
        "fonts": B1_FONTS,
        "licenses": B1_LICENSES,
    },
    # B2 (charts) and B3 (shaders) share B1's theme and fonts (scripts/build_bench_decks.py).
    **{
        name: {
            "title": f"Benchmark {name.upper()}",
            "bundle": ROOT / f"tests/bench/{name}.scaena",
            "deck": "deck.json",
            "every_family": True,
            "fonts": B1_FONTS,
            "licenses": B1_LICENSES,
        }
        for name in ["b2", "b3"]
    },
    "revenue": {
        "title": "Example decks'",
        "bundle": ROOT / "docs/examples",
        "deck": "revenue.deck.json",
        # Decks beside it that share its fonts: the subset covers them all, and each is checked.
        "also": ["trails.deck.json", "higher-ed.deck.json"],
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


def build(name: str, decks: list[dict], cache: Path | None) -> None:
    import fontTools
    from fontTools import subset
    from fontTools.ttLib import TTFont

    if fontTools.version != torture.FONTTOOLS:
        print(f"warning: fontTools {fontTools.version}, pinned {torture.FONTTOOLS}; subset bytes may differ", file=sys.stderr)
    cfg = BUNDLES[name]
    fonts = cfg["bundle"] / "fonts"
    unicodes = {ord(c) for deck in decks for s in torture.deck_strings(deck) for c in s}
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
        f"**Subset:** every character in {' ∪ '.join(f'`{d}`' for d in [cfg['deck'], *cfg.get('also', [])])} ∪ {ranges}. "
        "All OpenType layout features, all name records, "
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
    ap.add_argument("--only", action="append", choices=list(BUNDLES), help="build only this bundle (repeatable); all are checked")
    args = ap.parse_args()
    ok = True
    for name, cfg in BUNDLES.items():
        files = [cfg["deck"], *cfg.get("also", [])]
        decks = [json.loads((cfg["bundle"] / f).read_text(encoding="utf-8")) for f in files]
        if not args.check and (not args.only or name in args.only):
            build(name, decks, args.cache)
        for file, deck in zip(files, decks):
            label = name if file == cfg["deck"] else file.removesuffix(".deck.json")
            for theme_path in cfg.get("themes", [deck["theme"]]):
                theme = json.loads((cfg["bundle"] / theme_path).read_text(encoding="utf-8"))
                ok &= check(name, deck, theme, f"{label} ({theme_path})")
    sys.exit(0 if ok else 1)


if __name__ == "__main__":
    main()

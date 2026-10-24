"""Prototype: derive a 9:16 arrangement for a theme's 16:9 layouts (scratch, not the build).

The portrait grid is 12 columns by 24 rows. Slots that span most of the width keep their place,
their rows doubled. The rest are grouped into columns (bands) by their column ranges; the bands
stack top to bottom in the rows the narrow slots took, each an equal share, and each slot is
mapped into its band's share in proportion, its columns widened to the band's full width.

    python3 derive.py THEME.json OUT.json [layout ...]
"""

import json
import math
import sys

COLS, ROWS = 12, 24
WIDE = 10  # a slot this many columns wide or more keeps its place


def rng(v):
    return (v[0], v[1]) if isinstance(v, list) else (v, v)


def bands(slots):
    out = []
    for name, s in sorted(slots.items(), key=lambda kv: rng(kv[1]["col"])):
        a, b = rng(s["col"])
        if out and a <= out[-1]["col"][1]:
            out[-1]["col"] = (out[-1]["col"][0], max(out[-1]["col"][1], b))
            out[-1]["slots"].append(name)
        else:
            out.append({"col": (a, b), "slots": [name]})
    return out


def doubled(row):
    a, b = rng(row)
    return [2 * a - 1, 2 * b]


def derive(layout):
    slots = layout["slots"]
    placed = {k: s for k, s in slots.items() if "col" in s and "row" in s}
    wide = {k for k, s in placed.items() if rng(s["col"])[1] - rng(s["col"])[0] + 1 >= WIDE}
    narrow = {k: placed[k] for k in placed if k not in wide}
    out = {}
    for k in wide:
        s = placed[k]
        out[k] = {**s, "row": doubled(s["row"])}
    # The foot (rows 11 and 12): a caption or a source there keeps its place, its rows doubled,
    # so it is not taken into the band of the figure above it.
    for k in [k for k, s in narrow.items() if rng(s["row"])[0] >= 11]:
        s = narrow.pop(k)
        out[k] = {**s, "row": doubled(s["row"])}
    # The rest: bands of columns, stacked, two to a row from four on.
    if narrow:
        r0 = min(rng(s["row"])[0] for s in narrow.values())
        r1 = max(rng(s["row"])[1] for s in narrow.values())
        top, height = 2 * r0 - 1, 2 * (r1 - r0 + 1)
        bs = bands(narrow)
        across = 2 if len(bs) >= 4 else 1
        down = math.ceil(len(bs) / across)
        share = [height // down + (1 if i < height % down else 0) for i in range(down)]
        for i, band in enumerate(bs):
            line, place = divmod(i, across)
            h = share[line]
            at = top + sum(share[:line])
            left, right = place * COLS // across + 1, (place + 1) * COLS // across
            c0, c1 = band["col"]
            w = c1 - c0 + 1
            # The band's own rows, so a band that sat low in the 16:9 room fills its share.
            b0 = min(rng(narrow[k]["row"])[0] for k in band["slots"])
            b1 = max(rng(narrow[k]["row"])[1] for k in band["slots"])
            bh = b1 - b0 + 1
            span = right - left + 1
            for k in band["slots"]:
                s = narrow[k]
                a, b = rng(s["col"])
                ra, rb = rng(s["row"])
                col = [left + math.floor((a - c0) / w * span), left + math.ceil((b - c0 + 1) / w * span) - 1]
                row = [at + math.floor((ra - b0) / bh * h), at + math.ceil((rb - b0 + 1) / bh * h) - 1]
                out[k] = {**s, "col": col, "row": [max(1, row[0]), min(ROWS, row[1])]}
    return {"slots": {k: out[k] for k in slots if k in out}}


def tiers(slots):
    """The slots grouped by the rows they share, top to bottom."""
    out = []
    for k, s in sorted(slots.items(), key=lambda kv: rng(kv[1]["row"])):
        a, b = rng(s["row"])
        if out and a <= out[-1][1]:
            out[-1] = (out[-1][0], max(out[-1][1], b), out[-1][2] + [k])
        else:
            out.append((a, b, [k]))
    return [t[2] for t in out]


def main():
    src, dst, *names = sys.argv[1:]
    theme = json.load(open(src))
    margin = theme["grid"]["margin"]
    m = rng(margin) if isinstance(margin, list) else (margin, margin)
    # The portrait grid: the landscape margins turned, the same gutter and baseline.
    theme["formats"] = {"9:16": {"grid": {**theme["grid"], "columns": COLS, "rows": ROWS, "margin": [m[1], max(48, m[0] * 3 // 4)]}}}
    for name, layout in theme["layouts"].items():
        if names and name not in names:
            continue
        layout["formats"] = {"9:16": derive(layout)}
    json.dump(theme, open(dst, "w"), ensure_ascii=False, indent=1)


if __name__ == "__main__":
    main()

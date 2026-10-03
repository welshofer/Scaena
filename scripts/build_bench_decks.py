#!/usr/bin/env python3
"""Build and check the benchmark decks B2 and B3 (SPEC §15, PLAN 1.24).

B2 is chart-heavy: 12 states, six charts, each shown and then updated by key (the next
quarter, the next months, the next year), so every second state is a data morph. B3 is
shader-heavy: 8 states, each over a full-bleed mesh with grain laid over it, the mesh's
seed and palette changing from state to state so every transition moves its uniforms.

Both use B1's theme and fonts (`scripts/build_bundle_fonts.py` subsets them to each
deck's text), and set text in every family the theme ships, so no role silently falls
back. The data is invented, round, and deterministic.

    python3 scripts/build_bench_decks.py           # write tests/bench/b2.scaena and b3.scaena
    python3 scripts/build_bench_decks.py --check   # check the committed ones match
"""

import argparse
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
BENCH = ROOT / "tests" / "bench"
VERSION = json.loads((BENCH / "b1.scaena" / "deck.json").read_text())["scaena"]


def csv(rows: list[dict]) -> str:
    keys = list(rows[0])
    lines = [",".join(keys)] + [",".join(str(r[k]) for k in keys) for r in rows]
    return "\n".join(lines) + "\n"


QUARTERS = ["2025-Q1", "2025-Q2", "2025-Q3", "2025-Q4", "2026-Q1", "2026-Q2", "2026-Q3"]
PRODUCTS = ["Core", "Pro", "Enterprise"]
MONTHS = [f"2026-{m:02d}" for m in range(1, 13)] + ["2027-01", "2027-02", "2027-03"]
REGIONS = ["Americas", "Europe", "Asia Pacific", "Middle East", "Africa"]
ACCOUNTS = ["Acme", "Globex", "Initech", "Umbrella", "Hooli", "Stark", "Wayne", "Tyrell", "Cyberdyne", "Soylent"]


def b2_data() -> dict[str, list[dict]]:
    revenue = {q: 28 + 6 * i + (3 if i % 2 else 0) for i, q in enumerate(QUARTERS)}
    mix = {
        (q, p): round(revenue[q] * share, 1)
        for i, q in enumerate(QUARTERS)
        for p, share in zip(PRODUCTS, [0.62 - 0.03 * i, 0.22 + 0.02 * i, 0.16 + 0.01 * i])
    }
    users = {(m, s): base + step * i for i, m in enumerate(MONTHS) for s, base, step in [("Web", 120, 9), ("Mobile", 80, 14)]}
    share = {(m, s): v for i, m in enumerate(MONTHS) for s, v in zip(["Ours", "Rival"], [30 + i, 70 - i])}
    regions_now = dict(zip(REGIONS, [42, 27, 18, 8, 5]))
    regions_next = dict(zip(REGIONS, [38, 26, 22, 9, 5]))
    accounts = [
        {"account": a, "growth": 4 + (i * 7) % 31, "margin": 12 + (i * 11) % 29, "revenue": 3 + (i * 5) % 17}
        for i, a in enumerate(ACCOUNTS)
    ]
    accounts_next = [
        {**r, "growth": r["growth"] + (5 if i % 3 == 0 else -2), "margin": r["margin"] + (3 if i % 2 else -1)}
        for i, r in enumerate(accounts)
    ]
    return {
        "revenue": [{"quarter": q, "revenue": revenue[q]} for q in QUARTERS[:6]],
        "revenue-next": [{"quarter": q, "revenue": revenue[q]} for q in QUARTERS[1:]],
        "mix": [{"quarter": q, "product": p, "revenue": mix[(q, p)]} for q in QUARTERS[:6] for p in PRODUCTS],
        "mix-next": [{"quarter": q, "product": p, "revenue": mix[(q, p)]} for q in QUARTERS[1:] for p in PRODUCTS],
        "users": [{"month": m, "segment": s, "users": users[(m, s)]} for m in MONTHS[:12] for s in ["Web", "Mobile"]],
        "users-next": [{"month": m, "segment": s, "users": users[(m, s)]} for m in MONTHS[3:] for s in ["Web", "Mobile"]],
        "share": [{"month": m, "brand": s, "share": share[(m, s)]} for m in MONTHS[:12] for s in ["Ours", "Rival"]],
        "share-next": [{"month": m, "brand": s, "share": share[(m, s)]} for m in MONTHS[3:] for s in ["Ours", "Rival"]],
        "regions": [{"region": r, "revenue": v} for r, v in regions_now.items()],
        "regions-next": [{"region": r, "revenue": v} for r, v in regions_next.items()],
        "accounts": accounts,
        "accounts-next": accounts_next,
    }


SCHEMAS = {
    "revenue": {"quarter": "string", "revenue": "number"},
    "mix": {"quarter": "string", "product": "string", "revenue": "number"},
    "users": {"month": "string", "segment": "string", "users": "number"},
    "share": {"month": "string", "brand": "string", "share": "number"},
    "regions": {"region": "string", "revenue": "number"},
    "accounts": {"account": "string", "growth": "number", "margin": "number", "revenue": "number"},
}

# (chart id, its spec, headline before, headline after, caption, the quote or code it carries)
CHARTS = [
    (
        "revenue",
        {
            "kind": "bar",
            "x": {"field": "quarter", "type": "ordinal"},
            "y": {"field": "revenue", "type": "quantitative", "format": "$,.0f", "domain": [0, None]},
            "labels": {"show": "all"},
        },
        "Revenue grew every quarter",
        "And the next quarter grew again",
        "Revenue, $M, by quarter.",
    ),
    (
        "mix",
        {
            "kind": "stackedBar",
            "x": {"field": "quarter", "type": "ordinal"},
            "y": {"field": "revenue", "type": "quantitative", "format": "$,.0f", "domain": [0, None]},
            "series": {"field": "product"},
            "color": {"field": "product", "scale": "categorical"},
            "legend": "direct",
        },
        "Pro took a bigger share",
        "The shift held into Q3",
        "Revenue, $M, by product and quarter.",
    ),
    (
        "users",
        {
            "kind": "line",
            "x": {"field": "month", "type": "ordinal"},
            "y": {"field": "users", "type": "quantitative", "format": ",.0f", "domain": [0, None]},
            "series": {"field": "segment"},
            "color": {"field": "segment", "scale": "categorical"},
            "legend": "direct",
        },
        "Mobile caught up with the web",
        "Three months on, it passed it",
        "Monthly active users, thousands.",
    ),
    (
        "share",
        {
            "kind": "area",
            "x": {"field": "month", "type": "ordinal"},
            "y": {"field": "share", "type": "quantitative", "format": ".0f", "domain": [0, 100]},
            "series": {"field": "brand"},
            "color": {"field": "brand", "scale": "categorical"},
            "legend": "direct",
        },
        "Our share rose a point a month",
        "And kept rising",
        "Share of category sales, %.",
    ),
    (
        "regions",
        {
            "kind": "donut",
            "x": {"field": "region", "type": "nominal"},
            "y": {"field": "revenue", "type": "quantitative", "format": ".0f"},
            "color": {"field": "region", "scale": "categorical"},
            "legend": "direct",
        },
        "The Americas are still most of it",
        "Asia Pacific is closing in",
        "Revenue by region, %.",
    ),
    (
        "accounts",
        {
            "kind": "scatter",
            "x": {"field": "growth", "type": "quantitative", "format": ".0f", "title": "Growth, %"},
            "y": {"field": "margin", "type": "quantitative", "format": ".0f", "title": "Margin, %"},
            "sizeEncoding": {"field": "revenue"},
            "key": "account",
            "labels": {"show": "all", "collide": "hide"},
        },
        "Growth and margin rose together",
        "Next quarter, most moved up and right",
        "Top ten accounts: growth against margin, sized by revenue.",
    ),
]


def b2() -> tuple[dict, dict[str, str]]:
    data = b2_data()
    nodes = {
        "headline": {
            "type": "text",
            "role": "headline",
            "text": CHARTS[0][2],
            "fit": "shrink",
            "semantic": "claim",
            "at": {"in": "header"},
        },
    }
    for cid, spec, *_ in CHARTS:
        nodes[cid] = {
            "type": "chart",
            **spec,
            "data": f"@{cid}",
            "alt": "",
            "semantic": "evidence",
            "at": {"in": "main"},
        }
    nodes["caption"] = {"type": "text", "role": "caption", "text": CHARTS[0][4], "semantic": "source", "at": {"in": "footer"}}
    nodes["source"] = {"type": "text", "role": "code", "text": "data/revenue.csv", "semantic": "source", "at": {"in": "source"}}
    nodes["quote"] = {
        "type": "text",
        "role": "quote",
        "text": "“We stopped guessing.”",
        "fit": "shrink",
        "semantic": "context",
        "at": {"in": "aside"},
    }
    states = []
    for i, (cid, spec, before, after, caption) in enumerate(CHARTS):
        previous = CHARTS[i - 1][0] if i else None
        first = {
            "id": cid,
            "layout": "figure",
            "transition": "standard",
            "props": {
                "headline": {"text": before},
                cid: {"alt": f"{caption} {before}."},
                "caption": {"text": caption},
                "source": {"text": f"data/{cid}.csv"},
            },
            "hold": 3000,
        }
        if previous:
            first["remove"] = [previous]
        if i == 0:
            first["props"]["quote"] = {}
        if i == 1:
            first["remove"].append("quote")
        second = {
            "id": f"{cid}-next",
            "slide": cid,
            "transition": "standard",
            "props": {
                "headline": {"text": after},
                cid: {"data": f"@{cid}-next", "alt": f"{caption} {after}."},
                "source": {"text": f"data/{cid}-next.csv"},
            },
            "hold": 3000,
        }
        states += [first, second]
    deck = {
        "scaena": VERSION,
        "meta": {
            "title": "B2: six charts, each updated",
            "lang": "en-US",
            "description": "SPEC §15 benchmark B2: chart-heavy, 12 states, six charts with key morphs.",
        },
        "canvas": {"width": 1920, "height": 1080},
        "theme": "theme.json",
        "fonts": json.loads((BENCH / "b1.scaena" / "deck.json").read_text())["fonts"],
        "data": {
            name: {"source": f"data/{name}.csv", "schema": SCHEMAS[name.removesuffix("-next")]} for name in data
        },
        "nodes": nodes,
        "states": states,
    }
    files = {f"data/{name}.csv": csv(rows) for name, rows in data.items()}
    return deck, files


PALETTES = ["ambient", "ember"]
# A seed a state: each puts the mesh's bright points where the text stays readable on it.
SEEDS = [1, 2, 3, 9, 5, 6, 7, 8]
B3_LINES = [
    ("Light that moves", "Eight states over a mesh that never rests."),
    ("Every frame is shaded", "A full-bleed mesh, with grain laid over it."),
    ("The seed changes", "So the field reshapes between states."),
    ("The palette changes", "So its colors travel through Oklab."),
    ("Grain at 24 frames a second", "Film's rate, on every state."),
    ("Text sits on top", "And has to stay readable on all of it."),
    ("Nothing here is a picture", "Each frame is computed from its time."),
    ("That is the benchmark", "Shaders on every state, nothing cached away."),
]


def b3() -> dict:
    nodes = {
        "mesh": {
            "type": "shader",
            "kind": "mesh",
            "seed": 1,
            "palette": "ambient",
            "params": {"points": 6, "drift": 0.14, "softness": 0.85, "grain": 0.02},
            "at": {"in": "canvas"},
            "z": -100,
            "alt": "",
            "semantic": "decoration",
        },
        "grain": {
            "type": "shader",
            "kind": "grain",
            "palette": "ambient",
            "params": {"amount": 0.06, "fps": 24},
            "at": {"in": "canvas"},
            "z": -90,
            "blend": "overlay",
            "alt": "",
            "semantic": "decoration",
        },
        "title": {"type": "text", "role": "display", "text": B3_LINES[0][0], "semantic": "claim", "at": {"in": "title"}},
        "subtitle": {"type": "text", "role": "title", "text": B3_LINES[0][1], "semantic": "context", "at": {"in": "subtitle"}},
        "kicker": {"type": "text", "role": "code", "text": "mesh + grain", "semantic": "navigation", "at": {"col": [1, 4], "row": 1}},
        "quote": {
            "type": "text",
            "role": "quote",
            "text": "“A frame is a function of time.”",
            "semantic": "context",
            "at": {"col": [6, 12], "row": 1},
        },
    }
    states = []
    for i, (title, subtitle) in enumerate(B3_LINES):
        props = {
            "mesh": {"seed": SEEDS[i], "palette": PALETTES[i % 2]},
            "title": {"text": title},
            "subtitle": {"text": subtitle},
        }
        if i == 0:
            props = {k: {} for k in ["mesh", "grain", "title", "subtitle", "kicker", "quote"]}
        state = {"id": f"s{i + 1}", "layout": "title", "props": props, "hold": 2500}
        if i:
            state["transition"] = "slow"
        states.append(state)
    return {
        "scaena": VERSION,
        "meta": {
            "title": "B3: shaders on every state",
            "lang": "en-US",
            "description": "SPEC §15 benchmark B3: shader-heavy, 8 states, a full-bleed mesh with grain over it on every state.",
        },
        "canvas": {"width": 1920, "height": 1080},
        "theme": "theme.json",
        "fonts": json.loads((BENCH / "b1.scaena" / "deck.json").read_text())["fonts"],
        "nodes": nodes,
        "states": states,
    }


# B2's one layout beyond B1's theme: a headline over a figure, its caption and source below.
FIGURE = {
    "description": "A headline over one figure, its caption and its source below.",
    "slots": {
        "header": {"col": [1, 9], "row": 1, "role": "headline"},
        "aside": {"col": [10, 12], "row": 1, "role": "quote"},
        "main": {"col": [1, 12], "row": [2, 5]},
        "footer": {"col": [1, 8], "row": 6, "role": "caption"},
        "source": {"col": [9, 12], "row": 6, "role": "code"},
    },
}


def outputs(bench: Path) -> dict[Path, str]:
    """Every file the script writes, by path, and its text."""
    theme = json.loads((BENCH / "b1.scaena" / "theme.json").read_text())
    b2_theme = {
        **theme,
        "name": "B2 bench",
        "description": "B1's theme with a `figure` layout: the SPEC §15 B2 benchmark theme. "
        "Written by scripts/build_bench_decks.py.",
        "layouts": {**theme["layouts"], "figure": FIGURE},
    }
    b3_theme = {
        **theme,
        "name": "B3 bench",
        "description": "B1's theme: the SPEC §15 B3 benchmark theme. Written by scripts/build_bench_decks.py.",
    }
    b2_deck, b2_files = b2()

    def dump(value) -> str:
        return json.dumps(value, indent=2, ensure_ascii=False) + "\n"

    out = {
        bench / "b2.scaena" / "deck.json": dump(b2_deck),
        bench / "b2.scaena" / "theme.json": dump(b2_theme),
        bench / "b3.scaena" / "deck.json": dump(b3()),
        bench / "b3.scaena" / "theme.json": dump(b3_theme),
    }
    out.update({bench / "b2.scaena" / rel: text for rel, text in b2_files.items()})
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="check the committed decks match, write nothing")
    parser.add_argument("--out", type=Path, default=BENCH, help="where to write (default tests/bench)")
    args = parser.parse_args()
    files = outputs(args.out)
    if args.check:
        stale = [p for p, text in files.items() if not p.exists() or p.read_text() != text]
        if stale:
            sys.exit("stale, rerun scripts/build_bench_decks.py: " + ", ".join(str(p.relative_to(ROOT)) for p in stale))
        print(f"bench decks: {len(files)} files match")
        return
    for path, text in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text)
    print(f"wrote {len(files)} files under {args.out}")


if __name__ == "__main__":
    main()

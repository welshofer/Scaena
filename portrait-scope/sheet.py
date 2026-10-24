"""Contact sheet: each layout started on a slide, drawn in 9:16 by the theme as it ships and by the
theme with derived portrait slots, side by side, its 16:9 drawing first.

    python3 sheet.py THEME_SLUG OUT.png layout ...
"""

import json
import shutil
import subprocess
import sys
from pathlib import Path

from PIL import Image, ImageDraw

CLI = "target/debug/scaena"
WORK = Path("target/portrait-scope")


def run(*args):
    p = subprocess.run([CLI, "--json", *args], capture_output=True, text=True)
    out = json.loads(p.stdout) if p.stdout.strip() else {}
    if p.returncode or "error" in out:
        raise SystemExit(f"{args}: {p.stdout} {p.stderr}")
    return out


def bundle(slug, layouts, theme_file=None):
    d = WORK / (slug + ("-derived" if theme_file else "-shipped"))
    shutil.rmtree(d, ignore_errors=True)
    run("new", str(d), "--theme", slug, "--title", "Sheet")
    if theme_file:
        shutil.copy(theme_file, d / "themes" / f"{slug}.theme.json")
    deck = json.load(open(d / "deck.json"))
    deck["formats"] = ["9:16"]
    json.dump(deck, open(d / "deck.json", "w"), indent=2)
    states, after = [], deck["states"][0]["id"]
    for name in layouts:
        before = {s["id"] for s in json.load(open(d / "deck.json"))["states"]}
        run("start", str(d), after, "--layout", name)
        new = [s["id"] for s in json.load(open(d / "deck.json"))["states"] if s["id"] not in before]
        after = new[0]
        states.append(after)
    return d, states


def draw(d, state, fmt=None):
    out = d / f"{state}-{fmt or 'own'}.png".replace(":", "x")
    run("render", str(d), "--state", state, "--out", str(out), *(["--format", fmt] if fmt else []))
    return Image.open(out).convert("RGB")


def main():
    slug, out, *layouts = sys.argv[1:]
    src = Path(f"docs/examples/themes/{slug}.theme.json")
    derived = WORK / f"{slug}.derived.json"
    WORK.mkdir(parents=True, exist_ok=True)
    subprocess.run(["python3", "portrait-scope/derive.py", str(src), str(derived), *layouts], check=True)
    shipped, states = bundle(slug, layouts)
    mine, states2 = bundle(slug, layouts, derived)
    h = 480
    rows = []
    for name, s1, s2 in zip(layouts, states, states2):
        own = draw(shipped, s1)
        a = draw(shipped, s1, "9:16")
        b = draw(mine, s2, "9:16")
        tiles = [own.resize((h * own.width // own.height, h)), a.resize((h * a.width // a.height, h)), b.resize((h * b.width // b.height, h))]
        row = Image.new("RGB", (sum(t.width for t in tiles) + 40, h + 30), "white")
        x = 0
        for t in tiles:
            row.paste(t, (x, 30))
            x += t.width + 20
        ImageDraw.Draw(row).text((4, 8), f"{name}: 16:9 · 9:16 as shipped · 9:16 derived", fill="black")
        rows.append(row)
    sheet = Image.new("RGB", (max(r.width for r in rows), sum(r.height for r in rows)), "white")
    y = 0
    for r in rows:
        sheet.paste(r, (0, y))
        y += r.height
    sheet.save(out)


if __name__ == "__main__":
    main()

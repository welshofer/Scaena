#!/usr/bin/env python3
"""A contact sheet of a deck's state PNGs in play order, each labelled with its slide number and id.

  sheet.py BUNDLE PNGDIR OUT.png [--cols 3] [--width 640] [--states a,b,c]
"""
import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont


def main():
    bundle, pngdir, out = sys.argv[1], sys.argv[2], sys.argv[3]
    args = sys.argv[4:]
    cols, width, only = 3, 640, None
    i = 0
    while i < len(args):
        if args[i] == "--cols":
            cols = int(args[i + 1]); i += 2
        elif args[i] == "--width":
            width = int(args[i + 1]); i += 2
        elif args[i] == "--states":
            only = args[i + 1].split(","); i += 2
        else:
            i += 1
    deck = json.load(open(os.path.join(bundle, "deck.json")))
    states = [s["id"] for s in deck["states"]]
    slide_of = {}
    n = 0
    for s in deck["states"]:
        if not s.get("slide"):
            n += 1
        slide_of[s["id"]] = n
    if only:
        states = [s for s in states if s in only]
    tiles = []
    for sid in states:
        p = os.path.join(pngdir, f"{sid}.png")
        if not os.path.exists(p):
            continue
        im = Image.open(p).convert("RGB")
        h = int(im.height * width / im.width)
        im = im.resize((width, h))
        tiles.append((sid, im))
    if not tiles:
        raise SystemExit("no tiles")
    th = tiles[0][1].height
    label = 22
    rows = (len(tiles) + cols - 1) // cols
    sheet = Image.new("RGB", (cols * (width + 8) + 8, rows * (th + label + 8) + 8), (128, 128, 128))
    d = ImageDraw.Draw(sheet)
    try:
        font = ImageFont.truetype("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf", 15)
    except Exception:
        font = ImageFont.load_default()
    for k, (sid, im) in enumerate(tiles):
        r, c = divmod(k, cols)
        x, y = 8 + c * (width + 8), 8 + r * (th + label + 8)
        d.text((x, y), f"{slide_of.get(sid, '?')}. {sid}", fill=(255, 255, 255), font=font)
        sheet.paste(im, (x, y + label))
    sheet.save(out)
    print(out, sheet.size)


if __name__ == "__main__":
    main()

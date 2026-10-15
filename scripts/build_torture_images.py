#!/usr/bin/env python3
"""Build the torture deck's pictures: the test card (PLAN 1.7, torture case 25), one 480 x 240 RGBA
PNG, and the map card (ADR-0030, torture case 67), one 800 x 450 PNG drawn as a provider's map is.

The card is made to show what an image node's `fit`, `focal`, `crop`, and `radius` did, and
to make the painters disagree if they can:

- eight vertical bands, 60 px each, in eight distinct colors: which part of the image a
  `cover` or a `crop` kept is the bands you see;
- a white disc ringed in ink at the center, the default focal point;
- a corner mark in each corner, the top-left one in a color of its own, so a flip or a
  rotation shows and `contain` visibly keeps all four;
- a 2 px checkerboard in the top-left 60 x 30 px, the region the `zoom` case magnifies: a
  bilinear filter blurs it the same way in every painter or the parity harness says not;
- a half-transparent bottom strip (alpha 128): straight alpha in the file, so a painter that
  forgets to premultiply, or premultiplies twice, draws the wrong color there.

The map card stands in for a fetched map, which no test fetches: a pale ground, a park, a river
across it, streets every 100 px, a main road through the middle, a cross of ink at its middle,
where its place is and a pin's tip goes, and a block in its bottom-right corner where a service
writes its own credit, which Scaena's must keep clear of.

Pure Python (zlib, struct): no imaging library. The pixels are exact integers; the disc and
ring edges are 4 x 4 supersampled.

    python3 scripts/build_torture_images.py           # write the PNGs
    python3 scripts/build_torture_images.py --check   # the committed PNGs have these pixels

The committed file is the truth for its content id (`sha256:` of its bytes, in the goldens);
`--check` compares decoded pixels, so a zlib that compresses differently cannot fail it.
"""

import struct
import sys
import zlib
from pathlib import Path

ASSETS = Path(__file__).resolve().parent.parent / "tests/fixtures/torture.scaena/assets"
OUT = ASSETS / "test-card.png"
W, H = 480, 240
MAP = ASSETS / "map-card.png"
MW, MH = 800, 450

BANDS = ["#0F766E", "#4338CA", "#C2410C", "#F5C451", "#BE123C", "#15803D", "#0369A1", "#7E22CE"]
INK = (0x16, 0x14, 0x0F)
WHITE = (0xFF, 0xFF, 0xFF)
MARK_TL = (0xF5, 0xC4, 0x51)


def hex_rgb(h):
    return tuple(int(h[i : i + 2], 16) for i in (1, 3, 5))


def disc_cover(x, y, cx, cy, r):
    """The fraction of pixel (x, y) inside the circle at (cx, cy) of radius r, 4 x 4 samples."""
    inside = 0
    for j in range(4):
        for i in range(4):
            dx = x + (i + 0.5) / 4 - cx
            dy = y + (j + 0.5) / 4 - cy
            inside += dx * dx + dy * dy <= r * r
    return inside / 16


def mix(a, b, t):
    """a over b by coverage t, rounded to the nearest integer."""
    return tuple(int(round(bv + (av - bv) * t)) for av, bv in zip(a, b))


def pixels():
    rows = []
    cx, cy = W / 2, H / 2
    for y in range(H):
        row = []
        for x in range(W):
            rgb = hex_rgb(BANDS[x // 60])
            # The checkerboard: 2 px cells, top-left 60 x 30 px (inside the zoom crop).
            if 4 <= x < 56 and 4 <= y < 26:
                rgb = INK if ((x // 2) + (y // 2)) % 2 == 0 else WHITE
            # Corner marks, 12 px: the top-left one in a color of its own.
            if (x < 12 or x >= W - 12) and (y < 12 or y >= H - 12):
                rgb = MARK_TL if (x < 12 and y < 12) else INK
            # The focal disc: an ink ring (r 44) around a white disc (r 38).
            if abs(x + 0.5 - cx) < 46 and abs(y + 0.5 - cy) < 46:
                rgb = mix(INK, rgb, disc_cover(x, y, cx, cy, 44))
                rgb = mix(WHITE, rgb, disc_cover(x, y, cx, cy, 38))
            alpha = 128 if y >= H - 24 else 255
            row.append((*rgb, alpha))
        rows.append(row)
    return rows


def map_pixels():
    """The map card's pixels, every one opaque."""
    ground, park, river = (0xEE, 0xF0, 0xEC), (0xCF, 0xE3, 0xC4), (0xBB, 0xD6, 0xE8)
    street, curb, road = (0xFF, 0xFF, 0xFF), (0xD5, 0xD8, 0xD2), (0xF7, 0xE3, 0xB0)
    credit, words = (0xF8, 0xF8, 0xF6), (0x6B, 0x6E, 0x68)
    rows = []
    for y in range(MH):
        row = []
        for x in range(MW):
            rgb = ground
            if 80 <= x < 260 and 60 <= y < 200:
                rgb = park
            if abs(y - (0.35 * x + 250)) < 18:
                rgb = river
            # Streets every 100 px, 8 px wide with a 1 px curb each side, over the river as bridges.
            for at in (x % 100, y % 100):
                if 45 <= at < 55:
                    rgb = curb if at in (45, 54) else street
            # The main road through the middle.
            if 218 <= y < 232:
                rgb = road
            # The cross at the middle: where the place is.
            if (abs(x - 400) < 12 and 224 <= y < 226) or (abs(y - 225) < 12 and 399 <= x < 401):
                rgb = INK
            # The service's own credit, in the bottom-right corner, its words as blocks.
            if 610 <= x < 794 and 426 <= y < 446:
                rgb = credit
                if 431 <= y < 441 and (x - 616) % 14 < 10 and x < 788:
                    rgb = words
            row.append((*rgb, 255))
        rows.append(row)
    return rows


def encode(rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + bytes(c for px in row for c in px) for row in rows)
    ihdr = struct.pack(">IIBBBBB", len(rows[0]), len(rows), 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def decode(data):
    """RGBA8 rows of a non-interlaced 8-bit RGBA PNG, any filter."""
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat, size = 8, b"", None
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        kind, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            assert (depth, color, interlace) == (8, 6, 0), "expected 8-bit RGBA, not interlaced"
            size = (w, h)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    w, h = size
    raw, stride, bpp = zlib.decompress(idat), w * 4, 4
    rows, prev = [], bytes(stride)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            if f == 1:
                line[i] = (line[i] + a) & 0xFF
            elif f == 2:
                line[i] = (line[i] + b) & 0xFF
            elif f == 3:
                line[i] = (line[i] + (a + b) // 2) & 0xFF
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 0xFF
        rows.append([tuple(line[i : i + 4]) for i in range(0, stride, 4)])
        prev = bytes(line)
    return rows


def main():
    for out, rows in ((OUT, pixels()), (MAP, map_pixels())):
        if "--check" in sys.argv[1:]:
            if not out.exists() or decode(out.read_bytes()) != rows:
                sys.exit(f"{out}: its pixels are not what scripts/build_torture_images.py makes; rebuild and re-bless")
            print(f"{out.name}: pixels match")
            continue
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_bytes(encode(rows))
        print(f"wrote {out} ({out.stat().st_size} bytes)")


if __name__ == "__main__":
    main()

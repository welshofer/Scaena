#!/usr/bin/env python3
"""Build the torture deck's test card (PLAN 1.7, torture case 25): one 480 x 240 RGBA PNG.

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

Pure Python (zlib, struct): no imaging library. The pixels are exact integers; the disc and
ring edges are 4 x 4 supersampled.

    python3 scripts/build_torture_images.py           # write the PNG
    python3 scripts/build_torture_images.py --check   # the committed PNG has these pixels

The committed file is the truth for its content id (`sha256:` of its bytes, in the goldens);
`--check` compares decoded pixels, so a zlib that compresses differently cannot fail it.
"""

import struct
import sys
import zlib
from pathlib import Path

OUT = Path(__file__).resolve().parent.parent / "tests/fixtures/torture.scaena/assets/test-card.png"
W, H = 480, 240

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


def encode(rows):
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    raw = b"".join(b"\0" + bytes(c for px in row for c in px) for row in rows)
    ihdr = struct.pack(">IIBBBBB", W, H, 8, 6, 0, 0, 0)
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
    rows = pixels()
    if "--check" in sys.argv[1:]:
        if decode(OUT.read_bytes()) != rows:
            sys.exit(f"{OUT}: its pixels are not what scripts/build_torture_images.py makes; rebuild and re-bless")
        print(f"{OUT.name}: pixels match")
        return
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(encode(rows))
    print(f"wrote {OUT} ({OUT.stat().st_size} bytes)")


if __name__ == "__main__":
    main()

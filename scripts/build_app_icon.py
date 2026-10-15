#!/usr/bin/env python3
"""Build Scaena's app icons and the iPad's accent color: the Mac's icon set, which
`apps/mac/build-app.sh` makes an `.icns` of, and the iPad's and the remote's asset catalogs.

The mark is a stage: drapes and a valance framing the boards, and a spotlight's pool of light on
them, in the Dusk theme's colors (paper #101014, ink #F2F0E9, accent #FF6A3D, accent 2 #FFC857).
No words. The same art on each: on the Mac, a rounded square 824 px wide in a 1024 px canvas,
clear around it, as macOS draws an app's icon; on the iPad, the whole square, opaque, which
iPadOS rounds. The remote's is the same stage on the ink.

The iPad's accent is a deeper shade of the Dusk accent. The launch screen draws New Deck in it,
and the words over it lighter by 130 in each channel, not white (CI's screenshots, PR #366): the
shade is deep enough that those words read at 4.5 to 1 or more, as white would. In dark mode the
accent is a lighter shade, which reads as text on the dark backgrounds.

Pure Python (zlib, struct): no imaging library. Each edge is antialiased by its distance from the
pixel's center; gradients are evaluated at each pixel. The smaller sizes are the 1024 px picture
halved, two by two pixels averaged, alpha premultiplied.

    python3 scripts/build_app_icon.py            # write the icons and the catalogs' JSON
    python3 scripts/build_app_icon.py --check    # the committed files are what this makes

`--check` compares decoded pixels, each channel within 1 of what this draws (a libm may round an
`exp` otherwise), and the JSON as written.
"""

import json
import math
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ICONSET = ROOT / "apps/mac/Scaena/AppIcon.iconset"
CATALOG = ROOT / "apps/ipad/Assets.xcassets"
REMOTE_CATALOG = ROOT / "apps/ipad/Remote/Assets.xcassets"

SIZE = 1024
# macOS's icon grid: the body a rounded square 824 px wide, centered, 100 px clear each side.
BODY = 824
# The body's corners: a superellipse of this exponent, continuous in curvature as macOS's are.
CORNER = 5.0

PAPER = (0x10, 0x10, 0x14)
INK = (0xF2, 0xF0, 0xE9)
ACCENT = (0xFF, 0x6A, 0x3D)
ACCENT_2 = (0xFF, 0xC8, 0x57)


def shade(color, k):
    """`color` toward black, each channel times `k`, rounded half up."""
    return tuple(int(c * k + 0.5) for c in color)


# The iPad's accent: the Dusk accent at half its value, and at nine tenths in dark mode.
ACCENT_LIGHT = shade(ACCENT, 0.5)
ACCENT_DARK = shade(ACCENT, 0.9)
# How much lighter the launch screen draws a button's words than the button, each channel.
LIFTED = 130


def luminance(rgb):
    """WCAG 2's relative luminance of an sRGB color, 0 to 255 a channel."""

    def linear(c):
        c /= 255
        return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4

    r, g, b = (linear(c) for c in rgb)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(a, b):
    """WCAG 2's contrast ratio of two sRGB colors, 1 to 21."""
    la, lb = luminance(a), luminance(b)
    return (max(la, lb) + 0.05) / (min(la, lb) + 0.05)


def lifted(rgb):
    """The words the launch screen draws over a button of `rgb`."""
    return tuple(min(255, c + LIFTED) for c in rgb)


# The art, in units of its square, 0 to 1 from the top left: on the paper for the app, on the
# ink for the remote.
BG = [(0.0, (0x1E, 0x1C, 0x25)), (0.5, PAPER), (1.0, (0x08, 0x08, 0x0B))]
REMOTE_BG = [(0.0, (0xFA, 0xF8, 0xF2)), (0.5, INK), (1.0, (0xDD, 0xD8, 0xCC))]
# The proscenium: a box FRAME_X each side of the middle, from FRAME_TOP down to the boards' front
# edge at FLOOR, its corners rounded by FRAME_R. The drapes and the valance fill its edges.
FRAME_X, FRAME_TOP, FLOOR, FRAME_R = 0.420, 0.150, 0.845, 0.030
# The stage behind the drapes: its backdrop lit by the pool, warm about it and dark above.
BACKDROP_FAR, BACKDROP_NEAR = (0x17, 0x13, 0x18), (0x45, 0x22, 0x1B)
# The drapes: each from the frame's side, its inner edge swept from DRAPE_TOP_X from the middle
# back to its tie at TIE_V, TIE_X from the middle, then hanging to the boards, DRAPE_FOOT_X; two
# folds each, lit at their crests and in shadow toward the stage.
DRAPE_TOP_X, TIE_V, TIE_X, DRAPE_FOOT_X = 0.215, 0.600, 0.305, 0.275
DRAPE_DEEP, DRAPE_LIT = (0x7A, 0x22, 0x14), ACCENT
FOLDS = 2
# The valance across the top: three swags, each hanging SWAG below VALANCE_V at its middle.
VALANCE_V, SWAG = 0.235, 0.040
# The beam: from under the valance, BEAM_TOP each side of the middle, down to the pool, BEAM_FOOT;
# brighter toward the pool and toward its middle.
BEAM_TOP_V, BEAM_TOP, BEAM_FOOT_V, BEAM_FOOT = 0.255, 0.035, 0.745, 0.215
BEAM_ALPHA = [(0.255, 0.0), (0.36, 0.16), (0.745, 0.42)]
# The pool of light on the boards: an ellipse, hot at its middle, the accent at its rim.
POOL_V, POOL_RX, POOL_RY = 0.745, 0.215, 0.055
POOL = [(0.0, (0xFF, 0xF6, 0xE0)), (0.40, (0xFF, 0xD9, 0x7A)), (0.70, ACCENT_2), (1.0, ACCENT)]
# The light the pool throws on the boards and the backdrop about it, in the accent.
GLOW_ALPHA, GLOW_WIDTH = 0.50, 0.75


def ramp(stops, t):
    """The value at `t` along `stops`, (position, value) pairs, each value a number or a color."""
    if t <= stops[0][0]:
        return stops[0][1]
    for (t0, a), (t1, b) in zip(stops, stops[1:]):
        if t <= t1:
            k = (t - t0) / (t1 - t0)
            if isinstance(a, tuple):
                return tuple(x + (y - x) * k for x, y in zip(a, b))
            return a + (b - a) * k
    return stops[-1][1]


def cover(d):
    """How much of a pixel lies inside an edge `d` pixels from its center, outside positive."""
    return 0.0 if d >= 0.5 else 1.0 if d <= -0.5 else 0.5 - d


def over(dst, src, a):
    """`src` over `dst`, both colors, by alpha `a`."""
    return (dst[0] + (src[0] - dst[0]) * a, dst[1] + (src[1] - dst[1]) * a, dst[2] + (src[2] - dst[2]) * a)


def ellipse(du, dv, rx, ry):
    """How far, roughly, (du, dv) from an ellipse's center lies outside it, in its square's units;
    the first-order distance, which is right where an edge is drawn."""
    f = math.hypot(du / rx, dv / ry)
    if f == 0:
        return -min(rx, ry)
    g = math.hypot(du / (rx * rx), dv / (ry * ry)) / f
    return (f - 1) / g


def box(du, v, half, top, bottom, r):
    """How far (du, v) lies outside a box `half` each side of the middle, from `top` to `bottom`,
    its corners rounded by `r`."""
    qx = abs(du) - half + r
    qy = max(top - v, v - bottom) + r
    return math.hypot(max(qx, 0.0), max(qy, 0.0)) + min(max(qx, qy), 0.0) - r


def drape_edge(v):
    """How far from the middle a drape's inner edge is at `v`, and how fast it moves with `v`."""
    if v <= TIE_V:
        t = (v - FRAME_TOP) / (TIE_V - FRAME_TOP)
        e = t * t * (3 - 2 * t)
        de = 6 * t * (1 - t) / (TIE_V - FRAME_TOP)
        return DRAPE_TOP_X + (TIE_X - DRAPE_TOP_X) * e, (TIE_X - DRAPE_TOP_X) * de
    t = (v - TIE_V) / (FLOOR - TIE_V)
    return TIE_X + (DRAPE_FOOT_X - TIE_X) * t, (DRAPE_FOOT_X - TIE_X) / (FLOOR - TIE_V)


def squircle(x, y, half):
    """How far (x, y), pixels from the body's center, lies outside a superellipse `half` pixels
    each side of it, in pixels: first order, right at the edge."""
    ax, ay = abs(x) / half, abs(y) / half
    if ax == 0 and ay == 0:
        return -half
    n = CORNER
    g = (ax**n + ay**n) ** (1 / n)
    grad = math.hypot(ax ** (n - 1), ay ** (n - 1)) * g ** (1 - n)
    return (g - 1) / grad * half


def draw(size, body, bg=BG):
    """Premultiplied RGBA rows, floats 0 to 255: the art on `bg` in a `size` px square, all of it if
    `body` is None, else in a rounded square `body` px wide in its middle, clear around it. What
    depends on a pixel's column alone, or its row alone, is worked out once for each."""
    span = size if body is None else body
    margin = (size - span) / 2
    px = span
    # Each column's place across the art, and what the stage draws there whatever the row.
    cols = []
    for x in range(size):
        du = (x + 0.5 - margin) / span - 0.5
        ax = abs(du)
        k = (du + FRAME_X) / (2 * FRAME_X) * 3
        middle = math.sin(math.pi * (k - math.floor(k)))
        cols.append((du, ax, math.exp(-((du / 0.40) ** 2)), middle, VALANCE_V + SWAG * middle))
    # The body's edge: each column's share of the superellipse, in units of its half width.
    half = (body or size) / 2
    edge = [(abs(x + 0.5 - size / 2) / half) ** CORNER for x in range(size)]
    near = (1 - 1.5 / half) ** CORNER, (1 + 1.5 / half) ** CORNER
    rows = []
    for y in range(size):
        v = (y + 0.5 - margin) / span
        dv = v - POOL_V
        ground = ramp(bg, v)
        on_stage = FRAME_TOP - 1 / px < v < FLOOR + 1 / px
        lit_v = math.exp(-((dv / 0.34) ** 2))
        in_beam = BEAM_TOP_V <= v <= BEAM_FOOT_V
        if in_beam:
            t = (v - BEAM_TOP_V) / (BEAM_FOOT_V - BEAM_TOP_V)
            beam_half = BEAM_TOP + (BEAM_FOOT - BEAM_TOP) * t
            beam_slope = (BEAM_FOOT - BEAM_TOP) / (BEAM_FOOT_V - BEAM_TOP_V)
            beam_den = math.sqrt(1 + beam_slope * beam_slope)
            beam_alpha = ramp(BEAM_ALPHA, v)
        drape_x, drape_slope = drape_edge(v)
        drape_den = math.sqrt(1 + drape_slope * drape_slope)
        drape_fall = 1 - 0.25 * (v - FRAME_TOP) / (FLOOR - FRAME_TOP)
        under_valance = v <= VALANCE_V + SWAG + 1 / px
        ay = abs(y + 0.5 - size / 2) / half
        row_edge = ay**CORNER
        row = []
        for x in range(size):
            # How much of the pixel the body covers.
            if body is None:
                alpha = 1.0
            else:
                share = edge[x] + row_edge
                if share <= near[0]:
                    alpha = 1.0
                elif share >= near[1]:
                    row.append((0.0, 0.0, 0.0, 0.0))
                    continue
                else:
                    alpha = cover(squircle(x + 0.5 - size / 2, y + 0.5 - size / 2, half))
                    if alpha == 0:
                        row.append((0.0, 0.0, 0.0, 0.0))
                        continue
            c = ground
            du, ax, lit_u, middle, bottom = cols[x]
            frame = cover(box(du, v, FRAME_X, FRAME_TOP, FLOOR, FRAME_R) * px) if on_stage else 0.0
            if frame > 0:
                # The stage: the backdrop, lit by the pool about it.
                stage = over(BACKDROP_FAR, BACKDROP_NEAR, lit_u * lit_v)
                f = math.hypot(du / POOL_RX, dv / POOL_RY)
                if f < 1 + 4 * GLOW_WIDTH:
                    stage = over(stage, ACCENT, GLOW_ALPHA * math.exp(-(((max(f, 1.0) - 1) / GLOW_WIDTH) ** 2)))
                # The beam, from under the valance to the pool, brighter toward its middle.
                if in_beam:
                    a = cover((ax - beam_half) / beam_den * px)
                    if a > 0:
                        a *= beam_alpha * (1 - 0.45 * min(1.0, ax / beam_half) ** 2)
                        stage = over(stage, ACCENT_2, a)
                # The pool, hot at its middle.
                if f < 1.2:
                    a = cover(ellipse(du, dv, POOL_RX, POOL_RY) * px)
                    if a > 0:
                        stage = over(stage, ramp(POOL, min(f, 1.0)), a)
                # The drapes, each from the frame's side to its inner edge: folds lit at their
                # crests, in shadow toward the stage and toward the boards.
                a = cover((drape_x - ax) / drape_den * px)
                if a > 0:
                    across = (FRAME_X - ax) / (FRAME_X - drape_x)
                    crest = 0.5 + 0.5 * math.cos(2 * math.pi * FOLDS * across)
                    tone = (0.42 + 0.58 * crest) * (1 - 0.30 * across) * drape_fall
                    stage = over(stage, over(DRAPE_DEEP, DRAPE_LIT, tone), a)
                # The valance: swags across the top, each lit at its middle, shadowed at its hem.
                if under_valance:
                    a = cover((v - bottom) * px)
                    if a > 0:
                        depth = max((v - FRAME_TOP) / (bottom - FRAME_TOP), 0.0)
                        tone = (0.55 + 0.45 * middle) * (1 - 0.55 * depth**3)
                        stage = over(stage, over(DRAPE_DEEP, DRAPE_LIT, tone), a)
                c = over(c, stage, frame)
            row.append((c[0] * alpha, c[1] * alpha, c[2] * alpha, 255 * alpha))
        rows.append(row)
    return rows


def halved(rows):
    """`rows` at half their size, each two by two pixels averaged."""
    out = []
    for y in range(0, len(rows), 2):
        top, bottom = rows[y], rows[y + 1]
        out.append(
            [
                tuple((a + b + c + d) / 4 for a, b, c, d in zip(top[x], top[x + 1], bottom[x], bottom[x + 1]))
                for x in range(0, len(top), 2)
            ]
        )
    return out


def quantized(rows, alpha):
    """8-bit rows of `rows`, premultiplied floats: RGBA straight if `alpha`, else RGB."""
    out = []
    for row in rows:
        line = []
        for r, g, b, a in row:
            if alpha:
                if a <= 0:
                    line.append((0, 0, 0, 0))
                    continue
                k = 255 / a
                line.append(
                    (
                        min(255, int(r * k + 0.5)),
                        min(255, int(g * k + 0.5)),
                        min(255, int(b * k + 0.5)),
                        min(255, int(a + 0.5)),
                    )
                )
            else:
                line.append((min(255, int(r + 0.5)), min(255, int(g + 0.5)), min(255, int(b + 0.5))))
        out.append(line)
    return out


def png(rows, alpha):
    """A PNG of 8-bit `rows`, RGBA if `alpha` else RGB, marked sRGB; each row filtered by the row
    above it, which suits the gradients."""

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    h, w = len(rows), len(rows[0])
    raw, prev = bytearray(), bytes(w * (4 if alpha else 3))
    for row in rows:
        line = bytes(c for px in row for c in px)
        raw += b"\x02" + bytes((a - b) & 0xFF for a, b in zip(line, prev))
        prev = line
    header = struct.pack(">IIBBBBB", w, h, 8, 6 if alpha else 2, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"sRGB", b"\x00")
        + chunk(b"IDAT", zlib.compress(bytes(raw), 9))
        + chunk(b"IEND", b"")
    )


def unpng(data):
    """The 8-bit rows of a non-interlaced 8-bit RGB or RGBA PNG, any filter."""
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ValueError("not a PNG")
    pos, idat, size, bpp = 8, b"", None, 0
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        kind, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + n]
        if kind == b"IHDR":
            w, h, depth, color, _, _, interlace = struct.unpack(">IIBBBBB", body)
            if depth != 8 or color not in (2, 6) or interlace:
                raise ValueError("expected 8-bit RGB or RGBA, not interlaced")
            size, bpp = (w, h), 4 if color == 6 else 3
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    w, h = size
    raw, stride = zlib.decompress(idat), w * bpp
    rows, prev = [], bytes(stride)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        if f == 2:
            line = bytearray((a + b) & 0xFF for a, b in zip(line, prev))
        elif f:
            for i in range(stride):
                a = line[i - bpp] if i >= bpp else 0
                b = prev[i]
                c = prev[i - bpp] if i >= bpp else 0
                if f == 1:
                    line[i] = (line[i] + a) & 0xFF
                elif f == 3:
                    line[i] = (line[i] + (a + b) // 2) & 0xFF
                elif f == 4:
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 0xFF
        rows.append([tuple(line[i : i + bpp]) for i in range(0, stride, bpp)])
        prev = bytes(line)
    return rows


def catalog_json(kind):
    """An asset catalog's Contents.json, as Xcode writes one: `kind` "catalog", "icon", or
    "accent"."""
    info = {"author": "xcode", "version": 1}
    if kind == "catalog":
        body = {"info": info}
    elif kind == "icon":
        body = {
            "images": [{"filename": "AppIcon.png", "idiom": "universal", "platform": "ios", "size": "1024x1024"}],
            "info": info,
        }
    else:

        def color(rgb):
            components = {
                "alpha": "1.000",
                "red": "0x%02X" % rgb[0],
                "green": "0x%02X" % rgb[1],
                "blue": "0x%02X" % rgb[2],
            }
            return {"color-space": "srgb", "components": components}

        dark = [{"appearance": "luminosity", "value": "dark"}]
        body = {
            "colors": [
                {"color": color(ACCENT_LIGHT), "idiom": "universal"},
                {"appearances": dark, "color": color(ACCENT_DARK), "idiom": "universal"},
            ],
            "info": info,
        }
    return json.dumps(body, indent=2, sort_keys=True, separators=(",", " : ")) + "\n"


# The Mac's icon set: each file iconutil reads, by the size it is drawn at.
MAC_SIZES = {
    "icon_16x16.png": 16,
    "icon_16x16@2x.png": 32,
    "icon_32x32.png": 32,
    "icon_32x32@2x.png": 64,
    "icon_128x128.png": 128,
    "icon_128x128@2x.png": 256,
    "icon_256x256.png": 256,
    "icon_256x256@2x.png": 512,
    "icon_512x512.png": 512,
    "icon_512x512@2x.png": 1024,
}


def outputs():
    """Each file this writes, by its path: PNGs as (rows, alpha), the rest as text."""
    files = {}
    mac = draw(SIZE, BODY)
    by_size = {SIZE: mac}
    size = SIZE
    while size > 16:
        mac = halved(mac)
        size //= 2
        by_size[size] = mac
    for name, px in MAC_SIZES.items():
        files[ICONSET / name] = (quantized(by_size[px], True), True)
    for catalog, bg in ((CATALOG, BG), (REMOTE_CATALOG, REMOTE_BG)):
        files[catalog / "Contents.json"] = catalog_json("catalog")
        files[catalog / "AppIcon.appiconset/Contents.json"] = catalog_json("icon")
        files[catalog / "AppIcon.appiconset/AppIcon.png"] = (quantized(draw(SIZE, None, bg), False), False)
        files[catalog / "AccentColor.colorset/Contents.json"] = catalog_json("accent")
    return files


def check(files):
    """What differs between the committed files and `files`, one line each."""
    problems = []
    for path, want in files.items():
        name = path.relative_to(ROOT)
        if not path.exists():
            problems.append(f"{name}: missing")
            continue
        if isinstance(want, str):
            if path.read_text() != want:
                problems.append(f"{name}: not what scripts/build_app_icon.py writes")
            continue
        rows, _ = want
        try:
            have = unpng(path.read_bytes())
        except (ValueError, zlib.error) as e:
            problems.append(f"{name}: {e}")
            continue
        if len(have) != len(rows) or len(have[0]) != len(rows[0]):
            problems.append(f"{name}: {len(have[0])} x {len(have)} px, not {len(rows[0])} x {len(rows)}")
            continue
        worst = max(abs(a - b) for hr, wr in zip(have, rows) for hp, wp in zip(hr, wr) for a, b in zip(hp, wp))
        if worst > 1:
            problems.append(f"{name}: its pixels differ by up to {worst} from what scripts/build_app_icon.py draws")
    return problems


def main():
    files = outputs()
    if "--check" in sys.argv[1:]:
        problems = check(files)
        if problems:
            sys.exit("\n".join(problems) + "\nrun python3 scripts/build_app_icon.py and commit what it writes")
        print(f"app icons: {len(files)} files match")
        return
    for path, want in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(want, str):
            path.write_text(want)
        else:
            path.write_bytes(png(*want))
        print(f"wrote {path.relative_to(ROOT)}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build the lint fixtures' own font (SPEC §7.5 W230, PLAN 2.14).

W230 warns where a font's license, in its OS/2 embedding bits (`fsType`), does not
allow what Scaena does with a font. Its trigger needs a font that says so, and no font
anyone ships should be bent to say it. So this one is made here, from nothing: one
family, `Scaena Restricted`, whose glyphs are plain boxes for the ASCII letters and
digits, with `fsType` set to a restricted license that may not be subset. It is no one
else's work, so it carries no license of its own.

The output is byte-for-byte the same on every run: fixed timestamps, no checksum date.

    pip install fonttools==4.66.1
    python3 scripts/build_lint_fonts.py            # write tests/lint/bundle/fonts/
    python3 scripts/build_lint_fonts.py --check    # fail if the file differs
"""

import argparse
import io
import sys
from pathlib import Path

from fontTools.fontBuilder import FontBuilder
from fontTools.pens.ttGlyphPen import TTGlyphPen

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "tests/lint/bundle/fonts/ScaenaRestricted.ttf"

FAMILY = "Scaena Restricted"
# OS/2 fsType: restricted license embedding (bit 1), and no subsetting (bit 8).
FS_TYPE = 0x0002 | 0x0100
# 2026-10-04 00:00:00 UTC, in seconds since 1904-01-01, as `head` counts.
STAMP = 3842294400


def build() -> bytes:
    chars = [chr(c) for c in range(0x30, 0x3A)] + [chr(c) for c in range(0x41, 0x5B)] + [chr(c) for c in range(0x61, 0x7B)]
    names = [".notdef", "space"] + [f"u{ord(c):04X}" for c in chars]
    fb = FontBuilder(1000, isTTF=True)
    fb.setupGlyphOrder(names)
    fb.setupCharacterMap({0x20: "space", **{ord(c): f"u{ord(c):04X}" for c in chars}})
    glyphs = {}
    for name in names:
        pen = TTGlyphPen(None)
        if name != "space":
            top = 500 if name.startswith("u") and chr(int(name[1:], 16)).islower() else 700
            pen.moveTo((60, 0))
            pen.lineTo((60, top))
            pen.lineTo((540, top))
            pen.lineTo((540, 0))
            pen.closePath()
        glyphs[name] = pen.glyph()
    fb.setupGlyf(glyphs)
    fb.setupHorizontalMetrics({name: (250 if name == "space" else 600, 0 if name == "space" else 60) for name in names})
    fb.setupHorizontalHeader(ascent=900, descent=-250)
    fb.setupNameTable(
        {
            "familyName": FAMILY,
            "styleName": "Regular",
            "uniqueFontIdentifier": "Scaena Restricted Regular lint fixture",
            "fullName": "Scaena Restricted Regular",
            "psName": "ScaenaRestricted-Regular",
            "version": "Version 1.000",
        }
    )
    fb.setupOS2(
        sTypoAscender=700,
        sTypoDescender=-250,
        sTypoLineGap=200,
        usWinAscent=900,
        usWinDescent=250,
        sxHeight=500,
        sCapHeight=700,
        fsType=FS_TYPE,
    )
    fb.setupPost()
    fb.font["head"].created = STAMP
    fb.font["head"].modified = STAMP
    fb.font.recalcTimestamp = False
    buf = io.BytesIO()
    fb.save(buf)
    return buf.getvalue()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--check", action="store_true", help="fail if the committed font differs")
    args = parser.parse_args()
    data = build()
    if args.check:
        if not OUT.exists() or OUT.read_bytes() != data:
            print(f"{OUT.relative_to(ROOT)} differs from what this script builds", file=sys.stderr)
            return 1
        print(f"{OUT.relative_to(ROOT)}: as built")
        return 0
    OUT.parent.mkdir(parents=True, exist_ok=True)
    OUT.write_bytes(data)
    print(f"wrote {OUT.relative_to(ROOT)} ({len(data)} bytes, fsType 0x{FS_TYPE:04x})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

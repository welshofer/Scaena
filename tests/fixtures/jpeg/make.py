"""The JPEGs that hold scaena-core's decoder to zune-jpeg's (ADR-0017).

    python3 tests/fixtures/jpeg/make.py     (from the repository's root; Pillow and ImageMagick)

Each file is a case the decoder must read as zune-jpeg does: a sampling, progressive scans,
restart intervals, Huffman tables of its own, gray, RGB, a size that is not a whole number of
blocks, and one that is. `orientation-6.jpg` also says where, when, and with what it was taken,
which `stripped` drops. `cmyk.jpg` is refused. The files are what the tests read; this script
says how they were made, by Pillow's libjpeg, so a newer libjpeg may write other bytes.
"""

import io
import random
import subprocess
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).parent


def picture(w, h, seed=7):
    """A photo's worth of what a JPEG codes: gradients, edges, saturated color, and noise."""
    rng = random.Random(seed)
    im = Image.new("RGB", (w, h))
    px = im.load()
    for y in range(h):
        for x in range(w):
            r = int(255 * x / max(w - 1, 1))
            g = int(255 * y / max(h - 1, 1))
            b = int(128 + 127 * ((x * 7 + y * 3) % 31) / 30) - 64
            n = rng.randint(-12, 12)
            px[x, y] = (max(0, min(255, r + n)), max(0, min(255, g - n)), max(0, min(255, b + n)))
    d = ImageDraw.Draw(im)
    d.rectangle([w // 8, h // 6, w // 3, h // 2], fill=(220, 30, 40))
    d.ellipse([w // 2, h // 3, w - w // 8, h - h // 8], fill=(20, 160, 60), outline=(250, 250, 250))
    d.line([0, h - 1, w - 1, 0], fill=(10, 10, 200), width=2)
    # The top-left corner is the one place this color is: how the orientation turns is seen.
    d.rectangle([0, 0, 3, 3], fill=(255, 255, 0))
    return im


def save(name, im, **options):
    out = HERE / name
    im.save(out, "JPEG", **options)
    return out


def magick(name, im, *args):
    src = io.BytesIO()
    im.save(src, "PNG")
    out = HERE / name
    subprocess.run(["convert", "png:-", *args, str(out)], input=src.getvalue(), check=True)


small = picture(67, 45)
save("baseline-444.jpg", small, quality=92, subsampling=0)
save("baseline-422.jpg", small, quality=92, subsampling=1)
save("baseline-420.jpg", small, quality=92, subsampling=2)
save("progressive-420.jpg", small, quality=90, subsampling=2, progressive=True)
save("progressive-444.jpg", small, quality=90, subsampling=0, progressive=True, optimize=True)
save("optimized-420.jpg", small, quality=85, subsampling=2, optimize=True)
save("restart-420.jpg", small, quality=90, subsampling=2, restart_marker_blocks=3)
save("restart-progressive.jpg", small, quality=90, subsampling=2, progressive=True, restart_marker_rows=1)
save("gray.jpg", small.convert("L"), quality=90)
save("gray-progressive.jpg", small.convert("L"), quality=90, progressive=True)
save("rgb.jpg", small, quality=92, keep_rgb=True, subsampling=0)
save("exact-420.jpg", picture(64, 48, seed=3), quality=90, subsampling=2)
save("tiny.jpg", picture(1, 1, seed=5), quality=90, subsampling=2)
save("large-420.jpg", picture(300, 200, seed=11), quality=80, subsampling=2, progressive=True)
magick("sampled-411.jpg", small, "-quality", "90", "-sampling-factor", "4x1")
magick("sampled-440.jpg", small, "-quality", "90", "-sampling-factor", "1x2")
save("cmyk.jpg", small.convert("CMYK"), quality=90)

# Turned a quarter clockwise to be seen, and saying more than it needs to: the camera, when, a
# comment, XMP, and a color profile.
exif = Image.Exif()
exif[0x0112] = 6
exif[0x010F] = "Scaena Test Camera"
exif[0x0132] = "2026:10:06 12:00:00"
save(
    "orientation-6.jpg",
    small,
    quality=92,
    subsampling=2,
    exif=exif.tobytes(),
    comment=b"taken somewhere",
    xmp=b"<x:xmpmeta xmlns:x='adobe:ns:meta/'/>",
    icc_profile=b"\0" * 128,
)
for f in sorted(HERE.glob("*.jpg")):
    print(f"{f.name}: {f.stat().st_size} B")

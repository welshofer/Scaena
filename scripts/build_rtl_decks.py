#!/usr/bin/env python3
"""Build and check the decks that read right to left (PLAN T.5a): one in Arabic and one in Hebrew.

Each is the same talk in its language, on Dusk (a copy that keeps the layouts the decks use, its
italics left out, and Noto Sans Arabic and Noto Sans Hebrew named last in its text families'
fallbacks), so its golden display lists show the slide mirrored as it is laid out: a title
slide, a list, two columns, a process whose chevrons point the way the deck reads, a figure, a
picture beside words, and a slide placed by hand, a chart by cells, a stack of three words, and
a caption by a `rect`. The picture is drawn here, a sun low on its left, so a mirrored picture
would show. `scripts/build_bundle_fonts.py` subsets the fonts to both decks' text.

    python3 scripts/build_rtl_decks.py           # write tests/fixtures/rtl
    python3 scripts/build_rtl_decks.py --check   # check the committed files match
"""

import argparse
import json
import struct
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
RTL = ROOT / "tests" / "fixtures" / "rtl"
DUSK = ROOT / "docs" / "examples" / "themes" / "dusk.theme.json"
VERSION = json.loads((ROOT / "tests" / "bench" / "b1.scaena" / "deck.json").read_text())["scaena"]
LAYOUTS = ["full", "title", "bullets", "two-columns", "process", "stat", "photo-right"]
FONTS = [
    {"family": "Fraunces", "file": "fonts/Fraunces-VF.ttf", "axes": {"wght": [100, 900], "opsz": [9, 144]}},
    {"family": "Inter", "file": "fonts/Inter-VF.ttf", "axes": {"wght": [100, 900], "opsz": [14, 32]}},
    {"family": "JetBrains Mono", "file": "fonts/JetBrainsMono-VF.ttf", "axes": {"wght": [100, 800]}},
    {"family": "Noto Sans Arabic", "file": "fonts/NotoSansArabic-VF.ttf", "axes": {"wght": [100, 900]}},
    {"family": "Noto Sans Hebrew", "file": "fonts/NotoSansHebrew-VF.ttf", "axes": {"wght": [100, 900]}},
]
SCRIPTS = {"arabic": FONTS[3], "hebrew": FONTS[4]}

# The talk, in each language: what each slide says.
WORDS = {
    "arabic": {
        "lang": "ar",
        "title": "مياه أنظف لمدننا",
        "kicker": "التقرير السنوي · 2026",
        "subtitle": "ما تعلّمناه في عام من العمل",
        "points-header": "ما الذي تغيّر",
        "points": ["انخفض الهدر بنسبة 18% منذ 2024", "أربع محطات جديدة تعمل الآن", "يصل الماء النظيف إلى 1.2 مليون شخص"],
        "sides-header": "قبل وبعد",
        "before": "كانت الأنابيب القديمة تفقد ثلث الماء قبل أن يصل إلى البيوت.",
        "after": "اليوم تقيس الحساسات كل خط، ونصلح التسرب في يوم واحد.",
        "steps-header": "كيف نعمل",
        "steps": [("نقيس", "حساسات في كل حي"), ("نخطط", "أولويات كل ربع"), ("نبني", "فرق محلية للإصلاح"), ("نراجع", "تقرير مفتوح كل شهر")],
        "number": "42%",
        "claim": "من الأحياء لديها عدادات ذكية",
        "detail": "بدأنا بالأحياء التي تفقد أكثر من غيرها، ونصل إلى الباقي في 2027.",
        "photo-header": "على الأرض",
        "photo-body": "فرقنا تعمل مع السكان في كل حي، من التخطيط إلى الإصلاح.",
        "photo-alt": "شمس منخفضة فوق تلال عند الغروب.",
        "free-header": "الأرقام شهرًا بشهر",
        "months": ["يناير", "فبراير", "مارس", "أبريل"],
        "chart-alt": "الماء المفقود شهريًا بالمليون لتر: يناير 31، فبراير 27، مارس 22، أبريل 18.",
        "order": ["أولًا", "ثانيًا", "ثالثًا"],
        "source": "المصدر: سجلات الشبكة، 2026",
    },
    "hebrew": {
        "lang": "he",
        "title": "מים נקיים לערים שלנו",
        "kicker": "דוח שנתי · 2026",
        "subtitle": "מה למדנו בשנה של עבודה",
        "points-header": "מה השתנה",
        "points": ["הבזבוז ירד ב־18% מאז 2024", "ארבע תחנות חדשות פועלות", "מים נקיים מגיעים ל־1.2 מיליון איש"],
        "sides-header": "לפני ואחרי",
        "before": "הצינורות הישנים איבדו שליש מהמים לפני שהגיעו לבתים.",
        "after": "היום חיישנים מודדים כל קו, ואנחנו מתקנים דליפה ביום אחד.",
        "steps-header": "איך אנחנו עובדים",
        "steps": [("מודדים", "חיישנים בכל שכונה"), ("מתכננים", "סדרי עדיפויות בכל רבעון"), ("בונים", "צוותים מקומיים לתיקון"), ("בודקים", "דוח פתוח בכל חודש")],
        "number": "42%",
        "claim": "מהשכונות עם מונים חכמים",
        "detail": "התחלנו בשכונות שמאבדות הכי הרבה, ונגיע לשאר ב־2027.",
        "photo-header": "בשטח",
        "photo-body": "הצוותים שלנו עובדים עם התושבים בכל שכונה, מהתכנון ועד התיקון.",
        "photo-alt": "שמש נמוכה מעל גבעות בשקיעה.",
        "free-header": "המספרים חודש אחר חודש",
        "months": ["ינואר", "פברואר", "מרץ", "אפריל"],
        "chart-alt": "מים שאבדו בכל חודש, במיליוני ליטרים: ינואר 31, פברואר 27, מרץ 22, אפריל 18.",
        "order": ["ראשית", "שנית", "שלישית"],
        "source": "מקור: רישומי הרשת, 2026",
    },
}
LOST = [31, 27, 22, 18]


def flipped(points: list) -> list:
    """A shape's points mirrored across its box, as a slide started in a deck that reads right to
    left writes a theme's shape (`scaena_core::direction::flipped`)."""
    return [[round(1 - x, 6), y] for x, y in points]


def deck(words: dict, theme: dict) -> dict:
    nodes: dict = {}
    states: list = []

    def text(id: str, role: str, said: str, at: dict, **more) -> str:
        nodes[id] = {"type": "text", "role": role, "text": said, "at": at, **more}
        return id

    def state(id: str, layout: str | None, ids: list[str]) -> None:
        s = {"id": id, "mode": "absolute", "props": {i: {} for i in ids}}
        if layout:
            s["layout"] = layout
        states.append(s)

    state(
        "cover",
        "title",
        [
            text("cover-kicker", "kicker", words["kicker"], {"in": "kicker"}),
            text("cover-title", "display", words["title"], {"in": "title"}, semantic="navigation"),
            text("cover-subtitle", "title", words["subtitle"], {"in": "subtitle"}),
        ],
    )
    points = words["points"]
    state(
        "points",
        "bullets",
        [
            text("points-header", "headline", words["points-header"], {"in": "header"}, semantic="navigation"),
            text("points-body", "lede", "\n".join(points), {"in": "body"}, list=[{"kind": "bullet"}] * len(points)),
        ],
    )
    state(
        "sides",
        "two-columns",
        [
            text("sides-header", "headline", words["sides-header"], {"in": "header"}, semantic="navigation"),
            text("sides-before", "body", words["before"], {"in": "first"}),
            text("sides-after", "body", words["after"], {"in": "second"}),
        ],
    )
    process = theme["layouts"]["process"]["slots"]
    ordinals = ["first", "second", "third", "fourth"]
    steps = [text("steps-header", "headline", words["steps-header"], {"in": "header"}, semantic="navigation")]
    for ordinal, (title, body) in zip(ordinals, words["steps"]):
        chevron = process[f"{ordinal}-chevron"]["prompt"]
        nodes[f"steps-{ordinal}-chevron"] = {
            "type": "shape",
            "kind": chevron["shape"],
            "fill": chevron["fill"],
            "points": flipped(chevron["points"]),
            "semantic": "decoration",
            "at": {"in": f"{ordinal}-chevron"},
            "z": -1,
        }
        steps += [
            f"steps-{ordinal}-chevron",
            text(f"steps-{ordinal}-title", "title", title, {"in": f"{ordinal}-title"}),
            text(f"steps-{ordinal}", "body", body, {"in": ordinal}),
        ]
    state("steps", "process", steps)
    state(
        "figure",
        "stat",
        [
            text("figure-number", "hero", words["number"], {"in": "number"}),
            text("figure-claim", "title", words["claim"], {"in": "claim"}, semantic="claim"),
            text("figure-detail", "body", words["detail"], {"in": "detail"}),
        ],
    )
    nodes["photo-picture"] = {
        "type": "image",
        "src": "assets/sunset.png",
        "alt": words["photo-alt"],
        "semantic": "evidence",
        "at": {"in": "art"},
    }
    state(
        "photo",
        "photo-right",
        [
            text("photo-header", "headline", words["photo-header"], {"in": "header"}, semantic="navigation"),
            text("photo-body", "body", words["photo-body"], {"in": "body"}, semantic="claim"),
            "photo-picture",
        ],
    )
    nodes["free-chart"] = {
        "type": "chart",
        "kind": "bar",
        "data": "@lost",
        "x": {"field": "month", "type": "ordinal"},
        "y": {"field": "lost", "type": "quantitative", "format": "d"},
        "alt": words["chart-alt"],
        "semantic": "evidence",
        "at": {"col": [1, 7], "row": [3, 10]},
    }
    nodes["free-order"] = {"type": "stack", "axis": "x", "gap": "space.3", "at": {"col": [8, 12], "row": [3, 5]}}
    order = [
        text(f"free-order-{i + 1}", "title", said, {"parent": "free-order", "index": i})
        for i, said in enumerate(words["order"])
    ]
    state(
        "free",
        None,
        [
            text("free-header", "headline", words["free-header"], {"col": [1, 12], "row": [1, 2]}, semantic="claim"),
            "free-chart",
            "free-order",
            *order,
            text("free-source", "caption", words["source"], {"rect": [120, 900, 640, 72]}, semantic="source"),
        ],
    )
    return {
        "scaena": VERSION,
        "meta": {
            "title": words["title"],
            "lang": words["lang"],
            "description": "A deck that reads right to left, laid out mirrored (PLAN T.5a). "
            "Written by scripts/build_rtl_decks.py.",
        },
        "canvas": {"width": 1920, "height": 1080, "unit": "cu"},
        "theme": "theme.json",
        "fonts": FONTS,
        "data": {
            "lost": {
                "source": {"inline": [{"month": m, "lost": v} for m, v in zip(words["months"], LOST)]},
                "schema": {"month": "string", "lost": "number"},
            }
        },
        "nodes": nodes,
        "states": states,
    }


def theme() -> dict:
    dusk = json.loads(DUSK.read_text(encoding="utf-8"))
    families = dusk["type"]["families"]
    for family in families.values():
        family.pop("italic", None)
    for key, font in SCRIPTS.items():
        families[key] = {"family": font["family"], "file": font["file"], "axes": font["axes"]}
    # Each text family's fallbacks end in the scripts' families, as `scaena_ops::scripts::taken`
    # names a script's family.
    for key in {r["family"] for name, r in dusk["type"]["roles"].items() if name != "code"}:
        fallback = families[key].setdefault("fallback", [])
        fallback += [k for k in SCRIPTS if k not in fallback]
    furniture = dusk["furniture"]
    return {
        **dusk,
        "name": "Dusk, right to left",
        "description": "Dusk with Noto Sans Arabic and Noto Sans Hebrew last in its text families' fallbacks, "
        "its italics left out, and the layouts its decks use: the PLAN T.5a fixtures' theme. "
        "Written by scripts/build_rtl_decks.py.",
        "layouts": {name: dusk["layouts"][name] for name in LAYOUTS},
        "furniture": {**furniture, "except": [name for name in furniture.get("except", []) if name in LAYOUTS]},
    }


def png(width: int, height: int, pixel) -> bytes:
    """An RGB PNG, each pixel `pixel(x, y)`."""
    raw = b"".join(b"\0" + bytes(c for x in range(width) for c in pixel(x, y)) for y in range(height))

    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)

    head = struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", head) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def sunset() -> bytes:
    """A dusk sky over two hills, its sun low on the left: mirrored, it would set on the right."""
    width, height = 160, 120

    def pixel(x: int, y: int) -> tuple[int, int, int]:
        if (x - 40) ** 2 + (y - 62) ** 2 < 14**2:
            return (255, 196, 96)
        if y > 80 + 10 * ((x / width) - 0.5) ** 2 * 4 or y > 96 - x // 8:
            return (34, 30, 52)
        t = y / height
        return (int(255 - 140 * t), int(120 - 60 * t), int(90 + 40 * t))

    return png(width, height, pixel)


def outputs(out: Path) -> dict[Path, bytes]:
    """Every file the script writes, by path, and its bytes."""
    t = theme()

    def dump(value) -> bytes:
        return (json.dumps(value, indent=2, ensure_ascii=False) + "\n").encode("utf-8")

    files = {out / f"{name}.deck.json": dump(deck(words, t)) for name, words in WORDS.items()}
    files[out / "theme.json"] = dump(t)
    files[out / "assets" / "sunset.png"] = sunset()
    return files


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--check", action="store_true", help="check the committed files match, write nothing")
    parser.add_argument("--out", type=Path, default=RTL, help="where to write (default tests/fixtures/rtl)")
    args = parser.parse_args()
    files = outputs(args.out)
    if args.check:
        stale = [p for p, data in files.items() if not p.exists() or p.read_bytes() != data]
        if stale:
            sys.exit("stale, rerun scripts/build_rtl_decks.py: " + ", ".join(str(p.relative_to(ROOT)) for p in stale))
        print(f"right-to-left decks: {len(files)} files match")
        return
    for path, data in files.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    print(f"wrote {len(files)} files under {args.out}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Build and check the icons a deck inserts (PLAN 2.104): Phosphor Icons 2.1.1, MIT.

Each icon is a path shape (SPEC §3.3) in Phosphor's regular and fill weights, read from the npm
package @phosphor-icons/core at its pinned version, sha256-verified, with its name, tags, and
categories. Brands are left out: a logo is its owner's mark, not an icon to set in a deck.

Each path is written again in absolute moves, lines, cubic curves, and closes alone, its numbers
to two places, so that any reader parses it: the engine, the browser's picker, and the Mac's and
the iPad's, which draw it in SwiftUI. Arcs become cubics of at most 90°, each within 0.03% of its
radius; quadratics and smooth curves become the cubics they are.

Check (no network; CI runs this): docs/icons/phosphor.json is sorted by name, each name is unique
and in kebab case, and each path is of those four commands, starts with a move, and stays in the
icons' 256 by 256 view; and the paths the engine compiles in (`map-pin` in its fill weight, the pin
a picture's `pin` draws, and a forecast's eleven in their regular weight, ADR-0030) are the set's.

    python3 scripts/build_icons.py            # fetch the package, write the icons, then check
    python3 scripts/build_icons.py --cache ~/.cache/scaena-icons
    python3 scripts/build_icons.py --weather  # write the icons the engine compiles in from the set, no network
    python3 scripts/build_icons.py --toolbar  # write the editor's toolbar icons from the set, no network
    python3 scripts/build_icons.py --check
"""

import argparse
import hashlib
import http.client
import io
import json
import math
import re
import sys
import tarfile
import time
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "docs/icons/phosphor.json"
LICENSE = ROOT / "docs/icons/LICENSE-phosphor.txt"
# The icons the engine compiles in, each by its name and weight, as a constant of this file.
COMPILED = ROOT / "crates/scaena-engine/src/pin.rs"
# A forecast's icons, a line each, `name path`: compiled into the engine natively and into the
# player's module, and handed over to the editor's the first time a forecast lays out (ADR-0015).
WEATHER_FILE = ROOT / "crates/scaena-engine/icons/weather.txt"
COMPILED_ICONS = [("MAP_PIN", "map-pin", "fill")]
# A forecast's icons (ADR-0030, PLAN O.2b): each sky's, by day and by night, a drop, and the wind,
# in their regular weight, by their names as `scaena_core::weather` names them.
# The editor's toolbar icons (lift-up ATTR-5), each button's by its name, in the regular weight:
# written to a module the page compiles in, so the toolbar draws them before the set is fetched.
TOOLBAR_FILE = ROOT / "web/src/toolbar-icons.ts"
TOOLBAR_ICONS = [
    "file",
    "file-plus",
    "sparkle",
    "folder-open",
    "file-archive",
    "bookmarks",
    "floppy-disk",
    "floppy-disk-back",
    "download-simple",
    "export",
    "play",
    "code",
    "squares-four",
    "device-mobile",
    "grid-four",
    "bounding-box",
    "layout",
    "timer",
    "magnifying-glass",
    "command",
    "keyboard",
    "magnifying-glass-minus",
    "magnifying-glass-plus",
    "corners-in",
    "palette",
    "plus-circle",
    "slideshow",
    "frame-corners",
]
WEATHER_ICONS = [
    "sun",
    "cloud-sun",
    "cloud",
    "cloud-fog",
    "cloud-rain",
    "cloud-snow",
    "cloud-lightning",
    "moon",
    "cloud-moon",
    "drop",
    "wind",
]

VERSION = "2.1.1"
PACKAGE = f"https://registry.npmjs.org/@phosphor-icons/core/-/core-{VERSION}.tgz"
SHA256 = "313332be6190b724da24107addd781799b48bf76b13963f24501112ffe1baadd"
VIEW = [0, 0, 256, 256]
WEIGHTS = ["regular", "fill"]
# Every category but brands, as the package names them.
CATEGORIES = {
    "arrows",
    "commerce",
    "communications",
    "design",
    "editor",
    "finances",
    "games",
    "health & wellness",
    "maps & travel",
    "media",
    "nature",
    "objects",
    "office",
    "people",
    "system",
    "technology & development",
    "weather",
}
BRANDS = "brands"


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def fetch(cache: Path | None) -> bytes:
    """The package's bytes, verified against the pinned sha256 (from `cache` when it holds them)."""
    cached = cache / SHA256 if cache else None
    if cached and cached.exists() and sha256(cached.read_bytes()) == SHA256:
        return cached.read_bytes()
    for attempt in range(5):
        try:
            with urllib.request.urlopen(PACKAGE, timeout=300) as r:
                data = r.read()
            break
        except (OSError, http.client.HTTPException) as e:
            if attempt == 4:
                sys.exit(f"download failed for {PACKAGE}: {e}")
            time.sleep(2**attempt)
    if sha256(data) != SHA256:
        sys.exit(f"sha256 mismatch for {PACKAGE}: got {sha256(data)}, pinned {SHA256}")
    if cached:
        cache.mkdir(parents=True, exist_ok=True)
        cached.write_bytes(data)
    return data


# --- The package's catalog -------------------------------------------------------------------


def catalog(index: str) -> list[dict]:
    """Each icon's name, categories, and tags, from the package's `dist/index.mjs`: its `icons`
    array is a JavaScript literal, keys bare and categories named by an enum, made JSON here."""
    enum = re.search(r"e = /\* @__PURE__ \*/ \(\(i\) => \((.*?)\)\)\(e \|\| \{\}\)", index)
    figma = re.search(r"a = /\* @__PURE__ \*/ \(\(i\) => \((.*?)\)\)\(a \|\| \{\}\)", index)
    if not enum or not figma:
        sys.exit("dist/index.mjs: no category enums")
    names = dict(re.findall(r'i\.(\w+) = "([^"]+)"', enum.group(1)))
    figmas = dict(re.findall(r'i\.(\w+) = "([^"]+)"', figma.group(1)))
    array = re.search(r"^const n = (\[.*?^\]);", index, re.S | re.M)
    if not array:
        sys.exit("dist/index.mjs: no icons array")
    text = array.group(1)
    text = re.sub(r"\be\.(\w+)", lambda m: json.dumps(names[m.group(1)]), text)
    text = re.sub(r"\ba\.(\w+)", lambda m: json.dumps(figmas[m.group(1)]), text)
    text = re.sub(r"([{,]\s*)([a-z_]+):", r'\1"\2":', text)
    return json.loads(text)


# --- Path data -------------------------------------------------------------------------------

NUMBER = re.compile(r"[-+]?(?:\d+\.?\d*|\.\d+)(?:[eE][-+]?\d+)?")


class Scanner:
    """SVG path data, read a command, a number, or an arc's flag at a time."""

    def __init__(self, d: str):
        self.d, self.i = d, 0

    def skip(self) -> None:
        while self.i < len(self.d) and self.d[self.i] in " \t\r\n,":
            self.i += 1

    def done(self) -> bool:
        self.skip()
        return self.i == len(self.d)

    def command(self) -> str | None:
        self.skip()
        if self.i < len(self.d) and self.d[self.i] in "MmLlHhVvCcSsQqTtAaZz":
            self.i += 1
            return self.d[self.i - 1]
        return None

    def number(self) -> float:
        self.skip()
        m = NUMBER.match(self.d, self.i)
        if not m:
            raise ValueError(f"a number at {self.i} of {self.d[:40]!r}…")
        self.i = m.end()
        return float(m.group())

    def flag(self) -> bool:
        # A flag is one character, which may run into the number after it: `a8 8 0 0116 136`.
        self.skip()
        if self.i == len(self.d) or self.d[self.i] not in "01":
            raise ValueError(f"an arc's flag at {self.i} of {self.d[:40]!r}…")
        self.i += 1
        return self.d[self.i - 1] == "1"

    def point(self) -> tuple[float, float]:
        return (self.number(), self.number())


def arc(p0, rx, ry, angle, large, sweep, p1) -> list[list[tuple[float, float]]]:
    """The cubics an elliptical arc from `p0` to `p1` is drawn by, at most 90° each (SVG 1.1,
    implementation notes F.6.5 and F.6.6): each a list of its two controls and its end."""
    if p0 == p1:
        return []
    rx, ry = abs(rx), abs(ry)
    if rx == 0 or ry == 0:
        return [[p0, p1, p1]]
    phi = math.radians(angle % 360)
    cos_phi, sin_phi = math.cos(phi), math.sin(phi)
    dx, dy = (p0[0] - p1[0]) / 2, (p0[1] - p1[1]) / 2
    x1, y1 = cos_phi * dx + sin_phi * dy, -sin_phi * dx + cos_phi * dy
    reach = x1 * x1 / (rx * rx) + y1 * y1 / (ry * ry)
    if reach > 1:
        rx, ry = rx * math.sqrt(reach), ry * math.sqrt(reach)
    num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1
    den = rx * rx * y1 * y1 + ry * ry * x1 * x1
    coef = math.sqrt(max(0.0, num / den)) * (-1 if large == sweep else 1)
    cx1, cy1 = coef * rx * y1 / ry, -coef * ry * x1 / rx
    cx = cos_phi * cx1 - sin_phi * cy1 + (p0[0] + p1[0]) / 2
    cy = sin_phi * cx1 + cos_phi * cy1 + (p0[1] + p1[1]) / 2

    def between(ux, uy, vx, vy) -> float:
        return math.atan2(ux * vy - uy * vx, ux * vx + uy * vy)

    ux, uy = (x1 - cx1) / rx, (y1 - cy1) / ry
    vx, vy = (-x1 - cx1) / rx, (-y1 - cy1) / ry
    start = between(1, 0, ux, uy)
    sweep_angle = between(ux, uy, vx, vy)
    if not sweep and sweep_angle > 0:
        sweep_angle -= 2 * math.pi
    elif sweep and sweep_angle < 0:
        sweep_angle += 2 * math.pi
    n = max(1, math.ceil(abs(sweep_angle) / (math.pi / 2) - 1e-9))
    step = sweep_angle / n
    k = 4 / 3 * math.tan(step / 4)

    def on(x: float, y: float) -> tuple[float, float]:
        return (cx + rx * x * cos_phi - ry * y * sin_phi, cy + rx * x * sin_phi + ry * y * cos_phi)

    out = []
    t = start
    for _ in range(n):
        u = t + step
        c1 = on(math.cos(t) - k * math.sin(t), math.sin(t) + k * math.cos(t))
        c2 = on(math.cos(u) + k * math.sin(u), math.sin(u) - k * math.cos(u))
        out.append([c1, c2, on(math.cos(u), math.sin(u))])
        t = u
    out[-1][2] = p1
    return out


def absolute(d: str) -> list[tuple[str, list[tuple[float, float]]]]:
    """`d` as absolute moves (M), lines (L), cubics (C), and closes (Z), each with its points."""
    s = Scanner(d)
    out: list[tuple[str, list[tuple[float, float]]]] = []
    here = start = (0.0, 0.0)
    control = None  # the last curve's second control, which a smooth curve reflects
    quad = None  # the last quadratic's control, which a smooth quadratic reflects
    last = None
    while not s.done():
        c = s.command()
        if c is None:
            if last is None:
                raise ValueError(f"path data starts with a number: {d[:40]!r}…")
            c = last
        rel = c.islower()
        C = c.upper()

        def at(p: tuple[float, float]) -> tuple[float, float]:
            return (p[0] + here[0], p[1] + here[1]) if rel else p

        if C == "Z":
            out.append(("Z", []))
            here, control, quad, last = start, None, None, None
            continue
        if C == "M":
            here = start = at(s.point())
            out.append(("M", [here]))
            control = quad = None
            last = "l" if rel else "L"
            continue
        if C in "LHV":
            if C == "L":
                p = at(s.point())
            elif C == "H":
                x = s.number()
                p = (x + here[0] if rel else x, here[1])
            else:
                y = s.number()
                p = (here[0], y + here[1] if rel else y)
            out.append(("L", [p]))
            here, control, quad = p, None, None
        elif C in "CS":
            if C == "C":
                c1 = at(s.point())
            else:
                c1 = (2 * here[0] - control[0], 2 * here[1] - control[1]) if control else here
            c2 = at(s.point())
            p = at(s.point())
            out.append(("C", [c1, c2, p]))
            here, control, quad = p, c2, None
        elif C in "QT":
            if C == "Q":
                q = at(s.point())
            else:
                q = (2 * here[0] - quad[0], 2 * here[1] - quad[1]) if quad else here
            p = at(s.point())
            c1 = (here[0] + 2 / 3 * (q[0] - here[0]), here[1] + 2 / 3 * (q[1] - here[1]))
            c2 = (p[0] + 2 / 3 * (q[0] - p[0]), p[1] + 2 / 3 * (q[1] - p[1]))
            out.append(("C", [c1, c2, p]))
            here, control, quad = p, None, q
        else:  # A
            rx, ry, angle = s.number(), s.number(), s.number()
            large, sweep = s.flag(), s.flag()
            p = at(s.point())
            for cubic in arc(here, rx, ry, angle, large, sweep, p):
                out.append(("C", cubic))
            here, control, quad = p, None, None
        last = c
    return out


def number(v: float) -> str:
    text = f"{round(v, 2):.2f}".rstrip("0").rstrip(".")
    return "0" if text in ("-0", "") else text


def written(d: str) -> str:
    """`d` normalized: absolute M, L, C, and Z, numbers to two places, commas between them."""
    return "".join(c + ",".join(f"{number(x)},{number(y)}" for x, y in points) for c, points in absolute(d))


# --- Build and check -------------------------------------------------------------------------


def build(cache: Path | None) -> None:
    tar = tarfile.open(fileobj=io.BytesIO(fetch(cache)), mode="r:gz")
    read = lambda name: tar.extractfile(f"package/{name}").read().decode("utf-8")  # noqa: E731
    icons = []
    for entry in sorted(catalog(read("dist/index.mjs")), key=lambda e: e["name"]):
        if BRANDS in entry["categories"]:
            continue
        unknown = set(entry["categories"]) - CATEGORIES
        if unknown:
            sys.exit(f"{entry['name']}: categories this does not know: {sorted(unknown)}")
        icon = {
            "name": entry["name"],
            "tags": sorted({t for t in entry["tags"] if not t.startswith("*")} - {entry["name"]}),
            "categories": entry["categories"],
        }
        for weight in WEIGHTS:
            file = entry["name"] if weight == "regular" else f"{entry['name']}-{weight}"
            svg = read(f"assets/{weight}/{file}.svg")
            if 'viewBox="0 0 256 256"' not in svg:
                sys.exit(f"{file}.svg is not drawn on a 256 by 256 view")
            paths = re.findall(r'<path d="([^"]+)"', svg)
            if not paths:
                sys.exit(f"{file}.svg draws no path")
            # A fill icon is one path; where one is drawn in several, they are one shape here.
            icon[weight] = "".join(written(d) for d in paths)
        icons.append(icon)
    OUT.parent.mkdir(parents=True, exist_ok=True)
    doc = {
        "_comment": (
            f"Phosphor Icons {VERSION} (MIT, LICENSE-phosphor.txt), from @phosphor-icons/core "
            f"(sha256 {SHA256}), written by scripts/build_icons.py: every icon but brands, in the "
            "regular and fill weights, each path in absolute M, L, C, and Z on a 256 by 256 view."
        ),
        "name": "Phosphor",
        "version": VERSION,
        "license": "MIT",
        "view": VIEW,
        "weights": WEIGHTS,
        "icons": icons,
    }
    OUT.write_text(json.dumps(doc, ensure_ascii=False, separators=(",", ":")) + "\n", encoding="utf-8")
    LICENSE.write_text(read("LICENSE"), encoding="utf-8")
    COMPILED.write_text(compiled(doc), encoding="utf-8")
    WEATHER_FILE.write_text(weather_file(doc), encoding="utf-8")
    TOOLBAR_FILE.write_text(toolbar_file(doc), encoding="utf-8")
    print(f"wrote {len(icons)} icons to {OUT.relative_to(ROOT)} ({OUT.stat().st_size // 1024} KB)")


def compiled(doc: dict) -> str:
    """The Rust file of the icons the engine compiles in, from `doc`."""
    by_name = {icon["name"]: icon for icon in doc["icons"]}
    lines = [
        f"//! Phosphor Icons {VERSION} (MIT, docs/icons/LICENSE-phosphor.txt) the engine draws itself, each",
        "//! path on the icons' 256 by 256 view: `map-pin` in its fill weight, the pin a picture's `pin`",
        "//! draws, and a forecast's icons in their regular weight (ADR-0030). Written by",
        "//! scripts/build_icons.py from docs/icons/phosphor.json, which its `--check` holds this file to.",
        "",
    ]
    for constant, name, weight in COMPILED_ICONS:
        lines.append(f"/// `{name}`, {weight}.")
        lines.append(f'pub const {constant}: &str = "{by_name[name][weight]}";')
    digest = hashlib.sha256(weather_file(doc).encode("utf-8")).hexdigest()
    lines += [
        "",
        "/// The SHA-256 of `icons/weather.txt`, a forecast's icons in their regular weight, a line each,",
        "/// `name path` (ADR-0030): what is handed over where they are not compiled in is held to it.",
        f'pub const WEATHER_SHA256: &str = "{digest}";',
    ]
    return "\n".join(lines) + "\n"


def weather_file(doc: dict) -> str:
    """A forecast's icons, a line each: `name path`, in their regular weight."""
    by_name = {icon["name"]: icon for icon in doc["icons"]}
    return "".join(f"{name} {by_name[name]['regular']}\n" for name in WEATHER_ICONS)


def toolbar_file(doc: dict) -> str:
    """The editor's toolbar icons as a TypeScript module: each by its name, its regular path."""
    by_name = {icon["name"]: icon for icon in doc["icons"]}
    lines = [
        f"// The editor's toolbar icons (lift-up ATTR-5): Phosphor Icons {VERSION} (MIT,",
        "// docs/icons/LICENSE-phosphor.txt) in their regular weight, each path on the icons' 256 by 256",
        "// view, as the set that ships draws them (docs/icons/phosphor.json). Written by",
        "// scripts/build_icons.py --toolbar, which its `--check` holds this file to.",
        "",
        "/** The icons' view: `[x, y, width, height]`. */",
        f"export const VIEW: [number, number, number, number] = {json.dumps(doc['view'])};",
        "",
        "/** Each icon's path, by its name. */",
        "export const TOOLBAR_ICONS: Record<string, string> = {",
    ]
    for name in TOOLBAR_ICONS:
        lines.append(f'  "{name}": "{by_name[name]["regular"]}",')
    lines.append("};")
    return "\n".join(lines) + "\n"


WRITTEN = re.compile(r"(?:M-?[\d.]+,-?[\d.]+|L-?[\d.]+,-?[\d.]+|C(?:-?[\d.]+,){5}-?[\d.]+|Z)+")
KEBAB = re.compile(r"[a-z0-9]+(?:-[a-z0-9]+)*")


def check() -> bool:
    if not OUT.exists():
        print(f"FAIL {OUT.relative_to(ROOT)} is missing: run scripts/build_icons.py")
        return False
    if not LICENSE.exists():
        print(f"FAIL {LICENSE.relative_to(ROOT)} is missing: run scripts/build_icons.py")
        return False
    doc = json.loads(OUT.read_text(encoding="utf-8"))
    problems = []
    if doc.get("view") != VIEW or doc.get("weights") != WEIGHTS or doc.get("version") != VERSION:
        problems.append("the view, the weights, or the version is not this script's")
    names = [icon["name"] for icon in doc["icons"]]
    if names != sorted(names) or len(set(names)) != len(names):
        problems.append("the icons are not sorted by name, each once")
    for icon in doc["icons"]:
        name = icon["name"]
        if not KEBAB.fullmatch(name):
            problems.append(f"{name}: not a name in kebab case")
        if not set(icon["categories"]) <= CATEGORIES:
            problems.append(f"{name}: categories {icon['categories']}")
        for weight in WEIGHTS:
            d = icon.get(weight, "")
            if not d.startswith("M") or not WRITTEN.fullmatch(d):
                problems.append(f"{name} ({weight}): not absolute M, L, C, and Z")
                continue
            # Where the outline passes, each segment's end: a curve's controls may stand outside.
            ends = [
                [float(n) for n in numbers.split(",")[-2:]]
                for command, numbers in re.findall(r"([MLC])([^MLCZ]*)", d)
            ]
            if any(v < VIEW[0] - 1 or v > VIEW[2] + 1 for end in ends for v in end):
                problems.append(f"{name} ({weight}): drawn outside its view")
    if not COMPILED.exists() or COMPILED.read_text(encoding="utf-8") != compiled(doc):
        problems.append(f"{COMPILED.relative_to(ROOT)} is not the set's: run scripts/build_icons.py --weather")
    if not WEATHER_FILE.exists() or WEATHER_FILE.read_text(encoding="utf-8") != weather_file(doc):
        problems.append(f"{WEATHER_FILE.relative_to(ROOT)} is not the set's: run scripts/build_icons.py --weather")
    if not TOOLBAR_FILE.exists() or TOOLBAR_FILE.read_text(encoding="utf-8") != toolbar_file(doc):
        problems.append(f"{TOOLBAR_FILE.relative_to(ROOT)} is not the set's: run scripts/build_icons.py --toolbar")
    for p in problems[:20]:
        print(f"FAIL {p}")
    if not problems:
        print(f"ok   {len(names)} icons in {len(WEIGHTS)} weights, each path absolute and in its view")
    return not problems


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--check", action="store_true", help="check only; no network, no writes")
    ap.add_argument("--cache", type=Path, help="directory of upstream files keyed by sha256 (read, then filled)")
    ap.add_argument("--weather", action="store_true", help="write the icons the engine compiles in from the set; no network")
    ap.add_argument("--toolbar", action="store_true", help="write the editor's toolbar icons from the set; no network")
    args = ap.parse_args()
    if args.toolbar:
        doc = json.loads(OUT.read_text(encoding="utf-8"))
        TOOLBAR_FILE.write_text(toolbar_file(doc), encoding="utf-8")
    elif args.weather:
        doc = json.loads(OUT.read_text(encoding="utf-8"))
        COMPILED.write_text(compiled(doc), encoding="utf-8")
        WEATHER_FILE.parent.mkdir(exist_ok=True)
        WEATHER_FILE.write_text(weather_file(doc), encoding="utf-8")
    elif not args.check:
        build(args.cache)
    sys.exit(0 if check() else 1)


if __name__ == "__main__":
    main()

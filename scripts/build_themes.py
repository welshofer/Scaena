#!/usr/bin/env python3
"""The shipped themes, each written from its spec and a shared base (PLAN T.0a, ADR-0031).

- **The registry,** `docs/themes/registry.json`, lists each theme: its slug, its name, its family,
  its geometry family, its fonts' keys (as `build_bundle_fonts.py`'s pinned table names them), and
  its file or files.
  Its description, mode, colors, and type voice are its spec's, copied in here for the clients
  that list themes.
- **A spec,** `docs/themes/<slug>.spec.json`, holds only what sets the theme apart from the shared
  base, `docs/themes/base.json`: its name and description, its mode, its seven colors, its data
  palettes, its families, its type voice and each role's deltas, its margin, its shader palettes,
  its code's look (`code`, theme format 0.18), and any chart, table, stroke, or shader preset unlike the base's.
- **The base** holds what every theme shares: motion, shadows, density, space, radius, stroke, the
  color roles, the grid but its margin, the shader presets, the charts and tables, and the three
  type voices (Dusk's, Ember's, and Gallery's) a spec starts its roles from. A key the base sets
  to `null` is the spec's to give, or left out; a spec's `null` takes the base's key away. Either
  way a key stands where the base puts it, so the file comes out as the four are written.

This writes every key of a theme file but its `layouts` and `furniture`, which
`scripts/build_layouts.py` writes and this keeps as they are. It writes `scaena_ops::shipped`'s
table (`crates/scaena-ops/src/shipped/table.rs`), where its build script finds each font
(`crates/scaena-ops/src/shipped/fonts.tsv`: in git for the four, else in the sha256 cache, PLAN
T.0c), and the browser's (`web/src/themes.ts`) from the registry, and the registry's copies of
each spec.

It refuses a new theme's spec whose colors miss ADR-0031's bars (decision 3), worked as
`scaena_core::palette` works them: WCAG's contrast for the ratio, Oklab for the lightness that
would meet it. Dusk, Daybreak, Ember, and Gallery are held to their files as they ship until
PLAN T.6 (call 10): their cards sit 1.06–1.11:1 off the page, and Gallery's line 1.29:1.

    python3 scripts/build_themes.py           # write the themes, the registry's copies, and the tables
    python3 scripts/build_themes.py --check   # fail where any is not what this writes
"""

import json
import math
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
THEMES = ROOT / "docs/themes"
REGISTRY = THEMES / "registry.json"
BASE = THEMES / "base.json"
SHIPPED = ROOT / "crates/scaena-ops/src/shipped/table.rs"
# Where each font the themes name comes from, which scaena-ops' build script reads (PLAN T.0c).
CARRIED = ROOT / "crates/scaena-ops/src/shipped/fonts.tsv"
WEB = ROOT / "web/src/themes.ts"
# Where the fonts git holds are, by the path a theme gives them (`fonts/…`): the four's. Every
# other family's files are the upstream files in the cache `build_bundle_fonts.py --fetch` fills,
# by sha256 (PLAN T.0b, T.0c).
FONTS = ROOT / "docs/examples/fonts"

# Held to their files as they ship, not to the bars, until PLAN T.6 (ADR-0031, call 10).
AS_THEY_SHIP = {"dusk", "daybreak", "ember", "gallery"}

COLORS = ["ink", "paper", "paper-2", "accent", "accent-2", "muted", "line"]
FAMILIES = ["Stage", "Executive", "Analyst", "Civic", "Launch", "Editorial", "Scholar", "Classroom", "Lens", "Ceremony", "Craft", "Systems"]
# What a spec may hold, and where in the theme file each goes.
SPEC_KEYS = {
    "name": ("name",),
    "description": ("description",),
    "colors": ("tokens", "color"),
    "data": ("tokens", "data"),
    "stroke": ("tokens", "stroke"),
    "families": ("type", "families"),
    "scale": ("type", "scale"),
    "roles": ("type", "roles"),
    "margin": ("grid", "margin"),
    "shaders": ("shaders",),
    "charts": ("charts",),
    "tables": ("tables",),
    "code": ("code",),
}
# What a spec holds that is the registry's, not the file's.
SPEC_ONLY = {"mode", "voice", "contrast"}


class Refused(Exception):
    pass


# Colors, as `scaena_core::palette` reads them.


def rgb(hex_: str) -> tuple[int, int, int]:
    return tuple(int(hex_[i : i + 2], 16) for i in (1, 3, 5))


def linear(byte: float) -> float:
    c = byte / 255
    return c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4


def luminance(c: tuple[int, int, int]) -> float:
    r, g, b = (linear(x) for x in c)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b


def contrast(x: tuple[int, int, int], y: tuple[int, int, int]) -> float:
    a, b = luminance(x), luminance(y)
    return (max(a, b) + 0.05) / (min(a, b) + 0.05)


def oklab(c: tuple[int, int, int]) -> tuple[float, float, float]:
    r, g, b = (linear(x) for x in c)
    l_ = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b) ** (1 / 3)
    m_ = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b) ** (1 / 3)
    s_ = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b) ** (1 / 3)
    return (
        0.2104542553 * l_ + 0.7936177850 * m_ - 0.0040720468 * s_,
        1.9779984951 * l_ - 2.4285922050 * m_ + 0.4505937099 * s_,
        0.0259040371 * l_ + 0.7827717662 * m_ - 0.8086757660 * s_,
    )


def from_oklab(l: float, a: float, b: float) -> tuple[int, int, int]:
    l_ = l + 0.3963377774 * a + 0.2158037573 * b
    m_ = l - 0.1055613458 * a - 0.0638541728 * b
    s_ = l - 0.0894841775 * a - 1.2914855480 * b
    lin = (
        4.0767416621 * l_**3 - 3.3077115913 * m_**3 + 0.2309699292 * s_**3,
        -1.2684380046 * l_**3 + 2.6097574011 * m_**3 - 0.3413193965 * s_**3,
        -0.0041960863 * l_**3 - 0.7034186147 * m_**3 + 1.7076147010 * s_**3,
    )

    def encode(v: float) -> int:
        v = min(max(v, 0.0), 1.0)
        v = 12.92 * v if v <= 0.0031308 else 1.055 * v ** (1 / 2.4) - 0.055
        return round(v * 255)

    return tuple(encode(v) for v in lin)


def lightness_for(color: str, ground: str, bar: float) -> float | None:
    """The Oklab lightness `color` would need, its hue and chroma kept, to read at `bar` on
    `ground`, moved away from the ground; None where no lightness does."""
    _, a, b = oklab(rgb(color))
    darker = luminance(rgb(color)) < luminance(rgb(ground))
    lo, hi = (0.0, oklab(rgb(color))[0]) if darker else (oklab(rgb(color))[0], 1.0)
    reads = lambda l: contrast(from_oklab(l, a, b), rgb(ground)) >= bar  # noqa: E731
    end = lo if darker else hi
    if not reads(end):
        return None
    for _ in range(40):
        mid = (lo + hi) / 2
        if reads(mid) == darker:
            lo = mid
        else:
            hi = mid
    return lo if darker else hi


def bars(colors: dict, high: bool) -> list[tuple[str, str, float]]:
    """ADR-0031's bars (decision 3): each color, the ground it reads on, and the ratio it needs."""
    text = 7.0 if high else 4.5
    out = []
    for ground in ["paper", "paper-2"]:
        out.append(("ink", ground, 7.0))
        out += [("muted", ground, text), ("accent", ground, text)]
        out.append(("accent-2", ground, 3.0))
    out.append(("paper", "accent", 4.5))
    out.append(("line", "paper", 1.3))
    out.append(("paper-2", "paper", 1.2))
    return out


def missed(slug: str, spec: dict) -> list[str]:
    """Each bar `spec`'s colors miss, said with by how much and the Oklab lightness that meets it."""
    colors = spec["colors"]
    out = []
    for color, ground, bar in bars(colors, spec.get("contrast") == "high"):
        ratio = contrast(rgb(colors[color]), rgb(colors[ground]))
        if ratio + 1e-9 >= bar:
            continue
        said = f"{slug}: {color} {colors[color]} reads {ratio:.2f}:1 on {ground} {colors[ground]}, under {bar:g}:1"
        need = lightness_for(colors[color], colors[ground], bar)
        if need is not None:
            said += f"; its Oklab lightness {oklab(rgb(colors[color]))[0]:.3f} would need to be {need:.3f}"
        out.append(said)
    return out


# A theme from its spec.


def merged(base, delta):
    """`delta` over `base`, each key where the base puts it, the delta's own after; a `null` in
    the delta takes a key away, and one in the base is left out unless the delta gives it."""
    if isinstance(base, dict) and isinstance(delta, dict):
        out = {}
        for k, v in base.items():
            if k in delta:
                if delta[k] is not None:
                    out[k] = merged(v, delta[k])
            elif v is not None:
                out[k] = merged(v, {}) if isinstance(v, dict) else v
        for k, v in delta.items():
            if k not in base and v is not None:
                out[k] = v
        return out
    return delta


def themed(slug: str, spec: dict, base: dict) -> dict:
    """The theme file `spec` makes on `base`, its layouts left empty."""
    extra = set(spec) - set(SPEC_KEYS) - SPEC_ONLY
    if extra:
        raise Refused(f"{slug}: the spec holds what a spec does not: {sorted(extra)}")
    if list(spec.get("colors", {})) != COLORS:
        raise Refused(f"{slug}: the spec's colors are not {', '.join(COLORS)}, in that order")
    if spec.get("mode") not in ("light", "dark"):
        raise Refused(f"{slug}: the spec's mode is light or dark")
    dark = luminance(rgb(spec["colors"]["paper"])) < luminance(rgb(spec["colors"]["ink"]))
    if dark != (spec["mode"] == "dark"):
        raise Refused(f"{slug}: its paper and ink are not a {spec['mode']} theme's")
    voices = base["voices"]
    if spec.get("voice") not in voices:
        raise Refused(f"{slug}: its voice is one of {', '.join(voices)}")
    if slug not in AS_THEY_SHIP:
        wrong = missed(slug, spec)
        if wrong:
            raise Refused("\n".join(wrong))
    voice = voices[spec["voice"]]
    delta: dict = {"type": {"scale": merged(voice["scale"], spec.get("scale", {})), "roles": merged(voice["roles"], spec.get("roles", {}))}}
    for key, path in SPEC_KEYS.items():
        if key in ("scale", "roles") or key not in spec:
            continue
        at = delta
        for p in path[:-1]:
            at = at.setdefault(p, {})
        at[path[-1]] = spec[key]
    theme = {k: v for k, v in base.items() if k != "voices"}
    theme = merged(theme, delta)
    for need in ["name", "description"]:
        if need not in theme:
            raise Refused(f"{slug}: the spec gives no {need}")
    return theme


# Written as the four are.


def compact(v) -> str:
    """JSON on one line, a space inside each brace, as a theme file sets a shadow or a layout."""
    if isinstance(v, dict):
        if not v:
            return "{}"
        return "{ " + ", ".join(f"{json.dumps(k, ensure_ascii=False)}: {compact(x)}" for k, x in v.items()) + " }"
    if isinstance(v, list):
        return "[" + ", ".join(compact(x) for x in v) + "]"
    return json.dumps(v, ensure_ascii=False)


def dumped(v, path=(), depth=0) -> str:
    pad, end = "  " * (depth + 1), "  " * depth
    if path[:2] == ("tokens", "shadow") and len(path) == 3:
        return compact(v)
    if isinstance(v, dict):
        if not v:
            return "{}"
        inner = ",\n".join(f"{pad}{json.dumps(k, ensure_ascii=False)}: {dumped(x, path + (k,), depth + 1)}" for k, x in v.items())
        return "{\n" + inner + "\n" + end + "}"
    if isinstance(v, list):
        if not v:
            return "[]"
        return "[\n" + ",\n".join(f"{pad}{dumped(x, path, depth + 1)}" for x in v) + "\n" + end + "]"
    return json.dumps(v, ensure_ascii=False)


def block(text: str, key: str) -> list[str] | None:
    """The lines of the top-level `key`'s value in a theme file, from its first to its last."""
    lines = text.split("\n")
    head = f"  {json.dumps(key)}: {{"
    if head not in lines:
        return None
    start = lines.index(head)
    end = next(i for i in range(start + 1, len(lines)) if lines[i] in ("  },", "  }"))
    return lines[start : end + 1]


def written(theme: dict, old: str | None, base: dict) -> str:
    """The theme file: `theme`'s keys as the four are written, and the layouts and furniture of
    the file as it was (`old`), which build_layouts.py writes; a new file's layouts empty."""
    layouts = block(old, "layouts") if old else None
    furniture = block(old, "furniture") if old else None
    keys = {**theme, "layouts": {}}
    placed = [k for k in base if k != "voices"]
    order = [k for k in placed if k in keys] + [k for k in keys if k not in placed]
    parts = []
    for k in order:
        if k == "layouts":
            lines = layouts or ['  "layouts": {', "  }"]
            parts.append("\n".join(lines).rstrip(","))
        else:
            parts.append(f"  {json.dumps(k)}: {dumped(keys[k], (k,), 1)}")
    if furniture:
        parts.append("\n".join(furniture).rstrip(","))
    return "{\n" + ",\n".join(parts) + "\n}\n"


# The registry and what reads it.


def font_keys() -> dict[str, str]:
    """Each family's key in `build_bundle_fonts.py`'s pinned table (PLAN T.0b), by its name."""
    sys.path.insert(0, str(ROOT / "scripts"))
    import build_bundle_fonts  # noqa: PLC0415

    return {f["name"]: key for key, f in build_bundle_fonts.FAMILIES.items()}


def fonts_of(theme: dict) -> list[str]:
    """The keys of the families `theme` names, in its order; a family the table does not pin is refused."""
    keys = font_keys()
    missing = [f["family"] for f in theme["type"]["families"].values() if f["family"] not in keys]
    if missing:
        raise Refused(f"{theme['name']} names families build_bundle_fonts.py does not pin: {missing}")
    return [keys[f["family"]] for f in theme["type"]["families"].values()]


def font_files(themes: list[dict]) -> list[str]:
    """Every font file the themes name, by the path they give it: for each theme in turn, the
    uprights it adds, then their italics."""
    out: list[str] = []
    for theme in themes:
        families = theme["type"]["families"].values()
        for f in [f["file"] for f in families] + [f["italic"]["file"] for f in families if "italic" in f]:
            if f not in out:
                out.append(f)
    return out


def pinned_families() -> dict:
    """`build_bundle_fonts.py`'s pinned table (PLAN T.0b), by key."""
    sys.path.insert(0, str(ROOT / "scripts"))
    import build_bundle_fonts  # noqa: PLC0415

    return build_bundle_fonts.FAMILIES


def font_sources(themes: list[dict]) -> list[tuple[str, str, str]]:
    """Every font file the themes name, in `font_files`' order, as `(path, family key, source)`:
    its source the file's path in the repository where git holds it (the four's, in
    docs/examples/fonts), else `sha256:…`, the upstream file in the cache by its pin. A face the
    table does not pin is refused, naming its family."""
    keys, pins = font_keys(), pinned_families()
    out: dict[str, tuple[str, str, str]] = {}
    for theme in themes:
        families = theme["type"]["families"].values()
        faces = [(f["file"], f["family"], "upright") for f in families]
        faces += [(f["italic"]["file"], f["family"], "italic") for f in families if "italic" in f]
        for path, name, face in faces:
            if path in out:
                continue
            if name not in keys:
                raise Refused(f"{theme['name']} names families build_bundle_fonts.py does not pin: {[name]}")
            key = keys[name]
            if (FONTS.parent / path).is_file():
                source = (FONTS.parent / path).relative_to(ROOT).as_posix()
            elif pins[key].get(face):
                source = "sha256:" + pins[key][face][1]
            else:
                raise Refused(f"{theme['name']} names {name}'s {face} face as {path}, which build_bundle_fonts.py pins none of")
            out[path] = (path, key, source)
    order = font_files(themes)
    return [out[p] for p in order]


def carried_tsv(themes: list[dict]) -> str:
    out = [
        "# Every font the themes that ship name, and where scaena-ops' build script finds it (PLAN T.0c):",
        "# the path a theme gives it, its family's key in build_bundle_fonts.py's pinned table, and its",
        "# source: a file in the repository, or `sha256:…`, an upstream file in the cache that",
        "# `build_bundle_fonts.py --fetch --shipped` fills. Written by scripts/build_themes.py; do not edit.",
    ]
    out += ["\t".join(row) for row in font_sources(themes)]
    return "\n".join(out) + "\n"


def shipped_rs(entries: list[dict], themes: list[dict]) -> str:
    out = [
        "//! The themes that ship, and the fonts they name, as `docs/themes/registry.json` lists them.",
        "//! Written by `scripts/build_themes.py` from the registry; do not edit by hand.",
        "",
        "use super::Shipped;",
        "",
        "/// Every theme that ships, in the registry's order.",
        f"pub const THEMES: [Shipped; {len(entries)}] = [",
    ]
    for e, theme in zip(entries, themes):
        first = e["files"][0]
        out += [
            "    Shipped {",
            f"        name: {json.dumps(e['slug'])},",
            f"        file: {json.dumps(Path(first).name)},",
            f'        text: include_str!("../../../../{first}"),',
        ]
        line = f"        font_files: &[{', '.join(json.dumps(f) for f in font_files([theme]))}],"
        # As rustfmt sets it: an array past the 120 columns of rustfmt.toml on a line of its own each.
        if len(line) <= 120:
            out.append(line)
        else:
            out += ["        font_files: &["] + [f"            {json.dumps(f)}," for f in font_files([theme])] + ["        ],"]
        out.append("    },")
    out += ["];", ""]
    return "\n".join(out)


def ident(path: str) -> str:
    stem = Path(path).name.split(".")[0]
    words = re.split(r"[^A-Za-z0-9]+", stem)
    name = "".join(w[:1].upper() + w[1:] for w in words if w)
    return name[:1].lower() + name[1:]


def themes_ts(entries: list[dict], themes: list[dict]) -> str:
    sources = font_sources(themes)
    imports = [(ident(e["slug"]), "../../" + e["files"][0]) for e in entries]
    # Each drawn small by the CPU painter at build, which `web/vite.config.ts` runs first (PLAN T.0c).
    imports += [(ident(e["slug"]) + "Thumbnail", f"../../target/themes/{e['slug']}.png") for e in entries]
    # A font git holds from the repository; any other from the sha256 cache, by its pin.
    imports += [
        (ident(path) + "Font", "../../" + source if not source.startswith("sha256:") else "@scaena/font-cache/" + source.removeprefix("sha256:"))
        for path, _, source in sources
    ]
    out = [
        "// What the editor starts a new deck from (PLAN 2.12): the themes that ship, which share one",
        "// vocabulary on one grid (SPEC §3.6), and the fonts they name, as docs/themes/registry.json",
        "// lists them. The build copies each file beside the pages, and the worker fetches them to make a",
        "// deck, and a theme to bring its layout library into a bundle's older copy of it (PLAN 3.31). A",
        "// single-file export's worker is built with `no-themes.ts`: its page plays a bundle, and never",
        "// makes one. Each theme's thumbnail is its title slide drawn at build by the CPU painter, and a",
        "// font git does not hold comes from the sha256 cache (PLAN T.0c). Written by",
        "// scripts/build_themes.py from the registry; do not edit by hand.",
    ]
    out += [f'import {name} from "{path}?url";' for name, path in imports]
    out += [
        "",
        "/** Each theme by its name: its file's name in a bundle (`themes/…`), where the page has it, and",
        " * where it has its thumbnail, its title slide drawn small. */",
        "export const themes: Record<string, { file: string; url: string; thumbnail: string }> = {",
    ]
    for e in entries:
        slug = ident(e["slug"])
        out.append(f'  {e["name"]}: {{ file: {json.dumps(Path(e["files"][0]).name)}, url: {slug}, thumbnail: {slug}Thumbnail }},')
    out += [
        "};",
        "",
        "/** The fonts the themes name, by the paths they give them, and where the page has each: each",
        " * family's, and its italic's (PLAN 2.40). */",
        "export const fonts: Record<string, string> = {",
    ]
    out += [f"  {json.dumps(path)}: {ident(path)}Font," for path, _, _ in sources]
    out += ["};", ""]
    return "\n".join(out)


REGISTRY_KEYS = ["slug", "name", "family", "geometry", "fonts", "files", "description", "mode", "colors", "voice"]


def registered(entries: list[dict]) -> str:
    lines = ["{", '  "themes": [']
    for i, e in enumerate(entries):
        lines.append("    {")
        keys = [k for k in REGISTRY_KEYS if k in e]
        for j, k in enumerate(keys):
            lines.append(f"      {json.dumps(k)}: {compact(e[k])}{',' if j < len(keys) - 1 else ''}")
        lines.append("    }" + ("," if i < len(entries) - 1 else ""))
    lines += ["  ]", "}", ""]
    return "\n".join(lines)


def build(registry: dict, base: dict, specs: dict) -> tuple[dict[Path, str], list[str]]:
    """Every file this writes, by its path, and what is wrong with the registry or a spec."""
    wrong: list[str] = []
    out: dict[Path, str] = {}
    entries, themes = [], []
    slugs = [e["slug"] for e in registry["themes"]]
    if len(set(slugs)) != len(slugs):
        wrong.append(f"a slug is listed twice: {slugs}")
    for e in registry["themes"]:
        slug = e["slug"]
        spec = specs.get(slug)
        if spec is None:
            wrong.append(f"{slug}: no spec at docs/themes/{slug}.spec.json")
            continue
        try:
            theme = themed(slug, spec, base)
        except Refused as r:
            wrong.append(str(r))
            continue
        if e.get("name") != theme["name"]:
            wrong.append(f"{slug}: the registry names it {e.get('name')!r}, its spec {theme['name']!r}")
        if e.get("family") not in FAMILIES:
            wrong.append(f"{slug}: its family is one of {', '.join(FAMILIES)}")
        if e.get("geometry") not in slugs:
            wrong.append(f"{slug}: its geometry family is a theme's slug: {e.get('geometry')!r}")
        try:
            fonts = fonts_of(theme)
        except Refused as r:
            wrong.append(str(r))
            continue
        if e.get("fonts") != fonts:
            wrong.append(f"{slug}: the registry's fonts {e.get('fonts')} are not its families' {fonts}")
        try:
            font_sources([theme])
        except Refused as r:
            wrong.append(str(r))
        for f in e.get("files") or []:
            path = ROOT / f
            old = path.read_text(encoding="utf-8") if path.exists() else None
            out[path] = written(theme, old, base)
        if not e.get("files"):
            wrong.append(f"{slug}: the registry gives it no file")
        entries.append({**e, **{k: spec[k] for k in ("description", "mode", "colors", "voice")}})
        themes.append(theme)
    out[REGISTRY] = registered(entries)
    out[SHIPPED] = shipped_rs(entries, themes)
    out[CARRIED] = carried_tsv(themes)
    out[WEB] = themes_ts(entries, themes)
    return out, wrong


def read() -> tuple[dict, dict, dict]:
    registry = json.loads(REGISTRY.read_text(encoding="utf-8"))
    base = json.loads(BASE.read_text(encoding="utf-8"))
    specs = {}
    for p in sorted(THEMES.glob("*.spec.json")):
        specs[p.name.removesuffix(".spec.json")] = json.loads(p.read_text(encoding="utf-8"))
    return registry, base, specs


def main():
    check = "--check" in sys.argv[1:]
    registry, base, specs = read()
    unlisted = sorted(set(specs) - {e["slug"] for e in registry["themes"]})
    out, wrong = build(registry, base, specs)
    if unlisted:
        wrong.append(f"specs the registry does not list: {unlisted}")
    if wrong:
        sys.exit("error: " + "\nerror: ".join(wrong))
    stale = [p for p, text in out.items() if not p.exists() or p.read_text(encoding="utf-8") != text]
    if check:
        if stale:
            sys.exit(f"not what scripts/build_themes.py writes: {', '.join(str(p.relative_to(ROOT)) for p in stale)}")
        print(f"{len(registry['themes'])} themes, each as its spec writes it")
        return
    for p in stale:
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(out[p], encoding="utf-8")
    print(f"{len(registry['themes'])} themes; wrote {len(stale)} files")


if __name__ == "__main__":
    main()

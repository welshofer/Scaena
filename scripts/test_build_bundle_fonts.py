"""Fonts by sha256 (PLAN T.0b): every family a theme names pinned, fetched into a cache and read
back from it offline, and each `SOURCES.md` held to the files beside it."""

import hashlib
import re
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_bundle_fonts as fonts  # noqa: E402

ADR = fonts.ROOT / "docs/adr/0031-fifty-themes-generated-from-specs.md"


def sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def provenance(rows: list[tuple[str, str]]) -> str:
    """A `SOURCES.md` as the scripts write one: a row for each file, with its subset's sha256."""
    lines = ["# Fonts: provenance", "", "| Bundle file | Upstream path | Subset sha256 |", "|---|---|---|"]
    lines += [f"| `{file}` | `ofl/x/{file}` | `{digest[:16]}…` |" for file, digest in rows]
    return "\n".join(lines) + "\n"


class Table(unittest.TestCase):
    def test_every_family_adr_0031_names_is_pinned(self):
        table = ADR.read_text(encoding="utf-8").split("## The fonts", 1)[1]
        named = {line.split("|")[1].strip() for line in table.splitlines() if re.match(r"\| [A-Z]", line)}
        named.discard("Family")
        self.assertEqual(len(named), 46)
        self.assertEqual(named, {f["name"] for f in fonts.FAMILIES.values()})

    def test_each_row_says_where_its_files_are_what_they_hash_to_and_what_they_draw(self):
        for key, family in fonts.FAMILIES.items():
            self.assertEqual(key, family["name"].lower().replace(" ", "-"))
            for path, digest in fonts.family_files(family):
                self.assertRegex(digest, r"^[0-9a-f]{64}$", path)
                if family.get("source", fonts.GOOGLE_FONTS) == fonts.GOOGLE_FONTS:
                    self.assertTrue(path.startswith("ofl/"), f"{path}: OFL only")
            self.assertIn("Latin", family["scripts"], key)
            for axis, (lo, hi) in family["axes"].items():
                self.assertLess(lo, hi, f"{key} {axis}")
        # Pretendard comes from its own repository, at its tag, and reserves its name.
        pretendard = fonts.FAMILIES["pretendard"]
        self.assertEqual(pretendard["source"], ("orioncactus/pretendard", "v1.3.9"))
        self.assertIn("Pretendard", pretendard["reserved"])
        # Families of a script with no italic convention have none (ADR-0031, call 6).
        upright = {k for k, f in fonts.FAMILIES.items() if f["italic"] is None}
        self.assertEqual(upright, {"noto-sans-jp", "noto-serif-jp", "pretendard"})

    def test_the_shipped_fonts_pins_are_the_tables(self):
        self.assertTrue(fonts.pins_agree())


class Cache(unittest.TestCase):
    def test_a_clean_cache_is_filled_then_read_offline(self):
        data = b"a font's bytes"
        asked: list[str] = []

        def get(url: str) -> bytes:
            asked.append(url)
            return data

        def unreachable(url: str) -> bytes:
            raise OSError(f"no network: {url}")

        with tempfile.TemporaryDirectory() as cache:
            cache = Path(cache) / "fonts"
            got = fonts.pinned(fonts.PRETENDARD, "LICENSE", sha(data), cache, offline=False, get=get)
            self.assertEqual(got, data)
            self.assertEqual(asked, ["https://raw.githubusercontent.com/orioncactus/pretendard/v1.3.9/LICENSE"])
            self.assertTrue((cache / sha(data)).exists())
            # Offline, from the cache alone.
            self.assertEqual(fonts.pinned(fonts.PRETENDARD, "LICENSE", sha(data), cache, True, unreachable), data)
            # A file the cache lacks fails offline, and asks nothing.
            with self.assertRaises(SystemExit):
                fonts.pinned(fonts.PRETENDARD, "OTHER", sha(b"other"), cache, True, unreachable)

    def test_bytes_that_differ_from_the_pin_are_refused(self):
        with tempfile.TemporaryDirectory() as cache:
            with self.assertRaises(SystemExit):
                fonts.pinned(fonts.GOOGLE_FONTS, "ofl/x/X.ttf", sha(b"pinned"), Path(cache), False, lambda _: b"other")
            self.assertEqual(list(Path(cache).iterdir()), [])


class Sources(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())
        self.font = b"subset bytes"
        (self.dir / "A-VF.ttf").write_bytes(self.font)
        (self.dir / "OFL-A.txt").write_text("license")

    def held(self, rows: list[tuple[str, str]]) -> bool:
        (self.dir / "SOURCES.md").write_text(provenance(rows), encoding="utf-8")
        return fonts.sources_held(self.dir / "SOURCES.md")

    def test_a_row_for_each_file_with_its_sha256_holds(self):
        self.assertTrue(self.held([("A-VF.ttf", sha(self.font))]))

    def test_a_file_whose_sha256_differs_from_its_row_fails(self):
        self.assertFalse(self.held([("A-VF.ttf", sha(b"what was built before"))]))

    def test_a_row_with_no_file_fails(self):
        self.assertFalse(self.held([("A-VF.ttf", sha(self.font)), ("Gone-VF.ttf", sha(b"gone"))]))

    def test_a_font_with_no_row_fails(self):
        (self.dir / "B-VF.ttf").write_bytes(b"another")
        self.assertFalse(self.held([("A-VF.ttf", sha(self.font))]))

    def test_every_sources_md_in_the_repository_holds(self):
        found = fonts.every_sources()
        self.assertGreaterEqual(len(found), 8)
        for sources in found:
            self.assertTrue(fonts.sources_held(sources), sources)


if __name__ == "__main__":
    unittest.main()

"""The library of layouts keyed by geometry family (PLAN T.0a, ADR-0031): each theme the registry
lists takes its family's layouts, and a new theme its family's first file's."""

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_layouts as layouts  # noqa: E402
import build_themes as themes  # noqa: E402

_, BASE, SPECS = themes.read()

REGISTRY = json.loads(layouts.REGISTRY.read_text())
LISTED = layouts.themes(REGISTRY)
FAMILIES = {geometry for _, geometry, _ in LISTED}


def own(paths):
    return json.loads(paths[0].read_text())["layouts"]


class ByGeometry(unittest.TestCase):
    def test_what_differs_by_theme_is_keyed_by_family(self):
        self.assertEqual(FAMILIES, {"dusk", "ember"})
        self.assertLessEqual(set(layouts.OLD_SLOTS_IN), FAMILIES)
        for name, told in layouts.DESCRIPTIONS.items():
            self.assertLessEqual(set(told), FAMILIES | {"*"}, name)

    def test_every_theme_draws_its_familys_layouts(self):
        first = {}
        for slug, geometry, paths in LISTED:
            mine = own(paths)
            first.setdefault(geometry, (slug, mine))
            with self.subTest(theme=slug):
                self.assertEqual(mine, first[geometry][1], f"{slug} draws {first[geometry][0]}'s layouts")
        self.assertNotEqual(first["dusk"][1], first["ember"][1])

    def test_a_new_theme_takes_its_familys_layouts_from_its_first_file(self):
        had = layouts.olds(LISTED)
        with tempfile.TemporaryDirectory() as dir:
            for geometry in sorted(FAMILIES):
                slug, _, paths = next(t for t in LISTED if t[1] == geometry)
                # A new theme's file as build_themes.py writes one, from a spec of this family: its
                # layouts empty and no furniture, for this script to fill.
                theme = themes.themed(slug, SPECS[slug], BASE)
                file = Path(dir) / f"basel-{geometry}.theme.json"
                file.write_text(themes.written(theme, None, BASE))
                self.assertEqual(json.loads(file.read_text())["layouts"], {})
                with self.subTest(family=geometry):
                    old = layouts.olds([*LISTED, ("basel", geometry, [file])])["basel"]
                    self.assertEqual(old, had[slug])
                    out = json.loads(layouts.written(file.read_text(), layouts.library("basel", old, geometry)))
                    self.assertEqual(out["layouts"], own(paths))
                    self.assertEqual(out["furniture"], json.loads(paths[0].read_text())["furniture"])

    def test_a_layout_the_family_lacks_is_refused(self):
        _, geometry, paths = LISTED[0]
        old = dict(own(paths))
        gone = next(n for n in old if n not in layouts.NEW)
        del old[gone]
        with self.assertRaisesRegex(AssertionError, f"has no layout `{gone}`"):
            layouts.library("basel", old, geometry)


if __name__ == "__main__":
    unittest.main()

"""The library of layouts keyed by geometry family (PLAN T.0a, ADR-0031): each theme the registry
lists takes its family's layouts, and a new theme its family's first file's."""

import json
import math
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



class ClearOfDescenders(unittest.TestCase):
    """Words under a text set on the foot of its cells stand clear of its descenders (theme format
    0.27): their slot takes the descent at its top, its other sides as they were."""

    def test_the_quote_credit_takes_the_quotes_descent_at_its_top(self):
        for slug, geometry, paths in LISTED:
            with self.subTest(theme=slug):
                who = own(paths)["quote"]["slots"]["who"]
                descent = layouts.family(geometry)[0]["quote"][0]
                self.assertEqual(who["inset"]["top"], math.ceil(descent))
                self.assertEqual(who["col"], own(paths)["quote"]["slots"]["quote"]["col"])
                roles, row, gutter = layouts.family(geometry)
                given = {k: v for k, v in who.items() if k != "inset"}
                self.assertGreaterEqual(layouts.lines(who, roles, row, gutter), layouts.lines(given, roles, row, gutter))

    def test_words_under_a_text_keep_every_line_they_held(self):
        # Pitch 2's lede under its statement held two lines before its clearance (F4): it reaches
        # into the free row under it rather than losing one.
        for slug, geometry, paths in LISTED:
            roles, row, gutter = layouts.family(geometry)
            for name, layout in own(paths).items():
                for key, s in layout["slots"].items():
                    if s.get("role") not in roles or "inset" not in s or "row" not in s or s["role"] in layouts.FIGURES:
                        continue
                    if s["role"] == "display" and s.get("fit") == "shrink":
                        continue
                    given = dict(s)
                    apart = layouts.apart(s["inset"])
                    if apart is None:
                        del given["inset"]
                    else:
                        given["inset"] = apart
                    with self.subTest(theme=slug, layout=name, slot=key):
                        self.assertGreaterEqual(layouts.lines(s, roles, row, gutter), layouts.lines(given, roles, row, gutter))

    def test_the_statements_support_reaches_into_the_row_under_it(self):
        # In the Dusk family, whose statement sets its last line on the foot of its cells: two lines
        # of its lede, as before its clearance, and more.
        for slug, geometry, paths in LISTED:
            if geometry == "ember":
                continue
            support = own(paths)["statement"]["slots"]["support"]
            roles, row, gutter = layouts.family(geometry)
            with self.subTest(theme=slug):
                self.assertLess(support["inset"]["bottom"], 0)
                self.assertGreaterEqual(layouts.lines(support, roles, row, gutter), 2)

    def test_a_slot_the_clearance_costs_a_line_is_refused(self):
        _, geometry, _ = LISTED[0]
        given = {"note": {"col": [1, 6], "row": [5, 6], "role": "body"}}
        cost = {"note": {"col": [1, 6], "row": [5, 6], "role": "body", "inset": {"top": 60}}}
        with self.assertRaisesRegex(AssertionError, "the clearance costs `note`"):
            layouts.kept_lines("basel", "cost", cost, given, geometry)

    def test_a_card_already_clear_takes_nothing_more(self):
        _, geometry, paths = LISTED[0]
        cards = own(paths)["numbered-cards"]["slots"]
        self.assertEqual(cards["first"].get("inset"), cards["first-title"].get("inset"))

    def test_four_numbers_figures_stand_higher_where_their_labels_would_hold_no_line(self):
        for slug, geometry, paths in LISTED:
            slots = own(paths)["four-numbers"]["slots"]
            roles, row, gutter = layouts.family(geometry)
            with self.subTest(theme=slug):
                self.assertGreaterEqual(layouts.height(slots["first-title"], row, gutter), roles["body"][3])
                lifted = "bottom" in (slots["first-figure"].get("inset") or {})
                self.assertNotEqual(lifted, "top" in (slots["first-title"].get("inset") or {}))

    def test_a_slot_too_short_for_a_line_of_its_role_is_refused(self):
        _, geometry, _ = LISTED[0]
        short = {"note": {"col": [1, 6], "row": [5, 5], "role": "body", "inset": {"top": 30}}}
        with self.assertRaisesRegex(AssertionError, "less than a line of its body"):
            layouts.holding("basel", "short", short, geometry)

    def test_clearing_twice_clears_once(self):
        _, geometry, paths = LISTED[0]
        for name, layout in own(paths).items():
            with self.subTest(layout=name):
                self.assertEqual(layouts.cleared(layout["slots"], geometry)[0], layout["slots"])

if __name__ == "__main__":
    unittest.main()

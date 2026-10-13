"""The icons' paths written again in absolute moves, lines, cubics, and closes (PLAN 2.104)."""

import math
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import build_icons as icons  # noqa: E402


def points(d: str) -> list[tuple[str, list[tuple[float, float]]]]:
    return [(c, [(round(x, 4), round(y, 4)) for x, y in ps]) for c, ps in icons.absolute(d)]


class Absolute(unittest.TestCase):
    def test_relative_moves_and_lines_are_made_absolute(self):
        self.assertEqual(
            points("m10 10 l5 0 h5 v5 l-10 0 z"),
            [
                ("M", [(10, 10)]),
                ("L", [(15, 10)]),
                ("L", [(20, 10)]),
                ("L", [(20, 15)]),
                ("L", [(10, 15)]),
                ("Z", []),
            ],
        )

    def test_numbers_after_a_move_are_lines_and_a_close_returns_to_the_start(self):
        self.assertEqual(
            points("M0,0 10,0 10,10Z l5 5"),
            [("M", [(0, 0)]), ("L", [(10, 0)]), ("L", [(10, 10)]), ("Z", []), ("L", [(5, 5)])],
        )

    def test_quadratics_and_smooth_curves_become_the_cubics_they_are(self):
        # A quadratic's controls are two thirds of the way to its control point.
        (_, _), (c, [c1, c2, end]) = points("M0 0 Q30 30 60 0")
        self.assertEqual((c, c1, c2, end), ("C", (20, 20), (40, 20), (60, 0)))
        # A smooth cubic reflects the last control about the point it starts from.
        _, _, (c, [c1, _, _]) = points("M0 0 C0 10 10 10 10 0 S20 -10 20 0")
        self.assertEqual((c, c1), ("C", (10, -10)))
        # A smooth quadratic reflects the last quadratic's control.
        _, _, (c, [c1, c2, end]) = points("M0 0 Q10 10 20 0 T40 0")
        self.assertEqual((c, end), ("C", (40, 0)))
        self.assertAlmostEqual(c1[1], -20 / 3, places=3)

    def test_an_arc_becomes_cubics_of_at_most_a_quarter_turn_on_its_circle(self):
        steps = icons.absolute("M0 10 A10 10 0 0 1 20 10")  # a half circle about (10, 10), over the top
        cubics = [p for c, p in steps if c == "C"]
        self.assertEqual(len(cubics), 2)
        self.assertEqual(cubics[-1][2], (20, 10))
        # Each cubic's middle lies on the circle, within 0.03% of its radius.
        for (x0, y0), (c1, c2, end) in zip([(0, 10)] + [c[2] for c in cubics[:-1]], cubics):
            mx = (x0 + 3 * c1[0] + 3 * c2[0] + end[0]) / 8
            my = (y0 + 3 * c1[1] + 3 * c2[1] + end[1]) / 8
            self.assertAlmostEqual(math.hypot(mx - 10, my - 10), 10, delta=0.003)
            self.assertLess(my, 10.001, "over the top, as the sweep says")

    def test_an_arcs_flags_may_run_into_its_numbers(self):
        self.assertEqual(icons.absolute("M0 10 a10 10 0 0120 0"), icons.absolute("M0 10 A10 10 0 0 1 20 10"))

    def test_the_written_path_is_the_four_commands_to_two_places(self):
        written = icons.written("m1.004 2 c1 1 2 2 3.333 3 z")
        self.assertEqual(written, "M1,2C2,3,3,4,4.34,5Z")
        self.assertTrue(icons.WRITTEN.fullmatch(written))
        self.assertEqual(icons.number(-0.001), "0")

    def test_what_is_not_path_data_is_refused(self):
        for bad in ("10 10", "M0 0 A1 1 0 2 1 2 2"):
            with self.assertRaises(ValueError):
                icons.absolute(bad)


if __name__ == "__main__":
    unittest.main()

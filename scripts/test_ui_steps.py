"""scripts/ui_steps.py says where a UI test's time went: each step's own time, its steps' taken
out, summed by kind, a tap on one element and on another one kind."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import ui_steps  # noqa: E402

# A test of 60 s: a tap whose wait for the app to idle took 30 s of its 32, a wait of the test's
# own of 5 s, and another tap, the test's last step, running to the test's end.
ACTIVITIES = [
    {"title": 'Tap "Scaena" Other', "startTime": 1000.0, "childActivities": [
        {"title": "Wait for com.welshofer.Scaena to idle", "startTime": 1000.5},
        {"title": 'Find the "Scaena" Other', "startTime": 1030.5},
        {"title": "Synthesize event", "startTime": 1031.0},
    ]},
    {"title": "Wait 5.0s for a tenth", "startTime": 1032.0},
    {"title": 'Tap "Why this exists" Other', "startTime": 1037.0, "childActivities": [
        {"title": "Wait for com.welshofer.Scaena to idle", "startTime": 1037.0},
        {"title": "Synthesize event", "startTime": 1059.0},
    ]},
]


class Steps(unittest.TestCase):
    def test_each_step_runs_until_the_next_at_its_level_and_the_last_until_its_parent_ends(self):
        steps = ui_steps.spans(ACTIVITIES, 1060.0)
        took = {(title, depth): (span, own) for title, span, own, depth in steps if title.startswith("Tap")}
        self.assertEqual(took[('Tap "Scaena" Other', 0)], (32.0, 0.5))
        self.assertEqual(took[('Tap "Why this exists" Other', 0)], (23.0, 0.0))
        idles = [(span, own) for title, span, own, _ in steps if title.startswith("Wait for")]
        self.assertEqual(idles, [(30.0, 30.0), (22.0, 22.0)])
        # The last synthesis of the test runs until the test's end.
        self.assertEqual(steps[-1], ("Synthesize event", 1.0, 1.0, 1))

    def test_a_kind_puts_by_what_its_step_names(self):
        self.assertEqual(ui_steps.kind('Tap "Scaena" Other'), ui_steps.kind('Tap "Why this exists" Other'))
        self.assertEqual(ui_steps.kind("Wait 5.0s for a tenth"), ui_steps.kind("Wait 0.1s for a tenth"))

    def test_the_report_leads_with_the_kind_that_took_longest(self):
        lines = ui_steps.report("TouchTests/testSelectSeveralByTouch()", 60.0, ACTIVITIES)
        self.assertIn("60.0 s", lines[0])
        self.assertIn("52.0 s", lines[1])
        self.assertIn("2×", lines[1])
        self.assertIn("Wait for com.welshofer.Scaena to idle", lines[1])
        self.assertIn("the longest steps", "\n".join(lines))

    def test_a_test_with_no_steps_says_so(self):
        self.assertEqual(ui_steps.report("T/t()", 3.0, []), ["T/t(): no steps recorded"])

    def test_the_cases_read_by_their_ids_and_times(self):
        tree = {"testNodes": [{"nodeType": "Test Plan", "children": [{"nodeType": "Test Suite", "children": [
            {"nodeType": "Test Case", "name": "testA()", "nodeIdentifier": "S/testA()", "durationInSeconds": 4.5},
            {"nodeType": "Test Case", "name": "testB()", "nodeIdentifier": "S/testB()", "duration": "1m 2s"},
        ]}]}]}
        self.assertEqual(ui_steps.cases(tree), [(4.5, "S/testA()", "testA()"), (62.0, "S/testB()", "testB()")])

    def test_the_activities_are_the_last_runs(self):
        tree = {"testRuns": [{"activities": [{"title": "first"}]}, {"activities": [{"title": "again"}]}]}
        self.assertEqual(ui_steps.activities_of(tree), [{"title": "again"}])
        self.assertEqual(ui_steps.activities_of({}), [])


if __name__ == "__main__":
    unittest.main()

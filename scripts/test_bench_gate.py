"""The benchmark gate judges a pull request beside its base on one machine (PLAN 1.24, SPEC §15).

    python3 -m unittest discover -s scripts -p 'test_*.py'
"""

import contextlib
import io
import json
import math
import re
import statistics
import tempfile
import unittest
from pathlib import Path

import bench_gate


def run(machine="Apple M1 (Virtual), 3 cores", **benches) -> dict:
    """A run: each bench's time in ms, as `layout_one__b1=4.0`."""
    return {
        "runner": "macOS-ARM64",
        "machine": machine,
        "sha": "0123456789abcdef",
        "benches": {k.replace("__", "/"): {"ns": v * 1e6, "elements": None} for k, v in benches.items()},
    }


class Gate(unittest.TestCase):
    def setUp(self):
        self.dir = Path(tempfile.mkdtemp())

    def write(self, name: str, value: dict) -> str:
        path = self.dir / name
        path.write_text(json.dumps(value))
        return str(path)

    def gate(self, *argv) -> tuple[int, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = bench_gate.main([str(a) for a in argv])
        return code, out.getvalue() + err.getvalue()

    def check(self, now: dict, base=None, again=None, base_again=None, *extra) -> tuple[int, str]:
        argv = ["check", "--history", self.dir / "history.json", "--run", self.write("run.json", now)]
        for flag, value in (("--base", base), ("--again", again), ("--base-again", base_again)):
            if value is not None:
                argv += [flag, self.write(f"{flag[2:]}.json", value)]
        return self.gate(*argv, *extra)

    def verdicts(self, report: str) -> dict:
        rows = [line.split("|") for line in report.splitlines() if line.startswith("| `")]
        return {cells[1].strip(" `"): cells[-2].strip() for cells in rows}

    def test_a_bench_slower_than_its_base_twice_regresses(self):
        code, report = self.check(
            run(sample__b1=1.2, layout__b1=10.0),
            run(sample__b1=1.0, layout__b1=10.0),
            run(sample__b1=1.25),
            run(sample__b1=1.0),
        )
        self.assertEqual(code, 1, report)
        self.assertEqual(self.verdicts(report), {"layout/b1": "ok", "sample/b1": "**slower**"})
        self.assertIn("1 of 2 benches regressed", report)
        self.assertIn("::error title=bench regression::sample/b1", report)

    def test_a_bench_slower_once_is_not(self):
        code, report = self.check(run(sample__b1=1.3), run(sample__b1=1.0), run(sample__b1=1.03), run(sample__b1=1.0))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report), {"sample/b1": "ok: not slower again"})

    def test_a_bench_regresses_only_when_slower_in_every_turn(self):
        now, base = run(sample__b1=1.3), run(sample__b1=1.0)
        turns = [run(sample__b1=t) for t in (1.20, 1.25, 1.02)]
        bases = [run(sample__b1=1.0)] * 3

        def judge(turns):
            argv = ["check", "--history", self.dir / "history.json", "--run", self.write("run.json", now)]
            argv += ["--base", self.write("base.json", base), "--again"]
            argv += [self.write(f"again-{i}.json", t) for i, t in enumerate(turns)]
            argv += ["--base-again"] + [self.write(f"base-again-{i}.json", b) for i, b in enumerate(bases)]
            return self.gate(*argv)

        # One fast turn of three: a slow run swayed the other two.
        code, report = judge(turns)
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report), {"sample/b1": "ok: not slower again"})
        self.assertIn("| +20.0% · +25.0% · +2.0% |", report)
        code, report = judge(turns[:2] + [run(sample__b1=1.15)])
        self.assertEqual(code, 1, report)
        self.assertIn("::error title=bench regression::sample/b1", report)
        self.assertIn("+20.0% · +25.0% · +15.0% timed again", report)

    def test_within_the_floor_and_faster_pass(self):
        code, report = self.check(run(sample__b1=1.08, layout__b1=7.0), run(sample__b1=1.0, layout__b1=10.0))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report), {"layout/b1": "faster", "sample/b1": "ok"})

    def test_main_on_another_machine_does_not_judge(self):
        # Main's runs were all faster, elsewhere; beside its base, the run is no slower.
        history = {"benches": {"sample/b1": [{"ns": 0.5e6, "machine": "elsewhere"}] * 6}}
        self.write("history.json", history)
        code, report = self.check(run(sample__b1=1.0), run(sample__b1=1.0))
        self.assertEqual(code, 0, report)
        self.assertIn("500 µs (6, mixed machines), +100.0%", report)
        self.assertEqual(self.verdicts(report), {"sample/b1": "ok"})

    def test_the_probe_and_new_benches_are_not_judged(self):
        code, report = self.check(run(probe__cpu=2.0, sample__b9=5.0), run(probe__cpu=1.0))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report), {"probe/cpu": "the machine, not judged", "sample/b9": "new"})
        self.assertIn("The probe ran 100% slower than the base's on this machine.", report)

    def test_a_regression_passes_when_accepted_or_on_main(self):
        slow = (run(sample__b1=1.5), run(sample__b1=1.0), run(sample__b1=1.5), run(sample__b1=1.0))
        for flag in ("--accept", "--report-only"):
            code, report = self.check(*slow, flag)
            self.assertEqual(code, 0, report)
            self.assertIn("::warning title=bench regression::sample/b1", report)

    def test_main_is_recorded_not_judged(self):
        history = self.dir / "history.json"
        for k in range(25):
            self.gate("record", "--history", history, "--run", self.write("main.json", run(sample__b1=1.0 + k / 100)))
        kept = json.loads(history.read_text())["benches"]["sample/b1"]
        self.assertEqual(len(kept), 20)
        self.assertAlmostEqual(kept[-1]["ns"], 1.24e6)
        code, report = self.check(run(sample__b1=2.0), None, None, None, "--report-only")
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report), {"sample/b1": "recorded"})
        self.assertIn("Recorded, not judged", report)

    def spread(self, stray: float, **target) -> tuple[dict, dict]:
        """Forty benches beside an identical base that stray from it as a machine does, by
        `stray` (a standard deviation of the log ratio), and `target` (ms, base 1.0): a run
        and its base."""
        normal = statistics.NormalDist(0, math.log(1 + stray))
        now = {f"layout__b{i}": math.exp(normal.inv_cdf((i + 0.5) / 40)) for i in range(40)}
        return run(**now, **target), run(**{k: 1.0 for k in now}, **{k: 1.0 for k in target})

    def floor_in(self, report: str) -> int:
        return int(re.search(r"so the floor is (\d+)%", report).group(1))

    def test_a_noisy_machine_raises_the_floor(self):
        # Benches that stray by 10% when nothing changed: a 25% slowdown, twice, is noise there.
        now, base = self.spread(0.10, sample__b1=1.25)
        code, report = self.check(now, base, run(sample__b1=1.25), run(sample__b1=1.0))
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report)["sample/b1"], "ok")
        self.assertRegex(report, r"the 41 benches strayed by (9\.\d|10\.\d)% here")
        self.assertTrue(25 < self.floor_in(report) < 30, report)

    def test_a_quiet_machine_keeps_the_floor_at_ten_percent(self):
        now, base = self.spread(0.01, sample__b1=1.12)
        code, report = self.check(now, base, run(sample__b1=1.12), run(sample__b1=1.0))
        self.assertEqual(code, 1, report)
        self.assertEqual(self.verdicts(report)["sample/b1"], "**slower**")
        self.assertIn("so the floor is 10%: 2.5 σ, or 10% if that is more", report)

    def test_the_suspects_are_past_the_runs_floor(self):
        out = self.dir / "suspects.txt"
        now, base = self.spread(0.10, sample__b1=1.25, video__b1=1.4)
        self.gate("suspects", "--run", self.write("now.json", now), "--base", self.write("base.json", base), "--out", out)
        self.assertEqual(out.read_text(), "video/b1\n")

    def test_the_suspects_are_the_benches_slower_than_their_base(self):
        out = self.dir / "suspects.txt"
        now = self.write("now.json", run(sample__b1=1.2, layout__b1=10.5, probe__cpu=3.0, sample__b9=1.0))
        base = self.write("base.json", run(sample__b1=1.0, layout__b1=10.0, probe__cpu=1.0))
        self.gate("suspects", "--run", now, "--base", base, "--out", out)
        self.assertEqual(out.read_text(), "sample/b1\n")

    def test_collect_keeps_each_bench_at_its_fastest(self):
        def criterion(home: str, bench: str, ms: float):
            d = self.dir / home / bench.replace("/", "_") / "new"
            d.mkdir(parents=True)
            (d / "benchmark.json").write_text(json.dumps({"full_id": bench, "throughput": None}))
            median = {"point_estimate": ms * 1e6, "confidence_interval": {"lower_bound": 0, "upper_bound": 0}}
            (d / "estimates.json").write_text(json.dumps({"median": median}))

        criterion("again-1", "video/b1", 9.0)
        criterion("again-1", "sample/b1", 1.0)
        criterion("again-2", "video/b1", 6.0)
        code, out = self.gate("collect", self.dir / "again-1", self.dir / "again-2", "--out", self.dir / "again.json")
        self.assertEqual(code, 0, out)
        collected = json.loads((self.dir / "again.json").read_text())["benches"]
        self.assertEqual({k: v["ns"] / 1e6 for k, v in collected.items()}, {"video/b1": 6.0, "sample/b1": 1.0})


if __name__ == "__main__":
    unittest.main()

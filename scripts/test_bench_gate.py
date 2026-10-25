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

    def turns(self, now: dict, base: dict, turns: list, bases: list) -> tuple[int, str]:
        """`check` with a turn of timing again per pair."""
        argv = ["check", "--history", self.dir / "history.json", "--run", self.write("run.json", now)]
        argv += ["--base", self.write("base.json", base), "--again"]
        argv += [self.write(f"again-{i}.json", t) for i, t in enumerate(turns)]
        argv += ["--base-again"] + [self.write(f"base-again-{i}.json", b) for i, b in enumerate(bases)]
        return self.gate(*argv)

    # #626's cold renders: b1 +11.8% and its retimes +26%, +86%, +80%, the rest of the group as
    # its base. A process start slows every deck's cold render alike; one deck alone is noise.
    COLD_BASE = dict(render_cold__b1=13.2, render_cold__b2=13.3, render_cold__b3=66.3, render_cold__b4=51.9)
    COLD_NOW = dict(render_cold__b1=14.7, render_cold__b2=13.2, render_cold__b3=56.5, render_cold__b4=52.3)

    def test_a_cold_bench_is_judged_with_its_group(self):
        base = run(**self.COLD_BASE)
        turns = [run(**{**self.COLD_BASE, "render_cold__b1": 13.2 * r}) for r in (1.263, 1.865, 1.804)]
        code, report = self.turns(run(**self.COLD_NOW), base, turns, [base] * 3)
        self.assertEqual(code, 0, report)
        verdicts = self.verdicts(report)
        self.assertEqual(verdicts["render_cold/b1"], "ok: its group is not slower")
        self.assertEqual(verdicts["render_cold/b3"], "faster")
        self.assertNotIn("regressed", report)

    def test_a_cold_group_slower_every_time_regresses(self):
        base = run(**self.COLD_BASE)
        slower = run(**{k: v * 1.12 for k, v in self.COLD_BASE.items()})
        code, report = self.turns(slower, base, [slower] * 3, [base] * 3)
        self.assertEqual(code, 1, report)
        self.assertEqual(set(self.verdicts(report).values()), {"**slower** (its group)"})
        self.assertIn("::error title=bench regression::render_cold/b1", report)
        # Slower once, then not, the group passes.
        code, report = self.turns(slower, base, [slower, base, slower], [base] * 3)
        self.assertEqual(code, 0, report)

    def test_a_cold_deck_over_its_budget_fails_alone(self):
        # SPEC §15: a cold render of B1 within 300 ms. The group is as its base; B1 is over.
        over = {**self.COLD_BASE, "render_cold__b1": 320.0}
        code, report = self.check(run(**over), run(**{**over, "render_cold__b1": 310.0}))
        self.assertEqual(code, 1, report)
        self.assertEqual(self.verdicts(report)["render_cold/b1"], "**over its budget**")
        self.assertIn("render_cold/b1: 320 ms, over SPEC §15's 300 ms", report)
        # Within it, the same deck passes.
        code, report = self.check(run(**{**over, "render_cold__b1": 290.0}), run(**{**over, "render_cold__b1": 280.0}))
        self.assertEqual(code, 0, report)

    def test_the_suspects_are_a_cold_group_whole_when_it_is_slower(self):
        out = self.dir / "suspects.txt"
        base = self.write("base.json", run(**self.COLD_BASE, sample__b1=1.0))
        self.gate("suspects", "--run", self.write("now.json", run(**self.COLD_NOW, sample__b1=1.0)), "--base", base, "--out", out)
        self.assertEqual(out.read_text(), "", "one cold deck alone is not timed again")
        slower = run(**{k: v * 1.12 for k, v in self.COLD_BASE.items()}, sample__b1=1.0)
        self.gate("suspects", "--run", self.write("now.json", slower), "--base", base, "--out", out)
        self.assertEqual(out.read_text(), "".join(f"render_cold/b{i}\n" for i in range(1, 5)))

    # #701's fonts benches: B2 +10.4%, then +14.6%, +17.3%, and +10.6% timed again, while B1 and B3,
    # which run the same code, ran faster every time. One deck alone is where its code landed.
    FONTS = 0.0452

    def fonts(self, b1: float, b2: float, b3: float) -> dict:
        return run(fonts__b1=self.FONTS * b1, fonts__b2=self.FONTS * b2, fonts__b3=self.FONTS * b3)

    def test_a_fonts_bench_is_judged_with_its_group(self):
        base = self.fonts(1, 1, 1)
        turns = [self.fonts(0.807, 1.146, 0.904), self.fonts(0.855, 1.173, 0.998), self.fonts(0.933, 1.106, 0.867)]
        code, report = self.turns(self.fonts(0.807, 1.104, 0.904), base, turns, [base] * 3)
        self.assertEqual(code, 0, report)
        self.assertEqual(self.verdicts(report)["fonts/b2"], "ok: its group is not slower")
        self.assertNotIn("regressed", report)
        # Every deck slower, every time it was timed: the group regresses.
        slower = self.fonts(1.12, 1.12, 1.12)
        code, report = self.turns(slower, base, [slower] * 3, [base] * 3)
        self.assertEqual(code, 1, report)
        self.assertEqual(set(self.verdicts(report).values()), {"**slower** (its group)"})
        self.assertIn("::error title=bench regression::fonts/b2", report)

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

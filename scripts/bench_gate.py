#!/usr/bin/env python3
"""The benchmark gate (SPEC §15, PLAN 1.24): criterion's numbers against each runner's own history.

Every CI runner (an OS and an architecture: `macOS-ARM64`, `Linux-X64`) keeps a history of
what the benches measured on `main`, run by run. A run is judged against it, bench by bench:

- its **baseline** is the median of main's recent runs, on the same machine model when there
  are enough of those;
- its **noise** is how far those runs spread about the baseline: σ = 1.4826 × their median
  absolute deviation, relative to the baseline;
- it **regresses** when it is slower than the baseline by more than `max(floor, k × σ)`.

A bench with fewer than `--min-runs` runs behind it is recorded, not judged. A bench that
looks slower is run again before it is judged (`suspects`), and judged on the faster of
its runs, so one noisy moment on a shared runner fails nothing. When main itself is slower
on both runs (a regression accepted on purpose, or one let through), `record` starts that
bench's history over, so the baseline follows the code. `probe/` benches time the machine,
not Scaena: they are shown, never judged.

    python3 scripts/bench_gate.py collect DIR --out run.json      # criterion's results under DIR
    python3 scripts/bench_gate.py suspects --history H --run run.json --out suspects.txt
    python3 scripts/bench_gate.py check --history H --run run.json [--run again.json] [--summary FILE]
    python3 scripts/bench_gate.py record --history H --run run.json [--run again.json]

`check` prints a Markdown report, the SPEC §15 budgets first, and exits 1 on a regression
unless `--accept` (a pull request labelled `bench-accept`) or `--report-only` (main).
Standard library only.
"""

import argparse
import json
import os
import statistics
import sys
from datetime import datetime, timezone
from pathlib import Path

# SPEC §15's budgets, by bench group: the stage as SPEC names it, whether the budget is per
# iteration or per element (each state, cue, or frame the bench counts), and the budget in ms
# for each deck SPEC names, or why the stage is not judged against one. The video budgets are
# realtime at 60 fps: 1× is 16.7 ms a frame.
BUDGETS = [
    ("layout_one", "Resolve + layout, one snapshot (the slowest)", "iteration", {"b1": 15}),
    ("layout", "Resolve + layout, all snapshots", "iteration", {"b1": 400}),
    ("sample", "Sample one frame", "element", {"b1": 1, "b2": 1}),
    ("gpu_paint", "GPU paint, one frame at 1080p, and its readback", "element",
     "not judged: SPEC's ≤ 6 ms (B1, B2, B3) is for the paint alone"),
    ("cpu_paint_one", "CPU paint, one frame at 1080p (the slowest, one thread)", "iteration", {"b1": 12, "b2": 12, "b3": 25}),
    ("render_cold", "Headless PNG render, cold", "iteration", {"b1": 300}),
    ("lint_document", "Lint, document-level", "iteration", {"b1": 100}),
    ("lint_layout", "Lint, layout-level, all states", "iteration", {"b1": 1000}),
    ("mcp_render", "MCP `deck_render` round trip, cold process", "iteration", {"b1": 1000}),
    ("video", "Video export, CPU path, a frame: 1× realtime at 60 fps (0.5× for B3)", "element",
     {"b1": 1000 / 60, "b2": 1000 / 60, "b3": 2000 / 60}),
]
DECKS = ["b1", "b2", "b3", "b4"]
PROBE = "probe/"


def fmt_ns(ns: float) -> str:
    for unit, scale in (("s", 1e9), ("ms", 1e6), ("µs", 1e3)):
        if ns >= scale:
            v = ns / scale
            return f"{v:.3g} {unit}" if v < 100 else f"{v:.0f} {unit}"
    return f"{ns:.3g} ns"


def collect(args) -> None:
    benches = {}
    for meta in sorted(Path(args.dir).glob("**/new/benchmark.json")):
        info = json.loads(meta.read_text())
        median = json.loads((meta.parent / "estimates.json").read_text())["median"]
        throughput = info.get("throughput") or {}
        benches[info["full_id"]] = {
            "ns": median["point_estimate"],
            "lo": median["confidence_interval"]["lower_bound"],
            "hi": median["confidence_interval"]["upper_bound"],
            "elements": throughput.get("Elements"),
        }
    if not benches:
        sys.exit(f"no criterion results under {args.dir}")
    run = {
        "runner": args.runner,
        "machine": args.machine,
        "sha": args.sha,
        "date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "benches": benches,
    }
    Path(args.out).write_text(json.dumps(run, indent=1, sort_keys=True) + "\n")
    print(f"{len(benches)} benches from {args.dir} into {args.out}")


def runs(paths: list[str]) -> dict:
    """The runs, as one: each bench at the fastest any of them measured it."""
    merged = None
    for path in paths:
        run = json.loads(Path(path).read_text())
        if merged is None:
            merged = run
            continue
        for bench, now in run["benches"].items():
            if bench in merged["benches"] and now["ns"] < merged["benches"][bench]["ns"]:
                merged["benches"][bench] = {**now, "again": True}
    return merged


def load_history(path: str) -> dict:
    p = Path(path)
    return json.loads(p.read_text()) if p.is_file() else {"benches": {}}


class Judged:
    """One bench of a run against its history."""

    def __init__(self, bench: str, now: dict, past: list[dict], machine, opts):
        self.bench, self.now = bench, now
        same = [p for p in past if p.get("machine") == machine]
        # Like with like, when the history has enough of it.
        self.past = same if len(same) >= opts.min_runs else past
        self.mixed = bool(self.past) and self.past is not same
        values = [p["ns"] for p in self.past]
        self.n, self.needs = len(values), opts.min_runs
        self.judged = self.n >= opts.min_runs and not bench.startswith(PROBE)
        self.baseline = self.change = self.threshold = None
        if values:
            self.baseline = statistics.median(values)
            sigma = 1.4826 * statistics.median(abs(v - self.baseline) for v in values) / self.baseline
            self.threshold = max(opts.floor, opts.k * sigma)
            self.change = now["ns"] / self.baseline - 1

    @property
    def slower(self) -> bool:
        """Slower than its noise allows."""
        return self.judged and self.change > self.threshold

    @property
    def verdict(self) -> str:
        if self.baseline is None:
            return "new"
        if self.bench.startswith(PROBE):
            return "the machine, not judged"
        if not self.judged:
            return f"recorded: {self.n} of {self.needs} runs"
        if self.slower:
            return "**slower**"
        return "faster" if self.change < -self.threshold else "ok"


def judge(args, run: dict) -> list[Judged]:
    history = load_history(args.history)
    return [
        Judged(bench, now, history["benches"].get(bench, []), run.get("machine"), args)
        for bench, now in sorted(run["benches"].items())
    ]


def suspects(args) -> None:
    run = runs(args.run)
    slower = [j.bench for j in judge(args, run) if j.slower]
    Path(args.out).write_text("".join(f"{b}\n" for b in slower))
    print(f"{len(slower)} benches to run again" + (f": {', '.join(slower)}" if slower else ""))


def budget_rows(run: dict) -> list[str]:
    benches = run["benches"]
    head = "| SPEC §15 stage | Budget | " + " | ".join(d.upper() for d in DECKS) + " |"
    rows = [head, "|---|---|" + "---|" * len(DECKS)]
    for group, stage, per, budgets in BUDGETS:
        why = budgets if isinstance(budgets, str) else None
        budgets = {} if why else budgets
        cells = []
        for deck in DECKS:
            b = benches.get(f"{group}/{deck}")
            if b is None:
                cells.append("—")
                continue
            ns = b["ns"] / (b["elements"] or 1) if per == "element" else b["ns"]
            ms = budgets.get(deck)
            mark = "" if ms is None else " ✓" if ns <= ms * 1e6 else " ✗"
            cells.append(fmt_ns(ns) + mark)
        by_budget: dict[float, list[str]] = {}
        for deck, ms in budgets.items():
            by_budget.setdefault(ms, []).append(deck.upper())
        budget = why or " · ".join(f"≤ {fmt_ns(ms * 1e6)} ({', '.join(decks)})" for ms, decks in by_budget.items())
        rows.append(f"| {stage} | {budget} | " + " | ".join(cells) + " |")
    return rows


def check(args) -> int:
    run = runs(args.run)
    judged = judge(args, run)
    slower = [j for j in judged if j.slower]
    lines = [f"### Benchmarks on {run.get('runner') or 'this runner'}", ""]
    about = [run.get("machine"), run.get("sha") and f"commit `{run['sha'][:12]}`"]
    if any(about):
        lines += ["; ".join(a for a in about if a) + ".", ""]
    lines += budget_rows(run)
    lines += [
        "",
        "✓ and ✗ mark the decks SPEC names a budget for; the rest are recorded. A frame is 1080 pixels high. "
        "Per element is per state, cue, or frame.",
        "",
    ]
    probe = [j for j in judged if j.bench.startswith(PROBE) and j.change is not None]
    if slower:
        verdict = f"**{len(slower)} of {len(judged)} benches regressed** beyond main's noise"
        if args.accept:
            verdict += "; the pull request accepts it (`bench-accept`)"
    elif not any(j.judged for j in judged):
        most = max((j.n for j in judged), default=0)
        verdict = f"Not judged yet: main has {most} of the {args.min_runs} runs a bench needs behind it"
    else:
        verdict = f"No bench slower than main's runs allow, of {sum(j.judged for j in judged)} judged"
    lines += [
        f"{verdict}. A bench regresses when it is slower than the median of main's runs on this runner by "
        f"more than max({args.floor:.0%}, {args.k:g}σ), σ being their spread; one that looks slower is run "
        "again and judged on the faster of its runs.",
    ]
    for j in probe:
        lines.append(f"The probe ran {abs(j.change):.0%} {'slower' if j.change > 0 else 'faster'} than on main's runs.")
    lines += [
        "",
        "| Bench | This run | Per element | Baseline (runs) | Change | Noise | Verdict |",
        "|---|---|---|---|---|---|---|",
    ]
    for j in judged:
        el = j.now.get("elements")
        per = f"{fmt_ns(j.now['ns'] / el)} × {el}" if el else "—"
        if j.baseline is None:
            base = change = noise = "—"
        else:
            base = f"{fmt_ns(j.baseline)} ({j.n}{', mixed machines' if j.mixed else ''})"
            change = f"{j.change:+.1%}"
            noise = f"±{j.threshold:.0%}"
        again = " (run again)" if j.now.get("again") else ""
        lines.append(
            f"| `{j.bench}` | {fmt_ns(j.now['ns'])}{again} | {per} | {base} | {change} | {noise} | {j.verdict} |"
        )
    report = "\n".join(lines) + "\n"
    print(report)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as f:
            f.write(report + "\n")
    passes = args.accept or args.report_only
    for j in slower:
        print(
            f"::{'warning' if passes else 'error'} title=bench regression::{j.bench}: {fmt_ns(j.now['ns'])} "
            f"against main's {fmt_ns(j.baseline)}, {j.change:+.1%}, beyond its noise of ±{j.threshold:.0%}",
            file=sys.stderr,
        )
    return 1 if slower and not passes else 0


def record(args) -> None:
    run = runs(args.run)
    history = load_history(args.history)
    benches = history.setdefault("benches", {})
    restarted = []
    for j in judge(args, run):
        entry = {"ns": j.now["ns"], "sha": run.get("sha"), "machine": run.get("machine"), "date": run.get("date")}
        if j.slower:
            # Slower on both runs: a step, not noise. The baseline starts over from here.
            benches[j.bench] = [entry]
            restarted.append(f"{j.bench} ({j.change:+.1%})")
        else:
            benches[j.bench] = (benches.get(j.bench, []) + [entry])[-args.keep :]
    history["runner"] = run.get("runner")
    history["updated"] = run.get("date")
    Path(args.history).write_text(json.dumps(history, indent=1, sort_keys=True) + "\n")
    print(f"recorded {len(run['benches'])} benches into {args.history}")
    if restarted:
        print("slower on both runs, so their history starts over: " + ", ".join(restarted))


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    c = sub.add_parser("collect", help="read criterion's results into one run")
    c.add_argument("dir", help="criterion's output directory (target/criterion, or $CRITERION_HOME)")
    c.add_argument("--out", required=True)
    c.add_argument("--runner", default=os.environ.get("BENCH_RUNNER"))
    c.add_argument("--machine", default=os.environ.get("BENCH_MACHINE"))
    c.add_argument("--sha", default=os.environ.get("GITHUB_SHA"))
    for name, fn, about in (
        ("suspects", suspects, "list the benches slower than their noise allows, to run again"),
        ("check", check, "report a run against the history; fail on a regression"),
        ("record", record, "add a run of main to the history"),
    ):
        p = sub.add_parser(name, help=about)
        p.add_argument("--history", required=True)
        p.add_argument("--run", required=True, action="append", help="a run; again for a second (the faster counts)")
        p.add_argument("--min-runs", type=int, default=5, help="runs of main behind a bench before it is judged")
        p.add_argument("--k", type=float, default=4.0, help="multiples of σ a change must exceed")
        p.add_argument("--floor", type=float, default=0.10, help="the smallest change that counts")
        p.add_argument("--keep", type=int, default=20, help="runs kept per bench")
        p.set_defaults(fn=fn)
    sub.choices["suspects"].add_argument("--out", required=True)
    ck = sub.choices["check"]
    ck.add_argument("--summary", help="append the report here too ($GITHUB_STEP_SUMMARY)")
    ck.add_argument("--accept", action="store_true", help="report regressions without failing (`bench-accept`)")
    ck.add_argument("--report-only", action="store_true", help="never fail (main: the change is in)")
    args = ap.parse_args()
    if args.command == "collect":
        collect(args)
    else:
        sys.exit(args.fn(args))


if __name__ == "__main__":
    main()

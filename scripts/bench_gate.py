#!/usr/bin/env python3
"""The benchmark gate (SPEC §15, PLAN 1.24): a pull request's benches beside its base's, on one machine.

Runs of the same code on different CI machines spread too far to judge a change by. Of six
runs of `main` on the macOS runner, 46% of the times were more than 10% from their bench's
median and 13% more than 30%, and Linux runs land on different processors. So a pull
request is judged beside its base, built and timed on the same machine in the same job:

- the two are timed a group of benches at a time, the base first, so a spell of load on the
  machine falls on both;
- a bench is **slower** when the pull request takes more than `--floor` (10%) longer than
  its base;
- a slower bench is timed again, beside its base bench by bench, twice: the pull request
  first, then the base first. Each side counts at its fastest, since load on the machine
  only ever slows a run. The bench **regresses** when it is slower again.

Each runner keeps a history of what the benches measured on `main`, run by run. The report
shows it beside each bench for context; it does not judge. `probe/` benches time the
machine, not Scaena: they are shown, never judged.

    python3 scripts/bench_gate.py collect DIR... --out run.json   # criterion's results, each bench's fastest
    python3 scripts/bench_gate.py suspects --run run.json --base base.json --out suspects.txt
    python3 scripts/bench_gate.py check --history H --run run.json [--base base.json
        [--again again.json --base-again base-again.json]] [--summary FILE]
    python3 scripts/bench_gate.py record --history H --run run.json

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


def pct(x) -> str:
    return "—" if x is None else f"{x:+.1%}"


def collect(args) -> None:
    """Criterion's results under each directory, each bench at its fastest median."""
    benches = {}
    for d in args.dirs:
        for meta in sorted(Path(d).glob("**/new/benchmark.json")):
            info = json.loads(meta.read_text())
            median = json.loads((meta.parent / "estimates.json").read_text())["median"]
            throughput = info.get("throughput") or {}
            bench = {
                "ns": median["point_estimate"],
                "lo": median["confidence_interval"]["lower_bound"],
                "hi": median["confidence_interval"]["upper_bound"],
                "elements": throughput.get("Elements"),
            }
            if info["full_id"] not in benches or bench["ns"] < benches[info["full_id"]]["ns"]:
                benches[info["full_id"]] = bench
    if not benches:
        sys.exit(f"no criterion results under {', '.join(args.dirs)}")
    run = {
        "runner": args.runner,
        "machine": args.machine,
        "sha": args.sha,
        "date": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
        "benches": benches,
    }
    Path(args.out).write_text(json.dumps(run, indent=1, sort_keys=True) + "\n")
    print(f"{len(benches)} benches from {', '.join(args.dirs)} into {args.out}")


def load(path) -> dict:
    """A run, or an empty one."""
    return json.loads(Path(path).read_text()) if path else {"benches": {}}


def load_history(path: str) -> dict:
    p = Path(path)
    return json.loads(p.read_text()) if p.is_file() else {"benches": {}}


def ns(run: dict, bench: str):
    b = run["benches"].get(bench)
    return b["ns"] if b else None


class Judged:
    """One bench of a run: beside its base on the same machine, and main's runs for context."""

    def __init__(self, bench: str, run: dict, base: dict, again: dict, base_again: dict, past: list, opts):
        self.bench, self.now, self.floor = bench, run["benches"][bench], opts.floor
        self.probe = bench.startswith(PROBE)
        self.base = ns(base, bench)
        self.change = self.now["ns"] / self.base - 1 if self.base else None
        a, b = ns(again, bench), ns(base_again, bench)
        self.again = a / b - 1 if a and b else None
        # Main's runs, on the same machine model when there are enough of those.
        same = [p for p in past if p.get("machine") == run.get("machine")]
        past = same if len(same) >= opts.min_runs else past
        self.mixed = bool(past) and past is not same
        values = [p["ns"] for p in past]
        self.n = len(values)
        self.main = statistics.median(values) if values else None
        self.vs_main = self.now["ns"] / self.main - 1 if self.main else None

    @property
    def judged(self) -> bool:
        return self.change is not None and not self.probe

    @property
    def slower(self) -> bool:
        """Slower than its base by more than the floor, each time they were timed together."""
        return self.judged and self.change > self.floor and (self.again is None or self.again > self.floor)

    @property
    def verdict(self) -> str:
        if self.probe:
            return "the machine, not judged"
        if self.change is None:
            return "recorded" if self.main is not None else "new"
        if self.slower:
            return "**slower**"
        if self.change > self.floor:
            return "ok: not slower again"
        return "faster" if self.change < -self.floor else "ok"

    def error(self) -> str:
        again = "" if self.again is None else f", and {self.again:+.1%} timed again"
        return (
            f"{self.bench}: {fmt_ns(self.now['ns'])} against its base's {fmt_ns(self.base)} on the same machine, "
            f"{self.change:+.1%}{again}"
        )


def judge(args) -> tuple[dict, list[Judged]]:
    run = load(args.run)
    base, again, base_again = load(args.base), load(args.again), load(args.base_again)
    history = load_history(args.history)
    judged = [
        Judged(bench, run, base, again, base_again, history["benches"].get(bench, []), args)
        for bench in sorted(run["benches"])
    ]
    return run, judged


def suspects(args) -> None:
    """The benches slower than their base by more than the floor, to time again."""
    run, base = load(args.run), load(args.base)
    slower = [
        b for b in sorted(run["benches"])
        if not b.startswith(PROBE) and ns(base, b) and ns(run, b) / ns(base, b) - 1 > args.floor
    ]
    Path(args.out).write_text("".join(f"{b}\n" for b in slower))
    print(f"{len(slower)} benches to time again" + (f": {', '.join(slower)}" if slower else ""))


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
    run, judged = judge(args)
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
    floor, count = f"{args.floor:.0%}", sum(j.judged for j in judged)
    if not count:
        verdict = "Recorded, not judged: only a pull request is timed beside a base"
    elif slower:
        verdict = f"**{len(slower)} of {count} benches regressed**: slower than the base by more than {floor}, twice"
        if args.accept:
            verdict += "; the pull request accepts it (`bench-accept`)"
    else:
        verdict = f"No bench slower than the base by more than {floor} twice, of {count} judged"
    lines.append(
        f"{verdict}. The base, the commit a pull request merges onto, is built and timed on the same machine in "
        f"the same job, a group of benches at a time and the base first. A bench slower than it by more than {floor} "
        "is timed twice more beside it, and regresses when it is slower again, each side at its fastest. Main's runs "
        "on this runner, on other machines, are shown for context and do not judge."
    )
    for j in judged:
        if j.probe and j.change is not None:
            lines.append(f"The probe ran {abs(j.change):.0%} {'slower' if j.change > 0 else 'faster'} than the base's on this machine.")
    lines += [
        "",
        "| Bench | This run | Per element | Base, here | Change | Again | Main's runs | Verdict |",
        "|---|---|---|---|---|---|---|---|",
    ]
    for j in judged:
        el = j.now.get("elements")
        per = f"{fmt_ns(j.now['ns'] / el)} × {el}" if el else "—"
        base = fmt_ns(j.base) if j.base else "—"
        main = "—" if j.main is None else f"{fmt_ns(j.main)} ({j.n}{', mixed machines' if j.mixed else ''}), {pct(j.vs_main)}"
        lines.append(f"| `{j.bench}` | {fmt_ns(j.now['ns'])} | {per} | {base} | {pct(j.change)} | {pct(j.again)} | {main} | {j.verdict} |")
    report = "\n".join(lines) + "\n"
    print(report)
    if args.summary:
        with open(args.summary, "a", encoding="utf-8") as f:
            f.write(report + "\n")
    passes = args.accept or args.report_only
    for j in slower:
        print(f"::{'warning' if passes else 'error'} title=bench regression::{j.error()}", file=sys.stderr)
    return 1 if slower and not passes else 0


def record(args) -> None:
    run = load(args.run)
    history = load_history(args.history)
    benches = history.setdefault("benches", {})
    for bench, now in run["benches"].items():
        entry = {"ns": now["ns"], "sha": run.get("sha"), "machine": run.get("machine"), "date": run.get("date")}
        benches[bench] = (benches.get(bench, []) + [entry])[-args.keep :]
    history["runner"] = run.get("runner")
    history["updated"] = run.get("date")
    Path(args.history).write_text(json.dumps(history, indent=1, sort_keys=True) + "\n")
    print(f"recorded {len(run['benches'])} benches into {args.history}")


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="command", required=True)
    c = sub.add_parser("collect", help="read criterion's results into one run")
    c.add_argument("dirs", nargs="+", help="criterion's output directories ($CRITERION_HOME): each bench's fastest")
    c.add_argument("--out", required=True)
    c.add_argument("--runner", default=os.environ.get("BENCH_RUNNER"))
    c.add_argument("--machine", default=os.environ.get("BENCH_MACHINE"))
    c.add_argument("--sha", default=os.environ.get("GITHUB_SHA"))
    s = sub.add_parser("suspects", help="list the benches slower than their base, to time again")
    s.add_argument("--run", required=True)
    s.add_argument("--base", required=True, help="the base's run, on the same machine")
    s.add_argument("--out", required=True)
    ck = sub.add_parser("check", help="report a run beside its base and main's runs; fail on a regression")
    ck.add_argument("--history", required=True, help="main's runs on this runner")
    ck.add_argument("--run", required=True)
    ck.add_argument("--base", help="the base's run, on the same machine")
    ck.add_argument("--again", help="the benches slower than the base, timed again beside it")
    ck.add_argument("--base-again", help="the base's, timed again beside them")
    ck.add_argument("--min-runs", type=int, default=5, help="main's runs on one machine model before they stand alone")
    ck.add_argument("--summary", help="append the report here too ($GITHUB_STEP_SUMMARY)")
    ck.add_argument("--accept", action="store_true", help="report regressions without failing (`bench-accept`)")
    ck.add_argument("--report-only", action="store_true", help="never fail (main: the change is in)")
    for p in (s, ck):
        p.add_argument("--floor", type=float, default=0.10, help="the smallest slowdown that counts")
    r = sub.add_parser("record", help="add a run of main to the history")
    r.add_argument("--history", required=True)
    r.add_argument("--run", required=True)
    r.add_argument("--keep", type=int, default=20, help="runs kept per bench")
    args = ap.parse_args(argv)
    if args.command == "check":
        return check(args)
    {"collect": collect, "suspects": suspects, "record": record}[args.command](args)
    return 0


if __name__ == "__main__":
    sys.exit(main())

#!/usr/bin/env python3
"""Where a UI test's time goes, as its result bundle's activities say: each step XCTest took (a
tap, the wait for the app to go idle before it, a query's snapshot, a wait of the test's own),
its own time, the time of its steps within it taken out, summed by its kind; and the longest steps
one by one. `apps/ipad/results.sh` says it for the slowest tests of a run, so a test that grows
names the step that grew it.

    scripts/ui_steps.py RESULTS.xcresult [HOW_MANY]

An activity says when it began and not how long it took: it runs until the next one at its level
begins, the last of its level until its parent ends, and the test's last until the test does.
"""

import json
import re
import subprocess
import sys

# How many kinds and steps are said for each test.
KINDS = 8
STEPS = 8


def kind(title):
    """A step's kind: its title with what it names quoted, its numbers, and its addresses put by,
    so a tap on one element and a tap on another are one kind."""
    said = re.sub(r'"[^"]*"', '"…"', title)
    said = re.sub(r"0x[0-9a-fA-F]+", "0x…", said)
    said = re.sub(r"\d+(\.\d+)?", "N", said)
    return said[:90]


def spans(activities, end):
    """Each activity of `activities`, in the order begun, with its own time: (title, its time, its
    time less its steps', depth). It runs until the next at its level begins, the last until `end`."""
    out = []

    def walk(level, until, depth):
        level = sorted(level, key=lambda one: one.get("startTime", 0))
        spent = 0.0
        for i, one in enumerate(level):
            start = one.get("startTime", 0)
            stop = level[i + 1].get("startTime", until) if i + 1 < len(level) else until
            took = max(0.0, stop - start)
            spent += took
            at = len(out)
            out.append(None)
            within = walk(one.get("childActivities") or [], stop, depth + 1)
            out[at] = (one.get("title", ""), took, max(0.0, took - within), depth)
        return spent

    walk(activities, end, 0)
    return out


def report(name, took, activities):
    """Lines that say where `name`'s `took` seconds went, by kind, then step by step."""
    starts = [one.get("startTime") for one in activities if one.get("startTime") is not None]
    if not starts:
        return ["%s: no steps recorded" % name]
    end = min(starts) + took
    steps = spans(activities, end)
    by_kind = {}
    for title, _, own, _ in steps:
        key = kind(title)
        total, count = by_kind.get(key, (0.0, 0))
        by_kind[key] = (total + own, count + 1)
    lines = ["%s, %.1f s: its time by kind of step (own time, steps)" % (name, took)]
    for key, (total, count) in sorted(by_kind.items(), key=lambda item: -item[1][0])[:KINDS]:
        lines.append("  %7.1f s  %4d×  %s" % (total, count, key))
    lines.append("  the longest steps, by own time:")
    for title, span, own, depth in sorted(steps, key=lambda step: -step[2])[:STEPS]:
        lines.append("  %7.1f s  (of %.1f)  %s%s" % (own, span, "  " * depth, title[:110]))
    return lines


def cases(tree):
    """Each test case of `xcresulttool get test-results tests`: (seconds, id, name)."""
    found = []

    def seconds(node):
        if isinstance(node.get("durationInSeconds"), (int, float)):
            return float(node["durationInSeconds"])
        total = 0.0
        for amount, unit in re.findall(r"([0-9.]+)\s*(h|m|s)", node.get("duration") or ""):
            total += float(amount) * {"h": 3600, "m": 60, "s": 1}[unit]
        return total

    def walk(node):
        if node.get("nodeType") == "Test Case":
            found.append((seconds(node), node.get("nodeIdentifier") or node.get("name", ""), node.get("name", "")))
            return
        for child in node.get("children", []):
            walk(child)

    for node in tree.get("testNodes", []):
        walk(node)
    return found


def activities_of(tree):
    """The activities of `xcresulttool get test-results activities`, of the test's last run."""
    runs = tree.get("testRuns") or []
    return (runs[-1].get("activities") or []) if runs else []


def xcresult(path, *args):
    out = subprocess.run(
        ["xcrun", "xcresulttool", "get", "test-results", *args, "--path", path],
        capture_output=True, text=True, check=True).stdout
    return json.loads(out)


def main():
    path = sys.argv[1]
    many = int(sys.argv[2]) if len(sys.argv) > 2 else 6
    for took, ident, name in sorted(cases(xcresult(path, "tests")), reverse=True)[:many]:
        try:
            lines = report(ident, took, activities_of(xcresult(path, "activities", "--test-id", ident)))
        except (subprocess.CalledProcessError, ValueError) as error:
            lines = ["%s: its steps could not be read: %s" % (ident, error)]
        print("\n".join(lines))


if __name__ == "__main__":
    main()

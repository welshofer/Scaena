#!/usr/bin/env bash
# What a test run's result bundle holds that xcodebuild's -quiet keeps back: how many passed,
# each failure and why, and what the tests printed of the gates; with a folder, the screenshots
# the tests kept, written there (PLAN 4.1, 4.2).
#
#   apps/ipad/results.sh RESULTS.xcresult [SHOTS]
set -uo pipefail
results=$1
[ -d "$results" ] || exit 0
xcrun xcresulttool get test-results summary --path "$results" | python3 -c '
import json, sys
summary = json.load(sys.stdin)
counts = [summary.get(k) for k in ("passedTests", "failedTests", "skippedTests")]
print("%s passed, %s failed, %s skipped" % tuple(counts))
for failure in summary.get("testFailures", []):
    print("failed: %s: %s" % (failure.get("testName"), failure.get("failureText")))
'
# How long each test took, slowest first: a pass that grows names the test that grows it (#1240,
# the second pass from 1126 s to 2432 s in a day).
xcrun xcresulttool get test-results tests --path "$results" 2>/dev/null | python3 -c '
import json, re, sys
def seconds(node):
    if isinstance(node.get("durationInSeconds"), (int, float)):
        return float(node["durationInSeconds"])
    total = 0.0
    for amount, unit in re.findall(r"([0-9.]+)\s*(h|m|s)", node.get("duration") or ""):
        total += float(amount) * {"h": 3600, "m": 60, "s": 1}[unit]
    return total
cases = []
def walk(node, within):
    name = node.get("name", "")
    if node.get("nodeType") == "Test Case":
        cases.append((seconds(node), "%s/%s" % (within, name), node.get("result", "")))
        return
    for child in node.get("children", []):
        walk(child, name if node.get("nodeType") == "Test Suite" else within)
for node in json.load(sys.stdin).get("testNodes", []):
    walk(node, "")
if cases:
    print("each test, slowest first:")
    for took, name, result in sorted(cases, reverse=True):
        print("%8.1f s  %s  %s" % (took, name, result))
' || echo "no test durations in $results"
# Where the slowest tests' time went, step by step, as XCTest's activities say: the wait for the
# app to go idle, a query's snapshot, a wait of the test's own (scripts/ui_steps.py).
python3 "$(dirname "$0")/../../scripts/ui_steps.py" "$results" 6 || echo "no steps read from $results"
xcrun xcresulttool get log --path "$results" --type console 2>/dev/null | grep -oE "gate [0-9], criterion [0-9].*|skipping: .*"
if [ -n "${2:-}" ]; then
  rm -rf "$2"
  mkdir -p "$2"
  xcrun xcresulttool export attachments --path "$results" --output-path "$2" >/dev/null &&
    ls "$2"
fi
exit 0

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
xcrun xcresulttool get log --path "$results" --type console 2>/dev/null | grep -E "^gate |^skipping"
if [ -n "${2:-}" ]; then
  rm -rf "$2"
  mkdir -p "$2"
  xcrun xcresulttool export attachments --path "$results" --output-path "$2" >/dev/null &&
    ls "$2"
fi
exit 0

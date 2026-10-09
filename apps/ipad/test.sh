#!/usr/bin/env bash
# ScaenaKit's tests on an iPad simulator (PLAN 4.1): the engine built for the simulator, the
# project XcodeGen makes, and xcodebuild's test run on the newest iPad the simulator offers.
# Needs Xcode and XcodeGen (`brew install xcodegen`).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
"$here/engine.sh" sim debug
cd "$here"
xcodegen generate --quiet
udid=$(xcrun simctl list devices available --json | python3 -c '
import json, sys
pads = []
for runtime, devices in json.load(sys.stdin)["devices"].items():
    if ".iOS-" not in runtime:
        continue
    version = tuple(int(n) for n in runtime.rsplit(".iOS-", 1)[1].split("-"))
    for d in devices:
        if d["name"].startswith("iPad"):
            pads.append((version, d["name"].startswith("iPad Pro"), d["name"], d["udid"]))
if not pads:
    sys.exit("no iPad simulator: add one in Xcode")
pads.sort()
print(pads[-1][3])
print("on", pads[-1][2], "with iOS", ".".join(map(str, pads[-1][0])), file=sys.stderr)
')
results="$root/target/ipad/ScaenaKitTests.xcresult"
rm -rf "$results"
status=0
xcodebuild test -quiet -project Scaena.xcodeproj -scheme ScaenaKitTests \
  -destination "platform=iOS Simulator,arch=arm64,id=$udid" -resultBundlePath "$results" || status=$?
# What -quiet keeps back: each failure and why, and what the tests printed.
if [ -d "$results" ]; then
  xcrun xcresulttool get test-results summary --path "$results" | python3 -c '
import json, sys
summary = json.load(sys.stdin)
counts = [summary.get(k) for k in ("passedTests", "failedTests", "skippedTests")]
print("%s passed, %s failed, %s skipped" % tuple(counts))
for failure in summary.get("testFailures", []):
    print("failed: %s: %s" % (failure.get("testName"), failure.get("failureText")))
' || true
  xcrun xcresulttool get log --path "$results" --type console 2>/dev/null | grep -E "^gate |^skipping" || true
fi
exit "$status"

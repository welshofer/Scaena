#!/usr/bin/env bash
# The newest iPad the simulator offers, an iPad Pro where there is one, booted: its UDID, for
# `apps/ipad/test.sh` and `apps/ipad/app-test.sh` (PLAN 4.1, 4.2). Needs Xcode.
set -euo pipefail
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
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
echo "$udid"

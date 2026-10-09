#!/usr/bin/env bash
# ScaenaKit's tests on an iPad simulator (PLAN 4.1): the engine built for the simulator, the
# project XcodeGen makes, and xcodebuild's test run on the newest iPad the simulator offers.
# Needs Xcode and XcodeGen (`brew install xcodegen`).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
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
xcodebuild test -quiet -project Scaena.xcodeproj -scheme ScaenaKitTests -destination "id=$udid"

#!/usr/bin/env bash
# ScaenaKit's tests on an iPad simulator (PLAN 4.1): the engine built for the simulator, the
# project XcodeGen makes, and xcodebuild's test run on the newest iPad the simulator offers
# (`apps/ipad/simulator.sh`).
# Needs Xcode and XcodeGen (`brew install xcodegen`).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# What the engine's build phase said, where Xcode's log keeps none of it (apps/ipad/engine.sh).
engine_log() { [ -f "$root/target/ipad/engine.log" ] && { echo "--- target/ipad/engine.log:" >&2; tail -n 60 "$root/target/ipad/engine.log" >&2; }; return 0; }
"$here/engine.sh" sim debug
cd "$here"
xcodegen generate --quiet
udid=$("$here/simulator.sh")
results="$root/target/ipad/ScaenaKitTests.xcresult"
rm -rf "$results"
status=0
xcodebuild test -quiet -project Scaena.xcodeproj -scheme ScaenaKitTests \
  -destination "platform=iOS Simulator,arch=arm64,id=$udid" -resultBundlePath "$results" || status=$?
[ "$status" -eq 0 ] || engine_log
"$here/results.sh" "$results"
exit "$status"

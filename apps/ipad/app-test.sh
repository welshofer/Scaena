#!/usr/bin/env bash
# The iPad app on an iPad simulator (PLAN 4.2): built as Xcode builds it, B1 put in the app's own
# folder, and its UI tests run (`ScaenaUITests`): the app launched, and B1 opened in its window from
# the document browser, On My iPad › Scaena.
# The screenshot each test keeps of its window lands in target/ipad/shots. Needs Xcode and
# XcodeGen (`brew install xcodegen`).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
"$here/engine.sh" sim debug
cd "$here"
xcodegen generate --quiet
udid=$("$here/simulator.sh")
destination="platform=iOS Simulator,arch=arm64,id=$udid"
built="$root/target/ipad/derived"
xcodebuild build-for-testing -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
  -derivedDataPath "$built"
# Installed before the tests run, so that B1 can go in its folder: the tests install it again
# over itself, which keeps what the folder holds.
xcrun simctl install "$udid" "$built/Build/Products/Debug-iphonesimulator/ScaenaApp.app"
documents="$(xcrun simctl get_app_container "$udid" com.welshofer.Scaena data)/Documents"
mkdir -p "$documents"
rm -rf "$documents/b1.scaena"
cp -R "$root/tests/bench/b1.scaena" "$documents/b1.scaena"
results="$root/target/ipad/Scaena.xcresult"
rm -rf "$results"
# What the canvas's keys did while the tests ran, as the app logs them at the debug level: kept
# beside the screenshots, to say where a key went (PLAN 4.6).
keys="$root/target/ipad/keys.log"
xcrun simctl spawn "$udid" log stream --level debug --style compact \
  --predicate 'subsystem == "com.welshofer.Scaena"' >"$keys" 2>&1 &
logging=$!
status=0
xcodebuild test-without-building -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
  -derivedDataPath "$built" -resultBundlePath "$results" || status=$?
kill "$logging" 2>/dev/null || true
"$here/results.sh" "$results" "$root/target/ipad/shots"
cp "$keys" "$root/target/ipad/shots/keys.txt" 2>/dev/null || true
exit "$status"

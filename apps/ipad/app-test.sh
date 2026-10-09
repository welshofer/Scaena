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
status=0
xcodebuild test-without-building -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
  -derivedDataPath "$built" -resultBundlePath "$results" || status=$?
"$here/results.sh" "$results" "$root/target/ipad/shots"
exit "$status"

#!/usr/bin/env bash
# The iPad app on an iPad simulator (PLAN 4.2): built as Xcode builds it, B1 put in the app's own
# folder, and its UI tests run (`ScaenaUITests`): the app launched, and B1 opened in its window from
# the document browser, On My iPad › Scaena.
# The screenshot each test keeps of its window lands in target/ipad/shots. Needs Xcode and
# XcodeGen (`brew install xcodegen`).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# What the engine's build phase said, where Xcode's log keeps none of it (apps/ipad/engine.sh).
engine_log() { [ -f "$root/target/ipad/engine.log" ] && { echo "--- target/ipad/engine.log:" >&2; tail -n 60 "$root/target/ipad/engine.log" >&2; }; return 0; }
"$here/engine.sh" sim debug
cd "$here"
xcodegen generate --quiet
udid=$("$here/simulator.sh")
destination="platform=iOS Simulator,arch=arm64,id=$udid"
built="$root/target/ipad/derived"
xcodebuild build-for-testing -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
  -derivedDataPath "$built" || { engine_log; exit 1; }
# Installed before the tests run, so that B1 can go in its folder: the tests install it again
# over itself, which keeps what the folder holds.
xcrun simctl install "$udid" "$built/Build/Products/Debug-iphonesimulator/ScaenaApp.app"
container=$(xcrun simctl get_app_container "$udid" com.welshofer.Scaena data)
documents="$container/Documents"
mkdir -p "$documents" "$container/Library"
rm -rf "$documents/b1.scaena" "$container/Library/b1-shipped.scaena"
cp -R "$root/tests/bench/b1.scaena" "$documents/b1.scaena"
# B1 as shipped, kept where the document browser does not show it: each test launches the app with
# `-b1AsShipped YES`, which puts B1 back from it before any window opens, so no test sees what the
# one before it left (ScaenaApp's `putBackB1`).
cp -R "$root/tests/bench/b1.scaena" "$container/Library/b1-shipped.scaena"
results="$root/target/ipad/Scaena.xcresult"
rm -rf "$results"
# What the app says at the debug level while the tests run, kept beside the screenshots: where
# each key went (PLAN 4.6), and how long the GPU, each deck, and each canvas's surface took to open.
said="$root/target/ipad/log.txt"
xcrun simctl spawn "$udid" log stream --level debug --style compact \
  --predicate 'subsystem == "com.welshofer.Scaena" OR (process == "ScaenaApp" AND (messageType == error OR messageType == fault))' >"$said" 2>&1 &
logging=$!
status=0
xcodebuild test-without-building -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
  -derivedDataPath "$built" -resultBundlePath "$results" || status=$?
[ "$status" -eq 0 ] || engine_log
kill "$logging" 2>/dev/null || true
"$here/results.sh" "$results" "$root/target/ipad/shots"
cp "$said" "$root/target/ipad/shots/log.txt" 2>/dev/null || true
# What SwiftUI finds wrong in how the app's views are made (a change handler run twice in a frame,
# say) fails the run, the fault named: no tap or shot of the tests shows it.
if grep -F '[com.apple.SwiftUI:Invalid Configuration]' "$said" 2>/dev/null; then
  echo "SwiftUI found an invalid configuration while the UI tests ran: target/ipad/log.txt" >&2
  [[ "$status" -ne 0 ]] || status=1
fi
# The app carries its icon and its accent, compiled from apps/ipad/Assets.xcassets
# (scripts/build_app_icon.py): an app built without its icon fails the run, after its tests.
plist="$built/Build/Products/Debug-iphonesimulator/ScaenaApp.app/Info.plist"
named() { /usr/libexec/PlistBuddy -c "Print :$1" "$plist" 2>/dev/null; }
icon=$(named 'CFBundleIcons~ipad:CFBundlePrimaryIcon:CFBundleIconName' || named 'CFBundleIcons:CFBundlePrimaryIcon:CFBundleIconName' ||
  named CFBundleIconName || true)
echo "the app's icon: ${icon:-none}; its accent: $(named NSAccentColorName || echo none)"
[ "$icon" = AppIcon ] || status=1
exit "$status"

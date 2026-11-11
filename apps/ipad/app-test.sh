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
# Each test runs in a pass with the keyboard it needs (lift-up 20261110). Most run with no software
# keyboard, as KeysTests drives the canvas by a hardware one: with it, each key XCTest types in a
# text hides the software keyboard, which iOS brings back moments later, and on CI that flapping has
# left XCTest's wait for the app to go idle unended, each event then waiting out its two minutes.
# The simulator's own Connect Hardware Keyboard did not keep it down, booted headless or in
# Simulator.app (#1067's runs), so the tests launch the app with `-softwareKeyboard NO`, which in a
# debug build gives the canvas's text input no keyboard of iOS's (`CanvasKeys.inputView`). That
# pass tests keys with no software keyboard shown, not iOS's own hardware-keyboard mode or its
# shortcuts bar, which an iPad's keyboard checks. The tests that need the software keyboard up run
# after, in a pass of their own; none is left out. Each test reads its pass from `SCAENA_KEYBOARD`
# (`TEST_RUNNER_` hands it to the tests) and holds it: no software keyboard in the first, and one
# in the second.
software=(
  ScaenaUITests/TouchTests/testTheKeyboardKeepsTheLineTypedInAboveIt
  ScaenaUITests/KeysTests/testTypingInCodeSettles
)
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
softly="$root/target/ipad/Scaena-software-keyboard.xcresult"
rm -rf "$results" "$softly"
# What the app says at the debug level while the tests run, kept beside the screenshots: where
# each key went (PLAN 4.6), and how long the GPU, each deck, and each canvas's surface took to open.
said="$root/target/ipad/log.txt"
xcrun simctl spawn "$udid" log stream --level debug --style compact \
  --predicate 'subsystem == "com.welshofer.Scaena" OR (process == "ScaenaApp" AND (messageType == error OR messageType == fault)) OR (process == "ScaenaApp" AND subsystem == "com.apple.SwiftUI" AND category == "Changed Body Properties")' >"$said" 2>&1 &
logging=$!
status=0
# A test held up past ten minutes fails by its name, its screenshot and result bundle kept, and the
# run goes on to the next: a hang no longer takes the job to its limit unseen. The slowest test
# that passes takes some four and a half minutes on CI's simulator (TouchTests.testTheCanvasByTouch).
tests() {
  xcodebuild test-without-building -quiet -project Scaena.xcodeproj -scheme ScaenaApp -destination "$destination" \
    -derivedDataPath "$built" -test-timeouts-enabled YES -maximum-test-execution-time-allowance 600 "$@" || status=$?
}
echo "--- the UI tests with no software keyboard" >>"$said"
TEST_RUNNER_SCAENA_KEYBOARD=hardware tests -resultBundlePath "$results" "${software[@]/#/-skip-testing:}"
echo "--- the UI tests with the software keyboard" >>"$said"
TEST_RUNNER_SCAENA_KEYBOARD=software tests -resultBundlePath "$softly" "${software[@]/#/-only-testing:}"
kill "$logging" 2>/dev/null || true
[ "$status" -eq 0 ] || engine_log
# The two passes as one result bundle, so their counts, failures, and screenshots are read as one
# run's; where the second pass left no bundle, the first is read alone.
both="$root/target/ipad/Scaena-both.xcresult"
rm -rf "$both"
if [ -d "$softly" ] && [ -d "$results" ] && xcrun xcresulttool merge "$results" "$softly" --output-path "$both" >/dev/null; then
  rm -rf "$results" "$softly"
  mv "$both" "$results"
elif [ -d "$softly" ]; then
  echo "the two passes' result bundles did not merge: the software keyboard's pass is read apart" >&2
  "$here/results.sh" "$softly"
fi
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

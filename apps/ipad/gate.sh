#!/usr/bin/env bash
# Gate 4's readings on an iPad (docs/gate-4.md): the tests of its first two criteria
# (`ScaenaGateTests`, hosted by an app that does nothing), built in release for an iPad joined to
# this Mac and run on it. The torture deck's display lists are held to the goldens', B1's cover
# painted on a layer to the CPU painter's pixels, and B1 opened to its first frame; the lines they
# print, `gate 4, criterion …`, are the readings. With no iPad named, the newest iPad simulator runs
# them, built in debug, as CI's `ipad` job does to keep the command working: the simulator paints
# with the CPU painter (PLAN 4.1), and its readings are not the gate's. Needs Xcode and XcodeGen; an
# iPad needs a team that signs for it in `apps/ipad/Team.xcconfig` (`DEVELOPMENT_TEAM = <your
# team's ID>`), Developer Mode on, and to be unlocked.
#
#   apps/ipad/gate.sh [UDID]     (an iPad's, as `xcrun xctrace list devices` lists it)
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
# Words, not arrays: macOS's bash 3.2 calls an empty array unbound under `set -u`.
if [ -n "${1:-}" ]; then
  "$here/engine.sh" device release
  destination="platform=iOS,id=$1"
  configuration=Release
  signing=-allowProvisioningUpdates
else
  "$here/engine.sh" sim debug
  destination="platform=iOS Simulator,arch=arm64,id=$("$here/simulator.sh")"
  configuration=Debug
  signing=
fi
cd "$here"
xcodegen generate --quiet
results="$root/target/ipad/Gate.xcresult"
said="$root/target/ipad/gate.log"
rm -rf "$results"
status=0
# shellcheck disable=SC2086
xcodebuild test -project Scaena.xcodeproj -scheme ScaenaGate -configuration "$configuration" \
  -destination "$destination" -resultBundlePath "$results" $signing >"$said" 2>&1 || status=$?
summary=$("$here/results.sh" "$results")
echo "$summary"
# The readings as the tests printed them, from the result bundle or else from xcodebuild's log.
if ! grep -q "gate 4, criterion" <<<"$summary" && ! grep -oE "gate 4, criterion [12].*" "$said" | sort -u; then
  echo "no reading: no test printed one (xcodebuild's log: $said)" >&2
  tail -n 40 "$said" >&2
  exit 1
fi
exit "$status"

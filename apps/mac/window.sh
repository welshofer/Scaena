#!/usr/bin/env bash
# The Mac app as someone opens it (PLAN 3.3): the app built in debug, over the debug static library
# `swift build --product Scaena` linked, made a bundle macOS opens, the trails example saved as a
# deck and opened in its window, and the screen kept in target/mac/shots. Seen, not tested: CI
# pushes it beside the iPad's (scripts/shots.sh).
#
#   apps/mac/window.sh   (after `swift build --product Scaena` in apps/mac/ScaenaKit)
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=$(cd "$root/apps/mac/ScaenaKit" && swift build --show-bin-path)
out="$root/target/mac/window"
app="$out/Scaena.app"
rm -rf "$out"
mkdir -p "$app/Contents/MacOS" "$out/shots"
cp "$bin/Scaena" "$app/Contents/MacOS/Scaena"
cp "$root/apps/mac/Scaena/Info.plist" "$app/Contents/Info.plist"
codesign --force --sign - "$app"
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "$app"
cd "$root"
cargo run -q -p scaena-cli --locked -- save docs/examples/trails.deck.json --to "$out/trails.scaena"
system_profiler SPDisplaysDataType | grep -i resolution || true
open -a "$app" "$out/trails.scaena"
# The window, its states drawn small, and the canvas painted.
sleep 15
screencapture -x "$out/shots/mac-trails.png"
# Whether this runner lets a script press keys and click in another app (System Events), which a
# walk of the window would need.
osascript -e 'tell application "System Events" to tell process "Scaena" to get name of every window' || true
osascript -e 'tell application "Scaena" to quit' || pkill -x Scaena || true

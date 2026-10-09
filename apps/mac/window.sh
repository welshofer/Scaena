#!/usr/bin/env bash
# The Mac app as someone opens it (PLAN 3.3): the app built in debug, over the debug static library
# `swift build --product Scaena` linked, made a bundle macOS opens, the trails example saved as a
# deck and opened in its window, the screen kept in target/mac/window/shots, then a walk of the
# window, each step's screen kept. Seen, not tested: CI pushes them beside the iPad's
# (scripts/shots.sh).
#
#   apps/mac/window.sh   (after `swift build --product Scaena` in apps/mac/ScaenaKit)
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=$(cd "$root/apps/mac/ScaenaKit" && swift build --show-bin-path)
out="$root/target/mac/window"
app="$out/Scaena.app"
rm -rf "$out"
mkdir -p "$app/Contents/MacOS" "$out/shots"
# What it says goes with the screenshots too.
exec > >(tee "$out/shots/window.txt") 2>&1
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

# A walk of the window, as a person's first minute in it, by System Events: each step's screen
# kept, and where the window stands said. Seen, not tested: a step that fails says so and the
# walk goes on.
events() { osascript -e "tell application \"System Events\" to $1" 2>&1 || true; }
window() { events "tell process \"Scaena\" to get {position, size} of window 1" | tr -d ','; }
# The screen, and the part of it a window may take: width, height, then the part's left, top,
# width, and height, in points.
screen=$(osascript -l JavaScript -e 'ObjC.import("AppKit");
  const s = $.NSScreen.mainScreen, f = s.frame, v = s.visibleFrame;
  [f.size.width, f.size.height, v.origin.x, f.size.height - v.origin.y - v.size.height, v.size.width, v.size.height]
    .map(Math.round).join(" ")' 2>&1 || true)
echo "screen: $screen"
echo "window as opened: $(window)"
osascript -e 'tell application "Scaena" to activate' || true
read -r _ _ vx vy vw vh <<<"$screen" || true
if [[ "${vh:-}" =~ ^[0-9]+$ ]]; then
  # The window fitted to the screen, as small as its content lets it be.
  events "tell process \"Scaena\" to set position of window 1 to {$vx, $vy}"
  events "tell process \"Scaena\" to set size of window 1 to {$vw, $vh}"
  sleep 3
  echo "window fitted to the screen: $(window)"
  screencapture -x "$out/shots/mac-fitted.png"
  read -r wx wy ww wh <<<"$(window)" || true
  if [[ "${wh:-}" =~ ^[0-9]+$ ]]; then
    # A click in the middle of the slide, between the slides and the inspector: what draws there
    # selected, and the inspector showing it.
    cx=$((wx + 200 + (ww - 200 - 300) / 2))
    cy=$((wy + 52 + (wh - 52) / 2))
    echo "click at $cx, $cy: $(events "click at {$cx, $cy}")"
    sleep 2
    screencapture -x "$out/shots/mac-clicked.png"
    # Tab: the next object in reading order.
    events 'key code 48'
    sleep 2
    screencapture -x "$out/shots/mac-tabbed.png"
    # A text from the Insert menu, where the click was: typed in, its words selected (PLAN 3.21).
    echo "insert: $(events 'tell process "Scaena" to click menu item 1 of menu 1 of menu item "Text" of menu 1 of menu bar item "Insert" of menu bar 1')"
    sleep 2
    screencapture -x "$out/shots/mac-inserted.png"
    # What is typed takes the place of the words selected.
    events 'keystroke "Seen on a Mac"'
    sleep 2
    screencapture -x "$out/shots/mac-typed.png"
    events 'key code 53'
    sleep 1
  fi
fi
# The deck is edited now: no saving it, so no asking.
pkill -x Scaena || true

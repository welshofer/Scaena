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

# A walk of the window, as a person's first minute in it, each step's screen kept and where the
# window stands said. The pointer is the mouse's own events (`pointer.js`), which the canvas's
# gestures take; menus and keys go through System Events. Seen, not tested: a step that fails says
# so and the walk goes on.
events() { osascript -e "tell application \"System Events\" to $1" 2>&1 || true; }
app() { events "tell process \"Scaena\" to $1"; }
window() { app "get {position, size} of window 1" | tr -d ','; }
pointer() { osascript -l JavaScript "$root/apps/mac/pointer.js" "$@" 2>&1 || true; }
frames() { osascript -l JavaScript "$root/apps/mac/frames.js" "$@" 2>&1 || true; }
menu() {
  local said
  said=$(app "click menu item \"$2\" of menu 1 of menu bar item \"$1\" of menu bar 1")
  if [[ "$said" == *rror* ]]; then echo "$1 › $2: $said"; fi
}
shot() { screencapture -x "$out/shots/mac-$1.png"; }
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
  # The View menu's items as a person reads them: opened, so a title that says what it does now
  # (Hide Toolbar, say, or Show) is brought up to date first; then closed.
  viewing() {
    app 'click menu bar item "View" of menu bar 1' >/dev/null
    sleep 1
    app 'get name of every menu item of menu 1 of menu bar item "View" of menu bar 1'
    events 'key code 53' >/dev/null
    sleep 1
  }
  # The View menu's item that shows or hides `$1`, whichever it reads as now, chosen.
  toggle() {
    local items verb
    items=$(viewing)
    for verb in Hide Show Toggle; do
      if [[ ", $items, " == *", $verb $1, "* ]]; then
        menu View "$verb $1"
        sleep 2
        echo "View › $verb $1"
        return
      fi
    done
    echo "the View menu shows or hides no $1"
  }
  # How narrow the window goes, as a person drags its edge in: asked for 600 points, the width it
  # takes, and each split view's panes at that width; then the window as wide as the screen again
  # (PLAN 3.27).
  narrowest() {
    app "set size of window 1 to {600, $vh}" >/dev/null
    sleep 2
    local w
    read -r _ _ w _ <<<"$(window)"
    echo "$w"
    frames splits | sed 's/^/    /'
    app "set size of window 1 to {$vw, $vh}" >/dev/null
    sleep 1
  }
  inspector() { events 'keystroke "i" using {option down, command down}' >/dev/null; sleep 2; }
  app "set position of window 1 to {$vx, $vy}" >/dev/null
  echo "the View menu: $(viewing)"
  echo "narrowest, as it opens: $(narrowest)"
  inspector
  echo "narrowest, the inspector hidden: $(narrowest)"
  toggle Sidebar
  echo "narrowest, the slides and the inspector hidden: $(narrowest)"
  inspector
  echo "narrowest, the slides hidden: $(narrowest)"
  toggle Sidebar
  toggle Toolbar
  echo "narrowest, the toolbar hidden: $(narrowest)"
  toggle Toolbar
  # A second pane beside the canvas, in a split view the user divides.
  toggle Source
  echo "narrowest, the source shown: $(narrowest)"
  toggle Source

  # The window fitted to the screen, as small as its content lets it be; where it is still wider,
  # the walk goes on with the slides hidden, so what it clicks is on the screen.
  app "set position of window 1 to {$vx, $vy}" >/dev/null
  app "set size of window 1 to {$vw, $vh}" >/dev/null
  sleep 3
  echo "window fitted to the screen: $(window)"
  shot fitted
  read -r _ _ fw _ <<<"$(window)"
  if [[ "${fw:-}" =~ ^[0-9]+$ ]] && ((fw > vw)); then
    toggle Sidebar
    app "set size of window 1 to {$vw, $vh}" >/dev/null
    sleep 2
    echo "window fitted with the slides hidden: $(window)"
  fi
  echo "what it holds, three deep:"
  frames tree 3

  # A click in the middle of the slide: what draws there selected, and the inspector showing it.
  read -r sx sy sw sh <<<"$(frames find canvas)" || true
  if [[ "${sh:-}" =~ ^[0-9]+$ ]]; then
    echo "the slide: $sx $sy $sw $sh"
    cx=$((sx + sw / 2))
    cy=$((sy + sh / 2))
    pointer click "$cx" "$cy"
    sleep 2
    shot clicked
    # Dragged a little right and down, it lands where it is let go (PLAN 3.19); then undone.
    pointer drag "$cx" "$cy" $((cx + 90)) $((cy + 40))
    sleep 2
    shot dragged
    events 'keystroke "z" using command down'
    sleep 2
    # A right click on it offers what is done to it (PLAN 3.23).
    pointer right "$cx" "$cy"
    sleep 2
    shot menu
    events 'key code 53'
    sleep 1
  else
    echo "no slide found: $sx $sy $sw $sh"
  fi
  # Tab: the next object in reading order.
  events 'key code 48'
  sleep 2
  shot tabbed

  # A text from the toolbar's Text: typed in, its words selected (PLAN 3.21), and what is typed
  # takes their place.
  echo "frontmost: $(events 'get frontmost of process "Scaena"')"
  read -r tx ty tw th <<<"$(frames find insert-Text)" || true
  if [[ "${th:-}" =~ ^[0-9]+$ ]]; then
    echo "the toolbar's Text: $tx $ty $tw $th"
    pointer click $((tx + tw / 2)) $((ty + th / 2))
    # Typed once the hint over the slide says the text is typed in: on CI's virtual Mac the insert
    # has taken longer than two seconds, and words typed before it went to the canvas as its keys.
    typing="not within 20 s"
    for waited in $(seq 0 20); do
      if [[ "$(frames value hint)" == Editing* ]]; then
        typing="within $waited s"
        break
      fi
      sleep 1
    done
    echo "the text inserted, typed in: $typing"
    shot inserted
    events 'keystroke "Seen on a Mac"'
    sleep 2
    shot typed
    events 'key code 53'
    sleep 1
  else
    echo "no Text in the toolbar: $tx $ty $tw $th"
  fi
  # The same from the Insert menu, opened as a person opens it.
  echo "the slide holds, before:"
  frames holds canvas
  app 'click menu bar item "Insert" of menu bar 1' >/dev/null
  sleep 1
  app 'click menu item "Text" of menu 1 of menu bar item "Insert" of menu bar 1' >/dev/null
  sleep 1
  shot insert-menu
  echo "Insert › Text: $(app 'get {name, enabled} of every menu item of menu 1 of menu item "Text" of menu 1 of menu bar item "Insert" of menu bar 1')"
  echo "insert: $(app 'click menu item 1 of menu 1 of menu item "Text" of menu 1 of menu bar item "Insert" of menu bar 1')"
  sleep 2
  shot inserted-menu
  echo "the slide holds, after:"
  frames holds canvas
  events 'key code 53'
  sleep 1

  # The safe area (PLAN 3.29): View › Show Safe Area shades a strip 3/8 inch in from each edge of
  # the slide, a guide that holds nothing off it; the item then reads Hide and takes it away.
  toggle "Safe Area"
  shot safe-area
  toggle "Safe Area"

  # The Document tab's Theme (PLAN 3.28): each value a field that reads as one. A color typed in
  # the paper's field is set on Return, and the Edit menu names the step; ⌘Z takes it back and
  # ⌘⇧Z makes it again, the slide drawn in each; then ⌘Z leaves the deck as it was.
  # The Edit menu's Undo and Redo as a person reads them, and whether each is enabled: opened, so
  # their titles are brought up to date, its screen kept as `$1`, then closed.
  undoing() {
    app 'click menu bar item "Edit" of menu bar 1' >/dev/null
    sleep 1
    shot "$1"
    app 'get {name, enabled} of menu items 1 thru 2 of menu 1 of menu bar item "Edit" of menu bar 1'
    events 'key code 53' >/dev/null
    sleep 1
  }
  menu View Document
  sleep 2
  shot theme
  read -r px py pw ph <<<"$(frames find theme-color-paper)" || true
  if [[ "${ph:-}" =~ ^[0-9]+$ ]]; then
    echo "the theme's paper: $px $py $pw $ph, $(frames value theme-color-paper)"
    pointer click $((px + pw / 2)) $((py + ph / 2))
    sleep 1
    events 'keystroke "a" using command down'
    events 'keystroke "#2B1B3D"'
    events 'key code 36'
    sleep 3
    echo "typed and Return: $(frames value theme-color-paper); the Edit menu: $(undoing theme-edit-menu)"
    shot theme-typed
    events 'keystroke "z" using command down'
    sleep 3
    echo "⌘Z: $(frames value theme-color-paper); the Edit menu: $(undoing theme-edit-menu-undone)"
    shot theme-undone
    events 'keystroke "z" using {shift down, command down}'
    sleep 3
    echo "⌘⇧Z: $(frames value theme-color-paper)"
    shot theme-redone
    events 'keystroke "z" using command down'
    sleep 2
    echo "⌘Z again: $(frames value theme-color-paper)"
  else
    echo "no field for the theme's paper: $px $py $pw $ph"
  fi
fi
# The deck is edited now: no saving it, so no asking.
pkill -x Scaena || true

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
# What the canvas holds as VoiceOver hears it, read by the accessibility API (holds.swift).
swiftc -O -o "$out/holds" "$root/apps/mac/holds.swift" || echo "holds.swift: not built"
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
echo "the canvas as VoiceOver hears it: $("$out/holds" "$(pgrep -n -x Scaena)" canvas 2>&1 | head -3 | tr '\n' ';')"

# A walk of the window, as a person's first minute in it, each step's screen kept and where the
# window stands said. The pointer is the mouse's own events (`pointer.js`), which the canvas's
# gestures take; menus and keys go through System Events. Seen, not tested: a step that fails says
# so and the walk goes on.
events() { osascript -e "tell application \"System Events\" to $1" 2>&1 || true; }
app() { events "tell process \"Scaena\" to $1"; }
window() { app "get {position, size} of window 1" | tr -d ','; }
pointer() { osascript -l JavaScript "$root/apps/mac/pointer.js" "$@" 2>&1 || true; }
frames() { osascript -l JavaScript "$root/apps/mac/frames.js" "$@" 2>&1 || true; }
# What the objects an element holds read as to VoiceOver, one to a line: by the accessibility API,
# which reads the attributed description SwiftUI keeps a label in; else by System Events, which
# reads none of them.
# Asked again a moment later while the app is too busy to answer, as it may be just after an edit,
# up to three times.
holds() {
  local said
  for _ in 1 2 3; do
    said=$("$out/holds" "$(pgrep -n -x Scaena)" "$1" 2>/dev/null) && { echo "$said"; return; }
    sleep 1
  done
  frames holds "$1"
}
# What the element identified as `$1` reads as itself to VoiceOver, by the accessibility API, as
# `holds` reads what it holds; else its value, by System Events.
named() { "$out/holds" "$(pgrep -n -x Scaena)" "$1" self 2>/dev/null || frames value "$1"; }
# A menu's item chosen; again a moment later while the app is too busy to be asked, as it may be
# just after an edit, up to five times.
menu() {
  local said
  for _ in 1 2 3 4 5; do
    said=$(app "click menu item \"$2\" of menu 1 of menu bar item \"$1\" of menu bar 1")
    [[ "$said" == *rror* ]] || return 0
    sleep 1
  done
  echo "$1 › $2: $said"
}
# Wait until the canvas reads as other than `$1`, the slide it read as (with `same`, as `$1`), up
# to 20 s: "<what it reads as>, within N s".
reads() {
  local now waited
  for waited in $(seq 0 20); do
    now=$(named canvas)
    if [[ -n "$now" && "$now" != *rror* ]] && { [[ "${2:-}" == same && "$now" == "$1" ]] || [[ "${2:-}" != same && "$now" != "$1" ]]; }; then
      echo "$now, within $waited s"
      return
    fi
    sleep 1
  done
  echo "still ${now:-nothing} after 20 s"
}
shot() { screencapture -x "$out/shots/mac-$1.png"; }
# How long until what `READER IDENTIFIER` reads (`holds canvas`) is no longer BEFORE, and holds
# WORDS where they are given, up to 20 seconds.
changed() {
  local reader=$1 tag=$2 before=$3 words=${4:-} start=$SECONDS now
  while ((SECONDS - start <= 20)); do
    now=$("$reader" "$tag")
    if [[ "$now" != "$before" && "$now" == *"$words"* ]]; then
      echo "within $((SECONDS - start)) s"
      return
    fi
    sleep 1
  done
  echo "not within 20 s"
}
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
    # Dragged a little right and down, it lands where it is let go (PLAN 3.19); then undone. The
    # drop has landed once the objects the canvas holds stand otherwise: on CI's virtual Mac the
    # drag's guides were still drawn two seconds after it was let go.
    before=$(holds canvas)
    pointer drag "$cx" "$cy" $((cx + 90)) $((cy + 40))
    echo "the drop landed: $(changed holds canvas "$before")"
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
    before=$(holds canvas)
    events 'keystroke "Seen on a Mac"'
    echo "the words typed, shown: $(changed holds canvas "$before" "Seen on a Mac")"
    shot typed
    events 'key code 53'
    sleep 1
  else
    echo "no Text in the toolbar: $tx $ty $tw $th"
  fi
  # The same from the Insert menu, opened as a person opens it.
  echo "the slide holds, before:"
  holds canvas
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
  holds canvas
  events 'key code 53'
  sleep 1

  # The safe area (PLAN 3.29): View › Show Safe Area shades a strip 3/8 inch in from each edge of
  # the slide, a guide that holds nothing off it; the item then reads Hide and takes it away.
  toggle "Safe Area"
  shot safe-area
  toggle "Safe Area"

  # Add Slide's picker (PLAN 3.30, 3.31): the theme's layouts by their sections down the side,
  # each drawn as a slide started in it, Titles first. Frameworks shows its own, SWOT and the rest;
  # then a click on Titles' first, a title slide, starts it after the slide shown, and ⌘Z takes it
  # back.
  read -r ax ay aw ah <<<"$(frames find add-slide)" || true
  if [[ "${ah:-}" =~ ^[0-9]+$ ]]; then
    pointer click $((ax + aw / 2)) $((ay + ah / 2))
    drawn="not within 30 s"
    for waited in $(seq 0 30); do
      read -r bx by bw bh <<<"$(frames find start-title)" || true
      if [[ "${bh:-}" =~ ^[0-9]+$ ]]; then
        drawn="within $waited s"
        break
      fi
      sleep 1
    done
    echo "the gallery drawn: $drawn"
    shot gallery
    read -r fx fy fw fh <<<"$(frames find section-frameworks)" || true
    if [[ "${fh:-}" =~ ^[0-9]+$ ]]; then
      pointer click $((fx + fw / 2)) $((fy + fh / 2))
      shown="not within 20 s"
      for waited in $(seq 0 20); do
        read -r _ _ _ wh <<<"$(frames find start-swot)" || true
        if [[ "${wh:-}" =~ ^[0-9]+$ ]]; then
          shown="within $waited s"
          break
        fi
        sleep 1
      done
      echo "Frameworks shown: $shown"
      sleep 2
      shot gallery-frameworks
      read -r tx ty tw th <<<"$(frames find section-titles)" || true
      [[ "${th:-}" =~ ^[0-9]+$ ]] && pointer click $((tx + tw / 2)) $((ty + th / 2))
      sleep 1
      read -r bx by bw bh <<<"$(frames find start-title)" || true
    else
      echo "no sections in the picker: $fx $fy $fw $fh"
    fi
    if [[ "${bh:-}" =~ ^[0-9]+$ ]]; then
      pointer click $((bx + bw / 2)) $((by + bh / 2))
      sleep 2
      echo "the slide started holds:"
      holds canvas
      shot started
      events 'keystroke "z" using command down'
      sleep 2
      echo "⌘Z, the slide shown holds:"
      holds canvas
      shot started-undone
    else
      events 'key code 53'
      sleep 1
    fi
  else
    echo "no Add Slide in the toolbar: $ax $ay $aw $ah"
  fi

  # The Slide menu (PLAN 3.14), as a presentation app's: what it offers, opened so each item is
  # enabled as it is now; then New Slide adds one after the slide shown, in its layout, and ⌘Z
  # takes it back, each waited for until the canvas reads as the slide it shows then.
  app 'click menu bar item "Slide" of menu bar 1' >/dev/null
  sleep 1
  echo "the Slide menu: $(app 'get {name, enabled} of every menu item of menu 1 of menu bar item "Slide" of menu bar 1')"
  events 'key code 53'
  sleep 1
  before=$(named canvas)
  menu Slide "New Slide"
  echo "New Slide, the canvas shows: $(reads "$before")"
  shot slide-menu-new
  events 'keystroke "z" using command down'
  echo "⌘Z, the canvas shows: $(reads "$before" same)"
  shot slide-menu-undone

  # The cue and the canvas select together (PLAN 3.14): Funding's title, chart, and words each
  # enter in turn. With the timeline shown, a click on the chart's lane selects the chart, and its
  # lane stands out while the others step back; then, the canvas given the keys by a click on the
  # chart, Shift+Tab selects the words before it, a stack, and its title's and words' lanes stand
  # out. Funding is ten slides down from the cover. The canvas keeps the room the timeline leaves:
  # the slide as tall as it was, or near it, not its least.
  read -r _ _ bw bh <<<"$(frames find canvas)" || true
  toggle Timeline
  read -r _ _ tw th <<<"$(frames find canvas)" || true
  echo "the slide, the timeline hidden: ${bw:-?} × ${bh:-?}; shown: ${tw:-?} × ${th:-?}"
  read -r lx ly lw lh <<<"$(frames find state-cover)" || true
  if [[ "${lh:-}" =~ ^[0-9]+$ ]]; then
    pointer click $((lx + lw / 2)) $((ly + lh / 2))
    sleep 1
    for _ in $(seq 1 10); do
      events 'key code 125'
      sleep 0.5
    done
    sleep 2
    echo "ten slides down, the canvas shows: $(named canvas)"
    read -r gx gy gw gh <<<"$(frames find 'cue-funding-chart enter')" || true
    if [[ "${gh:-}" =~ ^[0-9]+$ ]]; then
      echo "the chart's lane: $gx $gy $gw $gh"
      # Its far end, past where its bar comes to rest: the lane, not the bar.
      pointer click $((gx + gw - 6)) $((gy + gh / 2))
      sleep 2
      shot cue-lane
      read -r sx sy sw sh <<<"$(frames find canvas)" || true
      pointer click $((sx + sw / 2)) $((sy + sh / 2))
      sleep 1
      events 'key code 48 using shift down'
      sleep 2
      shot cue-canvas
      events 'key code 53'
      sleep 1
    else
      echo "no lane for the chart: $gx $gy $gw $gh"
    fi
  else
    echo "no cover in the slides: $lx $ly $lw $lh"
  fi
  toggle Timeline

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

# A picture too large for a slide (SPEC §3.3), pasted on the canvas: the window asks whether a
# smaller copy goes in, the original left as it is, and takes it, a PNG as a PNG with its
# transparency. A PNG 9000 × 40 made here, its left half clear and its right half red.
python3 - "$out/wide.png" <<'PY'
import struct, sys, zlib
w, h = 9000, 40
row = b"\0" + bytes([0, 0, 0, 0]) * (w // 2) + bytes([255, 0, 0, 255]) * (w - w // 2)
chunk = lambda kind, data: struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
open(sys.argv[1], "wb").write(
    b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0))
    + chunk(b"IDAT", zlib.compress(row * h, 9)) + chunk(b"IEND", b""))
PY
osascript -e "set the clipboard to (read (POSIX file \"$out/wide.png\") as «class PNGf»)" 2>&1 || true
read -r px py pw ph <<<"$(frames find canvas)" || true
if [[ "${ph:-}" =~ ^[0-9]+$ ]]; then
  # A press on the slide gives the canvas the keyboard, and the paste lands about it.
  pointer click $((px + pw * 9 / 10)) $((py + ph * 17 / 20))
  sleep 1
  before=$(holds canvas)
  menu Edit "Paste"
  sleep 2
  shot oversized
  echo "a picture too large, pasted, asked about: $(app 'get value of every static text of sheet 1 of window 1' | tr '\n' ' ')"
  taken=$(app 'click button "Use a Smaller Copy" of sheet 1 of window 1')
  [[ "$taken" == *rror* ]] && taken=$(app 'click button "Use a Smaller Copy" of window 1')
  echo "Use a Smaller Copy: ${taken:-pressed}; the copy went in: $(changed holds canvas "$before")"
  sleep 1
  shot oversized-taken
else
  echo "no canvas to paste on: $px $py $pw $ph"
fi

# The light table (PLAN 3.14), as a presentation app's: every slide in a grid that fills the
# window, the slides down the side folded away, and a slider at its foot that makes the slides
# larger, fewer to a row. Where the first slides stand says how many fit across.
menu View "Light Table"
sleep 3
echo "the light table: cover $(frames find slide-cover), agenda $(frames find slide-agenda), promise $(frames find slide-promise), the slider $(frames find light-table-size)"
shot light-table
read -r lx ly lw lh <<<"$(frames find light-table-size)" || true
if [[ "${lh:-}" =~ ^[0-9]+$ ]]; then
  # At its far end, the largest the slides are drawn.
  pointer click $((lx + lw - 2)) $((ly + lh / 2))
  sleep 3
  echo "the slider at its end: cover $(frames find slide-cover), agenda $(frames find slide-agenda)"
  shot light-table-large
fi
menu View "Light Table"
sleep 2
echo "the light table put away, the slides down the side: $(frames find state-cover)"
shot light-table-closed

# The deck is edited now: no saving it, so no asking.
pkill -x Scaena || true

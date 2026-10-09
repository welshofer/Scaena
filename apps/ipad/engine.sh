#!/usr/bin/env bash
# The engine for the iPad (PLAN 4.1, ADR-0023): `scaena-ffi` as a static library for the iPad's
# simulator or the iPad, and `Engine.xcconfig` beside this script, which tells Xcode where the
# library is and the system libraries rustc says it links.
#
#   apps/ipad/engine.sh [sim|device] [debug|release]
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
case "${1:-sim}" in
  sim) target=aarch64-apple-ios-sim ;;
  device) target=aarch64-apple-ios ;;
  *) echo "usage: apps/ipad/engine.sh [sim|device] [debug|release]" >&2; exit 2 ;;
esac
profile=${2:-debug}
release=()
case "$profile" in
  debug) ;;
  release) release=(--release) ;;
  *) echo "usage: apps/ipad/engine.sh [sim|device] [debug|release]" >&2; exit 2 ;;
esac
cd "$root"
rustup target list --installed | grep -qx "$target" || rustup target add "$target"
libs=$(cargo rustc -q --color never -p scaena-ffi --lib --crate-type staticlib --target "$target" "${release[@]}" --locked -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
if [ -z "$libs" ]; then
  echo "rustc named no system libraries for $target: did the build fail?" >&2
  exit 1
fi
cat > apps/ipad/Engine.xcconfig <<XCCONFIG
// Written by apps/ipad/engine.sh ${1:-sim} $profile: the engine Xcode links, and what it links.
LIBRARY_SEARCH_PATHS = \$(inherited) $root/target/$target/$profile
OTHER_LDFLAGS = \$(inherited) $libs
XCCONFIG
echo "$root/target/$target/$profile/libscaena_ffi.a"

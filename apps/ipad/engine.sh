#!/usr/bin/env bash
# The engine for the iPad (PLAN 4.1, 4.2, ADR-0023): `scaena-ffi` as a static library for the
# iPad's simulator or an iPad, linked where Xcode looks for it for that platform and
# configuration (`target/ipad/<platform>-<configuration>`), and `Engine.xcconfig` beside this
# script, which names that folder and the system libraries rustc says the library links. The
# app's and the tests' build phase runs it for what Xcode builds; `just ipad` and `just
# ipad-test` run it first, since XcodeGen reads the xcconfig.
#
#   apps/ipad/engine.sh [sim|device] [debug|release]
#
# A team that signs the app for an iPad goes in `apps/ipad/Team.xcconfig`, kept out of the
# repository: `DEVELOPMENT_TEAM = <your team's ID>`.
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
usage() {
  echo "usage: apps/ipad/engine.sh [sim|device] [debug|release]" >&2
  exit 2
}
case "${1:-sim}" in
  sim) target=aarch64-apple-ios-sim platform=iphonesimulator ;;
  device) target=aarch64-apple-ios platform=iphoneos ;;
  *) usage ;;
esac
# Words, not arrays: macOS's bash 3.2 calls an empty array unbound under `set -u`.
case "${2:-debug}" in
  debug) profile=debug release= configuration=Debug ;;
  release) profile=release release=--release configuration=Release ;;
  *) usage ;;
esac
# Run from Xcode, cargo would inherit the iPad's SDK for the build scripts it compiles for this
# Mac; it gets a terminal's environment instead, with cargo's and rustup's own settings, so that
# what the terminal built is not built again. (`${keep[@]+…}`: bash 3.2's way to an array that
# may be empty.)
if [ -n "${SDKROOT:-}" ]; then
  keep=()
  while IFS= read -r setting; do
    case "$setting" in CARGO_*=* | RUSTUP_*=* | RUSTFLAGS=*) keep+=("$setting") ;; esac
  done < <(env)
  exec env -i HOME="$HOME" \
    PATH="${CARGO_HOME:-$HOME/.cargo}/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    ${keep[@]+"${keep[@]}"} "$0" "$@"
fi
cd "$root"
rustup target list --installed | grep -qx "$target" || rustup target add "$target"
libs=$(cargo rustc -q --color never -p scaena-ffi --lib --crate-type staticlib --target "$target" $release --locked -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
if [ -z "$libs" ]; then
  echo "rustc named no system libraries for $target: did the build fail?" >&2
  exit 1
fi
out="$root/target/ipad/$platform-$configuration"
mkdir -p "$out"
ln -sf "$root/target/$target/$profile/libscaena_ffi.a" "$out/libscaena_ffi.a"
# Written only when it changes: Xcode reads it as a build starts.
config=$(cat <<XCCONFIG
// Written by apps/ipad/engine.sh: the engine Xcode links, and what it links.
#include? "Team.xcconfig"
LIBRARY_SEARCH_PATHS = \$(inherited) $root/target/ipad/\$(PLATFORM_NAME)-\$(CONFIGURATION)
OTHER_LDFLAGS = \$(inherited) $libs
XCCONFIG
)
if [ ! -f "$here/Engine.xcconfig" ] || [ "$(cat "$here/Engine.xcconfig")" != "$config" ]; then
  printf '%s\n' "$config" > "$here/Engine.xcconfig"
fi
echo "$out/libscaena_ffi.a"

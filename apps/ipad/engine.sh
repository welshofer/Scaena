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
# Cargo's own folder comes first on PATH, before the re-exec: Xcode's phases get Xcode's PATH, not
# the shell's, and a runner may keep cargo only where rustup put it.
cargo_bin="${CARGO_HOME:-$HOME/.cargo}/bin"
if [ -n "${SDKROOT:-}" ]; then
  keep=()
  while IFS= read -r setting; do
    case "$setting" in CARGO_*=* | RUSTUP_*=* | RUSTFLAGS=*) keep+=("$setting") ;; esac
  done < <(env)
  exec env -i HOME="$HOME" \
    PATH="$cargo_bin:$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin" \
    ${keep[@]+"${keep[@]}"} "$0" "$@"
fi
cd "$root"
# What cargo and rustup said, kept: a build that fails says why, in Xcode's log and the CI scripts'.
log="$root/target/ipad/engine.log"
mkdir -p "$(dirname "$log")"
: >"$log"
failed() {
  {
    echo "apps/ipad/engine.sh: $1. The end of $log:"
    tail -n 40 "$log"
    echo "cargo: $(command -v cargo || echo none); rustup: $(command -v rustup || echo none)"
    echo "PATH=$PATH"
    env | grep -E '^(CARGO_|RUSTUP_|RUSTFLAGS=|HOME=)' | sort
  } >&2
  exit 1
}
# Cargo where PATH has none: where rustup says it is, or rustup's own default folder.
if ! command -v cargo >/dev/null 2>&1; then
  found=$(rustup which cargo 2>>"$log" || true)
  [ -z "$found" ] && [ -x "$HOME/.cargo/bin/cargo" ] && found="$HOME/.cargo/bin/cargo"
  [ -n "$found" ] || failed "no cargo on PATH, and rustup names none"
  PATH="$(dirname "$found"):$PATH"
fi
if ! rustup target list --installed 2>>"$log" | grep -qx "$target"; then
  rustup target add "$target" >>"$log" 2>&1 || failed "rustup could not add $target"
fi
cargo rustc -q --color never -p scaena-ffi --lib --crate-type staticlib --target "$target" $release --locked \
  -- --print native-static-libs >>"$log" 2>&1 || failed "cargo could not build the engine for $target"
libs=$(sed -n 's/.*native-static-libs: //p' "$log")
[ -n "$libs" ] || failed "rustc named no system libraries for $target"
# Each theme that ships drawn as its title slide (PLAN T.0c), which the app's theme chooser shows
# rather than making and drawing a deck in each as New opens (lift-up PERF-4); the app's build
# copies them in (project.yml's Themes phase).
cargo run -q --color never --locked -p scaena-cli --example theme_thumbnails -- "$root/target/ipad/themes" >>"$log" 2>&1 \
  || failed "cargo could not draw the themes' title slides"
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

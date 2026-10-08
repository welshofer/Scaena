#!/usr/bin/env bash
# Scaena.app (PLAN 3.3): the engine's static library in release, the Swift app over it, and the
# bundle macOS opens, signed for this machine alone. `just mac` builds and opens it.
#
#   apps/mac/build-app.sh [OUT]   (OUT defaults to target/mac; prints the app's path)
set -euo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd)
out=${1:-$root/target/mac}
cd "$root"
# The library, and the system libraries and frameworks rustc says it needs.
libs=$(cargo rustc -q -p scaena-ffi --lib --crate-type staticlib --release --locked -- --print native-static-libs 2>&1 | sed -n 's/.*native-static-libs: //p')
flags=(-Xlinker -L"$root/target/release")
for l in $libs; do flags+=(-Xlinker "$l"); done
cd apps/mac/ScaenaKit
swift build -c release --product Scaena "${flags[@]}"
bin=$(swift build -c release --show-bin-path)
app="$out/Scaena.app"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$bin/Scaena" "$app/Contents/MacOS/Scaena"
cp "$root/apps/mac/Scaena/Info.plist" "$app/Contents/Info.plist"
codesign --force --sign - "$app"
echo "$app"

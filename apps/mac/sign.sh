#!/usr/bin/env bash
# Scaena.app signed as it ships (the 20261024 plan's SEC-4): with the hardened runtime and the
# entitlements in apps/mac/Scaena/Scaena.entitlements, by the Developer ID $SCAENA_SIGNING_IDENTITY
# names, timestamped so it can be notarized; ad hoc, for this machine alone, where none is set. Then
# held to that: the runtime on, the app sandboxed, and each entitlement there.
#
#   apps/mac/sign.sh APP
set -euo pipefail
app=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
entitlements="$root/apps/mac/Scaena/Scaena.entitlements"
identity=${SCAENA_SIGNING_IDENTITY:--}
timestamp=()
[ "$identity" != "-" ] && timestamp=(--timestamp)
codesign --force --options runtime --entitlements "$entitlements" ${timestamp[@]+"${timestamp[@]}"} --sign "$identity" "$app"
codesign --verify --strict "$app"
said=$(codesign --display --verbose=2 "$app" 2>&1)
granted=$(codesign --display --entitlements - --xml "$app" 2>/dev/null)
fail() {
  echo "$app is not signed as it ships: $1" >&2
  exit 1
}
grep -q "flags=.*(runtime)" <<<"$said" || fail "no hardened runtime ($(grep flags= <<<"$said"))"
for key in app-sandbox files.user-selected.read-write network.client network.server; do
  grep -q "com.apple.security.$key" <<<"$granted" || fail "no com.apple.security.$key"
done

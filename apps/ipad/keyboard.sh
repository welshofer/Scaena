#!/usr/bin/env bash
# The simulator's keyboard for a pass of the iPad's UI tests (lift-up 20261110): `hardware`
# connects the simulator's hardware keyboard, so the software keyboard stays down while a text is
# typed in; `software` disconnects it, so the software keyboard comes up as on an iPad with none.
# The setting is the Simulator's own, Connect Hardware Keyboard, for this device and for every
# device, and a booted device reads it only as it boots: the device is shut down, the setting
# written, and the device booted again. The rest of the Simulator's settings are kept.
#
#   apps/ipad/keyboard.sh UDID hardware|software
set -euo pipefail
udid=$1
case "${2:-}" in
  hardware) connected=True ;;
  software) connected=False ;;
  *) echo "usage: $0 UDID hardware|software" >&2; exit 2 ;;
esac
xcrun simctl shutdown "$udid" 2>/dev/null || true
# Through cfprefsd, so that a copy it holds is not written back over the file.
{ defaults export com.apple.iphonesimulator - 2>/dev/null || true; } | python3 -c '
import plistlib, sys
raw = sys.stdin.buffer.read()
prefs = plistlib.loads(raw) if raw.strip() else {}
udid, connected = sys.argv[1], sys.argv[2] == "True"
prefs["ConnectHardwareKeyboard"] = connected
prefs.setdefault("DevicePreferences", {}).setdefault(udid, {})["ConnectHardwareKeyboard"] = connected
sys.stdout.buffer.write(plistlib.dumps(prefs, fmt=plistlib.FMT_XML))
' "$udid" "$connected" | defaults import com.apple.iphonesimulator -
xcrun simctl boot "$udid" 2>/dev/null || true
xcrun simctl bootstatus "$udid" -b >/dev/null
echo "the simulator's keyboard: $2 (ConnectHardwareKeyboard $connected)"

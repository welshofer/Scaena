#!/usr/bin/env bash
# Install Debian packages on a CI runner: `apt-get update`, then `install`, each held to a time
# limit. Where a mirror fails or is too slow, try the next one. A mirror that stalled
# mid-transfer held #182's `apt-get update` for 55 minutes, until the job's own limit cancelled
# it before any test ran. On 2026-10-23 one served #496's ffmpeg and #488's
# mesa-vulkan-drivers at 23 kB/s, an hour each.
#
# apt has no floor on speed, only a limit on silence. So each mirror gets a few minutes for what
# a healthy one does in seconds:
# - Acquire::http::Timeout gives up on a connection that says nothing.
# - `timeout` gives up on one that trickles.
# After that the archive's URIs point at the next mirror: the runner's own first, then
# Ubuntu's, then kernel.org's, two rounds through them. What was fetched stays in apt's cache.
#
# SCAENA_APT_SOURCES, SCAENA_APT_SUDO, and SCAENA_APT_LIMITS ("update install pause", in
# seconds) are for scripts/test_apt_install.py.
set -uo pipefail
sources=${SCAENA_APT_SOURCES:-/etc/apt/sources.list.d/ubuntu.sources}
[ -f "$sources" ] || sources=/etc/apt/sources.list
sudo=${SCAENA_APT_SUDO-sudo}
read -r update_limit install_limit pause <<<"${SCAENA_APT_LIMITS:-120 300 5}"
apt=(-o Acquire::Retries=1 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)

# The archive's mirror as the runner has it (not the security pocket's), then the fallbacks.
own=$(grep -oE '(https?|mirror\+file)://[^ ]+' "$sources" | grep -v security | head -n1)
mirrors=()
for m in "$own" http://archive.ubuntu.com/ubuntu/ http://mirrors.edge.kernel.org/ubuntu/; do
  [ -n "$m" ] || continue
  [[ " ${mirrors[*]} " == *" $m "* ]] || mirrors+=("$m")
done

current=$own
for round in 1 2; do
  for mirror in "${mirrors[@]}"; do
    if [ "$mirror" != "$current" ]; then
      echo "apt-get: trying $mirror in place of $current" >&2
      $sudo sed -i "s#$current#$mirror#g" "$sources" || exit 1
      current=$mirror
    fi
    if timeout "$update_limit" $sudo apt-get "${apt[@]}" -o APT::Update::Error-Mode=any update &&
      timeout "$install_limit" $sudo apt-get "${apt[@]}" install -y --no-install-recommends "$@"; then
      exit 0
    fi
    echo "apt-get: $mirror failed or was too slow (round $round of 2)" >&2
    sleep "$pause"
  done
done
exit 1

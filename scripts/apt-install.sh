#!/usr/bin/env bash
# Install Debian packages on a CI runner: `apt-get update`, then the packages, from the first
# mirror that serves them; a mirror that stops answering or falls under a floor of speed gives
# way to the next. A mirror that stalled mid-transfer held #182's `apt-get update` for 55
# minutes; on 2026-10-23 one served #496's ffmpeg and #488's mesa-vulkan-drivers at 23 kB/s, an
# hour each. A slow one that keeps going is let finish: a limit of time cut Azure's off at
# 56 kB/s on #497 and #510, 11 MB short, round after round.
#
# - Acquire::http::Timeout gives up on a connection that says nothing for 20 s.
# - The floor gives up on one that fetches less than 20 kB a second over two 30-second reads
#   of apt's lists and archives, which grow as apt fetches. Only the fetching is so watched:
#   the packages are downloaded first, then installed from the cache, so nothing stops dpkg.
# After that the archive's URIs point at the next mirror: the runner's own first, then
# Ubuntu's, then kernel.org's, two rounds through them. What was fetched stays in apt's cache.
#
# With SCAENA_APT_KEEP, a folder CI caches by the runner's image and the packages, no mirror is
# asked at all once a run has kept what it fetched there: apt's package lists and the packages
# themselves, put back and installed with no network. What does not install so is fetched as
# above, and kept again.
#
# SCAENA_APT_SOURCES, SCAENA_APT_SUDO, SCAENA_APT_CACHE ("lists archives"), and SCAENA_APT_LIMITS
# ("interval floor pause": seconds between reads, bytes a second, seconds between mirrors) are
# for scripts/test_apt_install.py.
set -uo pipefail
# The archive's mirror a sources file names, deb822's `URIs:` or a `deb` line, read from its
# lines that are no comment: an `http(s)://` address, or `mirror+file:` and a list of mirrors,
# as GitHub's runners name Azure's; not the security pocket's.
mirror_in() {
  grep -v '^[[:space:]]*#' "$1" 2>/dev/null | grep -oE '(https?://|mirror\+file:)[^ ]+' | grep -v security | head -n1
}
# The sources file that names it: the runner's deb822 file, else its classic list.
sources=${SCAENA_APT_SOURCES:-}
if [ -z "$sources" ]; then
  for f in /etc/apt/sources.list.d/ubuntu.sources /etc/apt/sources.list; do
    if [ -n "$(mirror_in "$f")" ]; then sources=$f && break; fi
  done
  sources=${sources:-/etc/apt/sources.list}
fi
sudo=${SCAENA_APT_SUDO-sudo}
read -r -a cache <<<"${SCAENA_APT_CACHE:-/var/lib/apt/lists /var/cache/apt/archives}"
read -r interval floor pause <<<"${SCAENA_APT_LIMITS:-30 20000 5}"
lists=${cache[0]}
archives=${cache[1]}
keep=${SCAENA_APT_KEEP:-}
apt=(-o Acquire::Retries=1 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20)

# The bytes apt has fetched so far, partial files among them.
fetched() { $sudo du -sb "${cache[@]}" 2>/dev/null | awk '{ s += $1 } END { print s + 0 }'; }

# Runs "$@", stopping it once two reads running find it fetched under the floor.
watched() {
  "$@" &
  local pid=$! before now slow=0 waited=0
  before=$(fetched)
  while kill -0 "$pid" 2>/dev/null; do
    sleep 1
    waited=$((waited + 1))
    ((waited % interval)) && continue
    now=$(fetched)
    if ((now - before < floor * interval)); then slow=$((slow + 1)); else slow=0; fi
    before=$now
    if ((slow >= 2)); then
      echo "apt-get: under $floor bytes a second for $((2 * interval)) s" >&2
      $sudo kill "$pid" 2>/dev/null || kill "$pid" 2>/dev/null
      wait "$pid" 2>/dev/null
      return 1
    fi
  done
  wait "$pid"
}

# Keeps the package lists and the packages fetched, for the next run on this image.
kept() {
  [ -n "$keep" ] || return 0
  rm -rf "$keep" && mkdir -p "$keep/lists" "$keep/archives" || return 0
  cp "$archives"/*.deb "$keep/archives/" 2>/dev/null
  find "$lists" -maxdepth 1 -type f ! -name lock -exec cp {} "$keep/lists/" \; 2>/dev/null
  echo "apt-get: kept $(du -sh "$keep" | cut -f1) for the next run in $keep" >&2
}

if [ -n "$keep" ] && compgen -G "$keep/archives/*.deb" >/dev/null; then
  $sudo dpkg --configure -a
  if $sudo cp "$keep"/lists/* "$lists/" && $sudo cp "$keep"/archives/*.deb "$archives/" &&
    $sudo apt-get "${apt[@]}" install -y --no-install-recommends --no-download "$@"; then
    echo "apt-get: installed from what an earlier run kept, with no network" >&2
    exit 0
  fi
  echo "apt-get: what was kept does not install; fetching" >&2
fi

# The archive's mirror as the runner has it (not the security pocket's), then the fallbacks.
own=$(mirror_in "$sources")
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
      $sudo sed -i "/^[[:space:]]*#/!s#$current#$mirror#g" "$sources" || exit 1
      current=$mirror
    fi
    # A dpkg an earlier run stopped mid-install refuses every install until it is configured.
    $sudo dpkg --configure -a
    if watched $sudo apt-get "${apt[@]}" -o APT::Update::Error-Mode=any update &&
      watched $sudo apt-get "${apt[@]}" install -y --no-install-recommends --download-only "$@" &&
      kept && $sudo apt-get "${apt[@]}" install -y --no-install-recommends "$@"; then
      exit 0
    fi
    echo "apt-get: $mirror failed or was too slow (round $round of 2)" >&2
    sleep "$pause"
  done
done
exit 1

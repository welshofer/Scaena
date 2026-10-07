#!/usr/bin/env bash
# Install Debian packages on a CI runner: `apt-get update`, then `install`, each held to a
# time limit and tried again. A mirror that stalled mid-transfer held #182's `apt-get update`
# for 55 minutes, until the job's own limit cancelled it before any test ran.
set -uo pipefail
for attempt in 1 2 3; do
  if timeout 300 sudo apt-get -o Acquire::Retries=3 update &&
    timeout 600 sudo apt-get -o Acquire::Retries=3 install -y --no-install-recommends "$@"; then
    exit 0
  fi
  echo "apt-get: try $attempt of 3 failed or stalled" >&2
  sleep 15
done
exit 1

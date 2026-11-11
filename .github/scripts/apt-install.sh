#!/bin/sh
# Installs Ubuntu packages on a GitHub runner, tolerating slow or broken mirrors.
# The runner's Azure mirror (plain HTTP) sometimes hangs for minutes, so the official
# archive over HTTPS is used from the start; each fetch has a short timeout and the
# whole thing is retried a few times.
set -u
missing=""
for p in "$@"; do
  dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing="$missing $p"
done
[ -z "$missing" ] && { echo "Already installed: $*"; exit 0; }

for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources /etc/apt/apt-mirrors.txt; do
  [ -f "$f" ] && sudo sed -i -E 's#https?://azure\.archive\.ubuntu\.com#https://archive.ubuntu.com#g' "$f"
done

apt="sudo timeout 300 apt-get -o Acquire::Retries=2 -o Acquire::http::Timeout=15 -o Acquire::https::Timeout=15"
for attempt in 1 2 3; do
  echo "::group::apt attempt $attempt:$missing"
  # Third-party sources on the runner image may fail to refresh; the Ubuntu archive is what matters.
  $apt update || true
  if $apt install -y --no-install-recommends $missing; then
    echo "::endgroup::"
    exit 0
  fi
  echo "::endgroup::"
  sleep $((attempt * 10))
done
echo "::error::Could not install:$missing"
exit 1

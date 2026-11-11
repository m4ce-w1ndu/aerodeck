#!/bin/sh
# Installs Ubuntu packages on a GitHub runner, tolerating slow or broken mirrors:
# short timeouts, retries, and a switch from the Azure mirror to archive.ubuntu.com.
set -u
missing=""
for p in "$@"; do
  dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing="$missing $p"
done
[ -z "$missing" ] && { echo "Already installed: $*"; exit 0; }

apt="sudo apt-get -o Acquire::Retries=3 -o Acquire::http::Timeout=20 -o Acquire::https::Timeout=20"
for attempt in 1 2 3; do
  echo "::group::apt attempt $attempt:$missing"
  # Third-party sources on the runner image may fail to refresh; the Ubuntu archive is what matters.
  $apt update || true
  if $apt install -y --no-install-recommends $missing; then
    echo "::endgroup::"
    exit 0
  fi
  echo "::endgroup::"
  echo "Install failed, switching to archive.ubuntu.com and retrying"
  for f in /etc/apt/sources.list /etc/apt/sources.list.d/*.list /etc/apt/sources.list.d/*.sources; do
    [ -f "$f" ] && sudo sed -i 's#http://azure.archive.ubuntu.com#http://archive.ubuntu.com#g' "$f"
  done
  sleep $((attempt * 15))
done
echo "::error::Could not install:$missing"
exit 1

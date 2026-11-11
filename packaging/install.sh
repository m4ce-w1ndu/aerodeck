#!/bin/sh
# Installs AeroDeck from the release tarball.
#   ./install.sh              -> ~/.local (no root needed)
#   sudo ./install.sh /usr/local
#   ./install.sh --uninstall [prefix]
set -eu
here=$(cd "$(dirname "$0")" && pwd)

uninstall=0
if [ "${1:-}" = "--uninstall" ]; then uninstall=1; shift; fi
prefix=${1:-"$HOME/.local"}

files="bin/aerodeck share/applications/aerodeck.desktop share/icons/hicolor/scalable/apps/aerodeck.svg"
for s in 16 32 48 64 128 256 512; do files="$files share/icons/hicolor/${s}x${s}/apps/aerodeck.png"; done

if [ "$uninstall" = 1 ]; then
  for f in $files; do rm -f "$prefix/$f"; done
  echo "AeroDeck removed from $prefix (your profiles in ~/.local/share/aerodeck are kept)."
  exit 0
fi

for f in $files; do
  mode=644; [ "$f" = bin/aerodeck ] && mode=755
  install -D -m "$mode" "$here/$f" "$prefix/$f"
done
# The desktop entry must start the installed binary.
sed -i "s|^Exec=.*|Exec=$prefix/bin/aerodeck|" "$prefix/share/applications/aerodeck.desktop"
command -v update-desktop-database >/dev/null && update-desktop-database "$prefix/share/applications" 2>/dev/null || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor" 2>/dev/null || true
command -v kbuildsycoca6 >/dev/null && kbuildsycoca6 >/dev/null 2>&1 || true
echo "AeroDeck installed in $prefix. Start it from the application menu or run: $prefix/bin/aerodeck"

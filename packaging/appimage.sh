#!/bin/sh
# Builds dist/AeroDeck-<version>-x86_64.AppImage from target/release/aerodeck.
# Needs linuxdeploy (https://github.com/linuxdeploy/linuxdeploy) as $LINUXDEPLOY or in PATH.
# OpenGL, Wayland, X11 and fontconfig come from the host system, as usual for AppImages.
set -eu
version=${1:?usage: appimage.sh <version>}
linuxdeploy=${LINUXDEPLOY:-linuxdeploy}
root=$(cd "$(dirname "$0")/.." && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

mkdir -p "$root/dist"
cd "$work"
export APPIMAGE_EXTRACT_AND_RUN=1 LINUXDEPLOY_OUTPUT_VERSION="$version"
"$linuxdeploy" --appdir AppDir \
  --executable "$root/target/release/aerodeck" \
  --desktop-file "$root/packaging/aerodeck.desktop" \
  --icon-file "$root/packaging/icons/hicolor/256x256/apps/aerodeck.png" \
  --icon-file "$root/packaging/icons/hicolor/scalable/apps/aerodeck.svg" \
  --output appimage
mv AeroDeck-*.AppImage "$root/dist/AeroDeck-$version-x86_64.AppImage"

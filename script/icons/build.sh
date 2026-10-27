#!/bin/sh
# Rebuilds Burrdisk's app icons: SVG masters, then every size each platform asks
# for, then the packed .icns and .ico. Each size range has its own master, so
# small icons are drawn for their size rather than shrunk.
#
# Needs Python 3 and the resvg CLI (`cargo install resvg`).
#
#   script/icons/build.sh
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
PYTHON=$(command -v python3 || command -v python)
"$PYTHON" script/icons/make_icons.py

TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT
render() { resvg -w "$3" -h "$3" "assets/icon/$1.svg" "$TMP/$2-$3.png"; }

for size in 16; do render macos-small mac $size; done
for size in 32 64; do render macos-medium mac $size; done
for size in 128 256 512 1024; do render macos-large mac $size; done
for size in 16 20 24; do render windows-small win $size; done
for size in 32 40 48 64; do render windows-medium win $size; done
for size in 128 256; do render windows-large win $size; done

"$PYTHON" script/icons/pack.py "$TMP"
cp "$TMP/mac-256.png" assets/icon/macos-256.png

#!/bin/sh
# Rebuilds the promotional covers and the README's window shots (at 2x): SVG masters, then 1920x1080 PNGs set in the
# bundled Mona Sans, plus two cuts in assets/promo/fonts: Black for the name and
# a static width-80 instance of the variable font ("Mona Sans W80") for the tagline.
#
# Needs Python 3 and the resvg CLI (`cargo install resvg`).
#
#   script/promo/build.sh
set -eu

ROOT=$(cd "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
PYTHON=$(command -v python3 || command -v python)
"$PYTHON" script/promo/make_cover.py
"$PYTHON" script/promo/make_cover_alt.py
"$PYTHON" script/promo/make_screenshots.py
for name in cover cover-alt cover-alt-plain; do
  resvg --skip-system-fonts --use-fonts-dir assets/fonts \
    --use-fonts-dir assets/promo/fonts -w 1920 -h 1080 \
    "assets/promo/$name.svg" "assets/promo/$name.png"
  echo "wrote assets/promo/$name.png"
done
for name in home explorer collector; do
  resvg --skip-system-fonts --use-fonts-dir assets/fonts --zoom 2 \
    "assets/screenshots/$name.svg" "assets/screenshots/$name.png"
  echo "wrote assets/screenshots/$name.png"
done

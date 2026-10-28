"""Draws the README's window shots: Home, the Explorer with an empty
Collector, and the Explorer with a full shelf. They are reconstructions built
from the same window drawings as the alternate cover, set upright on a
transparent ground with room for the window's shadow.

    python script/promo/make_screenshots.py    writes assets/screenshots/*.svg
    script/promo/build.sh                       also renders their PNGs
"""

import sys

sys.dont_write_bytecode = True
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_cover import ICONS, SHARES, icon, lighten, lobe_colors  # noqa: E402
from make_cover_alt import (  # noqa: E402
    EMBER_INK, FAINT, MUTED, TEXT, defs, explorer_window, home_window, text,
)

OUT = Path(__file__).resolve().parents[2] / "assets" / "screenshots"
# Room around the window: its shadow falls 30px down and spreads about as far
SIDE, TOP, BOTTOM = 56, 40, 100
# Sunburst hole and ring width that keep all three rings inside the window
CHART = (74, 64)

ICONS.update({
    "inbox": '<polyline points="22 12 16 12 14 15 10 15 8 12 2 12"/>'
             '<path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/>',
    "trash": '<path d="M3 6h18"/><path d="M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6"/><path d="M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2"/>'
             '<line x1="10" x2="10" y1="11" y2="17"/><line x1="14" x2="14" y1="11" y2="17"/>',
})


def empty_tray(w, ty):
    """The Collector before anything is in it: a dashed well and two lines."""
    return (
        f'<rect x="21" y="{ty + 17}" width="46" height="46" rx="13" fill="none" stroke="#ffffff" '
        f'stroke-opacity="0.18" stroke-width="2" stroke-dasharray="5 4"/>'
        + icon("inbox", 35, ty + 31, 18, FAINT)
        + text(84, ty + 37, "Drop files and folders here", 14, MUTED)
        + text(84, ty + 56, "Collect what you want gone, then move it all to the Trash at once.", 12, FAINT)
    )


SHELF = [
    ("folder", "DerivedData", "18.4 GB", 0),
    ("app", "Xcode.app", "11.2 GB", 1),
    ("archive", "Backup 2023.zip", "9.6 GB", 3),
    ("video", "Screen Recording.mov", "3.1 GB", 2),
    ("folder", "node_modules", "2.4 GB", 4),
]


def tile(x, y, glyph, name, size, color):
    """A shelf tile: icon and size over the name, on a Raised-to-Surface face."""
    w = 20 + max(len(name) * 6.0, 15 + 6 + len(size) * 6.0 + 16, 56)
    return w, (
        f'<rect x="{x}" y="{y}" width="{w:.1f}" height="58" rx="10" fill="url(#tile)"/>'
        + icon(glyph, x + 10, y + 12, 15, color)
        + text(x + 31, y + 24, size, 11, MUTED, 600)
        + text(x + 10, y + 45, name, 11.5, TEXT)
    )


def full_tray(w, ty):
    """The Collector holding five items: count, total, the shelf, Clear and
    Move to Trash."""
    colors = lobe_colors(SHARES)
    out = (
        '<linearGradient id="tile" x1="0" y1="0" x2="0" y2="1">'
        '<stop offset="0" stop-color="#2a2a2f"/><stop offset="1" stop-color="#1c1c1f"/></linearGradient>'
        f'<rect x="20" y="{ty + 16}" width="48" height="48" rx="14" fill="url(#ember)" filter="url(#btnglow)"/>'
        + text(44, ty + 47, str(len(SHELF)), 17, EMBER_INK, 700, "middle")
        + text(84, ty + 48, f"44.7<tspan font-size='13' fill='{MUTED}' font-weight='500'> GB</tspan>", 22, TEXT, 700)
    )
    x = 176
    for glyph, name, size, lobe in SHELF:
        tw, markup = tile(x, ty + 11, glyph, name, size, lighten(colors[lobe], 0.2))
        out += markup
        x += tw + 8
    trash_w = 11 + 14 + 6 + 13 * 7.7 + 14
    trash_x = w - 20 - trash_w
    out += (
        text(trash_x - 16 - 33, ty + 45, "Clear", 14, MUTED, 600, "middle")
        + f'<rect x="{trash_x:.1f}" y="{ty + 24}" width="{trash_w:.1f}" height="32" rx="9" fill="url(#red)" filter="url(#btnglow)"/>'
        + icon("trash", trash_x + 11, ty + 33, 14, "#ffffff")
        + text(trash_x + 31, ty + 45, "Move to Trash", 14, "#ffffff", 600)
    )
    return out


def shot(window):
    w, h, content = window
    cw, ch = w + 2 * SIDE, h + TOP + BOTTOM
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{cw}" height="{ch}" viewBox="0 0 {cw} {ch}">
<defs>{defs()}</defs>
<g transform="translate({SIDE},{TOP})">{content}</g>
</svg>
'''


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    for name, window in [
        ("home", home_window(h=680, oy=10)),
        ("explorer", explorer_window(empty_tray, CHART)),
        ("collector", explorer_window(full_tray, CHART)),
    ]:
        (OUT / f"{name}.svg").write_text(shot(window), encoding="utf-8")
    print(f"wrote {OUT}/home.svg, explorer.svg and collector.svg")

"""Draws Burrdisk's 16:9 promotional cover as an SVG master.

The same scanned burr the home screen and the app icon draw, at poster scale
and running off the frame: its lobes cast their own colors onto the graphite
ground and its spines bloom. Collector chips of things a scan turns up float
at different depths, beside the name, one line and the platform marks.

    python script/promo/make_cover.py        writes assets/promo/cover.svg
    script/promo/build.sh                    also renders cover.png (1920x1080)
"""

import math
import sys
sys.dont_write_bytecode = True
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "script" / "icons"))
from make_icons import GRAIN, burr, lighten, lobe_colors  # noqa: E402

OUT = ROOT / "assets" / "promo"
W, H = 1920, 1080

# Top-level folders of a typical startup disk, largest first: Users, System,
# Applications, Library, private, opt, the rest
SHARES = [0.38, 0.19, 0.14, 0.11, 0.08, 0.06, 0.04]

BX, BY, REACH = 1450, 560, 600

# Things a scan turns up, floating at different depths as Collector chips:
# (x, y, scale, tilt, blur, opacity, icon, name, size, lobe)
CHIPS = [
    (300, 168, 1.75, -5, 0.0, 0.92, "folder", "DerivedData", "18.4 GB", 0),
    (520, 900, 1.45, 4, 1.2, 0.72, "archive", "Backup 2023.zip", "9.6 GB", 3),
    (1010, 70, 1.2, -3, 2.4, 0.6, "video", "Screen Recording.mov", "3.1 GB", 2),
    (640, 300, 1.1, 6, 2.2, 0.5, "folder", "node_modules", "2.4 GB", 4),
    (112, 820, 1.3, -3, 0.8, 0.78, "app", "Xcode.app", "11.2 GB", 1),
    (560, 46, 1.05, 4, 2.6, 0.45, "image", "Photos Library", "7.8 GB", 5),
]

# Stroke icons from assets/icons (24 unit grid)
ICONS = {
    "folder": '<path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z"/>',
    "video": '<path d="m16 13 5.223 3.482a.5.5 0 0 0 .777-.416V7.87a.5.5 0 0 0-.752-.432L16 10.5"/><rect x="2" y="6" width="14" height="12" rx="2"/>',
    "archive": '<rect width="20" height="5" x="2" y="3" rx="1"/><path d="M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8"/><path d="M10 12h4"/>',
    "app": '<rect x="2" y="4" width="20" height="16" rx="2"/><path d="M10 4v4"/><path d="M2 8h20"/><path d="M6 4v4"/>',
    "x": '<path d="M18 6 6 18"/><path d="m6 6 12 12"/>',
    "file": '<path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z"/><path d="M14 2v4a2 2 0 0 0 2 2h4"/><path d="M10 9H8"/><path d="M16 13H8"/><path d="M16 17H8"/>',
    "image": '<rect width="18" height="18" x="3" y="3" rx="2" ry="2"/><circle cx="9" cy="9" r="2"/><path d="m21 15-3.086-3.086a2 2 0 0 0-2.828 0L6 21"/>',
    "music": '<path d="M9 18V5l12-2v13"/><circle cx="6" cy="18" r="3"/><circle cx="18" cy="16" r="3"/>',
    "disc": '<circle cx="12" cy="12" r="10"/><circle cx="12" cy="12" r="2"/>',
    "drive": '<line x1="22" x2="2" y1="12" y2="12"/><path d="M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z"/><line x1="6" x2="6.01" y1="16" y2="16"/><line x1="10" x2="10.01" y1="16" y2="16"/>',
}

# Platform marks, filled on a 24 unit grid. Apple from Simple Icons (CC0)
APPLE = (
    "M12.152 6.896c-.948 0-2.415-1.078-3.96-1.04-2.04.027-3.91 1.183-4.961 3.014-2.117 3.675-.546 9.103 "
    "1.519 12.09 1.013 1.454 2.208 3.09 3.792 3.039 1.52-.065 2.09-.987 3.935-.987 1.831 0 2.35.987 3.96.948 "
    "1.637-.026 2.676-1.48 3.676-2.948 1.156-1.688 1.636-3.325 1.662-3.415-.039-.013-3.182-1.221-3.22-4.857"
    "-.026-3.04 2.48-4.494 2.597-4.559-1.429-2.09-3.623-2.324-4.39-2.376-2-.156-3.675 1.09-4.61 1.09zM15.53 "
    "3.83c.843-1.012 1.4-2.427 1.245-3.83-1.207.052-2.662.805-3.532 1.818-.78.896-1.454 2.338-1.273 3.714 "
    "1.338.104 2.715-.688 3.559-1.701"
)
WINDOWS = "M1 1h10.6v10.6H1zM12.4 1H23v10.6H12.4zM1 12.4h10.6V23H1zM12.4 12.4H23V23H12.4z"


def icon(name, x, y, size, color, width=2.0):
    k = size / 24
    return (
        f'<g transform="translate({x:.2f},{y:.2f}) scale({k:.4f})" fill="none" stroke="{color}" '
        f'stroke-width="{width}" stroke-linecap="round" stroke-linejoin="round">{ICONS[name]}</g>'
    )


def chip(index, x, y, scale, tilt, blur, opacity, glyph, name, size, lobe):
    """The Collector chip, drawn at its 30px design size and scaled: a
    Raised-to-Surface pill with a border in the item's color, its icon in
    that color, the name, the size in faint, and the X."""
    color = lobe_colors(SHARES)[lobe]
    name_w = len(name) * 6.15 + 2
    size_w = len(size) * 6.1
    w = 10 + 13 + 7 + name_w + 6 + size_w + 6 + 22 + 4
    tx = 10 + 13 + 7
    blur_filter = f' filter="url(#cb{index})"' if blur else ""
    return (
        f'<linearGradient id="cg{index}" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="#2a2a2f"/><stop offset="1" stop-color="#1c1c1f"/></linearGradient>'
        f'<filter id="cb{index}" x="-20%" y="-60%" width="140%" height="220%"><feGaussianBlur stdDeviation="{blur / scale:.2f}"/></filter>'
        f'<g transform="translate({x},{y}) rotate({tilt}) scale({scale})" opacity="{opacity}">'
        f'<g{blur_filter}>'
        f'<rect x="0" y="0" width="{w:.1f}" height="30" rx="9" fill="url(#cg{index})" filter="url(#chipglow{lobe})"/>'
        f'<rect x="0.5" y="0.5" width="{w - 1:.1f}" height="29" rx="8.5" fill="none" stroke="{color}" stroke-opacity="0.35"/>'
        f'{icon(glyph, 10, 8.5, 13, lighten(color, 0.25), 2.0)}'
        f'<text x="{tx}" y="19.2" font-family="Mona Sans" font-size="12" fill="#f2f2f3">{name}'
        f'<tspan x="{tx + name_w + 6:.1f}" fill="#6b6b73">{size}</tspan></text>'
        f'{icon("x", w - 4 - 22 + 5.5, 9.5, 11, "#a1a1a8", 2.0)}'
        f'</g></g>'
    )


def chip_glows():
    """The Glow, tinted per lobe color, so each chip floats in its own light."""
    return "\n".join(
        f'<filter id="chipglow{i}" x="-40%" y="-120%" width="180%" height="340%">'
        f'<feFlood flood-color="{c}" flood-opacity="0.30"/><feComposite in2="SourceAlpha" operator="in"/>'
        f'<feGaussianBlur stdDeviation="9"/><feOffset dy="6" result="halo"/>'
        f'<feFlood flood-color="#000" flood-opacity="0.45"/><feComposite in2="SourceAlpha" operator="in"/>'
        f'<feGaussianBlur stdDeviation="1"/><feOffset dy="1" result="contact"/>'
        f'<feMerge><feMergeNode in="halo"/><feMergeNode in="contact"/><feMergeNode in="SourceGraphic"/></feMerge></filter>'
        for i, c in enumerate(lobe_colors(SHARES))
    )

def light_pool(reach):
    """Each folder lobe casts its own color onto the ground: one sector per
    lobe, blurred wide (The Halo Not Drop Rule, at poster scale)."""
    out, at = [], 0.0
    for share, color in zip(SHARES, lobe_colors(SHARES)):
        a0, a1 = at * math.tau, (at + share) * math.tau
        pt = lambda a: f"{BX + reach * math.sin(a):.1f},{BY - reach * math.cos(a):.1f}"
        large = 1 if share > 0.5 else 0
        out.append(f'<path d="M{BX},{BY} L{pt(a0)} A{reach},{reach} 0 {large} 1 {pt(a1)} Z" fill="{color}"/>')
        at += share
    return "\n".join(out)


def wallpaper(tile=440, step=110, size=34):
    """A repeating tile of the app's file-kind icons on a staggered grid, the
    things a disk fills up with. Drawn soft and dim so it reads as texture."""
    kinds = ["folder", "file", "image", "archive", "music", "video", "disc", "drive"]
    out, k = [], 0
    for row in range(tile // step):
        for col in range(tile // step):
            if (row + col) % 2:
                continue
            x, y = col * step + (step - size) / 2, row * step + (step - size) / 2
            out.append(icon(kinds[(k * 3) % len(kinds)], x, y, size, "#ffffff", 1.6))
            k += 1
    return (
        f'<pattern id="wall" width="{tile}" height="{tile}" patternUnits="userSpaceOnUse" '
        f'patternTransform="rotate(-14)">{"".join(out)}</pattern>'
    )


def cover():
    mark = burr(BX, BY, REACH, 0.34, 156, 4.8, 1.4, 4.8, 5.0, shares=SHARES, floor=0.58, jitter=0.07)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" width="{W}" height="{H}" viewBox="0 0 {W} {H}">
<defs>
<radialGradient id="glow" cx="{BX}" cy="{BY}" r="{REACH * 1.2}" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="#f97316" stop-opacity="0.22"/>
<stop offset="0.5" stop-color="#f97316" stop-opacity="0.06"/>
<stop offset="1" stop-color="#f97316" stop-opacity="0"/></radialGradient>
<radialGradient id="fieldfade" cx="{BX}" cy="{BY}" r="{W * 0.9}" gradientUnits="userSpaceOnUse">
<stop offset="0.25" stop-color="#ffffff" stop-opacity="0"/>
<stop offset="0.75" stop-color="#ffffff" stop-opacity="1"/></radialGradient>
{wallpaper()}
<filter id="soft" x="0" y="0" width="100%" height="100%"><feGaussianBlur stdDeviation="1.6"/></filter>
<mask id="field" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="{H}"><rect width="{W}" height="{H}" fill="url(#fieldfade)"/></mask>
<radialGradient id="vignette" cx="{W * 0.55}" cy="{H * 0.5}" r="{W * 0.75}" gradientUnits="userSpaceOnUse">
<stop offset="0.55" stop-color="#000000" stop-opacity="0"/>
<stop offset="1" stop-color="#000000" stop-opacity="0.55"/></radialGradient>
<filter id="wash" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="110"/></filter>
<filter id="haze" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="26"/></filter>
<filter id="bloom" x="-30%" y="-30%" width="160%" height="160%"><feGaussianBlur stdDeviation="9"/></filter>
<filter id="lift" x="-30%" y="-30%" width="160%" height="160%">
<feGaussianBlur in="SourceAlpha" stdDeviation="14"/><feOffset dy="12"/>
<feComponentTransfer><feFuncA type="linear" slope="0.55"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
{chip_glows()}
{GRAIN}
<g id="lead">
{mark}
</g>
</defs>
<rect width="{W}" height="{H}" fill="#141416"/>
<g mask="url(#field)"><rect width="{W}" height="{H}" fill="url(#wall)" filter="url(#soft)" opacity="0.075"/></g>
<g filter="url(#wash)" opacity="0.42">
{light_pool(REACH * 0.95)}
</g>
<circle cx="{BX}" cy="{BY}" r="{REACH * 1.2}" fill="url(#glow)"/>
{"".join(chip(i, *c) for i, c in enumerate(CHIPS) if c[4] >= 2)}
<use xlink:href="#lead" filter="url(#haze)" opacity="0.7"/>
<use xlink:href="#lead" filter="url(#bloom)" opacity="0.9"/>
<use xlink:href="#lead" filter="url(#lift)"/>
<circle cx="{BX}" cy="{BY}" r="{REACH * 0.34 - 6}" fill="none" stroke="#f97316" stroke-width="10" filter="url(#bloom)" opacity="0.55"/>
<text x="{BX}" y="{BY + 34}" text-anchor="middle" font-family="Mona Sans" font-weight="700" font-size="96" fill="#f2f2f3">68<tspan font-size="54" font-weight="500" fill="#a1a1a8">%</tspan></text>
{"".join(chip(i, *c) for i, c in enumerate(CHIPS) if c[4] < 2)}
<rect width="{W}" height="{H}" fill="url(#vignette)"/>
<rect width="{W}" height="{H}" filter="url(#grain)" opacity="0.04"/>
<g font-family="Mona Sans">
<text x="116" y="520" font-weight="900" font-size="156" letter-spacing="-4" fill="#f2f2f3">Burrdisk</text>
<text x="122" y="606" font-family="Mona Sans W80" font-weight="400" font-size="52" fill="#a1a1a8">Dig right in. Free up your disk.</text>
<g fill="#a1a1a8">
<path transform="translate(124,668) scale(1.5)" d="{APPLE}"/>
<path transform="translate(188,673) scale(1.3)" d="{WINDOWS}"/>
</g>
</g>
</svg>
'''


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "cover.svg").write_text(cover(), encoding="utf-8")
    print(f"wrote {OUT / 'cover.svg'}")

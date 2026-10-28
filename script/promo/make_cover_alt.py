"""Draws the alternate 16:9 cover: the brand centered, the app on the sides.

The all-Ember wordmark burr, the name, one line and the platform marks hold
the middle. Two app windows lean in from the corners, cropped by the frame and
drawn from the app's own components: Home on the left (the lead disk's burr,
its capacity sentence, Open, the other drives) and the Explorer on the right
(the sunburst, sidebar rows, the Collector with Move to Trash). Each window
casts light in the colors it holds.

    python script/promo/make_cover_alt.py    writes assets/promo/cover-alt.svg and
                                             cover-alt-plain.svg (no tagline)
    script/promo/build.sh                    also renders their PNGs
"""

import math
import sys

sys.dont_write_bytecode = True
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from make_cover import (  # noqa: E402
    APPLE, GRAIN, ICONS, OUT, SHARES, W, WINDOWS, burr, chip, chip_glows, icon, lighten, lobe_colors, wallpaper,
)
from make_icons import oklch  # noqa: E402

CX = W / 2

# Button glyphs from assets/icons, as the app's buttons use them.
ICONS.update({
    "open": '<path d="M7 7h10v10"/><path d="M7 17 17 7"/>',
    "scan": '<path d="M3 7V5a2 2 0 0 1 2-2h2"/><path d="M17 3h2a2 2 0 0 1 2 2v2"/><path d="M21 17v2a2 2 0 0 1-2 2h-2"/>'
            '<path d="M7 21H5a2 2 0 0 1-2-2v-2"/><circle cx="12" cy="12" r="3"/><path d="m16 16-1.9-1.9"/>',
    "folder-search": '<path d="M10.7 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v4.1"/>'
                     '<circle cx="17" cy="17" r="3"/><path d="m21 21-1.9-1.9"/>',
    "chevron": '<path d="m6 9 6 6 6-6"/>',
})
TILT = 30

EMBER, EMBER_HI, EMBER_LO, EMBER_INK = "#f97316", "#fdba74", "#ea580c", "#1f0b00"
GROUND, SURFACE, RAISED = "#141416", "#1c1c1f", "#2a2a2f"
TEXT, MUTED, FAINT = "#f2f2f3", "#a1a1a8", "#6b6b73"


def defs():
    return f'''
<linearGradient id="ember" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{EMBER_HI}"/><stop offset="1" stop-color="{EMBER_LO}"/></linearGradient>
<linearGradient id="red" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#f87171"/><stop offset="1" stop-color="#dc2626"/></linearGradient>
<linearGradient id="stone" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#34302d"/><stop offset="1" stop-color="#2a2725"/></linearGradient>
<linearGradient id="tray" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{SURFACE}"/><stop offset="1" stop-color="{GROUND}"/></linearGradient>
<linearGradient id="sheen" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ffffff" stop-opacity="0.035"/><stop offset="0.5" stop-color="#ffffff" stop-opacity="0"/></linearGradient>
<radialGradient id="markcore" cx="0.5" cy="0.3" r="0.8"><stop offset="0" stop-color="{EMBER_HI}"/><stop offset="1" stop-color="{EMBER_LO}"/></radialGradient>
<radialGradient id="centerglow" cx="{CX}" cy="400" r="520" gradientUnits="userSpaceOnUse">
<stop offset="0" stop-color="{EMBER}" stop-opacity="0.20"/><stop offset="0.5" stop-color="{EMBER}" stop-opacity="0.05"/><stop offset="1" stop-color="{EMBER}" stop-opacity="0"/></radialGradient>
<radialGradient id="wallfade" cx="{CX}" cy="540" r="1000" gradientUnits="userSpaceOnUse">
<stop offset="0.2" stop-color="#fff" stop-opacity="0"/><stop offset="0.7" stop-color="#fff" stop-opacity="1"/></radialGradient>
<mask id="wallmask" maskUnits="userSpaceOnUse" x="0" y="0" width="{W}" height="1080"><rect width="{W}" height="1080" fill="url(#wallfade)"/></mask>
<radialGradient id="vignette" cx="{CX}" cy="540" r="1150" gradientUnits="userSpaceOnUse">
<stop offset="0.55" stop-color="#000" stop-opacity="0"/><stop offset="1" stop-color="#000" stop-opacity="0.5"/></radialGradient>
<filter id="soft" x="0" y="0" width="100%" height="100%"><feGaussianBlur stdDeviation="1.6"/></filter>
<filter id="bloom" x="-50%" y="-50%" width="200%" height="200%"><feGaussianBlur stdDeviation="10"/></filter>
<filter id="pool" x="-60%" y="-60%" width="220%" height="220%"><feGaussianBlur stdDeviation="90"/></filter>
<filter id="btnglow" x="-30%" y="-80%" width="160%" height="260%">
<feGaussianBlur in="SourceGraphic" stdDeviation="7"/><feOffset dy="5"/><feComponentTransfer><feFuncA type="linear" slope="0.4"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
<filter id="window" x="-20%" y="-20%" width="140%" height="140%">
<feGaussianBlur in="SourceAlpha" stdDeviation="30"/><feOffset dy="30"/><feComponentTransfer><feFuncA type="linear" slope="0.6"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
{chip_glows()}
{GRAIN}
{wallpaper()}
'''


def text(x, y, s, size, color=TEXT, weight=400, anchor="start", extra=""):
    return (
        f'<text x="{x}" y="{y}" font-family="Mona Sans" font-size="{size}" font-weight="{weight}" '
        f'fill="{color}" text-anchor="{anchor}"{extra}>{s}</text>'
    )


def button(x, y, w, label, kind="primary", h=32):
    fill, ink = {"primary": ("url(#ember)", EMBER_INK), "danger": ("url(#red)", "#ffffff"), "secondary": ("url(#stone)", TEXT)}[kind]
    glow = ' filter="url(#btnglow)"' if kind != "secondary" else ""
    return (
        f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="9" fill="{fill}"{glow}/>'
        + text(x + w / 2, y + h / 2 + 5, label, 14, ink, 600, "middle")
    )


def icon_button(x, y, glyph, label, kind="secondary", split=False):
    """styled_button with a leading icon: 32 tall, 11 before a 14px icon, a 6px
    gap, the 14px semibold label, 14 after. `split` adds split_button's 28px
    chevron segment behind a 1px seam in the button's own ink. Returns the
    markup and its width."""
    fill, ink, seam = {
        "primary": ("url(#ember)", EMBER_INK, "#1f0b0040"),
        "secondary": ("url(#stone)", TEXT, "#ffffff1f"),
    }[kind]
    main = 11 + 14 + 6 + len(label) * 7.7 + 14
    w = main + (1 + 28 if split else 0)
    glow = ' filter="url(#btnglow)"' if kind == "primary" else ""
    out = (
        f'<rect x="{x}" y="{y}" width="{w:.1f}" height="32" rx="9" fill="{fill}"{glow}/>'
        + icon(glyph, x + 11, y + 9, 14, ink, 2.0)
        + text(x + 31, y + 21, label, 14, ink, 600)
    )
    if split:
        out += (
            f'<rect x="{x + main:.1f}" y="{y}" width="1" height="32" fill="{seam}"/>'
            + icon("chevron", x + main + 1 + 7.5, y + 9.5, 13, ink if kind == "primary" else MUTED, 2.0)
        )
    return out, w


def wordmark_burr(cx, cy, reach, spines=14):
    """The brand mark: the titlebar's 14-spine burr, all Ember, at poster scale."""
    r0 = reach * 0.34
    out = []
    for i in range(spines):
        a = i / spines * math.tau
        dx, dy = math.sin(a), -math.cos(a)
        nx, ny = -dy, dx
        root, tip = r0 - 4, r0 + (reach - r0) * 0.95
        bh, th = reach * 0.036, reach * 0.013
        p = lambda r, side: f"{cx + r * dx + side * nx:.2f},{cy + r * dy + side * ny:.2f}"
        out.append(f'<path d="M{p(root, -bh)} L{p(tip, -th)} L{p(tip, th)} L{p(root, bh)} Z" fill="{EMBER}"/>')
        out.append(f'<circle cx="{cx + tip * dx:.2f}" cy="{cy + tip * dy:.2f}" r="{reach * 0.04:.2f}" fill="{EMBER_HI}"/>')
    out.append(f'<circle cx="{cx}" cy="{cy}" r="{r0:.2f}" fill="url(#markcore)"/>')
    return "\n".join(out)


def fill_burr(cx, cy, reach, used, color, spines=40):
    """A drive before its scan: the used fraction lit in Fullness, the rest stubs."""
    r0 = reach * 0.34
    room = reach - r0
    out = []
    for i in range(spines):
        a = (i + 0.5) / spines * math.tau
        dx, dy = math.sin(a), -math.cos(a)
        lit = (i + 0.5) / spines < used
        length = room * (0.92 + 0.08 * math.sin(i * 2.399)) if lit else room * 0.34
        out.append(
            f'<line x1="{cx + r0 * dx:.2f}" y1="{cy + r0 * dy:.2f}" x2="{cx + (r0 + length) * dx:.2f}" '
            f'y2="{cy + (r0 + length) * dy:.2f}" stroke="{color if lit else "#ffffff24"}" '
            f'stroke-width="{reach * 0.045:.2f}" stroke-linecap="round"/>'
        )
    out.append(f'<circle cx="{cx}" cy="{cy}" r="{r0:.2f}" fill="#26201d"/>')
    return "\n".join(out)


def chrome(w, h, title=""):
    """A Burrdisk window: graphite ground, hairline edge, traffic lights."""
    lights = "".join(
        f'<circle cx="{24 + i * 20}" cy="28" r="6" fill="{c}"/>' for i, c in enumerate(["#ff5f57", "#febc2e", "#28c840"])
    )
    return (
        f'<rect width="{w}" height="{h}" rx="14" fill="{GROUND}" filter="url(#window)"/>'
        f'<rect width="{w}" height="{h}" rx="14" fill="url(#sheen)"/>'
        f'{lights}{title}'
    )


def edge(w, h):
    return f'<rect x="0.5" y="0.5" width="{w - 1}" height="{h - 1}" rx="13.5" fill="none" stroke="#ffffff" stroke-opacity="0.12"/>'


def home_window():
    """Home: the startup disk as a scanned burr beside its capacity sentence,
    then the other drives as small burrs lit to their fill. The window matches
    the Explorer's size; like the app, the column sits centered in it."""
    w, h = 1140, 740
    ox, oy = (w - 820) / 2, 40
    title = (
        f'{wordmark_burr(126, 28, 10)}'
        + text(144, 33, "Burrdisk", 14, TEXT, 700)
        + icon_button(w - 20 - (31 + 12 * 7.7 + 14), 12, "folder-search", "Scan Folder…")[0]
    )
    lead = burr(200, 250, 150, 0.34, 84, 2.6, 0.9, 2.2, 2.4, shares=SHARES, floor=0.62, jitter=0.06)
    body = (
        lead
        + text(200, 262, "68<tspan font-size='18' font-weight='500' fill='%s'>%%</tspan>" % MUTED, 34, TEXT, 700, "middle")
        + text(400, 212, "Macintosh HD", 32, TEXT, 700)
        + text(400, 248, f"<tspan font-weight='600' fill='{TEXT}'>312 GB</tspan> used of 460 GB", 17, MUTED)
        + text(400, 274, "APFS · Scanned 3 min ago", 12, EMBER_HI)
        + icon_button(400, 296, "open", "Open", "primary", split=True)[0]
        + text(40, 450, "Other drives", 16, TEXT, 600)
    )
    for i, (name, line, used, color) in enumerate(
        [("Backup", "640 GB free of 2 TB", 0.68, "#40dd7f"), ("Archive SSD", "38 GB free of 500 GB", 0.92, "#dd5c40")]
    ):
        x = 40 + i * 330
        body += (
            fill_burr(x + 46, 512, 46, used, color)
            + text(x + 108, 500, name, 14, TEXT, 600)
            + text(x + 108, 520, line, 12, MUTED)
            + icon_button(x + 108, 532, "scan", "Scan", split=True)[0]
        )
    body = f'<g transform="translate({ox},{oy})">{body}</g>'
    return w, h, chrome(w, h, title) + body + edge(w, h)


def arc(cx, cy, r0, r1, a0, a1):
    p = lambda r, a: f"{cx + r * math.sin(a):.2f},{cy - r * math.cos(a):.2f}"
    large = 1 if a1 - a0 > math.pi else 0
    return f"M{p(r0, a0)} L{p(r1, a0)} A{r1},{r1} 0 {large} 1 {p(r1, a1)} L{p(r0, a1)} A{r0},{r0} 0 {large} 0 {p(r0, a0)} Z"


def sunburst(cx, cy, inner, ring):
    """Three rings of a disk: the top folders, their children, theirs. Colors
    follow color_for (hue by position, darker each ring deeper)."""
    out = []
    first_mid = SHARES[0] / 2

    def hue(mid):
        return 52 + ((mid - first_mid) % 1.0) * 300

    def split(n, seed):
        parts = [1 + ((seed * 7 + k * 13) % 9) for k in range(n)]
        parts.sort(reverse=True)
        total = sum(parts)
        return [p / total for p in parts]

    def ring_of(start, span, depth, seed):
        if depth > 2 or span < 0.03:
            return
        at = start
        for k, part in enumerate(split(2 + seed % 3, seed)):
            share = span * part * (0.86 if depth else 1)
            mid = at + share / 2
            is_file = depth and k == 2
            l, c = (0.58 - depth * 0.03, 0.055 - depth * 0.006) if is_file else (0.74 - depth * 0.045, 0.15 - depth * 0.014)
            out.append(
                f'<path d="{arc(cx, cy, inner + depth * ring, inner + (depth + 1) * ring - 2, at * math.tau, (at + share) * math.tau)}" '
                f'fill="{oklch(l, c, hue(mid))}" stroke="{GROUND}" stroke-width="1.5"/>'
            )
            if not is_file:
                ring_of(at, share, depth + 1, seed * 3 + k + 1)
            at += share

    at = 0.0
    for i, share in enumerate(SHARES):
        mid = at + share / 2
        out.append(
            f'<path d="{arc(cx, cy, inner, inner + ring - 2, at * math.tau, (at + share) * math.tau)}" '
            f'fill="{oklch(0.74, 0.15, hue(mid))}" stroke="{GROUND}" stroke-width="1.5"/>'
        )
        ring_of(at, share, 1, i + 2)
        at += share
    out.append(f'<circle cx="{cx}" cy="{cy}" r="{inner - 4}" fill="{SURFACE}"/>')
    return "\n".join(out)


def explorer_window():
    """The Explorer: sunburst, the sidebar's rows with share bars, and the
    Collector holding three items above a red Move to Trash."""
    w, h = 1140, 740
    colors = lobe_colors(SHARES)
    title = (
        text(96 + 20, 33, f"Macintosh HD <tspan fill='{FAINT}'>›</tspan> Users", 14, MUTED, 500)
        + f'<rect x="{w - 20 - 300}" y="14" width="300" height="28" rx="9" fill="{SURFACE}"/>'
        + f'<rect x="{w - 20 - 298}" y="16" width="70" height="24" rx="7" fill="{EMBER}" fill-opacity="0.14"/>'
        + text(w - 20 - 263, 33, "Size", 13, EMBER_HI, 600, "middle")
        + text(w - 20 - 175, 33, "File Count", 13, MUTED, 400, "middle")
        + text(w - 20 - 62, 33, "File Types", 13, MUTED, 400, "middle")
    )
    side_x = w - 360
    body = sunburst(side_x / 2, 56 + (h - 56 - 80) / 2, 92, 74)
    body += text(side_x / 2, 56 + (h - 56 - 80) / 2 + 12, "312<tspan font-size='14' fill='%s' font-weight='500'> GB</tspan>" % MUTED, 30, TEXT, 700, "middle")
    body += f'<rect x="{side_x}" y="56" width="360" height="{h - 56 - 80}" fill="{SURFACE}"/><rect x="{side_x}" y="56" width="1" height="{h - 56 - 80}" fill="#ffffff" fill-opacity="0.07"/>'
    body += text(side_x + 20, 92, "Users", 16, TEXT, 600)
    body += text(side_x + 20, 124, f"118<tspan font-size='14' fill='{MUTED}' font-weight='500'> GB</tspan>", 24, TEXT, 700)
    rows = [("folder", "you", "92.4 GB", 0.78), ("folder", "Shared", "14.1 GB", 0.12), ("folder", "Guest", "6.8 GB", 0.06),
            ("app", "Xcode.app", "11.2 GB", 0.09), ("archive", "Backup 2023.zip", "9.6 GB", 0.08), ("video", "Screen Recording.mov", "3.1 GB", 0.03)]
    for i, (glyph, name, size, share) in enumerate(rows):
        y = 152 + i * 54
        c = colors[i % len(colors)]
        if i == 0:
            body += (f'<linearGradient id="rowhover" x1="0" y1="0" x2="1" y2="0"><stop offset="0" stop-color="{c}" stop-opacity="0.16"/>'
                     f'<stop offset="1" stop-color="{c}" stop-opacity="0.03"/></linearGradient>'
                     f'<rect x="{side_x + 8}" y="{y}" width="344" height="50" rx="11" fill="url(#rowhover)"/>')
        body += (
            icon(glyph, side_x + 22, y + 16, 18, lighten(c, 0.2), 2.0)
            + text(side_x + 52, y + 24, name, 14, TEXT, 500)
            + text(side_x + 336, y + 24, size, 12, MUTED, 400, "end")
            + f'<rect x="{side_x + 52}" y="{y + 34}" width="284" height="3" rx="1.5" fill="#ffffff" fill-opacity="0.08"/>'
            + f'<rect x="{side_x + 52}" y="{y + 34}" width="{284 * share:.1f}" height="3" rx="1.5" fill="{c}"/>'
        )
    ty = h - 80
    body += f'<rect x="0" y="{ty}" width="{w}" height="80" fill="url(#tray)"/><rect x="0" y="{ty}" width="{w}" height="1" fill="#ffffff" fill-opacity="0.07"/>'
    body += f'<rect x="20" y="{ty + 16}" width="48" height="48" rx="14" fill="url(#ember)" filter="url(#btnglow)"/>'
    body += text(44, ty + 48, "3", 22, EMBER_INK, 700, "middle")
    body += text(84, ty + 50, f"23.9<tspan font-size='14' fill='{MUTED}' font-weight='500'> GB</tspan>", 22, TEXT, 700)
    for i, (x, glyph, name, size, lobe) in enumerate(
        [(190, "app", "Xcode.app", "11.2 GB", 1), (370, "archive", "Backup 2023.zip", "9.6 GB", 3), (590, "video", "Screen Recording.mov", "3.1 GB", 2)]
    ):
        body += chip(40 + i, x, ty + 25, 1.0, 0, 0, 1, glyph, name, size, lobe)
    body += button(w - 20 - 140, ty + 24, 140, "Move to Trash", "danger")
    return w, h, chrome(w, h, title) + body + edge(w, h)


def placed(content, w, h, x, y, rot, scale=1.0):
    """Rotate (and scale) a window about its center and set that center at (x, y)."""
    return f'<g transform="translate({x},{y}) rotate({rot}) scale({scale}) translate({-w / 2},{-h / 2})">{content}</g>'


def pool(x, y, r, color, opacity):
    return f'<circle cx="{x}" cy="{y}" r="{r}" fill="{color}" opacity="{opacity}" filter="url(#pool)"/>'


def cover(tagline=True):
    """With the tagline, or without it: then the brand block drops 40px so the
    mark, name and platform marks stay centered between the windows."""
    dy = 0 if tagline else 40
    icons_y = 740 if tagline else 700
    hw, hh, home = home_window()
    ew, eh, explorer = explorer_window()
    colors = lobe_colors(SHARES)
    mark_y = 330 + dy
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="{W}" height="1080" viewBox="0 0 {W} 1080">
<defs>{defs()}</defs>
<rect width="{W}" height="1080" fill="{GROUND}"/>
<g mask="url(#wallmask)"><rect width="{W}" height="1080" fill="url(#wall)" filter="url(#soft)" opacity="0.07"/></g>
{pool(160, 720, 340, colors[0], 0.24)}
{pool(-40, 1000, 300, colors[4], 0.30)}
{pool(1760, 300, 380, colors[2], 0.28)}
{pool(1500, 40, 300, colors[5], 0.25)}
<circle cx="{CX}" cy="400" r="520" fill="url(#centerglow)"/>
{placed(home, hw, hh, 276, 912, TILT, 0.92)}
{placed(explorer, ew, eh, 1664, 158, TILT, 0.92)}
<g filter="url(#bloom)" opacity="0.6">{wordmark_burr(CX, mark_y, 120)}</g>
{wordmark_burr(CX, mark_y, 120)}
<g font-family="Mona Sans" text-anchor="middle">
<text x="{CX}" y="{612 + dy}" font-weight="900" font-size="150" letter-spacing="-4" fill="{TEXT}">Burrdisk</text>
{f'<text x="{CX}" y="690" font-family="Mona Sans W80" font-weight="400" font-size="48" fill="{MUTED}">Dig right in. Free up your disk.</text>' if tagline else ""}
</g>
<g fill="{MUTED}">
<path transform="translate({CX - 50},{icons_y}) scale(1.5)" d="{APPLE}"/>
<path transform="translate({CX + 14},{icons_y + 5}) scale(1.3)" d="{WINDOWS}"/>
</g>
<rect width="{W}" height="1080" fill="url(#vignette)"/>
<rect width="{W}" height="1080" filter="url(#grain)" opacity="0.04"/>
</svg>
'''


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    (OUT / "cover-alt.svg").write_text(cover(), encoding="utf-8")
    (OUT / "cover-alt-plain.svg").write_text(cover(tagline=False), encoding="utf-8")
    print(f"wrote {OUT / 'cover-alt.svg'} and cover-alt-plain.svg")

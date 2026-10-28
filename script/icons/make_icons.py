"""Draws Burrdisk's app icons as SVG masters.

The mark is a scanned burr, the same drawing the home screen makes of a disk:
a dark seed core with an Ember rim, and tapered spines in lobes, one lobe per
folder. The biggest lobe takes the data wheel's start hue (Ember orange), the
rest follow the sunburst's OKLCH wheel, and each lobe bulges in its middle.

    python script/icons/make_icons.py        writes assets/icon/*.svg

`script/icons/build.sh` then renders them and packs the .icns and .ico.
"""

import math
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT = ROOT / "assets" / "icon"

# Folder shares of the drawn disk, largest first. Five lobes read as a burr,
# not a sun, and stay distinct down to 32px
SHARES = [0.34, 0.22, 0.18, 0.14, 0.12]


def oklch(l, c, h):
    """OKLCH to an sRGB hex string, clipped into gamut."""
    a, b = c * math.cos(math.radians(h)), c * math.sin(math.radians(h))
    l_ = l + 0.3963377774 * a + 0.2158037573 * b
    m_ = l - 0.1055613458 * a - 0.0638541728 * b
    s_ = l - 0.0894841775 * a - 1.2914855480 * b
    l3, m3, s3 = l_**3, m_**3, s_**3
    rgb = (
        4.0767416621 * l3 - 3.3077115913 * m3 + 0.2309699292 * s3,
        -1.2684380046 * l3 + 2.6097574011 * m3 - 0.3413193965 * s3,
        -0.0041960863 * l3 - 0.7034186147 * m3 + 1.7076147010 * s3,
    )

    def gamma(x):
        x = min(max(x, 0.0), 1.0)
        return 12.92 * x if x <= 0.0031308 else 1.055 * x ** (1 / 2.4) - 0.055

    return "#" + "".join(f"{round(gamma(x) * 255):02x}" for x in rgb)


def lobe_colors(shares):
    """The sunburst's folder colors: hue = 52 + position * 300, rotated so the
    middle of the largest lobe sits on the start hue."""
    mids, at = [], 0.0
    for share in shares:
        mids.append(at + share / 2)
        at += share
    return [oklch(0.74, 0.15, 52 + ((mid - mids[0]) % 1.0) * 300) for mid in mids]


def lighten(hex_color, amount):
    r, g, b = (int(hex_color[i : i + 2], 16) for i in (1, 3, 5))
    mix = lambda v: round(v + (255 - v) * amount)
    return f"#{mix(r):02x}{mix(g):02x}{mix(b):02x}"


def squircle(cx, cy, half, n=5.0, steps=256):
    """Apple's continuous-corner plate, as a superellipse."""
    pts = []
    for i in range(steps):
        t = i / steps * math.tau
        c, s = math.cos(t), math.sin(t)
        x = cx + half * math.copysign(abs(c) ** (2 / n), c)
        y = cy + half * math.copysign(abs(s) ** (2 / n), s)
        pts.append(f"{x:.2f},{y:.2f}")
    return "M" + " L".join(pts) + " Z"


def burr(cx, cy, reach, core_ratio, spines_total, base_half, tip_half, bead, rim, shares=SHARES, floor=0.70, jitter=0.05):
    """SVG for one burr. Spines run all the way round, like the real seed; lobes
    read from color and from reach, which grows with the folder's share and
    bulges mid-lobe. `bead` > 0 caps each spine with its hook."""
    colors = lobe_colors(shares)
    r0 = reach * core_ratio
    room = reach - r0
    widest = max(shares)
    out = []
    at, index = 0.0, 0
    for share, color in zip(shares, colors):
        count = max(2, round(spines_total * share))
        lobe_reach = floor + (1 - floor) * math.sqrt(share / widest)
        hi, lo = lighten(color, 0.20), color
        for k in range(count):
            u = (k + 0.5) / count
            profile = 0.84 + 0.16 * math.sin(math.pi * u)
            wobble = 1 + jitter * math.sin(index * 2.399 + 0.7) * math.cos(index * 1.13)
            length = room * lobe_reach * profile * wobble
            a = (at + share * u) * math.tau
            dx, dy = math.sin(a), -math.cos(a)
            nx, ny = -dy, dx
            root, tip = r0 - base_half * 1.2, r0 + length
            p = lambda r, side: f"{cx + r * dx + side * nx:.2f},{cy + r * dy + side * ny:.2f}"
            gid = f"g{index}"
            out.append(
                f'<linearGradient id="{gid}" gradientUnits="userSpaceOnUse" '
                f'x1="{cx + r0 * dx:.2f}" y1="{cy + r0 * dy:.2f}" x2="{cx + tip * dx:.2f}" y2="{cy + tip * dy:.2f}">'
                f'<stop offset="0" stop-color="{lo}"/><stop offset="1" stop-color="{hi}"/></linearGradient>'
                f'<path d="M{p(root, -base_half)} L{p(tip, -tip_half)} L{p(tip, tip_half)} L{p(root, base_half)} Z" fill="url(#{gid})"/>'
            )
            if bead:
                out.append(
                    f'<circle cx="{cx + tip * dx:.2f}" cy="{cy + tip * dy:.2f}" r="{bead:.2f}" fill="{lighten(color, 0.38)}"/>'
                )
            index += 1
        at += share
    # The seed core: warm dark, lit from above, with the cached-scan Ember rim
    out.append(
        f'<radialGradient id="core" cx="{cx:.2f}" cy="{cy - r0 * 0.45:.2f}" r="{r0 * 1.45:.2f}" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="#3a302b"/><stop offset="1" stop-color="#1c1714"/></radialGradient>'
        f'<circle cx="{cx}" cy="{cy}" r="{r0:.2f}" fill="url(#core)"/>'
    )
    if rim:
        out.append(
            f'<circle cx="{cx}" cy="{cy}" r="{r0 - rim / 2:.2f}" fill="none" stroke="#f97316" stroke-width="{rim:.2f}"/>'
        )
    return "\n".join(out)


GRAIN = (
    # Gray noise laid over the plate at a few percent, to dither its dark gradient
    # (8-bit steps would otherwise show as bands)
    '<filter id="grain" x="0" y="0" width="100%" height="100%">'
    '<feTurbulence type="fractalNoise" baseFrequency="0.9" numOctaves="2" seed="7"/>'
    '<feColorMatrix values="1 0 0 0 0  1 0 0 0 0  1 0 0 0 0  0 0 0 0 1"/></filter>'
)


def plate_defs(prefix, top, bottom, glow_cx, glow_cy, glow_r):
    return (
        f'<linearGradient id="{prefix}plate" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="{top}"/><stop offset="1" stop-color="{bottom}"/></linearGradient>'
        f'<radialGradient id="{prefix}glow" cx="{glow_cx}" cy="{glow_cy}" r="{glow_r}" gradientUnits="userSpaceOnUse">'
        f'<stop offset="0" stop-color="#f97316" stop-opacity="0.30"/>'
        f'<stop offset="0.55" stop-color="#f97316" stop-opacity="0.07"/>'
        f'<stop offset="1" stop-color="#f97316" stop-opacity="0"/></radialGradient>'
        f'<linearGradient id="{prefix}sheen" x1="0" y1="0" x2="0" y2="1">'
        f'<stop offset="0" stop-color="#ffffff" stop-opacity="0.07"/>'
        f'<stop offset="0.45" stop-color="#ffffff" stop-opacity="0"/></linearGradient>'
    )


def mac(size_class="large"):
    """macOS: 1024 canvas, 824 squircle body per Apple's grid, shadow baked in.
    The 16 and 32 point slots get the simpler burr."""
    body = squircle(512, 512, 412)
    mark = MARKS[size_class](512, 512, 4.0)
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="1024" height="1024" viewBox="0 0 1024 1024">
<defs>
{plate_defs("m", "#2a2320", "#100d0c", 512, 470, 430)}
<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
<feGaussianBlur in="SourceAlpha" stdDeviation="14"/><feOffset dy="12"/>
<feComponentTransfer><feFuncA type="linear" slope="0.38"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
<filter id="lift" x="-30%" y="-30%" width="160%" height="160%">
<feGaussianBlur in="SourceAlpha" stdDeviation="9"/><feOffset dy="8"/>
<feComponentTransfer><feFuncA type="linear" slope="0.45"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
<clipPath id="body"><path d="{body}"/></clipPath>
{GRAIN}
</defs>
<g filter="url(#shadow)">
<path d="{body}" fill="url(#mplate)"/>
</g>
<g clip-path="url(#body)">
<rect width="1024" height="1024" filter="url(#grain)" opacity="0.045"/>
<rect width="1024" height="1024" fill="url(#msheen)"/>
<circle cx="512" cy="470" r="430" fill="url(#mglow)"/>
<g filter="url(#lift)">
{mark}
</g>
</g>
<path d="{body}" fill="none" stroke="#ffffff" stroke-opacity="0.09" stroke-width="3"/>
</svg>
'''


# One burr per size range, in 256 units scaled by `k`. Small sizes drop the hooks
# and thin spines, which would only blur, and keep color and silhouette
MARKS = {
    "large": lambda cx, cy, k: burr(cx, cy, 80 * k, 0.34, 76, 2.1 * k, 0.55 * k, 1.25 * k, 1.7 * k),
    "medium": lambda cx, cy, k: burr(cx, cy, 86 * k, 0.37, 34, 4.2 * k, 1.4 * k, 0, 3.2 * k, jitter=0.03),
    "small": lambda cx, cy, k: burr(cx, cy, 100 * k, 0.42, 14, 10 * k, 4.0 * k, 0, 7 * k,
                                    shares=[0.36, 0.26, 0.21, 0.17], floor=0.78, jitter=0),
}


def windows(size_class):
    """Windows: a Fluent tile that fills more of the canvas than the macOS plate,
    with gentler corners and a fine top highlight."""
    inset, radius = {"large": (10, 46), "medium": (8, 40), "small": (4, 34)}[size_class]
    mark = MARKS[size_class](128, 128, {"large": 1.3, "medium": 1.2, "small": 1.12}[size_class])
    stroke = 1.2 if size_class == "large" else 0
    side = 256 - inset * 2
    return f'''<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
<defs>
{plate_defs("w", "#2a2320", "#100d0c", 128, 112, 120)}
<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
<feGaussianBlur in="SourceAlpha" stdDeviation="2.5"/><feOffset dy="2"/>
<feComponentTransfer><feFuncA type="linear" slope="0.30"/></feComponentTransfer>
<feMerge><feMergeNode/><feMergeNode in="SourceGraphic"/></feMerge></filter>
<clipPath id="tile"><rect x="{inset}" y="{inset}" width="{side}" height="{side}" rx="{radius}"/></clipPath>
{GRAIN}
</defs>
<g filter="url(#shadow)">
<rect x="{inset}" y="{inset}" width="{side}" height="{side}" rx="{radius}" fill="url(#wplate)"/>
</g>
<g clip-path="url(#tile)">
<rect width="256" height="256" filter="url(#grain)" opacity="0.045"/>
<rect width="256" height="256" fill="url(#wsheen)"/>
<circle cx="128" cy="112" r="120" fill="url(#wglow)"/>
{mark}
</g>
{f'<rect x="{inset + stroke / 2}" y="{inset + stroke / 2}" width="{side - stroke}" height="{side - stroke}" rx="{radius - stroke / 2}" fill="none" stroke="#ffffff" stroke-opacity="0.10" stroke-width="{stroke}"/>' if stroke else ""}
</svg>
'''


if __name__ == "__main__":
    OUT.mkdir(parents=True, exist_ok=True)
    for size_class in MARKS:
        (OUT / f"macos-{size_class}.svg").write_text(mac(size_class), encoding="utf-8")
        (OUT / f"windows-{size_class}.svg").write_text(windows(size_class), encoding="utf-8")
    print(f"wrote {OUT}")

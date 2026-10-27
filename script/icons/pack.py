"""Packs rendered icon PNGs into Burrdisk.icns (macOS) and burrdisk.ico (Windows).

    python script/icons/pack.py <png-dir>

<png-dir> holds `mac-<px>.png` and `win-<px>.png` from `build.sh`. Both formats
take PNG payloads directly, so this needs no image library and runs anywhere.
"""

import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# icns slot type -> pixel size. Retina slots (@2x) reuse the larger renders.
ICNS_SLOTS = [
    ("icp4", 16), ("icp5", 32), ("ic11", 32),   # 16pt, 32px, 16pt@2x
    ("ic12", 64), ("ic07", 128), ("ic13", 256),  # 32pt@2x, 128pt, 128pt@2x
    ("ic08", 256), ("ic14", 512), ("ic09", 512), # 256pt, 256pt@2x, 512pt
    ("ic10", 1024),                              # 512pt@2x
]
ICO_SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]


def icns(pngs: Path, out: Path):
    chunks = b""
    for kind, size in ICNS_SLOTS:
        data = (pngs / f"mac-{size}.png").read_bytes()
        chunks += kind.encode() + struct.pack(">I", len(data) + 8) + data
    out.write_bytes(b"icns" + struct.pack(">I", len(chunks) + 8) + chunks)


def ico(pngs: Path, out: Path):
    images = [(size, (pngs / f"win-{size}.png").read_bytes()) for size in ICO_SIZES]
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries, body = b"", b""
    for size, data in images:
        dim = 0 if size >= 256 else size  # 0 means 256 in an ICO directory
        entries += struct.pack("<BBBBHHII", dim, dim, 0, 0, 1, 32, len(data), offset + len(body))
        body += data
    out.write_bytes(header + entries + body)


if __name__ == "__main__":
    pngs = Path(sys.argv[1])
    (ROOT / "assets" / "macos").mkdir(exist_ok=True)
    icns(pngs, ROOT / "assets" / "macos" / "Burrdisk.icns")
    ico(pngs, ROOT / "assets" / "windows" / "burrdisk.ico")
    print("wrote assets/macos/Burrdisk.icns and assets/windows/burrdisk.ico")

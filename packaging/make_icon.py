#!/usr/bin/env python3
"""Generate packaging/syscheck.png (128x128) with pure stdlib - a dark
terminal-style icon with green/yellow/red performance bars."""

import struct
import sys
import zlib

W = H = 128

# (x0, x1, y0, y1, rgb)
BARS = [
    (24, 40, 88, 112, (60, 200, 100)),
    (44, 60, 68, 112, (200, 190, 60)),
    (64, 80, 48, 112, (240, 130, 40)),
    (84, 100, 28, 112, (220, 70, 60)),
]
BG = (24, 26, 32)
FRAME = (70, 74, 86)


def px(x, y):
    if x < 2 or x >= W - 2 or y < 2 or y >= H - 2:
        return FRAME
    for (x0, x1, y0, y1, rgb) in BARS:
        if x0 <= x < x1 and y0 <= y < y1:
            # top highlight for a subtle 3D look
            if y == y0:
                return tuple(min(255, c + 45) for c in rgb)
            return rgb
    return BG


def chunk(tag, payload):
    return (struct.pack(">I", len(payload)) + tag + payload
            + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))


def row_bytes(y):
    row = bytearray(b"\x00")
    for x in range(W):
        row += bytes(px(x, y))
    return bytes(row)

rows = b"".join(row_bytes(y) for y in range(H))

png = (b"\x89PNG\r\n\x1a\n"
       + chunk(b"IHDR", struct.pack(">IIBBBBB", W, H, 8, 2, 0, 0, 0))
       + chunk(b"IDAT", zlib.compress(rows, 9))
       + chunk(b"IEND", b""))

out = sys.argv[1] if len(sys.argv) > 1 else "syscheck.png"
with open(out, "wb") as f:
    f.write(png)
print(f"wrote {out} ({len(png)} bytes)")

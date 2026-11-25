#!/usr/bin/env python3
"""Generate ATOM's application icons without external image tools.

Writes PNGs for Linux packaging plus a multi-resolution Windows .ico, all from
one vector-ish definition so the marks stay consistent. Run from the repo root:

    python3 scripts/make_icons.py
"""

from __future__ import annotations

import math
import os
import struct
import zlib

OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "apps", "desktop", "icons")
SS = 4  # supersampling factor


def rounded_rect(x: float, y: float, radius: float = 0.22) -> bool:
    if x < 0 or x > 1 or y < 0 or y > 1:
        return False
    cx = min(max(x, radius), 1 - radius)
    cy = min(max(y, radius), 1 - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius ** 2


def ellipse_ring(x: float, y: float, angle: float, a: float, b: float, width: float) -> float:
    """Distance (in normalised units) from the ellipse outline; negative inside."""
    ca, sa = math.cos(angle), math.sin(angle)
    dx, dy = x - 0.5, y - 0.5
    u = dx * ca + dy * sa
    v = -dx * sa + dy * ca
    return abs(math.hypot(u / a, v / b) - 1.0) * min(a, b) - width


def sample(x: float, y: float):
    if not rounded_rect(x, y):
        return (0, 0, 0, 0)

    # Background: near-black with a soft blue glow behind the nucleus.
    t = y
    r = int(0x10 + (0x07 - 0x10) * t)
    g = int(0x14 + (0x08 - 0x14) * t)
    b = int(0x20 + (0x0C - 0x20) * t)
    glow = max(0.0, 1.0 - math.hypot(x - 0.5, y - 0.5) / 0.62) ** 2
    r += int(18 * glow)
    g += int(24 * glow)
    b += int(46 * glow)

    rings = (
        (0.0, "#7f9dff"),
        (math.pi / 3, "#8f7dff"),
        (2 * math.pi / 3, "#59c9c1"),
    )
    for angle, colour in rings:
        d = ellipse_ring(x, y, angle, 0.345, 0.135, 0.030)
        if d < 0:
            alpha = min(1.0, -d / 0.012)
            cr = int(colour[1:3], 16)
            cg = int(colour[3:5], 16)
            cb = int(colour[5:7], 16)
            r = int(r * (1 - alpha) + cr * alpha)
            g = int(g * (1 - alpha) + cg * alpha)
            b = int(b * (1 - alpha) + cb * alpha)

    # Nucleus with a bright core.
    dist = math.hypot(x - 0.5, y - 0.5)
    if dist < 0.105:
        alpha = min(1.0, (0.105 - dist) / 0.014)
        core = max(0.0, 1.0 - dist / 0.105)
        cr = int(0xE8 + 0x10 * core)
        cg = int(0xEE + 0x08 * core)
        cb = 255
        r = int(r * (1 - alpha) + min(255, cr) * alpha)
        g = int(g * (1 - alpha) + min(255, cg) * alpha)
        b = int(b * (1 - alpha) + cb * alpha)

    return (r, g, b, 255)


def render(size: int) -> bytes:
    big = size * SS
    rows = []
    for py in range(size):
        row = bytearray()
        for px in range(size):
            acc = [0, 0, 0, 0]
            for sy in range(SS):
                for sx in range(SS):
                    x = (px * SS + sx + 0.5) / big
                    y = (py * SS + sy + 0.5) / big
                    r, g, b, a = sample(x, y)
                    acc[0] += r * a
                    acc[1] += g * a
                    acc[2] += b * a
                    acc[3] += a
            n = SS * SS
            a = acc[3] / n
            if a == 0:
                row += bytes((0, 0, 0, 0))
            else:
                row += bytes(
                    (
                        min(255, round(acc[0] / acc[3])),
                        min(255, round(acc[1] / acc[3])),
                        min(255, round(acc[2] / acc[3])),
                        min(255, round(a)),
                    )
                )
        rows.append(bytes(row))
    return png(size, rows)


def png(size: int, rows: list[bytes]) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return (
            struct.pack(">I", len(data))
            + kind
            + data
            + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
        )

    raw = b"".join(b"\x00" + row for row in rows)
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(raw, 9))
        + chunk(b"IEND", b"")
    )


def ico(images: list[tuple[int, bytes]]) -> bytes:
    header = struct.pack("<HHH", 0, 1, len(images))
    offset = 6 + 16 * len(images)
    entries = b""
    payload = b""
    for size, data in images:
        entries += struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(data), offset)
        offset += len(data)
        payload += data
    return header + entries + payload


def main() -> None:
    os.makedirs(OUT, exist_ok=True)
    sizes = {512: "icon.png", 256: "128x128@2x.png", 128: "128x128.png", 32: "32x32.png"}
    for size, name in sizes.items():
        data = render(size)
        with open(os.path.join(OUT, name), "wb") as fh:
            fh.write(data)
        print(f"wrote {name} ({size}x{size}, {len(data)} bytes)")

    small = [(16, render(16)), (32, render(32)), (48, render(48)), (64, render(64)), (128, render(128)), (256, render(256))]
    with open(os.path.join(OUT, "icon.ico"), "wb") as fh:
        fh.write(ico(small))
    print("wrote icon.ico (16-256)")


if __name__ == "__main__":
    main()

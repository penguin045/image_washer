#!/usr/bin/env python3

from __future__ import annotations

import math
import struct
import sys
import zlib
from pathlib import Path


def blend(a: tuple[int, int, int], b: tuple[int, int, int], t: float) -> tuple[int, int, int]:
    t = max(0.0, min(1.0, t))
    return tuple(round(a[i] * (1 - t) + b[i] * t) for i in range(3))


def inside_round_rect(
    x: float,
    y: float,
    left: float,
    top: float,
    right: float,
    bottom: float,
    radius: float,
) -> bool:
    cx = min(max(x, left + radius), right - radius)
    cy = min(max(y, top + radius), bottom - radius)
    return (x - cx) ** 2 + (y - cy) ** 2 <= radius**2


def icon_pixels(size: int) -> bytes:
    rows = bytearray()
    base_a = (245, 240, 232)
    base_b = (229, 212, 191)
    accent = (170, 59, 18)
    accent_light = (224, 114, 58)
    ink = (30, 24, 20)
    cream = (255, 250, 243)

    for y in range(size):
        rows.append(0)
        for x in range(size):
            nx = x / max(size - 1, 1)
            ny = y / max(size - 1, 1)
            color = blend(base_a, base_b, 0.65 * ny + 0.2 * nx)

            glow = max(0.0, 1.0 - math.hypot(nx - 0.72, ny - 0.26) / 0.55)
            color = blend(color, (240, 193, 159), glow * 0.48)

            if inside_round_rect(
                x,
                y,
                size * 0.18,
                size * 0.22,
                size * 0.82,
                size * 0.78,
                size * 0.10,
            ):
                color = blend(color, cream, 0.82)

            if inside_round_rect(
                x,
                y,
                size * 0.25,
                size * 0.30,
                size * 0.75,
                size * 0.68,
                size * 0.06,
            ):
                color = blend(color, (244, 218, 198), 0.88)

            slash = abs((y - size * 0.66) - (x - size * 0.24) * -0.28)
            if size * 0.18 <= x <= size * 0.82 and slash < size * 0.028:
                color = accent

            if inside_round_rect(
                x,
                y,
                size * 0.30,
                size * 0.39,
                size * 0.45,
                size * 0.63,
                size * 0.03,
            ):
                color = ink

            if inside_round_rect(
                x,
                y,
                size * 0.55,
                size * 0.39,
                size * 0.70,
                size * 0.63,
                size * 0.03,
            ):
                color = ink

            circle = math.hypot(x - size * 0.50, y - size * 0.73)
            if circle <= size * 0.075:
                color = accent_light

            rows.extend((*color, 255))

    return bytes(rows)


def chunk(kind: bytes, data: bytes) -> bytes:
    return (
        struct.pack(">I", len(data))
        + kind
        + data
        + struct.pack(">I", zlib.crc32(kind + data) & 0xFFFFFFFF)
    )


def write_png(path: Path, size: int) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    header = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    data = icon_pixels(size)
    png = (
        b"\x89PNG\r\n\x1a\n"
        + chunk(b"IHDR", header)
        + chunk(b"IDAT", zlib.compress(data, 9))
        + chunk(b"IEND", b"")
    )
    path.write_bytes(png)


def main() -> int:
    output_dir = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("docs/icons")
    write_png(output_dir / "icon-192.png", 192)
    write_png(output_dir / "icon-512.png", 512)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

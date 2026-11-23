"""Generate LightBrowser's icon (icon-256.png + multi-size icon.ico). Pure standard library.

Run: python assets/make_icon.py
"""
import math
import struct
import zlib
from pathlib import Path

OUT = Path(__file__).parent
SS = 4  # supersampling per axis


def png_bytes(size, pixels):
    raw = b"".join(b"\x00" + bytes(pixels[y * size * 4:(y + 1) * size * 4]) for y in range(size))

    def chunk(tag, data):
        return struct.pack(">I", len(data)) + tag + data + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def render(size):
    top, bot = (92, 158, 255), (40, 88, 222)
    radius = 0.22  # corner radius, fraction of size
    px = bytearray(size * size * 4)
    for y in range(size):
        for x in range(size):
            bg = ring = 0
            for sy in range(SS):
                for sx in range(SS):
                    u = (x + (sx + 0.5) / SS) / size  # 0..1
                    v = (y + (sy + 0.5) / SS) / size
                    # Rounded square
                    dx = max(abs(u - 0.5) - (0.5 - radius), 0)
                    dy = max(abs(v - 0.5) - (0.5 - radius), 0)
                    if math.hypot(dx, dy) > radius:
                        continue
                    bg += 1
                    # Globe: outer ring, meridian ellipse, equator
                    cx, cy = u - 0.5, v - 0.5
                    r = math.hypot(cx, cy)
                    w = 0.055
                    on = abs(r - 0.29) < w / 2
                    if r < 0.29:
                        ell = math.hypot(cx / 0.13, cy / 0.29)
                        on = on or abs(ell - 1.0) * 0.13 < w / 2
                        on = on or abs(cy) < w / 2
                    ring += on
            n = SS * SS
            a = bg / n
            if a == 0:
                continue
            t = (x + y) / (2 * size)
            col = [top[i] + (bot[i] - top[i]) * t for i in range(3)]
            k = ring / max(bg, 1)
            col = [c * (1 - k) + 255 * k for c in col]
            i = (y * size + x) * 4
            px[i:i + 4] = bytes([round(col[0]), round(col[1]), round(col[2]), round(a * 255)])
    return px


def main():
    sizes = [16, 32, 48, 256]
    pngs = {s: png_bytes(s, render(s)) for s in sizes}
    (OUT / "icon-256.png").write_bytes(pngs[256])
    header = struct.pack("<HHH", 0, 1, len(sizes))
    offset = 6 + 16 * len(sizes)
    entries, data = b"", b""
    for s in sizes:
        d = pngs[s]
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(d), offset + len(data))
        data += d
    (OUT / "icon.ico").write_bytes(header + entries + data)
    print("wrote", OUT / "icon.ico", OUT / "icon-256.png")


if __name__ == "__main__":
    main()

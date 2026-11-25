"""Pack the app icon PNGs (made by make_icon.ps1) into icon.ico. Pure standard library.

Run after make_icon.ps1:  python assets/make_icon.py
"""
import struct
from pathlib import Path

OUT = Path(__file__).parent
SIZES = [16, 32, 48, 256]


def main():
    pngs = {s: (OUT / f"icon-{s}.png").read_bytes() for s in SIZES}
    header = struct.pack("<HHH", 0, 1, len(SIZES))
    offset = 6 + 16 * len(SIZES)
    entries, data = b"", b""
    for s in SIZES:
        d = pngs[s]
        # Width/height 0 means 256 in the ICO format.
        entries += struct.pack("<BBBBHHII", s % 256, s % 256, 0, 0, 1, 32, len(d), offset + len(data))
        data += d
    (OUT / "icon.ico").write_bytes(header + entries + data)
    print("wrote", OUT / "icon.ico")


if __name__ == "__main__":
    main()

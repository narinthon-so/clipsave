"""Verify a PNG saved by clipsave against tests/fixtures/alpha.png (stdlib only).

usage: python3 tests/check_png.py OUT.png [--allow-opaque]
Checks size, RGBA colour type and that transparency survived.
--allow-opaque skips the alpha checks (clipboard paths that flatten alpha).
"""
import struct
import sys
import zlib


def read_png(path):
    data = open(path, "rb").read()
    assert data[:8] == b"\x89PNG\r\n\x1a\n", "not a PNG"
    pos, idat, ihdr = 8, b"", None
    while pos < len(data):
        (n,) = struct.unpack(">I", data[pos : pos + 4])
        kind, body = data[pos + 4 : pos + 8], data[pos + 8 : pos + 8 + n]
        if kind == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", body)
        elif kind == b"IDAT":
            idat += body
        pos += 12 + n
    w, h, depth, color, _, _, interlace = ihdr
    assert (depth, color, interlace) == (8, 6, 0), f"want 8-bit RGBA non-interlaced, got {ihdr}"
    raw, stride, bpp = zlib.decompress(idat), w * 4, 4
    pixels, prev = [], bytearray(stride)
    for y in range(h):
        f, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1 : (y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - bpp] if i >= bpp else 0
            b, c = prev[i], prev[i - bpp] if i >= bpp else 0
            if f == 1: line[i] = (line[i] + a) & 255
            elif f == 2: line[i] = (line[i] + b) & 255
            elif f == 3: line[i] = (line[i] + (a + b) // 2) & 255
            elif f == 4:
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        pixels.append(bytes(line))
        prev = line
    return w, h, lambda x, y: tuple(pixels[y][x * 4 : x * 4 + 4])


def main():
    path, allow_opaque = sys.argv[1], "--allow-opaque" in sys.argv
    w, h, px = read_png(path)
    assert (w, h) == (64, 32), f"size {w}x{h}"
    assert px(0, 0) == (255, 0, 0, 255), f"opaque red pixel is {px(0, 0)}"
    if not allow_opaque:
        assert px(63, 31)[3] == 0, f"transparent pixel is {px(63, 31)}"
        assert px(40, 5)[3] == 128, f"half-alpha pixel is {px(40, 5)}"
    print(f"ok  {path}  {w}x{h}" + ("" if allow_opaque else "  alpha preserved"))


main()

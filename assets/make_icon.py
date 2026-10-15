"""Draws the clipsave icon: a clipboard holding a picture, on a rounded tile.

    python3 assets/make_icon.py    ->  assets/clipsave.ico, assets/clipsave.png

Each size is drawn at 8x and downsampled, so edges stay smooth even at 16 px.
"""
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).parent
SIZES = [16, 20, 24, 32, 40, 48, 64, 128, 256]
SS = 8  # supersampling factor

TILE_TOP = (99, 102, 241)  # indigo
TILE_BOTTOM = (124, 58, 237)  # violet
BOARD = (255, 255, 255)
CLIP = (226, 232, 240)
SKY = (224, 242, 254)
HILL = (16, 185, 129)
HILL_BACK = (52, 211, 153)
SUN = (251, 191, 36)


def draw(size: int) -> Image.Image:
    s = size * SS
    u = s / 32  # design grid: 32 x 32 units
    img = Image.new("RGBA", (s, s), (0, 0, 0, 0))

    # Tile with a vertical gradient, masked to a rounded square.
    grad = Image.new("RGBA", (s, s))
    gd = ImageDraw.Draw(grad)
    for y in range(s):
        t = y / (s - 1)
        gd.line([(0, y), (s, y)], fill=tuple(int(a + (b - a) * t) for a, b in zip(TILE_TOP, TILE_BOTTOM)) + (255,))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([0, 0, s - 1, s - 1], radius=7 * u, fill=255)
    img.paste(grad, (0, 0), mask)

    d = ImageDraw.Draw(img)
    # Clipboard board.
    board = [7 * u, 6 * u, 25 * u, 28 * u]
    d.rounded_rectangle(board, radius=2.5 * u, fill=BOARD)
    # Clip at the top.
    d.rounded_rectangle([11.5 * u, 3.5 * u, 20.5 * u, 8.5 * u], radius=1.8 * u, fill=CLIP)
    d.rounded_rectangle([14 * u, 2.5 * u, 18 * u, 5 * u], radius=1.2 * u, fill=CLIP)

    # Picture on the board: sky, sun, two hills.
    pic = [10 * u, 11 * u, 22 * u, 24.5 * u]
    pmask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(pmask).rounded_rectangle(pic, radius=1.5 * u, fill=255)
    layer = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ld = ImageDraw.Draw(layer)
    ld.rectangle(pic, fill=SKY)
    ld.ellipse([17.2 * u, 13 * u, 20.4 * u, 16.2 * u], fill=SUN)
    ld.polygon([(13 * u, 24.5 * u), (19 * u, 17 * u), (25 * u, 24.5 * u)], fill=HILL_BACK)
    ld.polygon([(8 * u, 24.5 * u), (13.5 * u, 16 * u), (19.5 * u, 24.5 * u)], fill=HILL)
    img.paste(layer, (0, 0), Image.composite(layer, Image.new("RGBA", (s, s)), pmask).getchannel("A"))

    return img.resize((size, size), Image.LANCZOS)


def main() -> None:
    frames = [draw(n) for n in SIZES]
    big = frames[-1]
    big.save(HERE / "clipsave.png")
    big.save(HERE / "clipsave.ico", sizes=[(n, n) for n in SIZES], append_images=frames[:-1])
    # Preview sheet for review: every size at 1x on light and dark.
    (HERE.parent / "target").mkdir(exist_ok=True)
    sheet = Image.new("RGBA", (sum(SIZES) + 10 * len(SIZES) + 10, 2 * 256 + 30), (240, 240, 240, 255))
    dark = Image.new("RGBA", (sheet.width, 256 + 10), (32, 32, 32, 255))
    sheet.paste(dark, (0, 256 + 20))
    x = 10
    for f in frames:
        sheet.paste(f, (x, 10), f)
        sheet.paste(f, (x, 256 + 25), f)
        x += f.width + 10
    sheet.save(HERE.parent / "target" / "icon-preview.png")


if __name__ == "__main__":
    main()

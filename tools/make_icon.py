"""Generate assets/trimbar.ico (multi-size app + tray icon) and assets/icon.png (README).

Run from the repo root: python tools/make_icon.py
Requires Pillow (pip install pillow).
"""

from PIL import Image, ImageDraw

FRAME = (226, 230, 238, 255)
SCREEN = (64, 156, 255, 255)
DEAD = (24, 26, 32, 255)
CUT = (255, 84, 84, 255)

MASTER = 1024


def render() -> Image.Image:
    img = Image.new("RGBA", (MASTER, MASTER), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    s = MASTER / 32

    d.rounded_rectangle([1 * s, 3 * s, 31 * s, 24 * s], radius=3 * s, fill=FRAME)
    d.rectangle([3.5 * s, 5.5 * s, 28.5 * s, 21.5 * s], fill=SCREEN)
    d.rectangle([3.5 * s, 16.5 * s, 28.5 * s, 21.5 * s], fill=DEAD)
    d.rectangle([3.5 * s, 15 * s, 28.5 * s, 17 * s], fill=CUT)
    d.rectangle([14 * s, 24 * s, 18 * s, 27.5 * s], fill=FRAME)
    d.rounded_rectangle([9 * s, 27 * s, 23 * s, 30 * s], radius=1.5 * s, fill=FRAME)
    return img


def main() -> None:
    master = render()
    sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256]
    icon = master.resize((256, 256), Image.LANCZOS)
    icon.save("assets/icon.png")
    icon.save(
        "assets/trimbar.ico", sizes=[(n, n) for n in sizes]
    )


if __name__ == "__main__":
    main()

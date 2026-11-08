#!/usr/bin/env python3
"""Generates the application icon (assets/donustur.png).

Run from the repository root: python3 assets/generate-icon.py
Requires Pillow. The generated PNG is committed, so this is only needed when
the artwork changes.
"""

from PIL import Image, ImageDraw

SIZE = 512
OUT = "assets/donustur.png"


def rounded_gradient(size: int) -> Image.Image:
    image = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    gradient = Image.new("RGB", (size, size))
    top = (37, 99, 235)      # blue-600
    bottom = (30, 58, 138)   # blue-900
    for y in range(size):
        t = y / (size - 1)
        color = tuple(int(top[i] + (bottom[i] - top[i]) * t) for i in range(3))
        for x in range(size):
            gradient.putpixel((x, y), color)
    mask = Image.new("L", (size, size), 0)
    draw = ImageDraw.Draw(mask)
    draw.rounded_rectangle((8, 8, size - 8, size - 8), radius=112, fill=255)
    image.paste(gradient, (0, 0), mask)
    return image


def document(draw: ImageDraw.ImageDraw, box, color, line_color):
    x0, y0, x1, y1 = box
    fold = 46
    draw.rounded_rectangle((x0, y0, x1, y1), radius=18, fill=color)
    draw.polygon(
        [(x1 - fold, y0), (x1, y0 + fold), (x1 - fold, y0 + fold)],
        fill=line_color,
    )
    for index in range(3):
        y = y0 + 96 + index * 46
        draw.rounded_rectangle((x0 + 36, y, x1 - 36, y + 16), radius=8, fill=line_color)


def arrow(draw: ImageDraw.ImageDraw, points, color, width):
    draw.line(points, fill=color, width=width, joint="curve")
    # Arrow head at the end of the line.
    (x0, y0), (x1, y1) = points[-2], points[-1]
    import math

    angle = math.atan2(y1 - y0, x1 - x0)
    length = 38
    spread = 0.55
    left = (
        x1 - length * math.cos(angle - spread),
        y1 - length * math.sin(angle - spread),
    )
    right = (
        x1 - length * math.cos(angle + spread),
        y1 - length * math.sin(angle + spread),
    )
    draw.polygon([(x1, y1), left, right], fill=color)


def main() -> None:
    image = rounded_gradient(SIZE)
    draw = ImageDraw.Draw(image)
    document(draw, (96, 120, 238, 392), (248, 250, 252, 255), (148, 163, 184, 255))
    document(draw, (274, 120, 416, 392), (219, 234, 254, 255), (59, 130, 246, 255))
    accent = (251, 146, 60, 255)
    arrow(draw, [(238, 200), (274, 200)], accent, 16)
    arrow(draw, [(274, 312), (238, 312)], accent, 16)
    image = image.resize((256, 256), Image.LANCZOS)
    image.save(OUT, "PNG")
    print(f"yazıldı: {OUT}")


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Regenerate the README screenshot from the real application window.

The script builds the release binary, starts it with representative demo files
in a temporary English configuration, captures the active window with the
desktop's screenshot tool, and wraps the capture in a macOS-style SVG frame.
Run from the repository root: python3 scripts/make_screenshots.py
"""

from __future__ import annotations

import base64
import io
import os
import shutil
import subprocess
import sys
import time
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
OUT_DIR = ROOT / "docs" / "img"
DEMO = Path("/tmp/donustur-demo")
CONFIG = DEMO / "config" / "donustur" / "config.toml"
DEMO_FILES = [
    DEMO / "cover-photo.png",
    DEMO / "holiday.jpg",
    DEMO / "logo.webp",
]

WIDTH = 1000
HEIGHT = 760
TITLEBAR = 44
RADIUS = 12


def prepare_demo() -> None:
    """Create neutral demo data under /tmp (never a real home directory)."""
    if DEMO.exists():
        shutil.rmtree(DEMO)
    (DEMO / "config" / "donustur").mkdir(parents=True)
    CONFIG.write_text(
        "setup_done = true\n"
        "skipped_tools = []\n"
        'language = "en"\n'
        "language_explicit = true\n",
        encoding="utf-8",
    )
    colors = {
        "cover-photo.png": (59, 130, 246),
        "holiday.jpg": (251, 146, 60),
        "logo.webp": (16, 185, 129),
    }
    for name, color in colors.items():
        image = Image.new("RGB", (640, 400), color)
        draw = ImageDraw.Draw(image)
        draw.rectangle((40, 40, 600, 360), outline=(255, 255, 255), width=6)
        image.save(DEMO / name)


def capture_active_window(destination: Path) -> Image.Image:
    """Capture the focused application window with the available desktop tool."""
    last_error = None
    candidates = []
    if shutil.which("spectacle"):
        candidates.append(
            ["spectacle", "-a", "-b", "-n", "-o", str(destination)]
        )
    if shutil.which("gnome-screenshot"):
        candidates.append(["gnome-screenshot", "-w", "-f", str(destination)])
    if not candidates:
        raise SystemExit(
            "No screenshot tool found. Install spectacle (KDE) or gnome-screenshot."
        )

    for attempt in range(20):
        for command in candidates:
            destination.unlink(missing_ok=True)
            result = subprocess.run(
                command, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
            )
            if result.returncode == 0 and destination.is_file():
                image = Image.open(destination)
                # Reject full-desktop captures; the app window is about 1000x790.
                if 700 <= image.width <= 1400 and 500 <= image.height <= 1000:
                    return image
                last_error = f"unexpected capture size {image.size}"
            else:
                last_error = f"screenshot tool exited with {result.returncode}"
        time.sleep(0.4)
    raise SystemExit(f"Screenshot capture failed: {last_error}")


def crop_window(image: Image.Image) -> Image.Image:
    """Remove the KDE title bar and window borders from the capture."""
    border = max(0, (image.width - WIDTH) // 2)
    titlebar = max(0, image.height - HEIGHT)
    return image.crop(
        (border, titlebar, border + WIDTH, titlebar + HEIGHT)
    ).convert("RGB")


def svg_frame(content: Image.Image) -> str:
    """Wrap the real capture in a macOS-style rounded window."""
    buffer = io.BytesIO()
    content.save(buffer, format="PNG", optimize=True)
    encoded = base64.b64encode(buffer.getvalue()).decode("ascii")
    height = TITLEBAR + HEIGHT
    return f"""<svg xmlns="http://www.w3.org/2000/svg" width="{WIDTH}" height="{height}" viewBox="0 0 {WIDTH} {height}">
  <defs>
    <clipPath id="window">
      <rect width="{WIDTH}" height="{height}" rx="{RADIUS}"/>
    </clipPath>
  </defs>
  <g clip-path="url(#window)">
    <rect width="{WIDTH}" height="{TITLEBAR}" fill="#262b36"/>
    <image x="0" y="{TITLEBAR}" width="{WIDTH}" height="{HEIGHT}" href="data:image/png;base64,{encoded}"/>
  </g>
  <circle cx="22" cy="22" r="6" fill="#ff5f57"/>
  <circle cx="42" cy="22" r="6" fill="#febc2e"/>
  <circle cx="62" cy="22" r="6" fill="#28c840"/>
  <text x="{WIDTH / 2}" y="27" text-anchor="middle" font-family="-apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif" font-size="14" fill="#d7dde8">ConvertAnything</text>
  <rect x="0.5" y="0.5" width="{WIDTH - 1}" height="{height - 1}" rx="{RADIUS}" fill="none" stroke="#3a4150" stroke-width="1"/>
</svg>
"""


def main() -> int:
    prepare_demo()
    subprocess.run(
        ["cargo", "build", "--release"],
        cwd=ROOT,
        check=True,
    )

    env = os.environ.copy()
    env["XDG_CONFIG_HOME"] = str(DEMO / "config")
    env["XDG_DATA_HOME"] = str(DEMO / "data")
    env["LANG"] = "en_US.UTF-8"
    env["LC_ALL"] = "en_US.UTF-8"
    binary = ROOT / "target" / "release" / "donustur"
    process = subprocess.Popen(
        [str(binary), *[str(path) for path in DEMO_FILES]],
        cwd=ROOT,
        env=env,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )
    try:
        time.sleep(2.5)
        raw_path = DEMO / "window.png"
        raw = capture_active_window(raw_path)
        content = crop_window(raw)
        OUT_DIR.mkdir(parents=True, exist_ok=True)
        (OUT_DIR / "app.svg").write_text(svg_frame(content), encoding="utf-8")
        content.save(DEMO / "app-content.png", format="PNG")
        print(f"Wrote {OUT_DIR / 'app.svg'}")
        return 0
    finally:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()


if __name__ == "__main__":
    sys.exit(main())

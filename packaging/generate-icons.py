#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Regenerate checked-in brand assets. Requires Python 3, Pillow and rsvg-convert.

Run from any directory; no network access or build-time generation is needed.
All three SVG variants get PNGs. ICO/ICNS contain the main application icon.
"""

import io
from pathlib import Path
import shutil
import subprocess

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "crates/kinema-desktop/assets/brand"
SIZES = (16, 24, 32, 48, 64, 128, 256, 512)
APP_ID = "dev.tlmtech.kinema"


def render(svg, size):
    return subprocess.check_output(
        ["rsvg-convert", "--width", str(size), "--height", str(size), str(svg)]
    )


def main():
    if not shutil.which("rsvg-convert"):
        raise SystemExit("Install librsvg's rsvg-convert first")
    for suffix in ("", "-player", "-symbolic"):
        name = APP_ID + suffix
        for size in SIZES:
            output = BRAND / "png" / str(size) / f"{name}.png"
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(render(BRAND / f"{name}.svg", size))

    # Pillow embeds PNG-compressed images in ICO and modern PNG ICNS chunks.
    # ICNS needs a 1024px source for Retina slots; keep it in memory only.
    with Image.open(io.BytesIO(render(BRAND / f"{APP_ID}.svg", 1024))) as source:
        source.save(
            BRAND / f"{APP_ID}.ico",
            sizes=[(size, size) for size in SIZES if size <= 256],
        )
        source.save(
            BRAND / f"{APP_ID}.icns",
            sizes=[(size, size) for size in (16, 32, 64, 128, 256, 512, 1024)],
        )


if __name__ == "__main__":
    main()

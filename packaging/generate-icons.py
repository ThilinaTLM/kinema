#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Regenerate checked-in brand assets. Requires Python 3, Pillow and rsvg-convert.

Run from any directory; no network access or build-time generation is needed.
The app and player icons get PNGs at every size; 16px rasters come from the
pixel-hinted `*-16.svg` sources. The symbolic icon is SVG-only because its
colour is supplied by the desktop. ICO uses the Windows/Linux tile; ICNS uses
the macOS-grid source (`*-macos.svg`). See docs/design/brand.md.
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
HINTED_SIZE = 16


def render(svg, size):
    return subprocess.check_output(
        ["rsvg-convert", "--width", str(size), "--height", str(size), str(svg)]
    )


def source(name, size):
    hinted = BRAND / f"{name}-{HINTED_SIZE}.svg"
    if size == HINTED_SIZE and hinted.is_file():
        return hinted
    return BRAND / f"{name}.svg"


def image(svg, size):
    with Image.open(io.BytesIO(render(svg, size))) as png:
        return png.convert("RGBA")


def main():
    if not shutil.which("rsvg-convert"):
        raise SystemExit("Install librsvg's rsvg-convert first")
    for suffix in ("", "-player"):
        name = APP_ID + suffix
        for size in SIZES:
            output = BRAND / "png" / str(size) / f"{name}.png"
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_bytes(render(source(name, size), size))

    # Pillow embeds PNG-compressed images in ICO and ICNS; each slot uses the
    # exact-size raster so the hinted 16px artwork is preserved.
    ico_sizes = [size for size in SIZES if size <= 256]
    ico = [image(source(APP_ID, size), size) for size in ico_sizes]
    ico[-1].save(
        BRAND / f"{APP_ID}.ico",
        sizes=[(size, size) for size in ico_sizes],
        append_images=ico[:-1],
    )
    macos = BRAND / f"{APP_ID}-macos.svg"
    icns_sizes = (16, 32, 64, 128, 256, 512, 1024)
    icns = [image(macos, size) for size in icns_sizes]
    icns[-1].save(BRAND / f"{APP_ID}.icns", append_images=icns[:-1])


if __name__ == "__main__":
    main()

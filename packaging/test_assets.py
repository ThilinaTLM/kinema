#!/usr/bin/env python3
# SPDX-License-Identifier: Apache-2.0
"""Offline packaging contract checks; requires Pillow, no graphical session."""

from pathlib import Path
import configparser
import unittest
import xml.etree.ElementTree as ET

from PIL import Image

ROOT = Path(__file__).resolve().parents[1]
BRAND = ROOT / "crates/kinema-desktop/assets/brand"
APP_ID = "dev.tlmtech.kinema"
SIZES = (16, 24, 32, 48, 64, 128, 256, 512)


class BrandingTests(unittest.TestCase):
    def test_svg_palette_and_license(self):
        for suffix in ("", "-player", "-symbolic"):
            text = (BRAND / f"{APP_ID}{suffix}.svg").read_text()
            ET.fromstring(text)
            self.assertIn("SPDX-License-Identifier: Apache-2.0", text)
            self.assertIn("SPDX-FileCopyrightText:", text)
            self.assertNotIn("#1d99f3", text.lower())
            if suffix == "-symbolic":
                self.assertIn('fill="currentColor"', text)
                self.assertNotIn("#FF7A45", text)
            else:
                self.assertIn('fill="#FF7A45"', text)
                self.assertIn('fill="#ffffff"', text)

    def test_png_dimensions_and_alpha(self):
        for suffix in ("", "-player", "-symbolic"):
            for size in SIZES:
                with Image.open(BRAND / "png" / str(size) / f"{APP_ID}{suffix}.png") as image:
                    self.assertEqual(image.size, (size, size))
                    rgba = image.convert("RGBA")
                    # At 16px the rounded tile's antialiased corner is partial alpha.
                    self.assertLess(rgba.getpixel((0, 0))[3], 128)
                    if not suffix:
                        self.assertEqual(rgba.getpixel((size // 2, size // 10)), (255, 122, 69, 255))

    def test_platform_containers(self):
        with Image.open(BRAND / f"{APP_ID}.ico") as image:
            self.assertEqual(image.format, "ICO")
            self.assertEqual(image.ico.sizes(), {(s, s) for s in SIZES if s <= 256})
            image.load()
        with Image.open(BRAND / f"{APP_ID}.icns") as image:
            self.assertEqual(image.format, "ICNS")
            self.assertEqual(image.size, (1024, 1024))
            image.load()

    def test_linux_identity(self):
        desktop = configparser.ConfigParser(interpolation=None)
        desktop.read(ROOT / "packaging/linux" / f"{APP_ID}.desktop")
        entry = desktop["Desktop Entry"]
        self.assertEqual(entry["Exec"], "kinema-desktop")
        self.assertEqual(entry["Icon"], APP_ID)
        self.assertEqual(entry["StartupWMClass"], APP_ID)
        self.assertNotIn("X-DBUS-ServiceName", entry)
        self.assertEqual(entry["Categories"], "AudioVideo;Video;")
        meta = ET.parse(ROOT / "packaging/linux" / f"{APP_ID}.metainfo.xml").getroot()
        self.assertEqual(meta.findtext("id"), APP_ID)
        self.assertEqual(meta.findtext("launchable"), f"{APP_ID}.desktop")
        self.assertEqual(meta.findtext("provides/binary"), entry["Exec"])
        self.assertTrue((ROOT / "crates/kinema-desktop/src/main.rs").is_file())


if __name__ == "__main__":
    unittest.main()

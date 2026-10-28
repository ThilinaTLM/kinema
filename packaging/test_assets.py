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


ORANGE = (255, 122, 69, 255)
INK = (56, 34, 24, 255)
SVG_SOURCES = ("", "-16", "-player", "-player-16", "-symbolic", "-macos")


def pixel(size, suffix, x, y):
    with Image.open(BRAND / "png" / str(size) / f"{APP_ID}{suffix}.png") as image:
        return image.convert("RGBA").getpixel((x, y))


class BrandingTests(unittest.TestCase):
    def test_svg_palette_and_license(self):
        for suffix in SVG_SOURCES:
            text = (BRAND / f"{APP_ID}{suffix}.svg").read_text()
            root = ET.fromstring(text)
            self.assertIn("SPDX-License-Identifier: Apache-2.0", text)
            self.assertIn("SPDX-FileCopyrightText:", text)
            self.assertNotIn("#1d99f3", text.lower())
            # Clean vector sources: no traced-bitmap transforms or embedded images.
            self.assertNotIn("scale(0.1,-0.1)", text)
            self.assertNotIn("<image", text)
            if suffix.endswith("-16") or suffix == "-symbolic":
                self.assertEqual(root.get("viewBox"), "0 0 16 16")
            else:
                self.assertEqual(root.get("viewBox"), "0 0 1024 1024")
            if suffix == "-symbolic":
                self.assertIn('fill="currentColor"', text)
                self.assertIn('id="current-color-scheme"', text)
                self.assertNotIn("#FF7A45", text)
            elif suffix.startswith("-player"):
                self.assertIn('fill="#382218"', text)
                self.assertIn('fill="#FF7A45"', text)
            else:
                self.assertIn('fill="#FF7A45"', text)
                self.assertIn('fill="#ffffff"', text)

    def test_png_dimensions_and_alpha(self):
        for suffix, tile in (("", ORANGE), ("-player", INK)):
            for size in SIZES:
                with Image.open(BRAND / "png" / str(size) / f"{APP_ID}{suffix}.png") as image:
                    self.assertEqual(image.size, (size, size))
                # Tiles keep a transparent margin and rounded corners.
                self.assertEqual(pixel(size, suffix, 0, size // 2)[3], 0)
                self.assertLess(pixel(size, suffix, size // 16, size // 16)[3], 255)
                self.assertEqual(pixel(size, suffix, size // 2, size // 8), tile)

    def test_hinted_small_rasters_and_symbolic_policy(self):
        # The 16px rasters come from the pixel-hinted sources: the K stem is opaque.
        self.assertEqual(pixel(16, "", 5, 8), (255, 255, 255, 255))
        self.assertEqual(pixel(16, "-player", 5, 8), ORANGE)
        # Symbolic icons are recoloured by the desktop, so no baked PNGs ship.
        self.assertEqual(list((BRAND / "png").glob("*/*-symbolic.png")), [])

    def test_tray_glyphs(self):
        sizes = (16, 18, 20, 22, 24, 32, 36, 40, 44, 48, 64)
        for variant, rgb in (("dark", (0x23, 0x26, 0x29)), ("light", (255, 255, 255))):
            name = f"{APP_ID}-tray-{variant}"
            for suffix, box in (("", "0 0 1024 1024"), ("-16", "0 0 16 16")):
                text = (BRAND / f"{name}{suffix}.svg").read_text()
                self.assertEqual(ET.fromstring(text).get("viewBox"), box)
                self.assertIn("SPDX-License-Identifier: Apache-2.0", text)
                self.assertNotIn("<rect", text)
            for size in sizes:
                with Image.open(BRAND / "tray" / f"{name}-{size}.png") as image:
                    self.assertEqual(image.size, (size, size))
                    rgba = image.convert("RGBA")
                self.assertEqual(rgba.getpixel((0, 0))[3], 0)
                opaque = [p for p in rgba.getdata() if p[3] > 0]
                self.assertTrue(opaque)
                self.assertTrue(all(p[:3] == rgb for p in opaque if p[3] == 255))
                # Silhouette keeps a margin so no platform crops the glyph.
                box = rgba.getchannel("A").getbbox()
                self.assertGreaterEqual(box[0], 1)
                self.assertLessEqual(box[2], size - 1)
                # Hinted stem: its left pixel column is fully opaque mid-height.
                self.assertEqual(rgba.getpixel((box[0], size // 2))[3], 255)

    def test_platform_containers(self):
        with Image.open(BRAND / f"{APP_ID}.ico") as image:
            self.assertEqual(image.format, "ICO")
            self.assertEqual(image.ico.sizes(), {(s, s) for s in SIZES if s <= 256})
            image.load()
            hinted = image.ico.getimage((16, 16)).convert("RGBA")
            self.assertEqual(hinted.getpixel((5, 8)), (255, 255, 255, 255))
        with Image.open(BRAND / f"{APP_ID}.icns") as image:
            self.assertEqual(image.format, "ICNS")
            self.assertEqual(image.size, (1024, 1024))
            image.load()
            rgba = image.convert("RGBA")
            # macOS grid: 824px body inset 100px, so the edge is transparent.
            self.assertEqual(rgba.getpixel((60, 512))[3], 0)
            self.assertEqual(rgba.getpixel((512, 140)), ORANGE)

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

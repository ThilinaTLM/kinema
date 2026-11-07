# Brand

The brand artwork lives in `crates/kinema-desktop/assets/brand/`.
`packaging/generate-icons.py` regenerates it, and `packaging/test_assets.py`
checks it.

## The mark

The mark is a lowercase **k** whose arm and leg merge into a right-pointing
**play** arrow. The stem rises above the arm, so it reads as a letter first and a
play button second. Its construction is relative to the glyph height `H`:

- The stem is `0.228 H` wide. The arm cap starts `0.145 H` below the top.
- The edges of the upper arm run at 45°. The arrow edges run at ±35.5° and meet
  at the tip `(0.825 H, 0.54 H)`. The leg edges run at 56°.
- The corner radius is `0.02 H`. The tip uses 1.6× that radius, and the notches
  use 0.6×.
- On tiles, the glyph is nudged about 1 % to the right for optical centering.

## Colors

| Name | Hex | Use |
| --- | --- | --- |
| Kinema orange | `#FF7A45` | App tile, player glyph and seek progress. It equals the Dark `primary` token |
| Ink | `#382218` | Player tile |
| White | `#FFFFFF` | Glyph on orange, seek knob |
| Symbolic fallback | `#232629` | Used only until a desktop recolors the symbolic icon |

White on orange has a contrast of 2.6:1. That is fine for the logo shape but
**never acceptable for UI text or controls**. In the app, use the
`primary`/`primary_foreground`/`primary_text` tokens from the
[design system](design-system.md#color-themecolorpalette).

## Variants

| Variant | Source | Where |
| --- | --- | --- |
| App | `dev.tlmtech.kinema.svg` (white glyph on an orange tile) | Launcher, main window, Linux hicolor, ICO |
| App 16 px | `dev.tlmtech.kinema-16.svg` (hinted) | 16 px PNG and the ICO slot |
| Player | `dev.tlmtech.kinema-player.svg` (orange glyph on an ink tile, with a seek bar) | Player windows only |
| Player 16 px | `dev.tlmtech.kinema-player-16.svg` | 16 px player PNG |
| Symbolic | `dev.tlmtech.kinema-symbolic.svg` (`currentColor`, no tile) | Panels and monochrome contexts |
| Tray | `dev.tlmtech.kinema-tray-{dark,light}*` and `tray/*.png` (16–64 px) | System tray and menu bar |
| macOS | `dev.tlmtech.kinema-macos.svg` (Apple grid) | ICNS only |

Choosing the tray icon on each platform:

- **macOS:** use the `dark` 18/36 px files as a template image. The system
  tints them.
- **Windows:** follow the taskbar theme (`SystemUsesLightTheme`). A light
  taskbar gets `dark`, and a dark taskbar gets `light`.
- **Linux:** prefer the symbolic icon name where the host recolors it.
  Otherwise use the portal heuristic and keep a manual override.

## Rules

- Keep clear space of at least one-eighth of the mark's width around it. Do not
  rotate, outline, add gradients to or recolor the mark beyond the variants above.
- Use the app tile at 16 px or larger. Below that, use the symbolic glyph.
- **Never use the player variant as the app or launcher icon.**
- Edit the SVG sources only, and keep the `-16` hinted sources in step with
  them. Then run:

```sh
python3 packaging/generate-icons.py   # needs Pillow and rsvg-convert
python3 packaging/test_assets.py
```

The generator rebuilds the tray SVGs and PNGs from the sources, so never edit the
generated files. Generation never runs during a Cargo build. The artwork is
Apache-2.0 like the code, and there is no separate trademark policy.

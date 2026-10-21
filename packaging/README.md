# Desktop integration assets

`packaging/` owns integration source files and offline generation tooling; brand
artwork lives in `crates/kinema-desktop/assets/brand/`. These are not a release
packaging pipeline. The application ID is **dev.tlmtech.kinema**; Cargo's actual
application executable is **kinema-desktop** (the implicit `src/main.rs` binary).
The separate `ui-probe` executable is not installed.

## Regenerate icons

Install Python 3, Pillow (`python3-pil` on Debian/Ubuntu), and librsvg's
`rsvg-convert` (`librsvg2-bin`). Then, from the repository root:

```sh
python3 packaging/generate-icons.py
python3 packaging/test_assets.py
```

The SVGs are the editable sources, copied from the read-only historical C++
artwork and recolored to `#FF7A45`. They retain Thilina Lakshan's copyright and
Apache-2.0 SPDX headers. The application glyph is white; the player surface uses
warm dark accents. The symbolic variant is monochrome `currentColor` with a
`#232629` fallback and no colored tile. PNGs for all three variants are generated
at 16, 24, 32, 48, 64, 128, 256 and 512px. ICO uses sizes through 256px; ICNS also
includes a 1024px Retina image. Generation is explicit, never a Cargo build step.
Different librsvg/Pillow versions may change encoded bytes; this revision was
generated with librsvg 2.63.3 and Pillow 13.0.0.

## Linux staging

Build with `cargo build -p kinema-desktop --release --locked`. For a system prefix
`/usr`, stage these paths in the package root (do not install to the host as part
of tests):

| Source | Installed path |
| --- | --- |
| `target/release/kinema-desktop` | `/usr/bin/kinema-desktop` |
| `packaging/linux/dev.tlmtech.kinema.desktop` | `/usr/share/applications/dev.tlmtech.kinema.desktop` |
| `packaging/linux/dev.tlmtech.kinema.metainfo.xml` | `/usr/share/metainfo/dev.tlmtech.kinema.metainfo.xml` |
| `assets/brand/png/SIZE/dev.tlmtech.kinema.png` | `/usr/share/icons/hicolor/SIZExSIZE/apps/dev.tlmtech.kinema.png` |
| `assets/brand/dev.tlmtech.kinema.svg` | `/usr/share/icons/hicolor/scalable/apps/dev.tlmtech.kinema.svg` |
| `assets/brand/dev.tlmtech.kinema-player.svg` | `/usr/share/icons/hicolor/scalable/apps/dev.tlmtech.kinema-player.svg` |
| `assets/brand/dev.tlmtech.kinema-symbolic.svg` | `/usr/share/icons/hicolor/symbolic/apps/dev.tlmtech.kinema-symbolic.svg` |

Here `assets/brand/` is relative to `crates/kinema-desktop/`; repeat the PNG row
for each generated size. Packaging should update the desktop/icon caches using
its distribution's standard hooks. The launcher accepts no URI placeholders:
there is no implemented URI activation or D-Bus service. GPUI main-window
`app_id` matches the desktop ID (Wayland application ID / X11 WM_CLASS); the main
window also embeds the 256px PNG for GPUI's X11 icon API. Wayland compositors
resolve the installed desktop entry/icon. No notification service or embedded
mpv configuration is shipped: the Rust app launches an external player.

```sh
desktop-file-validate packaging/linux/dev.tlmtech.kinema.desktop
appstreamcli validate --no-net packaging/linux/dev.tlmtech.kinema.metainfo.xml
```

AppStream intentionally omits historical Qt/KDE release records and screenshots;
new Rust release history and accepted screenshots must be supplied when a real
release pipeline is chosen. Validation is not proof of store acceptance.

## Windows and macOS limits

The checked-in `.ico` and `.icns` are ready for a future installer/bundle pipeline.
Windows EXE resource embedding is **not implemented** (no build script or new
build dependency); an installer shortcut alone is not an embedded EXE icon.
macOS bundle creation, Info.plist registration, signing and notarization are
**not implemented**. A standalone `.icns` does not provide macOS app integration.
These assets have not been runtime-tested on Windows or macOS.

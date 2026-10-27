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

The SVGs are the editable sources for the Kinema K/play mark and retain
Thilina Lakshan's copyright and Apache-2.0 SPDX headers.
[Brand guidelines](../docs/design/brand.md) define geometry, colours and usage.

| Source | Used for |
| --- | --- |
| `dev.tlmtech.kinema.svg` | App icon: hicolor scalable, PNGs 24–512px, ICO 24–256px |
| `dev.tlmtech.kinema-16.svg` | Pixel-hinted app icon: 16px PNG and ICO slot |
| `dev.tlmtech.kinema-player.svg` | Player-window icon: hicolor scalable, PNGs 24–512px |
| `dev.tlmtech.kinema-player-16.svg` | Pixel-hinted player icon: 16px PNG |
| `dev.tlmtech.kinema-symbolic.svg` | 16px monochrome symbolic icon; SVG only |
| `dev.tlmtech.kinema-macos.svg` | ICNS only (Apple 824/1024 grid with shadow); not installed on Linux |

PNGs are generated at 16, 24, 32, 48, 64, 128, 256 and 512px. The symbolic
icon has no PNGs because desktops recolour it to match the panel/theme. ICNS
includes a 1024px Retina image. Generation is explicit, never a Cargo build step.
Different librsvg/Pillow versions may change encoded bytes; this revision was
generated with librsvg 2.63.4 and Pillow 13.0.0.

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
| `assets/brand/png/SIZE/dev.tlmtech.kinema-player.png` | `/usr/share/icons/hicolor/SIZExSIZE/apps/dev.tlmtech.kinema-player.png` |
| `assets/brand/dev.tlmtech.kinema-player.svg` | `/usr/share/icons/hicolor/scalable/apps/dev.tlmtech.kinema-player.svg` |
| `assets/brand/dev.tlmtech.kinema-symbolic.svg` | `/usr/share/icons/hicolor/symbolic/apps/dev.tlmtech.kinema-symbolic.svg` |

Here `assets/brand/` is relative to `crates/kinema-desktop/`; repeat the PNG row
for each generated size. Do not install the `-16` or `-macos` SVG sources. Packaging should update the desktop/icon caches using
its distribution's standard hooks. The launcher accepts no URI placeholders:
there is no implemented URI activation or D-Bus service. GPUI main-window
`app_id` matches the desktop ID (Wayland application ID / X11 WM_CLASS); the main
window also embeds the 256px PNG for GPUI's X11 icon API. Embedded player
windows use the player variant (orange mark on the ink tile with a seek bar)
through the same API, while retaining the main application ID. Wayland compositors
resolve the installed desktop entry/icon, so the distinct player icon is not
promised there; Windows/macOS per-window icons remain unqualified. No notification
service or mpv configuration files are shipped. Embedded MPV is the default
player and loads a separately installed libmpv; external mpv/VLC remain explicit
alternatives. See [playback prerequisites](../docs/development.md#embedded-mpv-prerequisites-and-validation-plan).

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

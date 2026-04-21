# Kinema

**Cinema in motion.** A native KDE Plasma 6 desktop app for discovering
movies and TV series, browsing torrent sources via
[Torrentio](https://torrentio.strem.fun/), and — for Real-Debrid
subscribers — streaming straight into **mpv** or **VLC**.

> **Status:** Milestone 3 — one-click Play, KNotifications, player
> auto-detection. Filters UI and full Settings dialog land in M4.

## Features

- Movie **and** TV series search by title or IMDB id (`tt…`)
- Series focus view: season dropdown, episode list with thumbnails,
  torrents table below — draggable splitter, state persisted
- Torrentio stream fetch per movie or episode
- **Real-Debrid** integration
  - Token validated against `/rest/1.0/user`, stored in the system
    keyring (Secret Service / KWallet) — **never on disk in plaintext**
  - `[RD+]` / `[RD]` badges on the torrents table
  - *Cached on Real-Debrid only* filter
  - **Copy / Open direct URL** context actions
- **Play** in mpv (default) or VLC — Enter, double-click, or
  right-click → Play on any cached row
- Sortable torrents table, poster & thumbnail cache (memory + disk)
- First-class Plasma 6 theming, keyboard navigation, KNotifications

## Requirements

- KDE Plasma 6 (or Qt 6.5+ with KDE Frameworks 6)
- C++20 compiler (GCC ≥ 12 / Clang ≥ 17), CMake ≥ 3.22
- `extra-cmake-modules`, `qcoro6`, `qtkeychain` (Qt 6)
- **mpv** or **VLC** on `$PATH` at runtime (soft dependency)

### Install dependencies

**Arch / Manjaro**

```bash
sudo pacman -S --needed base-devel cmake extra-cmake-modules \
    qt6-base qt6-tools qcoro qtkeychain-qt6 \
    kcoreaddons ki18n kio kconfigwidgets knotifications kxmlgui mpv
```

**Fedora 40+**

```bash
sudo dnf install gcc-c++ cmake extra-cmake-modules \
    qt6-qtbase-devel qt6-qttools-devel qcoro-qt6-devel qt6-qtkeychain-devel \
    kf6-kcoreaddons-devel kf6-ki18n-devel kf6-kio-devel \
    kf6-kconfigwidgets-devel kf6-knotifications-devel kf6-kxmlgui-devel mpv
```

**Debian trixie / Ubuntu 24.10+**

```bash
sudo apt install build-essential cmake extra-cmake-modules \
    qt6-base-dev qt6-tools-dev libqcoro6-dev qt6keychain-dev \
    libkf6coreaddons-dev libkf6i18n-dev libkf6kio-dev \
    libkf6configwidgets-dev libkf6notifications-dev libkf6xmlgui-dev mpv
```

> On non-KDE sessions (e.g. GNOME) the keyring backend is the system
> Secret Service (gnome-keyring by default) — encrypted, user-scoped,
> never plaintext on disk.

## Build & run

```bash
cmake -B build -S . -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build build -j$(nproc)
ctest --test-dir build --output-on-failure
./build/bin/kinema
```

Optional system install:

```bash
sudo cmake --install build --prefix /usr
```

Installs the `kinema` binary, `.desktop` entry, AppStream metainfo,
`kinema.notifyrc`, and the scalable app icon.

## Real-Debrid setup

1. Grab your token from <https://real-debrid.com/apitoken>.
2. **Tools → Real-Debrid…** (`Ctrl+Shift+R`), paste, click **Test
   connection** — green status line confirms username and plan.
3. **Save**. Torrents table now shows `[RD+]` / `[RD]` badges and the
   *Cached on Real-Debrid only* checkbox appears.
4. On cached rows: double-click / Enter / right-click → **Play** to
   stream in mpv or VLC. Right-click also offers **Copy / Open direct
   URL**.

Click **Remove token** in the same dialog to revert to magnet-only mode.

## Project layout

```
src/
  app/      — QApplication subclass, KAboutData
  config/   — KConfig-backed preferences singleton
  core/     — HttpClient, Magnet, Player, TokenStore, TorrentioConfig
  api/      — Cinemeta, Torrentio, Real-Debrid clients + JSON parsers
  ui/       — MainWindow, SearchBar, results/detail/series views,
              models & delegates, RealDebridDialog, ImageLoader
data/       — .desktop, AppStream metainfo, notifyrc, icon
tests/      — QtTest unit tests + JSON fixtures
```

See [`TESTING.md`](TESTING.md) for the manual smoke-test checklist.

## License

GPL-2.0-or-later. See [LICENSE](LICENSE).

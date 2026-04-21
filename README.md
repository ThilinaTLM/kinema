# Kinema

**Cinema in motion.** A native KDE Plasma 6 desktop app for discovering
movies and TV series, browsing torrent sources via
[Torrentio](https://torrentio.strem.fun/), and — for Real-Debrid
subscribers — streaming straight into **mpv** or **VLC**.

> **Status:** Milestone 4 — TMDB-powered Discover home, "More like
> this" strip on detail panes, and full Settings dialog (including
> opt-in TMDB token override).

## Features

- **Discover** home screen: TMDB-powered rows for Trending, Popular,
  Now Playing / On The Air, and Top Rated (movies + series), plus a
  "More like this" strip on every detail pane. Click a card and the
  existing Cinemeta + Torrentio pipeline takes over.
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

## Discover (TMDB)

Kinema's Discover surface is backed by [The Movie
Database](https://www.themoviedb.org/). It works out of the box using
a TMDB read-access token bundled at build time — there is **nothing
to configure** for typical use. Click a card in any row to open the
detail pane; the "More like this" strip below the torrents table
surfaces TMDB recommendations for the item you're viewing.

Users who'd rather use their own TMDB account (for privacy, to avoid
sharing the bundled rate-limit pool, or as a fallback if the bundled
token has been revoked) can paste a v4 Read Access Token into
**Settings → TMDB (Discover)**. The token is stored in the system
keyring, never on disk in plaintext.

### For packagers

The bundled token is opt-in at build time via a CMake cache variable:

```bash
cmake -B build -S . -DKINEMA_TMDB_DEFAULT_TOKEN="<your v4 bearer>"
```

Leaving it empty is also supported — in that build Discover shows a
"Set up TMDB" call-to-action until the user provides their own
token. Distros are encouraged to inject their own token rather than
shipping upstream's, so that a revocation upstream doesn't break
every downstream build.

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

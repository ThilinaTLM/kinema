# Kinema

**Cinema in motion.** A native Linux / KDE Plasma 6 desktop app for
discovering movies, browsing torrent sources via
[Torrentio](https://torrentio.strem.fun/), and — for Real-Debrid
subscribers — streaming the file directly in mpv or VLC.

Kinema is a graphical successor to
[`torrentio-cli`](https://github.com/tlm-engineering/torrentio-cli): same
upstream data sources, but with metadata-rich browsing (posters, titles,
descriptions) and one-click playback, without requiring Stremio.

> **Status — Milestone 3 (mpv / VLC playback).** Everything from
> M1 + M2, plus one-click **Play** into mpv (default) or VLC for
> Real-Debrid cached streams, KNotifications on launch / failure,
> and startup detection of available players. Filters UI and full
> Settings dialog are **M4**. See the [PRD](PRD.md).

## Features today (M2)

- Movie **and** TV series search by title or IMDB id (`tt…`)
- Series detail opens in a **focused full-window view** with a
  compact header, a season dropdown + episode list (thumbnails,
  titles, air dates), and the torrents table below — all draggable
  via a splitter, state persisted.
- Torrentio stream fetch per episode (`ttXXXXXX:S:E`)
- **Real-Debrid** token entry, validated against `/rest/1.0/user`
  - Token persisted in the system keyring (Secret Service /
    KWallet / gnome-keyring), **never on disk in plaintext**
  - `[RD+]` / `[RD]` badge icons on the torrents table
  - **Cached on Real-Debrid only** filter checkbox
  - **Copy / Open direct URL** on cached rows
- Poster & thumbnail cache (memory + disk)
- Sortable torrents table (click any column header)
- **Play** directly in mpv (default) or VLC via a detached process
  — Enter / double-click a cached row, or right-click → **Play**
- KNotifications for playback started / failed events
- First-class KDE Plasma 6 theming, keyboard navigation, About dialog

## Coming soon

- Quality / providers / sort UI, full Settings dialog (player picker,
  custom command, default filters), `.torrent` export, Flatpak
  packaging — **M4**

## Requirements

- **KDE Plasma 6** (or at minimum: Qt 6.5+ and KDE Frameworks 6)
- **C++20** compiler (GCC ≥ 12 or Clang ≥ 17)
- **CMake ≥ 3.22**
- **extra-cmake-modules** (ECM)
- **qcoro6** (C++20 coroutines for Qt)

### Install prerequisites

Kinema expects **mpv** (default) or **VLC** on `$PATH` at runtime
for the Play action; both are soft runtime dependencies, not
build-time requirements.

On **Arch Linux / Manjaro**:

```bash
sudo pacman -S --needed \
    base-devel cmake extra-cmake-modules \
    qt6-base qt6-tools \
    kcoreaddons ki18n kio kconfigwidgets knotifications kxmlgui \
    qcoro qtkeychain-qt6 \
    mpv
```

On **Fedora 40+**:

```bash
sudo dnf install \
    gcc-c++ cmake extra-cmake-modules \
    qt6-qtbase-devel qt6-qttools-devel \
    kf6-kcoreaddons-devel kf6-ki18n-devel kf6-kio-devel \
    kf6-kconfigwidgets-devel kf6-knotifications-devel kf6-kxmlgui-devel \
    qcoro-qt6-devel qt6-qtkeychain-devel \
    mpv
```

On **Debian trixie / Ubuntu 24.10+**:

```bash
sudo apt install \
    build-essential cmake extra-cmake-modules \
    qt6-base-dev qt6-tools-dev \
    libkf6coreaddons-dev libkf6i18n-dev libkf6kio-dev \
    libkf6configwidgets-dev libkf6notifications-dev libkf6xmlgui-dev \
    libqcoro6-dev qt6keychain-dev \
    mpv
```

> ℹ️ On non-KDE sessions (e.g. GNOME), Kinema talks to the system's
> **Secret Service** implementation (gnome-keyring by default). The
> token is stored the same way it would be in KWallet on a Plasma
> session — encrypted, user-scoped, never written to disk in plaintext.

## Build & run

```bash
cmake -B build -S . -DCMAKE_BUILD_TYPE=RelWithDebInfo
cmake --build build -j$(nproc)
ctest --test-dir build --output-on-failure

./build/bin/kinema
```

## Install (optional)

```bash
cmake --install build --prefix /usr
```

This installs:

- `kinema` binary to `/usr/bin/`
- `dev.tlmtech.kinema.desktop` to `/usr/share/applications/`
- `dev.tlmtech.kinema.metainfo.xml` to `/usr/share/metainfo/`
- `kinema.notifyrc` to `/usr/share/knotifications6/`
- App icon to `/usr/share/icons/hicolor/scalable/apps/`

## Real-Debrid setup

1. Sign in at <https://real-debrid.com/apitoken> and copy your token.
2. Open Kinema → **Tools → Real-Debrid…** (`Ctrl+Shift+R`).
3. Paste the token, click **Test connection**. A green status line
   shows your username, account type and premium expiry.
4. Click **Save**. The token is written to the system keyring and the
   torrents table starts showing `[RD+]` badges on cached releases
   and `[RD]` badges on uncached ones.
5. Check **Cached on Real-Debrid only** above the torrents table to
   hide non-cached rows.
6. On a cached row, the context menu gains **Copy direct URL** /
   **Open direct URL**. Player handoff to mpv/VLC is coming in M3.

To remove the token, open the dialog again and click **Remove token**.

## Manual smoke test

Run after a clean build. Every step should pass without a crash or
unlocalised user-facing string.

### M1 — movies & magnets

1. `./build/bin/kinema` — window titled *Kinema* opens.
2. Search `The Matrix` → poster grid with matches appears within ~2 s.
3. Click the top card → detail pane populates; torrents table appears
   within ~3 s.
4. Right-click a torrent → **Copy magnet link** → paste into a torrent
   client (e.g. qBittorrent) → the magnet is accepted and starts resolving.
5. Right-click a torrent → **Open magnet link** → the system's default
   handler launches.
6. Type `tt0133093` directly (IMDB-id shortcut) → the Matrix detail
   loads without a search step.
7. Disconnect network, search anything → a clean error state appears
   in the results pane; no crash, no frozen UI.
8. Close and reopen the app, repeat step 2 — poster grid populates
   near-instantly because posters are cached on disk.

### M2 — series & Real-Debrid

 9. Switch the search toggle to **TV Series**, search `Breaking Bad`.
    Poster grid appears within ~2 s.
10. Click the top card. The window switches into a **series focus
    view**: compact header at the top (back button, small poster,
    metadata), then a vertical split of episodes above and torrents
    below. S1E1 *Pilot* is auto-selected and its torrents load within
    ~3 s.
11. At least **4 episode rows** are visible without scrolling on a
    1100 px-tall window. The torrents table spans the full window
    width, with the Release column comfortably wide.
12. Drag the splitter between episodes and torrents; the ratio
    survives an app restart.
13. Pick **Season 2** from the dropdown. Episode list refreshes with
    thumbnails; clicking **E1** loads torrents for `tt0903747:2:1`.
14. Press **Esc** (or click **← Back to results** or **Alt+Left**).
    The grid reappears with the original scroll position and
    selection; the window title returns to *Kinema*.
15. Open **Tools → Real-Debrid…**, paste a real token, click **Test
    connection**. Green status line with your username.
16. **Save**. Run a movie search. Popular titles show `[RD+]` badges
    on at least a few rows.
17. Tick **Cached on Real-Debrid only**. Only `[RD+]` rows remain
    visible; un-ticking restores the full list. No refetch happens.
18. Right-click a cached row → **Copy direct URL** → paste into a
    browser: the file begins to stream from `*.real-debrid.com`.
19. Close + reopen the app. The SearchBar mode is remembered, badges
    still show, the cached-only checkbox state is preserved, and the
    focus-view splitter sits where you left it.
20. Open the dialog, click **Remove token**. Badges disappear, the
    checkbox hides; app reverts to pure magnet-mode.
21. Verify no plaintext token on disk:
    ```bash
    grep -rI "<first 10 chars of your token>" ~/.config ~/.local ~/.cache 2>/dev/null
    ```
    should yield nothing.

### M3 — mpv / VLC playback

22. With Real-Debrid still configured, search for any popular movie
    that has `[RD+]` cached rows. Right-click a cached row → **Play**.
    mpv opens and begins streaming within ~2 s; a desktop
    notification titled *Playing in mpv* appears.
23. **Double-click** a cached row — same behaviour, no menu step.
    Double-clicking an uncached row does nothing (tooltip on the
    disabled Play menu item explains why).
24. Close mpv. The Kinema window is unaffected — no dangling
    child process (`pgrep mpv` returns nothing).
25. Temporarily rename `mpv` out of `$PATH` (e.g.
    `sudo mv /usr/bin/mpv /usr/bin/mpv.bak`) and retry **Play**.
    Kinema falls back to **VLC** if present, otherwise shows a
    *Could not start mpv* notification and a status-bar line
    suggesting to install mpv. Restore mpv afterwards.
26. On a row without a direct URL (plain magnet), the **Play**
    menu item is greyed out and the context menu still offers
    **Copy magnet link** / **Open magnet link**.

## Project layout

```
src/
  main.cpp                 — tiny entry point (Qt + KAboutData CLI)
  app/KinemaApplication    — QApplication subclass; identity + KAboutData
  config/Config            — KConfig-backed singleton (preferences)
  core/HttpClient          — QNAM wrapper returning QCoro::Task
  core/Magnet              — pure magnet-URI builder
  core/Player              — pure player-invocation helpers
  core/PlayerLauncher      — detached-process spawn + KNotification
  core/TorrentioConfig     — pure Torrentio-config-string builder
  core/TokenStore          — QtKeychain wrapper returning QCoro::Task
  api/CinemetaClient       — Cinemeta search + movie/series meta
  api/TorrentioClient      — Torrentio stream list (movies + episodes)
  api/RealDebridClient     — RD /user endpoint for token validation
  api/*Parse               — pure JSON → typed-struct parsers
  ui/MainWindow            — search → results → detail → torrents (coroutines)
  ui/SearchBar             — search input + kind toggle (movies / series)
  ui/ResultsModel          — QAbstractListModel of MetaSummary
  ui/ResultCardDelegate    — poster card painter
  ui/DetailPane            — poster, metadata, torrents (movie preview)
  ui/SeriesFocusView       — full-window series view (header + splitter)
  ui/SeriesPicker          — season combo + episode list
  ui/EpisodesModel         — QAbstractListModel of Episode
  ui/EpisodeDelegate       — episode row painter (thumb + title + date)
  ui/TorrentsModel         — QAbstractTableModel of Stream (with RD icons)
  ui/RealDebridDialog      — token entry + test + save
  ui/ImageLoader           — two-level image cache + QCoro fetch
  ui/StateWidget           — empty/loading/error placeholder
data/                      — .desktop, AppStream metainfo, notifyrc, icon
tests/                     — QtTest unit tests + JSON fixtures
```

## License

GPL-2.0-or-later. See [LICENSE](LICENSE).

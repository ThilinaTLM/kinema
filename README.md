# Kinema

**Cinema in motion.** A native KDE Plasma 6 desktop app for discovering
movies and TV series, browsing torrent sources via
[Torrentio](https://torrentio.strem.fun/), and — for Real-Debrid
subscribers — streaming straight into **mpv** or **VLC**.

## Features

- TMDB-powered Discover home with recommendations on every detail pane
- **Continue Watching** row surfacing in-progress movies and series
- **Resume playback** at the exact timestamp in the embedded player,
  even when you switch to a different release of the same title
- Movie and TV series search (title or IMDB id)
- Series view with seasons, episodes, and per-episode torrents
- Real-Debrid integration with cached-only filter and direct URLs
- Embedded mpv player (optional) or external mpv / VLC / custom command
- System tray with close-to-tray support
- First-class Plasma 6 theming and keyboard navigation

## Requirements

- KDE Plasma 6 (or Qt 6.5+ with KDE Frameworks 6)
- C++20 compiler, CMake ≥ 3.22
- `extra-cmake-modules`, `qcoro6`, `qtkeychain` (Qt 6)
- **libmpv** (optional, for the embedded player) and the KDE
  **mpvqt** library — the embedded player composes its chrome in
  Qt Quick / QML on top of `MpvAbstractItem` from mpvqt. Packages:
  `mpvqt` (Arch / Fedora / openSUSE), `libmpvqt6-dev`
  (Debian / Ubuntu).
- **mpv** or **VLC** at runtime

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

To build without the embedded player, configure with
`-DKINEMA_ENABLE_MPV_EMBED=OFF`.

### Install locally (no sudo)

Helper scripts in `scripts/` build and install Kinema into
`$HOME/.local` by default, refresh the KDE/XDG caches, and make
Kinema appear in the Plasma launcher:

```bash
./scripts/install.sh              # build + install into ~/.local
./scripts/install.sh --test       # run ctest before installing
./scripts/install.sh --prefix /opt/kinema
./scripts/uninstall.sh            # remove what install.sh placed
./scripts/uninstall.sh --dry-run  # preview without deleting
```

Both scripts accept `-h/--help`. Uninstall uses
`build/install_manifest.txt` as the source of truth and leaves user
config (`~/.config/kinemarc`) and keyring tokens untouched.

## Configuration

- **TMDB**: works out of the box when built with a bundled token.
  Users can override it in **Settings → TMDB (Discover)**. Packagers
  can inject one via `-DKINEMA_TMDB_DEFAULT_TOKEN=...`.
- **Real-Debrid**: paste your token from
  <https://real-debrid.com/apitoken> in **Tools → Real-Debrid…**
  (`Ctrl+Shift+R`). Tokens are stored in the system keyring.

## Debugging

Kinema's runtime logs are split across per-subsystem `QLoggingCategory`
names so you can crank up exactly the noise you need:

| Category          | Covers                                                  |
|-------------------|---------------------------------------------------------|
| `kinema.app`      | App shell, controllers, view-models, generic UI        |
| `kinema.torrent`  | libtorrent session, alert pump, per-torrent telemetry  |
| `kinema.download` | Unified download manager, asset sessions, local server |
| `kinema.rd`       | Real-Debrid resolver + availability calls              |
| `kinema.http`     | `HttpClient`, image fetches, HTTP error presenter      |

All categories default to **Info**. Use `QT_LOGGING_RULES` to flip
debug-level lines on for one subsystem at a time, e.g. when chasing
a torrent that won't start:

```bash
QT_LOGGING_RULES='kinema.torrent.debug=true;kinema.download.debug=true' \
    ./scripts/run.sh
```

The torrent engine logs every libtorrent alert it consumes plus a
periodic `peers=N rate=X done=Y` line per active session, which is
usually enough to tell whether the issue is metadata fetching, DHT
bootstrap, tracker reachability, or an empty swarm.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The Apache-2.0 license applies only to the original code and assets in
this repository. Kinema links against third-party libraries (Qt, KDE
Frameworks, mpv, QCoro, QtKeychain, …) that remain under their own
licenses; see `NOTICE` for details.

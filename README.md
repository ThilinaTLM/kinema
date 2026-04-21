# Kinema

**Cinema in motion.** A native KDE Plasma 6 desktop app for discovering
movies and TV series, browsing torrent sources via
[Torrentio](https://torrentio.strem.fun/), and — for Real-Debrid
subscribers — streaming straight into **mpv** or **VLC**.

## Features

- TMDB-powered Discover home with recommendations on every detail pane
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
- **libmpv** (optional, for the embedded player)
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

## Configuration

- **TMDB**: works out of the box when built with a bundled token.
  Users can override it in **Settings → TMDB (Discover)**. Packagers
  can inject one via `-DKINEMA_TMDB_DEFAULT_TOKEN=...`.
- **Real-Debrid**: paste your token from
  <https://real-debrid.com/apitoken> in **Tools → Real-Debrid…**
  (`Ctrl+Shift+R`). Tokens are stored in the system keyring.

## License

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The Apache-2.0 license applies only to the original code and assets in
this repository. Kinema links against third-party libraries (Qt, KDE
Frameworks, mpv, QCoro, QtKeychain, …) that remain under their own
licenses; see `NOTICE` for details.

# AGENTS.md

- Pre-1.0 personal project. Refactor freely and prefer clean rewrites over compatibility shims. KConfig keys (`~/.config/kinemarc`), the `kinema.db` schema, and keyring entries can be reset on disk; no migrations required.

- Layer direction: `api/` → `core/` → `config/` → `controllers/` & `services/` → `ui/`. Lower layers never include upper ones. New flow logic goes in a `controllers/` class; cross-cutting actions in `services/`. Do not grow `src/ui/MainWindow.cpp` — it is the shell.

- `KinemaApplication` owns one `config::AppSettings`. Inject sub-settings (`SearchSettings`, `PlayerSettings`, `FilterSettings`, `BrowseSettings`, `TorrentioSettings`, `AppearanceSettings`, `RealDebridSettings`) through constructors. Do not introduce `Config::instance()` or any global settings singleton. Read `KSharedConfig` only from inside `config/`.

- TMDB and Real-Debrid tokens are owned by `controllers::TokenController` and persisted via `core::TokenStore` (system keyring). Other components hold `const QString&` aliases. Never write tokens to `KSharedConfig`.

- Every controller coroutine guards stale responses with an epoch counter — canonical shape: `controllers::SearchController::runQuery`. Translate thrown errors with `core::describeError(e, "context")` from a single `catch (const std::exception&)`; do not re-implement the `HttpError` / `std::exception` two-branch pattern, and do not silently swallow exceptions in UI code.

- Clipboard / external-launcher / URL-open on an `api::Stream` go through `services::StreamActions`. Widgets call its slots directly; do not add new pane-level signals for action routing.

- Embedded player is gated by `KINEMA_ENABLE_MPV_EMBED` (compile def `KINEMA_HAVE_LIBMPV`). All player-only sources live inside the `if(KINEMA_ENABLE_MPV_EMBED)` blocks in `src/CMakeLists.txt` and `tests/CMakeLists.txt`. Anything outside those blocks must build with the option `OFF`.

- Player chrome is QtQuick / QML inside the `dev.tlmtech.kinema.player` QML module under `src/ui/player/qml/`. The video surface is `ui::player::MpvVideoItem` (subclass of mpvqt's `MpvAbstractItem`). Do not parent QWidgets to it, do not stack pixels over it from Qt, and do not reintroduce mpv Lua overlays or an IPC bridge — that path was deleted on purpose.

- C++ types exposed to QML use `QML_ELEMENT` / `QML_NAMED_ELEMENT` in the header and live in `src/ui/player/`. New QML files must be added explicitly to the `QML_FILES` list in `qt_add_qml_module(...)` in `src/CMakeLists.txt`; QML singletons additionally need `set_source_files_properties(... QT_QML_SINGLETON_TYPE TRUE)` before that call.

- `src/CMakeLists.txt` lists every compilation unit explicitly. Add new sources to the matching section (`config/`, `core/`, `api/`, `ui/`, `ui/widgets/`, `ui/settings/`, `app/`, `controllers/`, `services/`).

- Tests link against `kinema_core` and register via `ecm_add_test(...)` in `tests/CMakeLists.txt`. Add the test name to the bottom `foreach` loop so it picks up `KINEMA_TEST_FIXTURES_DIR`. Fixtures live in `tests/fixtures/`.

- Validate with `cmake --build build -j$(nproc)` then `ctest --test-dir build --output-on-failure`. One-shot configure + build + (optional) test + run: `./scripts/run.sh` (`-t` to add ctest, `-n` to skip launch, `-r` to reconfigure).

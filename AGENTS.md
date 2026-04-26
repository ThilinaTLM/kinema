# AGENTS.md

- Pre-1.0 personal project. Refactor freely and prefer clean rewrites over compatibility shims. KConfig keys (`~/.config/kinemarc`), the `kinema.db` schema, and keyring entries can be reset on disk; no migrations required. The QML resource path `qrc:/qt/qml/dev/tlmtech/kinema/app/` ships every app page; missing entries from `QML_FILES` are runtime errors, not build errors. When adding a QML file, add it to the `qt_add_qml_module(...)` call in `src/CMakeLists.txt` for the `dev.tlmtech.kinema.app` module (the player module is a separate `qt_add_qml_module` under the libmpv block).

- Layer direction: `api/` → `core/` → `config/` → `controllers/` & `services/` → `ui/qml-bridge/` → `ui/qml/`. Lower layers never include upper ones. New flow logic goes in a `controllers/` class; cross-cutting actions in `services/`. The shell is `MainController` (`src/ui/qml-bridge/MainController.cpp`) — it owns long-lived services and exposes them as QML context properties. **Do not put page-specific state in MainController**; add a per-page view-model under `ui/qml-bridge/` instead.

- Top-level window is `Kirigami.ApplicationWindow` loaded from `src/ui/qml/ApplicationShell.qml`. There is no `MainWindow` QWidget. Pages are `Kirigami.ScrollablePage` instances pushed onto `pageStack`. App-level actions (Settings, About, Quit, Search-focus) are signals on `MainController` consumed via a `Connections` block in `ApplicationShell`. Per-page actions are declared in `Kirigami.Action` lists on the page.

- Theming follows `Kirigami.Theme` and `Kirigami.Units` exclusively. No hard-coded pixels, no hard-coded colours, no custom palette. The user's Plasma colour scheme is the source of truth. Two `Theme.qml` singletons exist: app-side (`src/ui/qml/Theme.qml`, Window colorSet) and player-side (`src/ui/player/qml/Theme.qml`, Header colorSet). Pages import the matching one.

- `KinemaApplication` owns one `config::AppSettings`. Inject sub-settings (`SearchSettings`, `PlayerSettings`, `FilterSettings`, `BrowseSettings`, `TorrentioSettings`, `AppearanceSettings`, `RealDebridSettings`) through constructors. Do not introduce `Config::instance()` or any global settings singleton. Read `KSharedConfig` only from inside `config/`.

- View-models bridge controllers to QML. Every page has a VM under `src/ui/qml-bridge/` exposing `Q_PROPERTY` state and `Q_INVOKABLE` slots. VMs forward to controllers; they do not reimplement coroutines or epoch handling.

- TMDB and Real-Debrid tokens are owned by `controllers::TokenController` and persisted via `core::TokenStore` (system keyring). Other components hold `const QString&` aliases. Never write tokens to `KSharedConfig`.

- Every controller coroutine guards stale responses with an epoch counter — canonical shape: `controllers::SearchController::runQuery`. Translate thrown errors with `core::describeError(e, "context")` from a single `catch (const std::exception&)`; do not re-implement the `HttpError` / `std::exception` two-branch pattern, and do not silently swallow exceptions in UI code.

- Clipboard / external-launcher / URL-open on an `api::Stream` go through `services::StreamActions`. QML stream cards call its slots through the detail-page view-models; do not add new pane-level signals for action routing.

- Image loading goes through `image://kinema/<role>?u=<url>` — `ui::qml::KinemaImageProvider` wraps the existing `ui::ImageLoader` cache. Do not bypass the provider with direct `Image { source: <http url> }`; that skips the cache.

- Status messages and inline errors use `Kirigami.PassiveNotification` (transient) or `Kirigami.InlineMessage` (persistent, in-page). There is no `QStatusBar`.

- Embedded player is gated by `KINEMA_ENABLE_MPV_EMBED` (compile def `KINEMA_HAVE_LIBMPV`). All player-only sources live inside the `if(KINEMA_ENABLE_MPV_EMBED)` blocks in `src/CMakeLists.txt` and `tests/CMakeLists.txt`. Anything outside those blocks must build with the option `OFF`.

- Player chrome is QtQuick / QML inside the `dev.tlmtech.kinema.player` QML module under `src/ui/player/qml/`. The video surface is `ui::player::MpvVideoItem` (subclass of mpvqt's `MpvAbstractItem`). Do not parent QWidgets to it, do not stack pixels over it from Qt, and do not reintroduce mpv Lua overlays or an IPC bridge — that path was deleted on purpose.

- C++ types exposed to QML use `QML_ELEMENT` / `QML_NAMED_ELEMENT` in the header and live in `src/ui/player/` for the player module. App UI types usually live in `src/ui/qml-bridge/` and are exposed as context properties or uncreatable model types from `MainController`. QML singletons need `set_source_files_properties(... QT_QML_SINGLETON_TYPE TRUE)` before `qt_add_qml_module(...)`.

- `src/CMakeLists.txt` lists every compilation unit explicitly. Add new sources to the matching section (`config/`, `core/`, `api/`, `ui/`, `ui/qml-bridge/`, `app/`, `controllers/`, `services/`). Add new app QML files to the `dev.tlmtech.kinema.app` `QML_FILES` list.

- Tests link against `kinema_core` and register via `ecm_add_test(...)` in `tests/CMakeLists.txt`. Add the test name to the bottom `foreach` loop so it picks up `KINEMA_TEST_FIXTURES_DIR`. Fixtures live in `tests/fixtures/`.

- Validate with `cmake --build build -j$(nproc)` then `ctest --test-dir build --output-on-failure`. One-shot configure + build + (optional) test + run: `./scripts/run.sh` (`-t` to add ctest, `-n` to skip launch, `-r` to reconfigure).

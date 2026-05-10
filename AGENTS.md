# AGENTS.md

- Pre-1.0 personal project: refactor freely, prefer clean rewrites over compatibility shims, and do not add migrations for `~/.config/kinemarc`, `kinema.db`, or keyring entries.
- `src/CMakeLists.txt` lists sources and QML explicitly. Add new C++ files to `kinema_core_SRCS`; add app QML to the `dev.tlmtech.kinema.app` `QML_FILES`; keep embedded-player sources/QML under `if(KINEMA_ENABLE_MPV_EMBED)`.
- `MainController` is the QML shell owner for long-lived services and context properties only. Put page state and page slots in per-page view-models under `src/ui/qml-bridge/`.
- Store TMDB, Real-Debrid, and OpenSubtitles credentials only through `controllers::TokenController` + `core::TokenStore` (system keyring). Never persist credentials in `KSharedConfig`.
- `config::AppSettings` owns `KSharedConfig`; read/write KConfig only inside `src/config/`. Inject settings objects instead of adding a global config singleton.
- Guard async UI/controller requests with an epoch counter, and translate thrown failures with `core::describeError(e, "context")` from a single `catch (const std::exception&)`.
- Route clipboard, external-player, URL-open, and play actions for `api::Stream` through `services::StreamActions`; QML stream cards call detail-page view-model slots.
- Load remote artwork through `image://kinema/<role>?u=<url>` so `ui::qml::KinemaImageProvider` and `ui::ImageLoader` handle caching; do not bind QML `Image.source` directly to HTTP URLs.
- Keep embedded-player code behind `KINEMA_ENABLE_MPV_EMBED` / `KINEMA_HAVE_LIBMPV`; the player chrome is QtQuick/QML in `dev.tlmtech.kinema.player`, with no QWidget overlays, Lua overlays, or IPC bridge.
- Follow Plasma theming in QML with `Kirigami.Theme` and `Kirigami.Units`; do not add custom palettes or hard-coded colors. Use `/home/tlm/Projects/Kirigami/` as the local Kirigami source reference.
- Add tests with `ecm_add_test(...)` in `tests/CMakeLists.txt` and add the target to the fixture `foreach` at the bottom.
- Validate with `cmake --build build -j$(nproc)` and `ctest --test-dir build --output-on-failure`.
- Downloaded Lucide icons are available in `/home/tlm/Downloads/lucide-icons/icons/`.
- Logging is set up by `core::installLogging()` (called from `KinemaApplication::configure()`): rich pattern with ISO-8601 ms timestamp / severity / category / `file:line in fn` (warn+) and a rotating file at `~/.local/share/kinema/logs/kinema.log` (5 MB × 5, rotated on startup; suppressed under `QStandardPaths::isTestModeEnabled()`). Use the per-subsystem categories declared in `src/CMakeLists.txt`: `KINEMA_APP` (shell/lifecycle), `KINEMA_DB` (Database + `*Store`), `KINEMA_CONTROLLER` (`controllers/*`), `KINEMA_UI` (view-models, image loader, `services/StreamActions`), `KINEMA_PLAYER` (embedded mpv), `KINEMA_API` (`api/*Client`), `KINEMA_HTTP`, `KINEMA_TORRENT`, `KINEMA_DOWNLOAD`. Tune verbosity via `kdebugsettings` or `QT_LOGGING_RULES` (e.g., `kinema.db.debug=true`); `./scripts/run.sh` exports a sensible default unless `-q`/`--quiet` is passed.


# AGENTS.md

Prefer `README.md` for build/run. This file captures conventions an
agent is likely to miss.

## Layering

- Direction: `api/` → `core/` → `config/` → `controllers/`,
  `services/` → `ui/`. Lower layers never include upper ones.
- `MainWindow` is a thin shell. New flow logic goes in a
  `controllers/` class; cross-cutting actions go in `services/`. Do
  not grow `MainWindow.cpp`.
- No QML layer exists. Do not introduce `src/ui/quick/` without a
  plan.

## Settings & singletons

- `KinemaApplication` owns one `config::AppSettings`. Inject the
  relevant sub-settings (`SearchSettings`, `PlayerSettings`,
  `FilterSettings`, `BrowseSettings`, `TorrentioSettings`,
  `AppearanceSettings`, `RealDebridSettings`) through constructors.
- Do not reintroduce `Config::instance()` or any global settings
  singleton. `rg "Config::instance\("` must stay at zero hits.
- Read `KSharedConfig` only from inside `config/`. Other layers go
  through a sub-settings object.
- Existing KConfig groups/keys (`[General]`, `[Filters]`, `[Player]`,
  `[Browse]`, `[RealDebrid]`, `[MainWindow]`) are stable. Add new
  keys to the matching sub-settings class; never rename without a
  migration. Add a `changed(...)` signal only if a consumer reacts
  live.

## Navigation

- Pages are `kinema::controllers::Page`. Use `nav->goTo(Page::X)` /
  `nav->goBack()`.
- Do not infer page state from `QStackedWidget::currentWidget()`
  pointers — ask `NavigationController`.

## Controllers & coroutines

- Every controller coroutine uses the epoch-counter stale-response
  guard. Canonical shape: `SearchController::runQuery`.
- Translate thrown errors with one `catch` calling
  `core::describeError(e, "context")`. Do not re-implement the
  `HttpError` + `std::exception` two-branch pattern.
- Do not silently `catch` `std::exception` in UI code; let the
  controller translate.

## Stream actions, widgets, tokens

- Clipboard / launcher / URL-open on an `api::Stream` go through
  `services::StreamActions`. Widgets call its slots directly; do not
  add new pane-level signals for action routing.
- `PlayerLauncher` takes `PlayerSettings&` via its constructor — no
  global reads.
- Compose detail surfaces from `ui/widgets/` (`MetaHeaderWidget`,
  `StreamsPanel`) instead of duplicating poster/streams plumbing.
- Inject `ImageLoader`, `StreamActions`, and settings sub-objects via
  the constructor. Never reach them via globals or `parent()`.
- TMDB and Real-Debrid tokens are owned by `TokenController`. Other
  components hold `const QString&` aliases — do not cache copies on
  widgets.
- Tokens live in the system keyring via `core::TokenStore`. Never
  write tokens to `KSharedConfig`.

## Strings & literals

- All user-facing strings: `i18nc("@context", "text", args...)`.
  Match existing contexts (`@info:status`, `@action:inmenu`,
  `@title:window`, `@label`, …).
- Non-translated `QString` literals: `QStringLiteral("...")`. Bytes:
  `QByteArrayLiteral(...)`.

## Imports

- Do not include `Types.h` from `src/`. Use the narrower
  `api/Media.h`, `api/Discover.h`, or `api/RealDebrid.h`.
  `rg -l "Types.h" src/` must stay at zero hits.

## Embedded player chrome

- Player chrome is rendered **inside mpv** by the Lua script
  directory `data/kinema/mpv/scripts/kinema-overlays/` (mpv loads
  `main.lua`; siblings are `require`d). It is not a Qt widget tree.
- Do not parent QWidgets to `MpvWidget`. Do not stack pixels over
  the video surface from Qt. `rg PlayerOverlay src/` and
  `rg WA_TransparentForMouseEvents src/` must stay at zero hits.
- There is no `src/ui/player/widgets/` directory.
- New chrome features extend an existing `kinema-overlays/` module
  or add a new one. Each module is small and single-purpose
  (`theme`, `state`, `ass`, `layout`, `render_*`, plus `main.lua`
  as the redraw / IPC dispatcher). Shared geometry lives in
  `layout.lua`; tween / visibility / hit-test helpers live in
  `state.lua`.
- Register every new `.lua` file in `_kinema_mpv_script_files` in
  `data/CMakeLists.txt` so it lands in install, the dev-tree
  mirror, and `lua-syntax-check`.
- C++ ↔ script IPC is `core::MpvIpc`. New player UI requires a
  matching IPC pair (host `set*/show*/hide*` helper +
  Lua `register_script_message` handler, or Lua send +
  `MpvIpc` typed signal) in the same commit.
- Identifiers are wired together: the libmpv handle name is
  `"main"` (no `client-name` option); the script directory
  basename is `"kinema-overlays"`. Renaming the directory means
  updating `kScriptName` in `core/MpvIpc.cpp`, the `KINEMA`
  constant in `main.lua` and renderer modules, and the test suite
  in lockstep.
- Protocol details: `/home/tlm/.pi/plans/mpv-osc-migration.md`.

## Build system

- `src/CMakeLists.txt` lists each compilation unit explicitly. Add
  new sources to the matching section (`core/`, `api/`, `config/`,
  `controllers/`, `services/`, `ui/`, `ui/widgets/`, `ui/settings/`,
  `app/`).
- Tests live in `tests/`, link against `kinema_core`, and register
  via `ecm_add_test(...)` in `tests/CMakeLists.txt`.

## Tests

- Add a unit test for any new pure logic in `core/` or any new
  parser in `api/`.
- Add a controller test for any new controller flow.

## Validation

- Build: `cmake --build build -j$(nproc)`
- Unit tests: `ctest --test-dir build --output-on-failure`
- Run: `./build/bin/kinema`
- Repo-specific grep guards (must stay at zero hits):
  - `rg "Config::instance\("`
  - `rg -l "Types.h" src/`
  - `rg PlayerOverlay src/`
  - `rg WA_TransparentForMouseEvents src/`
- Lua: the CMake `lua-syntax-check` target runs `luac -p` over
  every file in `_kinema_mpv_script_files`; it must exit 0.

# AGENTS.md

Prefer `README.md` for build/run. This file captures conventions an
agent is likely to miss.

## Architecture

- Layers: `api/` → `core/` → `config/` → `controllers/`, `services/` →
  `ui/`. Lower layers never include upper ones.
- The app owns a single `config::AppSettings` (built in
  `KinemaApplication`). Inject the relevant sub-settings
  (`SearchSettings`, `PlayerSettings`, `FilterSettings`,
  `BrowseSettings`, `TorrentioSettings`, `AppearanceSettings`,
  `RealDebridSettings`) into every collaborator.
- Do not add a global singleton or reintroduce `Config::instance()`.
- `MainWindow` is a thin shell. Async flows live in `controllers/`;
  cross-cutting actions live in `services/`. Adding new features means
  extending or adding a controller, not growing `MainWindow`.

## Navigation

- Pages are the `kinema::controllers::Page` enum. `NavigationController`
  owns the back stack; call `nav->goTo(Page::X)` / `nav->goBack()`.
- Do not compare `QStackedWidget::currentWidget()` pointers to decide
  navigation state. Ask the `NavigationController`.

## Coroutines

- Every controller coroutine uses the epoch-counter stale-response
  guard (see `SearchController::runQuery` for the canonical shape).
- Translate thrown errors with `core::describeError(e, "context")`
  inside a single `catch` block. Do not re-implement the
  `HttpError` + `std::exception` two-branch pattern.

## Settings

- KConfig groups/keys already in the schema (`[General]`, `[Filters]`,
  `[Player]`, `[Browse]`, `[RealDebrid]`, `[MainWindow]`) are stable.
  Preserve existing keys; never rename without a migration.
- Each sub-settings class owns exactly one concern. Add new keys to
  the matching class, or add a new sub-settings class if the concern
  is new. Expose a narrow `changed(...)` signal only if consumers
  need to react live.

## Stream actions

- Clipboard/launcher/URL-open operations on an `api::Stream` go through
  `services::StreamActions`. Widgets call its slots directly; do not
  add new pane-level signals for action routing.
- `PlayerLauncher` must not read settings globally. Pass the
  `PlayerSettings` reference through the constructor.

## Widgets

- Reusable detail blocks live in `ui/widgets/` (`MetaHeaderWidget`,
  `StreamsPanel`). New detail surfaces compose these rather than
  duplicating the poster/streams plumbing.
- `ImageLoader`, `StreamActions`, and settings sub-objects are
  injected — never acquired via a global or reached through `parent()`.

## Tokens

- TMDB and Real-Debrid tokens are owned by `TokenController`. Other
  components hold `const QString&` aliases, not copies. Do not cache
  tokens on widgets.
- Token storage is the system keyring via `core::TokenStore`. Never
  write tokens to `KSharedConfig`.

## Strings and i18n

- Use `i18nc(@context, "text", args...)` with an `@context` matching
  the existing convention (`@info:status`, `@action:inmenu`,
  `@title:window`, `@label`, etc.).
- All user-facing strings go through `i18n*`. QString literals are
  `QStringLiteral("...")`. Bytes are `QByteArrayLiteral(...)`.

## Validation

- Build: `cmake --build build -j$(nproc)`.
- Unit tests: `ctest --test-dir build --output-on-failure`.
- Run: `./build/bin/kinema`.
- Add a unit test for any new pure logic in `core/` or parser in
  `api/`. Add a controller test for any new controller flow.
- `rg "Config::instance\("` must return zero hits.
- `rg -l "Types.h"` must return zero hits under `src/` — use the
  narrower `api/Media.h`, `api/Discover.h`, or `api/RealDebrid.h`.

## Build system

- `src/CMakeLists.txt` lists each compilation unit explicitly. New
  source files go into the correct section (`core/`, `api/`,
  `config/`, `controllers/`, `services/`, `ui/`, `ui/widgets/`,
  `ui/settings/`, `app/`).
- Tests live in `tests/` and link against `kinema_core`. Register
  new tests in `tests/CMakeLists.txt` using `ecm_add_test(...)`.

## What not to do

- Do not reach into `KSharedConfig` directly from outside `config/`.
- Do not grow `MainWindow.cpp` with new flow logic; extract a
  controller.
- Do not make UI widgets own network or settings state beyond what
  their constructor signatures express.
- Do not silently catch `std::exception` in UI code; let controllers
  translate via `core::describeError`.
- Do not introduce a QML layer without a plan — there is no
  `src/ui/quick/` tree today.

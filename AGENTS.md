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
- `rg PlayerOverlay src/` and
  `rg WA_TransparentForMouseEvents src/` must return zero hits.
- `luac -p` must exit 0 for every file under
  `data/kinema/mpv/scripts/kinema-overlays/`. The CMake
  `lua-syntax-check` target runs this over the full module list.

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

## Embedded player chrome

The embedded player's chrome is rendered **inside mpv** by the
script **directory** at
`data/kinema/mpv/scripts/kinema-overlays/`. mpv loads `main.lua`
in that directory; sibling modules are `require`d by name. It is
not a Qt widget tree; there are no widgets stacked over
`MpvWidget`, and no `src/ui/player/widgets/` directory.

Chrome modules (keep each one small and single-purpose):

- `main.lua` — redraw dispatcher, mpv observers, IPC handlers,
  key bindings (including mouse wheel / right-click dispatch).
- `theme.lua` — palette, sizing, icon codepoints.
- `state.lua` — shared mutable state (`state`, `props`, `mouse`,
  `visibility`), zones (with optional `on_wheel` / `on_rclick`),
  and hit-test / time / chapter / chrome-visibility helpers.
- `ass.lua` — drawing vocabulary (primitives, composites, button,
  `band` for timeline range fills).
- `render_timeline.lua` — hero timeline (filled block, chapter /
  skip-range bands, scrub tooltip) and the persistent thin
  progress line drawn when chrome is hidden.
- `render_transport.lua` — flat transport backing + control row
  (play / skip / volume cluster / audio / subs / speed /
  fullscreen). Delegates timeline rendering to
  `render_timeline.lua`.
- `render_overlay.lua` — top header (fullscreen and windowed-
  on-proximity), plus popup surfaces (resume, next-episode,
  skip pill, buffering, cheatsheet).
- `render_picker.lua` — audio / subtitle / speed picker as a
  right-side full-height sheet with keyboard navigation.
- `render_pause_indicator.lua` — centre play/pause flash shown
  briefly when the pause property toggles while the control row
  is hidden.

New modules must be added to `_kinema_mpv_script_files` in
`data/CMakeLists.txt` so they land in the install and the dev-tree
mirror, and so the `lua-syntax-check` target covers them.

C++ talks to the script over a typed IPC protocol via
`core::MpvIpc`:

- Host → script: `MpvIpc::set*()` / `show*()` / `hide*()` helpers
  send `script-message-to kinema-overlays <cmd> <args…>` over
  `mpv_command_async`. The target name is the **directory**
  basename, not any file name inside it.
- Script → host: `MpvWidget::onMpvEvents` routes each
  `MPV_EVENT_CLIENT_MESSAGE` into `MpvIpc::deliver`, which emits
  typed signals (`resumeAccepted`, `nextEpisodeAccepted`,
  `skipRequested`, `audioPicked(int)`, `subtitlePicked(int)`,
  `speedPicked(double)`, `closeRequested`,
  `fullscreenToggleRequested`).

Do not parent QWidgets to `MpvWidget`. Do not add pixels on top of
the video surface from the Qt side. When a new player feature
needs UI, extend the appropriate `kinema-overlays/` module (or add
a new one and register it in `data/CMakeLists.txt`) and add a
matching `MpvIpc` message pair (host helper + Lua
`register_script_message` handler, or Lua send + `MpvIpc` signal)
in the same commit.

The primary libmpv handle is always named `"main"` (there is no
`client-name` option). The Lua script targets the host with
`script-message-to main …`; the host targets the script by its
own name — `"kinema-overlays"` (the basename of the **script
directory**). Renaming the directory is a breaking change: update
`kScriptName` in `core/MpvIpc.cpp`, the `KINEMA` constant in
`main.lua` / renderer modules, and the test suite in lockstep.

Protocol details and Lua script layout are documented in the
migration plan: `/home/tlm/.pi/plans/mpv-osc-migration.md`.

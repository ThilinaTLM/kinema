# Development

The Rust workspace composes metadata, preparation and playback services with
the native v3 UI; it is not a fully accepted [MVP](architecture.md).
Run commands from the repository root. Rustup selects **Rust 1.97.1** via
`rust-toolchain.toml`; install rustfmt and Clippy for that toolchain.

<a id="library-and-manual-watch-tracking-validation-pending"></a>

## Library and manual watch tracking — scoped validation

**Source implemented and lead-reviewed; scoped validation passed, not full MVP
acceptance.** V7 has 15 tables, Library paging/watch timestamp indexes and frozen
season identity/episode-number guards. Storage, shared state and native UI are
implemented. Earlier validation counts remain historical evidence for their own
increments. See the
[UI contract](design/library.md) and [v7 schema](catalog-schema.md#personal-library-and-manual-watch-state-proposed-schema-v7).

Use only synthetic temporary/in-memory databases and fixtures, never user DBs or
credentials. Do not reset `data/catalog.sqlite3`: the approved same-DB extension
contains durable membership/watch records; deleting it loses them. No server or
real-provider credentials are required for fixture validation.

Validation contract (the specific final evidence and limits follow):

- Fresh/v6/older-supported migration, rollback/newer-version refusal, constraints,
  read defaults versus failures, restart persistence and cleanup exclusions.
- Independent membership/watch writes, removal/Undo preserving added-at/watch
  state, unsaved-title tracking, canonical graph/episode merges, explicit unwatched
  rows, timestamp ties/clock rollback/overflow and contradictory-ID rollback.
- Local queries beyond a provider's first page, literal search, kind/status before
  pagination, stable ties; unknown/empty completeness, specials/future episodes,
  and atomic confirmed season snapshot boundaries, including stale identity after
  merges/reused IDs and changed episode-number rejection without partial writes.
- FIFO non-droppable accepted mutations, queue-full and commit failure, retry,
  stale reads after commits, metadata refresh and navigation during writes.
- Detail parity from Discover/Library/search/recommendations, child-toggle event
  isolation, keyboard/context-menu access, filtered focus/Undo, and credential-free
  canonical-only local Detail. Unknown personal state must not appear unwatched.

Capture and inspect native Discover, Library, movie and show/episode fixtures at
1280×800 and 960×600, dark/light, keyboard focus and reduced motion. Verify poster
bookmark/SERIES stacking, episode overlays, tooltip/title collisions and toolbar
fit. Follow [ui-probe](design/ui-probe.md) for feature-gated native interaction and
capture tooling. Record actual capture review and missing prerequisites; a fixture
pass does not prove real-provider playback, offline artwork or Windows/macOS support.

### Final lead-reviewed evidence — 2026-10-24

All commands used **Rust 1.97.1**, with no new dependencies. Final root format
check passed after one serialized formatter run; diff check passed. The reviewed
chain below passed again after formatting, including both root and native-feature
checks. Exact checks:

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo clippy -p kinema-desktop --all-targets --features ui-probe-test --locked -- -D warnings
cargo test -p kinema-desktop --features ui-probe-test --locked
git diff --check
node --test scripts/ui-probe/pixels.test.cjs
```

- Root fmt, strict all-targets locked workspace Clippy and diff check: **PASS**.
- Root tests: **546 passed, 12 ignored** (42 core + 458 desktop + 46 mpv passed).
- Desktop strict all-targets `ui-probe-test` Clippy: **PASS**.
- Desktop feature tests: **536 passed, 4 ignored**.
- Pixel helper tests: **4 passed, 0 failed**. An earlier pipeline exit 1 was a
  grep-no-matches result, not a failing test.

Scoped source tests cover real-worker SQLite durability, failed writes,
revisions/identity merges, partial refs, same-query 600-row results, summary-only
local TMDB acquisition and saved-data failure fallback, plus an injected Undo UI
response. These are synthetic/local checks, not authenticated-provider acceptance.

Lead-read Linux native captures, all under `target/ui-probe/lab/`:

- Production Library at 1280×800 dark; `final-prod-library-960-dark` and
  `final-prod-library-960-light`.
- Filtered one-row Library: `f-filtered-1280-light`.
- Empty Library: `prod-library-empty-1280-dark`.
- Focused watched movie: `f-movie-focus-watched-light/native-focused.png`.
- Series: `f-series-1280-dark` and `f-series-1280-light`.
- Series hero membership bookmark keyboard focus (Tab 6):
  `v-series-tab6/native-focused.png`.
- First episode watched-toggle keyboard focus (Tab 11):
  `v-series-tab11/native-focused.png`.
  Both `v-series` captures are 1280×800 dark with reduced motion, personally
  read and approved by the lead.

The captures exercise production widgets with synthetic data, not actual
providers; no user DB or credentials were used. Earlier failed focus captures
are not acceptance evidence; only the successful reviewed focus captures above
are included.

The Linux `scripts/ui-probe/native-verify.cjs` focus path now permits `library`
and `-live` scenes and accepts `--tabs N` (integer 1–40, default 8). This controls
keyboard Tab traversal in the isolated fixture, not a semantic focus guarantee
or a click oracle. Review `native-focused.png`; see the
[probe guide](design/ui-probe.md) for isolation and invocation.

**Limits:** generic/synthetic captures and tests do not establish live TMDB,
debrid or playback acceptance, Windows/macOS behavior or full MVP acceptance.
Tracking is manual only: no history, resume, sync or automatic completion.

## Native prerequisites

The desktop uses direct `gpui` (package `gpui-pre =0.3.9`) and
`gpui_platform`, with in-house widgets. V3 is the only/default UI; no environment
selector is needed. Consult the [upstream GPUI repository](https://github.com/zed-industries/zed/tree/main/crates/gpui)
for platform setup and troubleshooting.

- **macOS:** upstream currently requires macOS 15+ and Xcode Command Line Tools.
- **Windows:** Windows 10+, MSVC Rust, Visual Studio C++ build tools with a Windows
  SDK, and CMake on `PATH`.
- **Linux:** native font, Wayland/X11, WebKit, SSL, compression, and Vulkan libraries
  are required. For Ubuntu 24.04, the CI dependency set is:

```sh
sudo apt-get update
sudo apt-get install -y gcc g++ clang pkg-config libfontconfig-dev libwayland-dev \
  libwebkit2gtk-4.1-dev libxkbcommon-x11-dev libx11-xcb-dev \
  libssl-dev libzstd-dev vulkan-validationlayers libvulkan1
```

Other distributions need equivalent packages. ALSA development headers are only
needed if the optional speech feature is enabled; this scaffold does not enable it.
Running the desktop on Linux requires a graphical Wayland/X11 session and a working
Vulkan driver, not just the Vulkan loader. No KDE SDK is required.

These are upstream build prerequisites, not a Kinema release-support guarantee.
Native CI is configured to check compilation and tests on all three OSes; it does
not establish GUI rendering, secure storage, live-provider behavior, or external
playback support. The commands below are validation instructions, not a report of
checks or runtime verification for the current increment.

## Embedded mpv prerequisites and validation plan

**Source implemented; scoped Linux presentation and root checks reviewed.** Embedded mpv is the
playback default in source, with a separate managed GPUI window; Settings offers
embedded mpv, external mpv, and VLC. The backend import, original-player per-session
replay, close/lease release, retry, and stale-event logic are implemented. Actual
GPUI presentation has scoped Linux evidence; final option hardening and root/native
reruns passed, while broader acceptance remains incomplete. See the
[player contract and validation evidence](design/player.md#validation-evidence-lead-reviewed).

The new `kinema-mpv` backend ports the experiment without modifying it. Desktop
presentation must use the workspace's sole `gpui-pre =0.3.9`, not the experiment's
GPUI 0.2.2 view. Native **libmpv** is a separate runtime requirement for embedded
playback; an installed `mpv` executable alone does not satisfy it. External modes
continue to require their respective executables and argument-list launching.

The backend dynamically loads a trusted, architecture-compatible
shared library rather than link or bundle a download implicitly. Conventional
filenames include Linux `libmpv.so.2`, macOS `libmpv.2.dylib`, and Windows
`mpv-2.dll` / `libmpv-2.dll`; the OS loader must also resolve their codec/runtime
and graphics dependencies. On Debian/Ubuntu the runtime package is commonly
`libmpv2`; package names differ elsewhere. macOS installations must expose the
actual dylib and its dependencies; Windows requires matching DLL architecture
and a trusted dependency search path. Do not assume Unix loader environment
variables work on other platforms. A custom library path executes native code:
never discover libraries from media downloads or untrusted working directories.
Exact Kinema lookup order/path override UI or environment support awaits owner
confirmation; the experiment's configuration is not a published Kinema interface.
Missing libraries, symbols/API mismatch, unavailable render backends, and decoder
failures need redacted actionable errors, not automatic installation or silent
external fallback. Backend option hardening explicitly sets `load-scripts=no`,
`osc=no`, and `osd-level=0`, excluding user-autoloaded scripts and native mpv
chrome. Missing scripting/OSC options in no-engine builds are accepted only as
`OptionNotFound`; other disable failures fail startup. This is not sandboxing or
native crash isolation. Final root/native reruns passed with the added focused
test. Missing-option classification has pure-test coverage only; native no-Lua
builds are not runtime-qualified.

The experiment identifies its Rust source as Apache-2.0; preserve applicable
notices when porting. That does not license libmpv, FFmpeg, codecs, or other native
dependencies under Kinema's license. libmpv builds may have GPL or LGPL obligations
depending on build configuration and linked components. Dynamic loading is not a
licensing exemption. Before redistribution, review the exact native binaries,
source/notice requirements, dependencies, and applicable codec/patent terms.
No bundled-library packaging, signing, notarization, or licensing clearance is
claimed by this increment.

GPUI native prerequisites above still apply. Platform graphics contexts, audio,
decoder availability, DLL/dylib discovery, and Wayland/X11 behavior require native
testing. The experiment's copy/readback pipeline establishes neither zero-copy
nor HDR/wide-gamut output; hardware rendering and hardware decoding are separate
policies. CPU fallback is not a quality or performance guarantee. Compilation or
headless fixture tests cannot establish sustained decoding/rendering or OS support.

Reproducible native probe entry points (generated media, no provider credentials):

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
ffmpeg -f lavfi -i testsrc2=size=1280x720:rate=30 -t 10 -c:v mpeg4 -q:v 5 -an /tmp/kinema-player-test.mp4
target/debug/ui-probe --scene player-native --media /tmp/kinema-player-test.mp4 --smoke-seconds 6
xvfb-run -a -s '-screen 0 1200x760x24' env -u WAYLAND_DISPLAY XDG_SESSION_TYPE=x11 target/debug/ui-probe --scene player --light --smoke-seconds 8
```

`player --light` is a synthetic chrome fixture with disabled backend controls,
not native playback. `player-native` consumes the supplied media. Both accept
`--size WIDTHxHEIGHT` (default 1100x680). Use isolated app data/session resources
for automated interaction. Local ignored [artifact notes](../target/ui-probe/player/README.md)
record the evidence and commands; the files may be absent on another checkout.

Remaining validation plan (scoped results are recorded below):

- Run root fmt, strict locked workspace Clippy, and locked workspace tests below,
  including the imported backend. Keep deterministic
  no-library tests separate from opt-in native playback checks.
- Test missing/incompatible libraries, invalid input, worker/event cancellation,
  stale results, bounded delivery, and teardown; failures must release leases.
- Use temporary app data and synthetic/authorized local media for a native window
  smoke: play/pause, ±10-second and timeline seek, mute/volume, resize, fullscreen,
  keyboard, auto-hide/focus, EOF/replay, and close during initialization/buffering.
  Audio/subtitle selection, More menus, drag-seek preview, and custom pointer
  hiding are design follow-ups, not initial acceptance gates. No user tokens required.
- Exercise loopback managed-media buffering/missing-range seeks, pause intent
  surviving recovery, multiple leases, final-lease cancellation, and quit
  Cancel/confirm. Verify external mpv/VLC and their lifetime policy remain intact.
- Measure frame throughput, UI stalls, memory and teardown on Windows, macOS,
  Linux Wayland and X11. Record observed renderer/decoder, library build, OS,
  graphics/audio environment, command, and evidence limits; redact URLs/secrets.

Historical validation elsewhere in this document predates this increment and
must not be relabelled embedded-player acceptance. Download pause/prefetch
suspension and tracked persistent resume/history remain deferred.

## Run and validate

```sh
cargo run -p kinema-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo check --workspace --locked
cargo clippy -p kinema-desktop --all-targets --features ui-probe-test --locked -- -D warnings
cargo test -p kinema-desktop --features ui-probe-test --locked
```

Run the desktop from the repository root: with `debug_assertions` enabled
(default debug builds), it uses `./data/catalog.sqlite3` relative to the process
working directory. Root `/data/` is gitignored; launching from another directory
uses that directory's `data/` instead.

For the production/native location:

```sh
cargo run -p kinema-desktop --release --locked
```

The default release profile disables `debug_assertions` and uses
`ProjectDirs::from("", "", "kinema").data_local_dir()/catalog.sqlite3`, resolved
through native directory conventions rather than hardcoded HOME/APP_DATA paths.
Custom profiles follow their `debug_assertions` setting. Switching modes uses
separate databases without copying, moving, or deleting existing development or
historical files. This location separation does not prevent the authorized in-place
Rust v3 → v4 schema upgrade on next open with the new build. See [storage layout](storage-layout.md#implemented-catalog-location)
for the authoritative location policy.

For domain-only work without desktop native dependencies:

```sh
cargo clippy -p kinema-core --all-targets --locked -- -D warnings
cargo test -p kinema-core --locked
```

Keep shared dependency versions in the root workspace. Update and review
`Cargo.lock` intentionally when changing dependencies; do not omit `--locked` to
hide a stale lockfile during validation. There is no Rust release-packaging pipeline yet.

## Desktop branding and packaging assets

Root [`packaging/`](../packaging/README.md) contains Linux desktop/AppStream
integration files and the reproducible icon generator. Checked-in SVG, PNG, ICO
and ICNS artwork lives in `crates/kinema-desktop/assets/brand/`. Regenerate with
`python3 packaging/generate-icons.py` (Python 3, Pillow and `rsvg-convert`), then
run `python3 packaging/test_assets.py`. No generation runs during Cargo builds.
The main window uses application ID `dev.tlmtech.kinema` and an embedded X11
icon; the Linux launcher executes `kinema-desktop`. See the packaging README for
staging paths and validation commands. Windows EXE icon embedding and macOS
bundling/signing remain follow-ups; shipping ICO/ICNS files is not a packaging
pipeline or cross-platform runtime verification.

## Logging

Desktop startup installs human-readable stderr diagnostics and non-ANSI daily
files named `kinema.YYYY-MM-DD` under `AppPaths.cache/logs`: `./data/logs/`
relative to the working directory in development, native `cache_dir()/logs/`
in production. At most **7 matching log files** are retained (not a byte quota).
A bounded non-blocking writer keeps file writes off the UI thread; under overload
it may drop messages. Its guard flushes on normal exit; aborts or forced termination
can lose queued messages. Directory/file setup failure warns and leaves stderr
logging enabled; it never prevents startup. Panics are logged and then passed to
the previous panic hook. Panic messages must also be free of secrets.

`RUST_LOG` overrides the default filter (missing/invalid values use the default):

```text
info,kinema_desktop=info,kinema_core=info,wgpu_hal=warn,naga=warn,zbus=warn,reqwest=warn,hyper=warn,hyper_util=warn
```

```sh
RUST_LOG=kinema_desktop=debug cargo run -p kinema-desktop --locked
RUST_LOG=info,kinema_desktop=debug,kinema_core=debug cargo run -p kinema-desktop --locked
```

A target-only filter enables that target and suppresses unmatched dependencies.
There is no `KINEMA_LOG` setting. Startup records version, OS/architecture, build
mode (`debug_assertions`), and the log directory. This foundation does not yet
instrument every service failure.

Never log credentials, bearer tokens, debrid/signed URLs, magnet links, full query
strings, or raw reqwest errors/sources. Use `logging::safe_url` for host-only URL
diagnostics; `safe_url_with_path` is only for paths known to be public and free of
tokens/personal data. Use `http_error_kind` instead of displaying HTTP errors;
DNS/TLS failures are not distinguished from connect/request without stable typed
predicates. IDs (IMDb/TMDB), durations, status codes, counts, sizes and categories
are appropriate. Helpers do not sanitize arbitrary log fields or dependency logs;
review verbose dependency filters and logs before sharing them.

Levels: **error** for bugs/unrecoverable failures, **warn** for failed operations
seen by the user, **info** for lifecycle (startup, worker/session start/stop,
settings saved), **debug** for per-request timing/outcomes, **trace** for noisy
detail. See `crates/kinema-desktop/src/logging.rs` for the helper contract.

## Quickstart and setup

V3 is the sole/default UI. These steps describe current source wiring, not a
completed end-to-end GUI acceptance test. Historical validation totals below
predate the v3 cleanup and do not establish current provider/playback acceptance.

1. For the prior external-player flow, install a native **mpv or VLC** executable
   separately; neither is bundled. For the approved embedded-default increment,
   follow [libmpv prerequisites](#embedded-mpv-prerequisites-and-validation-plan);
   source integration and scoped Linux UI presentation are recorded, while full
   acceptance remains pending. Start
   `cargo run -p kinema-desktop --locked` from the repository root.
2. Open the separate **Settings** window. Defaults are Cinemeta, Torrentio,
   Real-Debrid, embedded mpv, and a **20 GiB** managed-media limit. Player choices
   include external mpv and VLC; changes apply to new sessions, while replay
   retains the original per-session player. Preferences apply
   immediately; wait for the save status and inspect errors. There is no legacy
   Save/Revert bar. Custom player selection requires an absolute executable path,
   not a shell command.
3. Configure account tokens as described below. Credential save/remove/test
   actions are separate from preference changes. Presence means stored, not
   accepted by the service. Test uses the saved token, not unsaved text. Do not
   enter credentials in fixtures or include them in reports/screenshots.
4. Use the **Movies / Series** toolbar and search, then open a poster for details.
   Select season/episode for series before requesting episode streams. Refresh
   explicitly requests the selected provider; there is no automatic metadata TTL
   or fallback. Saved query/page membership is not arbitrary offline full-text search.
5. Inspect a stream from the selected indexer, prepare through the selected debrid
   provider, inspect returned files and explicitly choose when ambiguous. Do not
   assume the largest file is the right episode. Use only authorized media.
6. The main-window overflow menu exposes **Open local file…**; the service can
   serve an existing file in place without metadata/debrid credentials. The removed
   Playback tab is not a navigation route. Player launch does not prove decoding
   or viewing; native chooser/player acceptance remains platform-dependent.
7. Inspect active sessions through the toolbar transfers indicator. Settings
   cleanup is manual and inactive-only; it never deletes original local files.
   Navigation keeps sessions alive. Quit/window close with active work asks for
   confirmation: Kinema's server stops, while the external player is not killed
   and loses access to its stream. There is no background daemon. See
   [session lifetime](architecture.md#agreed-session-lifetime-policy).

### Metadata and debrid credentials

- **Cinemeta:** no token is required. It remains the default metadata provider.
- **TMDB:** use the long **API Read Access Token**, not the short API Key, from
  [TMDB API settings](https://www.themoviedb.org/settings/api). Save and test it in
  Settings, then select TMDB and wait for the preference-save status. Requests use fixed `en-US`.
  TMDB titles must have an IMDb mapping before source lookup; missing mappings
  are shown as unavailable, never guessed. No automatic Cinemeta fallback occurs.
- **Real-Debrid:** obtain your own API token from
  [Real-Debrid](https://real-debrid.com/apitoken), save it in the Real-Debrid section,
  and use the account **Test** action. Select Real-Debrid for acquisition.
- **AllDebrid:** obtain your own API key from
  [AllDebrid API keys](https://alldebrid.com/apikeys/), save it in the AllDebrid
  section, and use the account **Test** action. Select AllDebrid for acquisition.

Torrentio/Peerflix selection is separate from debrid selection. A public source
result is not proof of account eligibility, debrid readiness, or playback.
Provider/account requirements and service terms still apply. Neither successful
credential storage nor a mock test establishes a live paid-provider integration.

### Native secure storage

Tokens are stored through the OS keyring, not `settings.toml`, SQLite, media
manifests, or `./data/`. Windows requires access to its native credential store;
macOS requires an accessible/unlocked Keychain. Linux requires a session D-Bus and
an available, unlocked Secret Service implementation (for example GNOME Keyring);
no KDE dependency is required. A headless shell or locked/missing store may fail.

Missing credentials, denied access, and unavailable storage are distinct states.
Restore/unlock the native store rather than placing tokens in ordinary files.
If initialization failed, restart Kinema after restoring access: the keyring
library can cache its initialization failure for the process. Do not put tokens
in command-line arguments, logs, screenshots, or shared issue reports.
Development/release ordinary-data separation does not imply separate keyring
accounts; do not assume deleting `./data/` deletes credentials.

## Validation status and limits

### Embedded-player increment — scoped validation

Lead-reported source status: backend imported into `kinema-mpv`; embedded default
and three primary player choices; original-player per-session replay; separate
managed window, close/lease release, retry, and stale-event logic implemented.
Control source covers play/pause/EOF reload, ±10-second seek, click-seek timeline,
mute/volume decrement/increment, fullscreen, Escape leaving fullscreen without
closing, keyboard focus, and chrome auto-hide. Download pause/prefetch suspension
remain deferred; minimize/occlusion suppression is not automatic.

Lead-reviewed passes (not rerun by this docs owner):

| Scope | Result |
| --- | --- |
| Backend deterministic, default | 37 passed |
| Backend deterministic, CPU-only | 26 passed |
| Native synthetic backend | `real_mpv_render_and_teardown`, `real_cpu_player_media_lifecycle`, `real_gpu_player_media_lifecycle`, `real_source_native_4k` passed |
| Scoped integration | app 35 passed; settings 13 passed; `ui-probe-test` `embedded_` 4 passed |
| Final root fmt / strict locked Clippy / locked tests | Passed; 477 tests passed (40 core + 400 desktop + 37 backend), 11 ignored |
| Full desktop `ui-probe-test` strict Clippy / tests | Passed; 447 tests passed, 4 ignored |

Native runs used local Linux Intel RPL-S, Mesa 26.2.5, OpenGL 4.6, and
software-decoded synthetic fixtures. Separately, the production GPUI native
Wayland probe consumed **172 GPU frames** from a generated 1280×720 clip into a
physical **1100×680 viewport**, background shutdown `true`, error `false`. Owned
isolated Xvfb playback used CPU fallback (expected DRI3 warnings): **22 frames**,
position **15/30 seconds**, paused/muted `true`, shutdown `true`. Space/M/Right/
Left/Escape and timeline click were exercised, not fullscreen or audio acceptance.
The lead inspected the raw logs and synthetic-Light/real-paused captures in
[ignored player artifacts](../target/ui-probe/player/README.md). Dark video chrome
under Light is intentional. Viewport resolution is deliberate, not a hidden
720p cap.

These establish scoped backend and Linux GPUI presentation evidence, not hardware
decoding, full-provider playback, cross-platform support, or performance claims.
Final backend review added script/OSC/OSD option hardening and a focused test;
all four native backend tests and the native Wayland UI smoke passed on rerun.
The lead directly inspected `target/ui-probe/player/final-validation.log`, ending
`TOTAL_FAILED_COMMANDS:0`. Track selection, More menus, drag-seek preview, and
custom pointer hiding remain design follow-ups.

The older records below are historical and are not embedded-player acceptance.

This matrix distinguishes source inspection, reported subsystem/live-GUI evidence,
and historical passing root checks from unperformed end-to-end acceptance.
The frozen outgoing snapshot is `107317a`; current HEAD is documentation commit
`f1beeb9`. Continuation evidence below is reported by the lead, not independently
rerun by this documentation owner.

### Catalog simplification (schema v6)

Fresh catalogs create twelve normalized tables directly. The v6 upgrade retires
all seven legacy provider-cache tables transactionally, following explicit user
authorization to discard old-only cached results. It does not replay legacy data
over a v5 canonical catalog. Identifier consolidation was reviewed and rejected:
separate tables retain simpler collision-domain and owner constraints.

Migration fixtures use temporary/in-memory databases, not `data/catalog.sqlite3`.
The development database was previously inspected read-only at the user's request;
no actual user-database upgrade is claimed. See the
[v6 policy](catalog-schema.md#legacy-cache-retirement-schema-v6).

Validation from the repository root (Rust 1.97.1):
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo test --workspace --locked` passed: **504 passed, 12 ignored**
(40 core, 418 desktop, 46 mpv). Focused catalog tests passed **92**, with **1 ignored**.
Fixtures cover fresh twelve-table creation, v1/v2/v3 upgrades, populated v5 exact
preservation of every normalized table, v4 facts-column/retirement rollback,
failed final-drop rollback, foreign-key integrity, and reopening. No live-provider,
GUI, or cross-platform migration acceptance is implied.

### Current normalized catalog batch

**Historical v4 evidence, superseded by v6 retirement above.**
Backend-owner evidence reports `catalog.rs` `SCHEMA_VERSION = 4`: twelve normalized
`STRICT` tables plus seven retained legacy tables (nineteen application tables).
Schema creation, conservative backfill, and `user_version` advance are atomic.
Live metadata/indexer paths read and write canonical rows; the rich domain API
includes artwork, ratings, external IDs, seasons/episodes, and provenance. Verified
IMDb mapping graph reconciliation and contradiction rollback are reported covered.
Existing GPUI projections remain narrower and do not render per-field origins;
this is not full target UI acceptance or an implemented GPUI redesign.

Preservation is not complete offline reuse: nonempty legacy Cinemeta exact-query
searches stay in legacy tables without backfill due to shared-field provenance
ambiguity (first canonical query needs refresh/fetch); known legacy detail fields
retain `Detail` provenance quality, while only fetch-resource completeness stays
`Summary` to force richer refetch on demand; unverified source-target mappings
stay legacy/unmapped. See [migration limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits).
No user database or credentials were accessed. The upgrade occurs on the user's
next open with the new build; no actual user-database migration is claimed.

Reported targeted checks: catalog/worker **97 passed**, core **3 passed**, TMDB
**8 passed**, SQL **8 passed** (separate scopes, not a workspace total). Shell
search/provider-detail cancellation fixes are done; **6 shell tests**, **10 state
tests**, and targeted strict Clippy passed, without establishing live GUI acceptance.

#### Intermediate validation history

An intermediate **pre-streaming-fix** root run passed fmt, strict locked workspace
Clippy, and locked tests: **291 passed, 3 ignored** (25 core, 266 desktop).
Actual-client Cinemeta smoke with a temporary v4 database passed browse (48),
search (27), movie and Spider-Man movie details, and series (128 episodes).
The credential-free TMDB probe returned expected HTTP 401 in 5.67 seconds, not
authenticated acceptance. Logs: `/tmp/kinema-final-root-lk7f16/`; the lead inspected
Clippy/test results and live-probe evidence.

**That run was not final:** the lead subsequently found whole-
legacy-table payload `Vec` materialization and authorized a bounded-memory
streaming-backfill correction. It is now **frozen**: single-row keyset batches
bound memory while preserving TMDB dependency order and savepoint rollback.
Four focused regressions passed; the catalog scope passed **63 tests, 1 ignored**;
strict package Clippy/fmt passed. The lead's structural check compared twelve ERD
entity field sets with actual DDL in fresh in-memory SQLite: **zero column
mismatches**. This is structural evidence, not full policy/UI acceptance, and no
user database was accessed. Those intermediate results did not validate later
changes; the final post-correction evidence follows.

#### Final post-correction validation

After bounded-memory streaming backfill and the field-quality fix, root
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, and `git diff --check` **all passed**.
Tests: **295 passed, 3 ignored** (25 core, 270 desktop).

The actual-client public Cinemeta smoke used a temporary v4 database and passed
browse (48), search (27), Matrix details, Spider-Man movie details without
`videos`, and series (128 episodes). The no-auth TMDB transport probe returned
expected HTTP 401 in **5.7815 seconds**; no authenticated acceptance follows.

Logs: `/tmp/kinema-final-root-rerun-aqcCLD/outcomes.txt` and
`{fmt,clippy,test,diff,live-cinemeta,live-tmdb}.log` in that directory. The lead
inspected outcomes, Clippy, test-result lines, and both live logs; this docs owner
also inspected those bounded results, without rerunning Rust checks. No user
database/token access or commits occurred; actual user-database migration is not
claimed.

Post-v4 authenticated-provider/debrid and new GUI acceptance remain unverified;
prior user-reported TMDB/AllDebrid success remains evidence for earlier versions
only. Existing search/detail/sources cancellation wiring was updated, not the
GPUI layout. The source/documentation batch is frozen. The [HTML prototype](design/desktop-wireframes.html)
is frozen with expanded-sidebar, rail, and top-navigation variants and awaits
user review; browser prototype checks do not validate GPUI or backend behavior.

### Prior evidence and remaining acceptance limits

| Area | Evidence / remaining limit |
| --- | --- |
| Linux X11 GUI | Popular scrolling/Load more, Ted Lasso details/episodes, Torrentio sorting/filter controls, clear missing-debrid credential error, quota-zero validation and masked unsaved input exercised; full workflow not accepted |
| Settings, secure tokens, provider selection, quit warning | Source wiring inspected; native keyring actions, TMDB missing-token routing/real flow, actual quit/OS-close, detail cancellation and theme switching still need live acceptance |
| Native file chooser | Frozen-snapshot binary in private Xvfb/D-Bus with temporary cwd: window/sidebar navigation verified; Transfers Open local file opened the chooser and Cancel returned to unchanged playback screen. Lead viewed chooser screenshot; screenshots `/tmp/kinema-ui-accept.dVHRhJ/{initial,settings,chooser-attempt,chooser-cancelled}.png`. No file selection, player launch, token UI actions, or token disclosure; selection/error handling and cross-OS behavior unverified. Isolation used private Xvfb, new D-Bus, redirected HOME/XDG, and temporary cwd, not a sandbox or secure-store interception. The portal chooser initially showed the repository directory, source unknown; underlying secure-store access is not ruled out. Theme toggle not found, so theme runtime remains unverified |
| Loopback local-file serving | Reported actual `MediaService` HEAD, full-body byte equality, and `206` smoke checks passed; revoked lease returned `404`, temporary files cleaned up |
| External mpv | Reported actual `player::launch` smoke used isolated no-config mpv, decoded/rendered a synthetic two-second MP4 to EOF and exited successfully; not a debrid stream |
| External VLC | Intentionally not launched, to avoid disturbing a user's existing instance |
| Managed media | Reported fixture tests cover recovery, HTTP headers, root aliases, and quota; not proof of live paid acquisition or crash safety on every OS |
| Torrentio / Peerflix | Reported actual public fetches returned source candidates; not comprehensive availability/terms/playability verification |
| Cinemeta | Historical client/GUI evidence resolved slow-DNS timeout (10-second connect, 15-second total; no TLS bypass). Uncommitted reviewed fix accepts omitted `videos` only for movies; null/malformed values and missing series videos remain rejected, with regression fixture and roundtrip coverage. Continuation actual-client public smoke passed browse (48), search (27), Matrix, `tt22084616` with no episodes, and series (128 episodes), using a temporary database. User launched the updated app using the Cargo run instructions and reported “Looks okay”; lead inspected `/home/tlm/Projects/nerve/data/storage-4/data/images/image-20261016T172106Z.png`: Spider-Man: Brand New Day details rendered with synopsis, 2026, Action/Adventure, green Fetched status, and last successful fetch 2026-10-16 17:20 UTC, without invalid catalog response. This establishes corrected-title GUI detail rendering only, not browse cancellation/navigation or other provider/GUI flows |
| Shell cancellation | Reviewed continuation cancels browse on navigation/search/detail/refresh while preserving cached values and invalidating generations; missing pages reload on return, replaced detail/source work is cancelled. Four targeted shell tests passed; live GUI behavior remains unverified |
| TMDB | User GUI failure and Settings Test connection screenshots (`image-20261016T172347Z.png`, `image-20261016T172847Z.png` under `/home/tlm/Projects/nerve/data/storage-4/data/images/`) were inspected by the lead: network/timeout messages, not credential rejection. Network agent reproduced system DNS taking 5.016 seconds twice versus prior production reqwest timeout at 5.002 seconds; corrected 10-second connect budget reached unauthenticated HTTP 401 in 5.807 seconds. Shared production/test `tmdb.rs` client builder retains 15-second total timeout, TLS, and no redirects. Deterministic delayed-DNS regression and ignored no-credential live probe added; 8 focused tests and targeted Clippy passed. Final root checks passed (see below); credential-free live probe reached HTTP 401 in 5.598 seconds. User subsequently confirmed “Yes TMDB worked” when asked whether post-restart Settings Test connection and Movies browse succeeded: user-reported authenticated check and browse success only, not details/IMDb mapping or universal provider support. No agent read, accessed, or tested the offered token file, which the user explicitly prohibited reading; no agent-run authenticated acceptance was performed |
| Real-Debrid / AllDebrid | Real-Debrid authenticated behavior remains unverified. User AllDebrid failure screenshot `image-20261016T173825Z.png` (same image directory above), inspected by the lead, shows preparation file listing with a selected 861.5 MiB file, an external player launched for a local session, red “Media provider request failed”, and 0 B downloaded / 4.4 GiB reserved. Reservation is aggregate; a mismatch is not established. User mpv/ffmpeg logs report a stream ending at byte 0 rather than 903319726, retries, then format not recognized: zero delivered bytes, not proof of a codec/file-format cause. At that point no successful paid range playback was established; exact original stream failure cause remains unproven. On 2026-10-17 the user replied “all good” to the requested post-fix restart/reprepare/play retry and whether playback starts/downloaded bytes increase: user-reported AllDebrid playback/download progress success only. No independently observed byte comparison, seek, interrupted acquisition, account-change safety, other provider/indexer, or cross-OS acceptance follows. Agents did not access tokens, user data, or the live stream |
| Media acquisition follow-up | Reviewed fix raises connect timeout to 10 seconds, retaining 20-second total timeout, TLS, and no redirects. Typed redacted errors distinguish `ConnectTimeout`, `RequestTimeout`, `BodyRead`, and `RenewalExhausted`, preserving generic `Upstream` elsewhere. Deterministic delayed-DNS/failure coverage: 24 media tests and targeted strict Clippy passed. Final root checks passed (see below); basic AllDebrid GUI retry subsequently succeeded by user report (see above). Bounded delayed-DNS/error fixtures do not establish live signed-URL or media acceptance. Timeout remains a hypothesis, not a proven cause of the user's stream failure; no credentials accessed |
| TMDB attribution | Original blue-green approved logo and exact notice visually verified in Linux X11 About; not a claim of all-platform rendering |
| Root validation | Historical `107317a`: Rust 1.97.1 fmt, strict locked workspace Clippy, locked tests (244 passed, 2 ignored), and diff check passed; logs `/tmp/kinema-final-validation.m3MRpY/`. Corrected continuation run: root fmt, strict locked workspace Clippy, locked workspace tests (247 passed, 2 ignored: 22 core, 225 desktop), and diff check all passed, as reported by the lead; the lead inspected matching results, Clippy, tests, and live-smoke logs in `/tmp/kinema-validation-corrected.kvGbz0/`. Initial test-only Clippy `unwrap_used` failures were fixed using `?`; initial logs `/tmp/kinema-validation.RohF4U/`. Opt-in public Cinemeta smoke separately passed (1 test). Subsequent TMDB correction: root fmt, strict locked workspace Clippy, locked workspace tests (248 passed, 3 ignored: 22 core, 226 desktop), and diff check all passed. Lead inspected Clippy, test result lines, and transport log in `/tmp/kinema-tmdb-validation-BujUJA/`. Subsequent media correction: root fmt, strict locked workspace Clippy, locked workspace tests (250 passed, 3 ignored: 22 core, 228 desktop), and diff check all passed; logs `/tmp/kinema-media-validation.Il8jwz/`. Lead inspected Clippy and test result lines and separately passed `git diff --check` |
| Local-session / quit / OS-close smoke | Follow-up stopped without a rerun because isolation was not guaranteed. Local-session, quit, and OS-close acceptance remain unverified; no no-op-player fixture result is claimed |
| Full MVP / desktops | Complete UI/provider acceptance and Windows/macOS runtime remain incomplete; passing root checks is not MVP completion |

See [cache/schema](catalog-schema.md), [refresh policy](cache-policy.md), and
[storage](storage-layout.md) for source behavior and remaining proposals.

## TMDB access and attribution

Both metadata providers are required by the [metadata contract](architecture.md#agreed-metadata-provider-contract).
TMDB access uses the user's own API Read Access Token (bearer token), stored in
OS secure storage, never ordinary settings or SQLite. No project API key is
embedded. Obtain access through [TMDB API settings](https://www.themoviedb.org/settings/api)
and review the [API terms](https://www.themoviedb.org/api-terms-of-use).
TMDB describes the API as free for non-commercial use with attribution;
commercial use requires separate licensing. See the [official FAQ](https://developer.themoviedb.org/docs/faq).

The [attribution requirements](https://developer.themoviedb.org/docs/faq#what-are-the-attribution-requirements)
require an approved logo and this exact notice in the application's About/Credits:

> This product uses the TMDB API but is not endorsed or certified by TMDB.

The [official branding page](https://www.themoviedb.org/about/logos-attribution)
provides approved logos. The unmodified blue short SVG is saved at
`crates/kinema-desktop/assets/tmdb-logo.svg`; its source and checksum are recorded
in [NOTICE](../NOTICE). Display must preserve its colors/aspect/orientation and be
less prominent than Kinema branding. The asset is subject to TMDB's branding terms,
not the project's Apache-2.0 license. The logo and exact About notice were visually
checked on Linux X11; this does not establish rendering on other platforms or
live provider/keyring behavior.

## Historical C++ source

[reference/cpp](../reference/cpp/README.md) preserves the previous source, tests,
packaging, and workflows as reference. Its workflows are inactive. Old build commands
must run with `reference/cpp` as the source/working directory, not the Rust root.
Old release automation assumes the former repository layout and is not maintained
as runnable release infrastructure. Generated user files left at the root are not
part of the Rust workspace and are not moved or cleaned by this migration.

## Native v3 lab

The Kit counter/gallery and browser probe have been removed. The offline native
lab remains; see [lab commands and evidence limits](design/ui-probe.md). Old
validation counts above are historical, not acceptance of the current v3 UI.

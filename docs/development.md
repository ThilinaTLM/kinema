# Development

The Rust workspace composes metadata, preparation and playback services with
the native v3 UI; it is not a fully accepted [MVP](architecture.md).
Run commands from the repository root. Rustup selects **Rust 1.97.1** via
`rust-toolchain.toml`; install rustfmt and Clippy for that toolchain.

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

## Quickstart and setup

V3 is the sole/default UI. These steps describe current source wiring, not a
completed end-to-end GUI acceptance test. Historical validation totals below
predate the v3 cleanup and do not establish current provider/playback acceptance.

1. Install a native **mpv or VLC** executable separately; neither is bundled.
   Start `cargo run -p kinema-desktop --locked` from the repository root.
2. Open the separate **Settings** window. Defaults are Cinemeta, Torrentio,
   Real-Debrid, mpv, and a **20 GiB** managed-media limit. Preferences apply
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

This matrix distinguishes source inspection, reported subsystem/live-GUI evidence,
and historical passing root checks from unperformed end-to-end acceptance.
The frozen outgoing snapshot is `107317a`; current HEAD is documentation commit
`f1beeb9`. Continuation evidence below is reported by the lead, not independently
rerun by this documentation owner.

### Current normalized catalog batch

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

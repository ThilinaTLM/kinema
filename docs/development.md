# Development

The Rust workspace implements a first bounded Cinemeta search/query-cache
increment, not the full [MVP](architecture.md).
Run commands from the repository root. Rustup selects **Rust 1.97.1** via
`rust-toolchain.toml`; install rustfmt and Clippy for that toolchain.

## Native prerequisites

Consult the official [GPUI Kit installation guide](https://gpui-kit.com/docs/installation)
for current platform setup and troubleshooting. The scaffold pins GPUI Kit **0.7.3**,
using its GPUI reexports, components, and default assets rather than a second GPUI dependency.

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
separate databases with no migration, moving, or deletion of existing development
or historical files. See [storage layout](storage-layout.md#implemented-catalog-location)
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

These steps describe the current source wiring, not a completed GUI acceptance
test. Final locked root checks passed; complete live UI/provider acceptance remains
incomplete.

1. Install a native **mpv or VLC** executable separately; neither is bundled.
   Start `cargo run -p kinema-desktop --locked` from the repository root.
2. Open **Settings**. Defaults are Cinemeta, Torrentio, Real-Debrid, mpv, and a
   **20 GiB** managed-media limit. Select the metadata provider, indexer, debrid
   provider, and player you intend to use; choose **Save preferences** and wait
   for confirmation. Custom player selection requires an absolute executable path,
   not a shell command. The quota field uses GiB.
3. Configure tokens as described below. Credential **Save token** is separate
   from **Save preferences**. A presence indicator means a token exists in the
   keyring, not that the service accepted it. **Test connection** checks the saved
   token; it ignores unsaved text. Saving clears the input and does not expose the
   saved value. **Remove token** requires **Confirm remove**; **Keep token** cancels.
4. Open **Movies** or **Series**, browse or enter a query and press **Search**.
   Open a title for details; for series, select the season/episode before source
   lookup. **Refresh** requests the selected provider again; there is no automatic
   metadata TTL or provider fallback. Saved query/page membership is reusable;
   this is not arbitrary offline full-text search over every saved title.
5. Choose **Find sources**, inspect a candidate from the selected indexer, then
   prepare it through the selected debrid provider. Inspect the returned file list
   and use **Play selected file**. An ambiguous selection requires a choice; do
   not assume the largest file is the right episode. Only media you are authorized
   to access should be used.
6. Alternatively, **Playback → Open local file…** serves an existing file in place
   through the same loopback server, without metadata or debrid credentials.
   Launching a player is not proof that it decoded media or that you watched it.
7. Release sessions when finished. **Remove unused cached media** performs manual,
   inactive-only cleanup; it never deletes original local files. Navigation keeps
   sessions alive. Quit/window close with active work asks confirmation that the
   server stops; the external player stays open but loses access to its stream.
   There is no background daemon. See [session lifetime](architecture.md#agreed-session-lifetime-policy).

### Metadata and debrid credentials

- **Cinemeta:** no token is required. It remains the default metadata provider.
- **TMDB:** use the long **API Read Access Token**, not the short API Key, from
  [TMDB API settings](https://www.themoviedb.org/settings/api). Save and test it in
  Settings, then select TMDB and save preferences. Requests use fixed `en-US`.
  TMDB titles must have an IMDb mapping before source lookup; missing mappings
  are shown as unavailable, never guessed. No automatic Cinemeta fallback occurs.
- **Real-Debrid:** obtain your own API token from
  [Real-Debrid](https://real-debrid.com/apitoken), save it in the Real-Debrid section,
  and use **Test connection**. Select Real-Debrid for acquisition.
- **AllDebrid:** obtain your own API key from
  [AllDebrid API keys](https://alldebrid.com/apikeys/), save it in the AllDebrid
  section, and use **Test connection**. Select AllDebrid for acquisition.

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
and passing final root checks from unperformed end-to-end acceptance.

| Area | Evidence / remaining limit |
| --- | --- |
| Linux X11 GUI | Popular scrolling/Load more, Ted Lasso details/episodes, Torrentio sorting/filter controls, clear missing-debrid credential error, quota-zero validation and masked unsaved input exercised; full workflow not accepted |
| Settings, secure tokens, provider selection, quit warning | Source wiring inspected; native keyring actions, TMDB missing-token routing/real flow, chooser, actual quit/OS-close, detail cancellation and theme switching still need live acceptance |
| Loopback local-file serving | Reported actual `MediaService` HEAD, full-body byte equality, and `206` smoke checks passed; revoked lease returned `404`, temporary files cleaned up |
| External mpv | Reported actual `player::launch` smoke used isolated no-config mpv, decoded/rendered a synthetic two-second MP4 to EOF and exited successfully; not a debrid stream |
| External VLC | Intentionally not launched, to avoid disturbing a user's existing instance |
| Managed media | Reported fixture tests cover recovery, HTTP headers, root aliases, and quota; not proof of live paid acquisition or crash safety on every OS |
| Torrentio / Peerflix | Reported actual public fetches returned source candidates; not comprehensive availability/terms/playability verification |
| Cinemeta | Actual desktop-client live smoke passed browse/search/movie details/series episodes after raising connect timeout to 10 seconds for measured slow OS DNS; total timeout remains 15 seconds, no TLS bypass. Linux X11 browse and Ted Lasso details worked. **New title-specific bug:** Spider-Man: Brand New Day details return invalid catalog response while its sources work; HTTP 200 movie payload omits required `videos`, causing Serde failure. Identified next-lead fix: movie-only omission handling with regression coverage, not relaxed series validation or the resolved timeout |
| TMDB / Real-Debrid / AllDebrid | Adapters and fixtures exist; no live credentials were available, so real authenticated-provider behavior is unverified |
| TMDB attribution | Original blue-green approved logo and exact notice visually verified in Linux X11 About; not a claim of all-platform rendering |
| Root validation | Final Rust 1.97.1 root fmt check, strict locked workspace Clippy, locked workspace tests, and diff check all passed; logs at `/tmp/kinema-final-validation.m3MRpY/`. Public-network opt-in tests remain ignored |
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

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

## Current desktop behavior and limits

Movies and Series start without a search. Enter a title query and use Search (or
Enter); use Refresh on the active query to request Cinemeta again. Results show
names, IMDb IDs, optional overviews, saved/fetched state, and the last-success time
as UTC Unix seconds. Settings remain a placeholder. No credentials are required
by this Cinemeta search adapter.

The [query-result cache](catalog-schema.md#implemented-first-increment) only
reuses saved membership for an exact query, not arbitrary offline search over all
saved titles. [Refresh policy](cache-policy.md#implemented-first-increment),
[resolved storage](storage-layout.md#implemented-catalog-location), and
[worker lifetime limits](architecture.md#implemented-worker-lifetime-limits)
describe the current source-level behavior. These notes are not a completed
manual smoke test or a claim that GUI/provider behavior has been verified.

## Historical C++ source

[reference/cpp](../reference/cpp/README.md) preserves the previous source, tests,
packaging, and workflows as reference. Its workflows are inactive. Old build commands
must run with `reference/cpp` as the source/working directory, not the Rust root.
Old release automation assumes the former repository layout and is not maintained
as runnable release infrastructure. Generated user files left at the root are not
part of the Rust workspace and are not moved or cleaned by this migration.

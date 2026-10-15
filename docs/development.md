# Development

The Rust workspace is an early scaffold, not the full [MVP](architecture.md).
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
Native CI checks compilation and tests on all three OSes; it does not verify GUI
rendering, secure storage, real providers, or external playback. Local validation
in the scaffold development environment covers Linux only.

## Run and validate

```sh
cargo run -p kinema-desktop --locked
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

For domain-only work without desktop native dependencies:

```sh
cargo clippy -p kinema-core --all-targets --locked -- -D warnings
cargo test -p kinema-core --locked
```

Keep shared dependency versions in the root workspace. Update and review
`Cargo.lock` intentionally when changing dependencies; do not omit `--locked` to
hide a stale lockfile during validation. There is no Rust release-packaging pipeline yet.

## Historical C++ source

[reference/cpp](../reference/cpp/README.md) preserves the previous source, tests,
packaging, and workflows as reference. Its workflows are inactive. Old build commands
must run with `reference/cpp` as the source/working directory, not the Rust root.
Old release automation assumes the former repository layout and is not maintained
as runnable release infrastructure. Generated user files left at the root are not
part of the Rust workspace and are not moved or cleaned by this migration.

# Development

This page covers how the workspace is built, checked and shipped, and why it is
set up that way.

## Toolchain and workspace

- **Rust 1.97.1** is pinned in `rust-toolchain.toml` (with clippy and rustfmt).
  The workspace uses edition 2024 and resolver 3.
- **Shared dependency versions live in the root `[workspace.dependencies]`.**
  Commit intentional `Cargo.lock` changes, and always pass `--locked` so a stale
  lockfile can never hide.
- **`unsafe_code = "forbid"`** applies to every crate except `kinema-mpv`, which
  denies `unsafe_op_in_unsafe_fn` instead and keeps its FFI in one place.
- In the dev profile, dependencies and `kinema-mpv` build at `opt-level = 2`, so
  codec and pixel loops stay usable in debug builds.

### Features (`kinema-desktop`)

| Feature | Purpose |
| --- | --- |
| `gpu-surface` (default) | The only embedded-player renderer. It enables WGPU, the patched GPUI backends and the `gpu-surface` feature of `kinema-mpv` |
| `ui-probe` | Builds the `ui-probe` binary (the offline design lab) |
| `ui-probe-test` | `ui-probe` plus GPUI test support, for interaction tests |
| `rqbit-poc` | Builds only the opt-in `examples/rqbit_poc.rs` torrent experiment. It is never a default |

## Prerequisites

Building follows [upstream GPUI's requirements](https://github.com/zed-industries/zed/tree/main/crates/gpui).

- **Linux** needs a Wayland or X11 session, a working Vulkan *driver* (not just
  the loader), and a session D-Bus with an unlocked Secret Service for
  credentials. KDE is not required. On Ubuntu 24.04:

  ```sh
  sudo apt-get install -y gcc g++ clang pkg-config libfontconfig-dev libwayland-dev \
    libwebkit2gtk-4.1-dev libxkbcommon-x11-dev libx11-xcb-dev libssl-dev libzstd-dev \
    vulkan-validationlayers libvulkan1 libmpv2 ffmpeg
  ```

- **macOS** needs version 15+, the Xcode Command Line Tools and an unlocked
  Keychain. Install mpv with `brew install mpv`, and point `MPV_LIBRARY_PATH` at
  the dylib if the loader can't find it.
- **Windows** needs Windows 10+, MSVC Rust, the VS C++ build tools with a
  Windows SDK, and CMake on `PATH`. Supply `libmpv-2.dll` through
  `MPV_LIBRARY_PATH` or the loader, and put ANGLE `libEGL.dll` and
  `libGLESv2.dll` **next to the executable**.

Embedded playback requires libmpv (client API 2). An `mpv` executable alone is
not enough. See [playback](playback.md#native-libmpv).

## Everyday commands

Run these from the repository root:

```sh
cargo run -p kinema-desktop --locked                 # debug build, data in ./data/
cargo run -p kinema-desktop --release --locked       # native app directories

# Required before merging
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked

# UI interaction tests and the lab binary
cargo clippy -p kinema-desktop --all-targets --features ui-probe-test --locked -- -D warnings
cargo test -p kinema-desktop --features ui-probe-test --locked

# Design-token / layer scan only
cargo test -p kinema-desktop --test design_tokens --locked

# Domain-only work (no native deps)
cargo clippy -p kinema-core --all-targets --locked -- -D warnings
cargo test -p kinema-core --locked
```

### Testing rules

- **Add focused tests for domain rules and failure paths.** Pure policies are
  tested in `kinema-core`. Adapters are tested against loopback HTTP fixtures
  and synthetic databases.
- **Never touch user data in tests.** Do not use `data/catalog.sqlite3`, real
  settings, the keyring or provider accounts.
- **Ignored native tests** (real libmpv and GPU) are opt-in. Run them one at a
  time (`--exact --test-threads=1`), and never alongside GUI work.
- Passing compilation, unit tests or fixtures **does not prove** OS, provider or
  playback support. Do not record it as if it did.

## Environment variables

| Variable | Effect |
| --- | --- |
| `RUST_LOG` | Overrides the log filter. For example, `RUST_LOG=kinema_desktop=debug`. It is the only log setting |
| `MPV_LIBRARY_PATH` | Full path to the libmpv shared library |
| `KINEMA_PLAYER_DIAGNOSTICS=1` | Opt-in acquisition and render cadence summaries (target `kinema_desktop::media_performance`). Off by default because it costs extra timers and log volume. Diagnostic counters for "accepted image" FPS count UI-accepted frames, not displayed frames |

Logs go to stderr and to `cache/logs/kinema.YYYY-MM-DD`, keeping 7 files
(`./data/logs/` in development). The privacy rules are in
[providers](providers.md#logging-and-privacy).

## UI probe and design lab

The `ui-probe` binary is the offline native lab. It uses synthetic in-memory
state, opens no production services, credentials or user database, and never
starts a real player.

```sh
cargo run -p kinema-desktop --bin ui-probe --features ui-probe --locked -- --lab --scene browse
# Options: --light, --reduce-motion, --size WxH
```

The scenes include `components`, `browse`, `menus`, `sheet`, `detail`,
`series`, `series-streams`, `states`, `tokens` and `motion`, plus `detail-live-*`
fixtures, `player-subtitles` (drop hint and subtitle notice held on screen),
and `player`/`player-native` (real libmpv on supplied media,
`--smoke-seconds` 1–15). The full list is in `design_lab.rs` and
`bin/ui-probe.rs`. **Add a scene or row for every new component or visual
token.**

On Linux, `scripts/ui-probe/native-verify.cjs` captures scenes in a private Xvfb.
It refuses to use `:0` and isolates HOME, XDG and D-Bus. Captures under
`target/ui-probe/` are candidates for review, not baselines. Never capture the
user's desktop. A visual-baseline pipeline is a
[proposal](../proposals/visual-baselines.md).

## Vendored patches

GPUI 0.3.9 cannot present externally produced GPU textures, cannot hide
individual windows and does not expose key repeat. Kinema therefore owns
**reproducible patches** for `gpui-pre`, `gpui-pre-wgpu`, `gpui-pre-apple`,
`gpui-pre-windows` and `wgpu-hal`. They are applied through root
`[patch.crates-io]` to sources in `vendor/`.

| Patch | Adds |
| --- | --- |
| 0001–0005 | GPU-surface ownership and presentation, shared WGPU context and queue gate, Windows HWND WGPU adapter, Apple surface path, Vulkan external-semaphore extensions |
| 0010–0011 | `Window::{hide_window, show_window}` for close-to-background |
| 0012 | `KeystrokeEvent::is_held` (native key-repeat state) |

`vendor/souvlaki` is a separate media-controls fork, documented in
`vendor/souvlaki/KINEMA-PATCH.md`.

Rules:

- **MUST: never edit registry sources,** and never require a wrapper workspace.
  `cargo run -p kinema-desktop --locked` must work as is. *Why:* edits to the
  registry cache are invisible to other machines and CI, so builds stop being
  reproducible.
- Verify with `python3 -B vendor/verify.py`. When you change a patch, review the
  patch hashes, the patched sources, the source inventory and the lockfile
  together. Never update inventories just to silence a failure.
- The API and synchronization constraints are in
  [`vendor/patches/README.md`](../../vendor/patches/README.md). Read them before
  touching GPU-surface code.

## CI

`.github/workflows/ci.yml` runs on pushes to `main` and on PRs targeting `main`.
Obsolete PR runs are cancelled.

| Job | Checks |
| --- | --- |
| `assets` | `packaging/test_assets.py`, pixel-tool tests, desktop-file and AppStream validation |
| `fmt` | `cargo fmt --check` |
| `native` (Ubuntu 24.04, macOS 15, Windows 2025) | Strict clippy, workspace tests, one ignored libmpv smoke test (CPU render only), and `ui-probe-test` clippy and lib tests |

CI installs libmpv and FFmpeg on each OS. Windows downloads are pinned and
checked with SHA-256. Compilation is cached in two layers, `rust-cache` and
`sccache`, and CI disables incremental builds and debug symbols. To reproduce
a failure locally, unset those variables.
**Hosted CI does not qualify GUI rendering, GPU-surface output, HDR, the
keyring or providers.** Run `actionlint` after you edit the workflow.

## Platform status

| Area | Linux | Windows | macOS |
| --- | --- | --- | --- |
| Build + unit tests (CI) | ✓ | ✓ | ✓ |
| Hands-on GUI use | ✓ (Wayland/X11) | — | — |
| Embedded GPU-surface playback | Scoped SDR tests on Intel/Mesa | Compiles only | Compiles only |
| Keyring, tray, media keys, inhibition | Scoped | Unqualified | Unqualified |
| HDR | Rejected by design | Rejected | Rejected |

> **Known debt:** on non-Unix systems, settings writes don't sync the parent
> directory, so a crash right after saving can lose the change. Fix:
> [cross-platform qualification](../proposals/cross-platform-qualification.md).

Other known gaps: upstream `LinuxPlatform::activate` does nothing (window-level
activation does the real work). Wayland can't hide the main window. See
[cross-platform qualification](../proposals/cross-platform-qualification.md).

## Packaging

`packaging/` holds the Linux `.desktop` and AppStream metadata (app ID
`dev.tlmtech.kinema`, binary `kinema-desktop`) and the icon generator (see
[brand](brand.md)). There is **no release pipeline** yet: no Windows installer
or embedded EXE icon, no macOS bundle, signing or notarization, and no AppImage.
See [desktop releases](../proposals/desktop-releases.md).

## Change risk

- **High: vendored patches.** Review the patch hashes, sources, inventory and
  lockfile together.
- **Medium: toolchain or dependency bumps.** Run every required check on all
  three OSes in CI.
- **Low: CI caching and lab tooling.**

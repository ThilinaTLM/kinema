# Cross-platform qualification

No OS is claimed release-qualified by this matrix. **2026-10-28 local Linux
baseline:** root `cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`, and
`cargo test --workspace --locked` passed with Rust 1.97.1. These checks do not
exercise ignored native tests or establish runtime support.

[CI configuration](../../.github/workflows/ci.yml) targets Ubuntu 24.04, macOS 15
and Windows 2025 for workspace compilation via Clippy, tests and GPUI fixture checks. This
is configured coverage, **not evidence that those jobs executed or passed**.
Build prerequisites are in [development](../development.md#native-prerequisites).

## Native libmpv CI smoke

Each native job installs libmpv and FFmpeg and runs this exact, otherwise ignored
test in isolation (with the default `gpu-surface` feature still compiled):

```sh
cargo test -p kinema-mpv --locked mpv::player::tests::real_mpv_render_and_teardown -- --ignored --exact --nocapture --test-threads=1
```

It generates synthetic local video and exercises native library loading, decoding,
CPU-rendered RGBA frames, controls and teardown. Windows playback dependencies are
version-pinned and SHA-256 verified; they are test inputs, not application bundling.
This test does **not** exercise the default GPU surface, hardware decoding or HDR.
Only actual successful CI runs establish this limited native runtime evidence.


Additional checks executed locally on 2026-10-28:
`cargo check -p kinema-mpv --all-targets --target x86_64-pc-windows-msvc --locked`
and the same command with `--target aarch64-apple-darwin` both **passed**.
These are backend cross-compilation checks only, not linking, workspace checks or
native runtime evidence. Full workspace checks using
`cargo check --workspace --all-targets --target <target> --locked` were attempted
for both targets but **blocked by this Linux host's missing target C toolchains**:
Windows dependency builds lacked MSVC tools (`lib.exe`); macOS dependency builds
used a host compiler that rejects `-arch`/`-mmacosx-version-min`. These are not
completed workspace checks or evidence of a native platform source defect.

| Platform | Evidence / qualification | Runtime prerequisites and open checks | Packaging gaps |
| --- | --- | --- | --- |
| Linux | Local baseline above; [prior scoped native probes](../development.md#experimental-gpu-player-integration--validation-pending) only, not general qualification. | Wayland/X11 session, working Vulkan and EGL/OpenGL drivers, libmpv; session D-Bus + unlocked Secret Service. Repeat native GUI, storage and player checks below on each target environment. | Integration assets and staging instructions only; no release packaging pipeline or validated dependency delivery/install/upgrade. |
| macOS | No native runtime qualification; configured CI is not executed evidence. | macOS 15+ build prerequisite; actual Metal/CGL device, trusted libmpv and unlocked Keychain. Native GUI, storage and player checks remain open. | No app bundle/Info.plist integration, signing or notarization; standalone ICNS is not app integration. |
| Windows | No native runtime qualification; configured CI is not executed evidence. | Windows 10+ build prerequisite, MSVC tools/SDK/CMake; actual DX12 presentation device, compatible D3D11-on-12/ANGLE, trusted libmpv and native credential store. Native GUI, storage and player checks remain open. | No EXE icon-resource embedding or installer pipeline; ANGLE/libmpv delivery and licensing/provenance review remain unresolved. |

## Default Windows GPU-surface prerequisites

Verified against [`gpu_surface/windows.rs`](../../crates/kinema-mpv/src/mpv/gpu_surface/windows.rs):

- Supply trusted, architecture-matching **`libEGL.dll` and `libGLESv2.dll` beside
  the actual running executable**, not merely in the working directory or on
  `PATH`. Default `cargo run -p kinema-desktop --locked` runs from `target/debug/`;
  Cargo test executables run from `target/debug/deps/`. Custom target/profile
  directories change these locations; use the actual executable's parent.
- The loader resolves both DLLs using `current_exe()` and explicit adjacent paths,
  with `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`
  for dependency resolution. Kinema does not download or bundle ANGLE
  automatically. No distribution source is prescribed or qualified here.
- A generic ANGLE DLL pair is not sufficient: the build must expose
  `EGL_ANGLE_device_creation`, `EGL_ANGLE_device_creation_d3d11`,
  `EGL_EXT_platform_device`, `EGL_EXT_device_query`,
  `EGL_ANGLE_d3d_texture_client_buffer` and device extension
  `EGL_ANGLE_device_d3d`, plus the entry points checked by the loader.
  ANGLE must retain the supplied D3D11-on-12 device (COM identity is checked).
  The host must also provide D3D11-on-12 video interfaces and a usable NV12
  decoder profile; these checks alone do not prove successful direct decoding.
- `Config.egl_library_path` / `Config.gles_library_path` select libraries only
  for the older EGL **readback** path (`platform/egl.rs`), not this default
  GPU-surface loader. They cannot redirect the adjacent-DLL requirement.

Embedded MPV uses the sole/default `gpu-surface` path and requires a trusted,
architecture-matching libmpv client API major 2 and its dependencies separately;
an external mpv executable is insufficient. Desktop has no CPU/readback fallback.
GPU-surface playback requires direct hardware decoding and qualified SDR metadata;
HDR and native Windows/macOS playback remain unqualified. External mpv/VLC are
unchanged. See [backend prerequisites](../../crates/kinema-mpv/README.md#runtime-prerequisites).

## Open qualification checklist (each OS)

Use synthetic/local media and isolated temporary ordinary data; never reset a
user database or disclose credentials. Record OS/build, architecture, GPU/driver,
libmpv/ANGLE versions and provenance, exact command, observed result and remaining
limits. Do not treat a skipped/ignored test or an unavailable service as a pass.

- **Native GUI:** launch the default root build in a real graphical session;
  also test launch outside the repository and from read-only install directories;
  inspect rendering, keyboard focus, resize/DPI, light/dark, main/player window
  lifecycle and teardown. Fixture tests are not native pixel acceptance.
- **Secure storage:** with a disposable test account, verify save/read/restart/
  delete, missing credentials, denied/locked/unavailable store and recovery.
  Confirm no secrets in settings, SQLite, manifests or logs; see
  [storage prerequisites](../development.md#native-secure-storage).
- **Players:** list ignored tests with
  `cargo test -p kinema-mpv --locked -- --list --ignored`, then run selected native
  tests explicitly with their prerequisites. Separately exercise default desktop
  GPU-surface SDR playback, actual direct-decode evidence, audio, seek/pause,
  resize, EOF/errors and teardown; check missing/incompatible libraries and
  unsupported metadata fail actionably. Older readback tests do not qualify
  surface presentation. Test external mpv and VLC launch/arguments, including
  paths with spaces and missing executables. Live-provider acceptance is separate.
- **Media storage:** verify loopback-streamed media, interruption/recovery and
  cache cleanup while readers are active. Managed cleanup must never delete
  original source files.
- **Settings durability:** `settings.rs::sync_parent` performs directory
  `sync_all()` on Unix but is a successful no-op on non-Unix (including Windows).
  Do not claim Windows crash-durable settings replacement; native durability
  validation and a Windows directory-sync strategy remain open. No behavior
  change or forced error is implied here.
- **macOS player discovery:** source now includes MacPorts `/opt/local/bin` and
  the user's `~/Applications` bundles alongside existing candidates; verify real
  discovery/launch on macOS. Candidate generation tests are not runtime evidence.
- **Distribution:** choose and validate dependency provenance/licensing, clean-host
  install/launch, upgrade/uninstall, OS integration and signing where applicable.
  No packaging pipeline is implemented by this document. See
  [assets, staging and current limits](../../packaging/README.md).

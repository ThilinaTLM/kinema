# Native texture interop probes

An isolated Rust 1.97.1 / edition 2024 library and CLI experiment. **Test-readback
only:** not libmpv playback, hardware-decoder qualification, GPUI integration,
WGPU integration, D3D11VA/VideoToolbox decoder support, presentation, HDR, or a
whole-pipeline zero-copy claim. Actual GPUI-device handoff and retention through
GPUI's eventual GPU sampling/completion are future work, including for the
macOS BGRA/Metal path; this probe retains owners only for its own native test.

## Implemented paths

- **Windows:** selects a hardware DXGI adapter (no WARP fallback), creates an
  actual D3D12 device/direct queue and a BGRA8 render-target texture initially
  COMMON. D3D11On12 uses that same device/queue. An explicit queue barrier
  establishes RENDER_TARGET before wrapped-resource use; AcquireWrappedResources
  is hazard tracking, **not** a state transition. The wrapped texture has a real
  D3D11 RTV. Executable-adjacent ANGLE DLLs create an EGL device from that exact
  D3D11On12 device, an EXT platform-device display, a GLES2 context, and a
  D3D_TEXTURE_ANGLE pbuffer over the wrapped texture. Canonical COM identity is
  checked; another internally selected device is rejected. GLES clears four
  unequal RGBA quadrants, flushes and unbinds, then ReleaseWrappedResources and
  D3D11 Flush hand the texture back in COMMON. Native D3D12 transitions to
  COPY_SOURCE, copies to a readback heap, and signals a queue fence. Only after
  completion does CPU test code map and check exact BGRA channels/alpha and row
  orientation. No shared/NT handle is needed: this is one originating resource.
- **macOS:** accelerated CGL 3.2 core context, IOSurface BGRA8 allocation bound
  to a GL rectangle texture/FBO, scissored GPU clears, GLsync worker wait, then
  same-IOSurface Metal texture import and Metal GPU copy to test-readback memory.
  CGL renderer registry-ID properties must uniquely match one enumerated Metal
  device; missing/ambiguous identity is rejected, not guessed from renderer names.
  See `src/macos.rs` for its native ownership and completion checks.

Both probes require a 4×4 non-symmetric pattern. Native row zero is expected to
contain GL y=0 for these offscreen surfaces; this is not a UI coordinate-system
promise. Mismatches fail; the code does not auto-detect and accept either flip.

## Prepare and check (from repository root)

```sh
python3 experiments/mpv-platform-probes/prepare.py --self-test
python3 experiments/mpv-platform-probes/prepare.py
CARGO_TARGET_DIR="$PWD/target" cargo check --manifest-path target/mpv-platform-probes/Cargo.toml --target x86_64-pc-windows-msvc --all-targets --locked
CARGO_TARGET_DIR="$PWD/target" cargo check --manifest-path target/mpv-platform-probes/Cargo.toml --target aarch64-apple-darwin --all-targets --locked
CARGO_TARGET_DIR="$PWD/target" cargo test --manifest-path target/mpv-platform-probes/Cargo.toml --locked
```

Preparation reads versions from root `[workspace.dependencies]` (`windows`
0.62.2, matching WGPU HAL's Windows crate family; `metal` 0.29; `libloading` 0.8).
Features are local to this experiment. It writes a marked, isolated workspace
under `target/mpv-platform-probes`, references the experiment's source files, and
copies its **own checked-in Cargo.lock**. Unknown preexisting directories,
symlink outputs, and symlink ancestors are rejected. No root workspace members,
root lockfile, SDKs, registry source, server, environment, or driver settings are
modified. Root toolchain discovery also covers the generated workspace.
`--update-lock` intentionally refreshes only the experimental lockfile, offline.

Cross-target `cargo check` needs installed Rust standard-library targets but does
not establish SDK/linker availability or any working native GPU interop.

## Run on a native host

```sh
python3 experiments/mpv-platform-probes/prepare.py
CARGO_TARGET_DIR="$PWD/target" cargo run --manifest-path target/mpv-platform-probes/Cargo.toml --locked
```

On Windows, use PowerShell `$env:CARGO_TARGET_DIR = "$PWD/target"` followed by
`cargo build --manifest-path target/mpv-platform-probes/Cargo.toml --locked`.
Supply a compatible, trusted ANGLE **libEGL.dll and libGLESv2.dll next to the
built executable** (and their required redistributables). The probe does not
fetch DLLs, search user configuration, alter PATH/DLL directories, or fall back
to a different graphics backend. DLL loading uses absolute executable-adjacent
names with DLL_LOAD_DIR | DEFAULT_DIRS. Microsoft D3D12/D3D11On12 and MSVC
build/link prerequisites must already be installed. On macOS a native SDK/linker
and functioning legacy OpenGL/Metal support are required.

The CLI accepts no arguments, reports stage/code and bounded metadata rather
than paths/configuration/network data, and prints its fixed PASS label only after
actual pixel verification. Unsupported Linux exits nonzero; Linux unit tests run
only pure layout, channel/orientation, and identity policies.

## Completion, diagnostics, limits

All native calls, immediate-context work, polling, and native destruction occur
on an owned worker, never a UI/render thread. Each completion wait has a five
second deadline. These are bounded polling waits, **not** a guarantee that a
broken graphics driver returns from every native API within a wall-clock bound.
On timeout, device loss before proven completion, panic, or any other error after
submission, the complete retained ownership graph is deliberately quarantined
(leaked to process exit), including device, queue, contexts, textures, allocators,
command lists, fences, EGL objects, and DLL owners. Never loop retries in a
long-running process: this library is a one-shot probe, not production recovery.

Windows does **not** activate a D3D12 debug layer or collect its messages; it
explicitly reports `debug-layer=not-enabled debug-messages=not-collected`.
A pixel pass is not a zero-debug-errors result. No diagnostics-qualified mode is
implemented. Native host execution and debug-layer validation remain required
before making backend-support claims.

Validated here on Linux: both installed cross-target checks (all targets),
format check, strict all-targets Clippy for Linux/Windows/macOS, four preparation
safety tests, four Linux pure tests, and unsupported-platform exit 1. Neither
native path was executed here. The macOS dependency `block 0.1.6` reports a Cargo
future-incompatibility warning; that is not evidence of native failure or success.

## API cross-checks

Implementation signatures were checked against cached Windows 0.62.2 / Metal
0.29 sources and these native specifications (reviewed 2026-10-25):

- [Microsoft D3D11On12 overview](https://learn.microsoft.com/en-us/windows/win32/direct3d12/direct3d-11-on-12)
- [CreateWrappedResource: InState / OutState](https://learn.microsoft.com/en-us/windows/win32/api/d3d11on12/nf-d3d11on12-id3d11on12device-createwrappedresource)
- [AcquireWrappedResources: hazard tracking](https://learn.microsoft.com/en-us/windows/win32/api/d3d11on12/nf-d3d11on12-id3d11on12device-acquirewrappedresources)
- [ANGLE device creation for D3D11](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_device_creation_d3d11.txt)
- [ANGLE D3D texture client buffers](https://github.com/google/angle/blob/main/extensions/EGL_ANGLE_d3d_texture_client_buffer.txt)
- [EXT device query: client vs device extension strings](https://github.com/KhronosGroup/EGL-Registry/blob/main/extensions/EXT/EGL_EXT_device_query.txt)
- [ANGLE Clear11: ordinary pbuffer native scissor y](https://github.com/google/angle/blob/main/src/libANGLE/renderer/d3d/d3d11/Clear11.cpp)
- [Apple SDK CGLTypes.h: registry ID properties 140/141](https://github.com/phracker/MacOSX-SDKs/blob/master/MacOSX11.3.sdk/System/Library/Frameworks/OpenGL.framework/Versions/A/Headers/CGLTypes.h)

# GPUI/libmpv GPU-surface experiment

Standalone **Linux Vulkan + Intel Mesa proof**, not the real application's
playback backend. This standalone harness does not use production settings, a DB,
network service or credentials. Its Linux native path is qualified narrowly below;
it does not provide Windows/macOS runtime support.

**Current direction:** the user has now authorized fourth-player integration as
`exp-embedded-mpv`, including Windows/macOS implementations, with native
Windows/macOS runtime tests and HDR explicitly deferred. The earlier
qualification-before-integration stage is historical, not a continuing ban on
integration. The [real-app wrapper](../kinema-gpu-app/README.md) builds the root
application's opt-in `experimental-gpu` feature in an isolated generated workspace;
it is not this harness's `Producer` inserted into the app. The ordinary three
player choices and embedded default remain unchanged. The fourth stable setting
is visible in all builds but fails clearly if the backend is not compiled—no
silent fallback. **The latest eight native real-Player backend tests, three offline
Linux PlayerWindow/multiwindow probes and final root/experimental checks passed**.
Standalone qualification remains distinct from that scoped app evidence. User
opt-in real-app launch: `experiments/kinema-gpu-app/run.sh run`; automated validation
uses only `run --probe`, never ordinary user app/accounts.

See the [VAAPI qualification report](QUALIFICATION.md) for the lead-verified
standalone 1080p/4K results and remaining limits. Explicit VAAPI passed on the
tested Intel Mesa system for H.264 8-bit, HEVC 8-/10-bit and AV1 8-/10-bit SDR
BT.709 at 24 fps, at both 1920×1080 and 3840×2160. These are ten-second
qualification runs, not long-soak or physical-presentation guarantees. HDR is
deferred; other drivers and platforms remain unqualified. This harness's software
decoding (`--hwdec no`) remains the default.

## Reproduce

From the repository root, with its Rust 1.97.1 toolchain and normal GPUI Linux
build prerequisites installed, plus Python 3.11+, `patch`, ffmpeg, libmpv, EGL,
and Vulkan/Mesa drivers:

```sh
experiments/gpui-mpv-gpu/run.sh prepare
experiments/gpui-mpv-gpu/run.sh check
experiments/gpui-mpv-gpu/run.sh test
experiments/gpui-mpv-gpu/run.sh run --seconds 10
# Optional native synthetic validation — TEST-ONLY CPU pixel readback, not playback:
experiments/gpui-mpv-gpu/run.sh test native_synthetic_orientation_readback -- --ignored --nocapture
# Optional explicit LOCAL media only (not a URL):
experiments/gpui-mpv-gpu/run.sh run --media /absolute/local/clip.mkv --seconds 10
# Explicit tested Intel VAAPI HD path, with a real HD render allocation:
experiments/gpui-mpv-gpu/run.sh run --hwdec vaapi --render-size 1920x1080 --seconds 10
# User-owned local file; explicit HD allocation, without changing the default:
experiments/gpui-mpv-gpu/run.sh run --media /absolute/local/clip.mkv --hwdec vaapi --render-size 1920x1080 --seconds 120
# User-owned local file; full 4K allocation (not a scaled 640×360 buffer):
experiments/gpui-mpv-gpu/run.sh run --media /absolute/local/clip.mkv --hwdec vaapi --render-size 3840x2160 --seconds 120
# Probe-only deterministic swapchain resize regression (not producer-buffer resize):
experiments/gpui-mpv-gpu/run.sh run --hwdec vaapi --render-size 1920x1080 --seconds 6 --stress-resize
```

The default software fixture remains MPEG-4 ffmpeg `testsrc2` at 640×360. With
`--hwdec vaapi`, the launcher instead encodes synthetic H.264 using `libx264`,
`yuv420p`, and two encoder threads, at the requested render resolution (even
source dimensions required for synthetic H.264). Fixtures live under the owned
`target/gpui-mpv-gpu/fixtures/` directory. Arguments are passed as an argument list;
media paths are not printed by the app. The app accepts an absolute local file
and duration 1–120 seconds, defaults to 10 seconds, auto-quits, and fails if
initialization fails, a producer error occurs, or fewer than three unique increasing
sequences are accepted into the UI view. `accepted_gpu_surfaces` measures UI
acceptance, **not physical presentation**. Error output contains only reviewed,
fixed category/operation codes (for example `capability.vk_external_semaphore_fd`
or `interop.handoff_wait`), never raw backend errors or media paths. Unknown
backend strings become `backend.unknown`.
Closing the window also requests worker cancellation. Worker teardown/join is
**after** `Application::run`, not on the UI thread.

`--hwdec no|vaapi` defaults to `no`. Automatic modes, `vaapi-copy`, `vulkan-copy`,
and other copy modes are rejected. `--render-size WIDTHxHEIGHT` defaults to
640×360, is fixed for each run, and is limited to 3840×2160 and checked RGBA
storage of at most 64 MiB; native device capability checks can reject it further.

VAAPI qualification requires actual `hwdec_current=vaapi`,
`decoder_pixel_format=vaapi`, and `output_pixel_format=vaapi`—not merely a requested
mode. `hw_pixel_format=nv12|p010` describes the underlying storage format, **not**
the hardware API. Missing/mismatched properties fail qualification rather than
falling back to software or a copy decoder. The summary prints only allowlisted
codec/format labels and numeric source/render dimensions. High-resolution runs
must use `--render-size` explicitly: enlarging the window alone still scales the
fixed per-run allocation. Source and render were verified at matching 1920×1080
and 3840×2160 allocations: there is no hidden 360p cap when `--render-size` is
explicit. Synthetic color/orientation correctness, native allocation, and zero
captured GPU Error/VUID diagnostics passed all ten tested codec/size combinations;
consult the report rather than extending that result to untested media or
capabilities.

`--stress-resize` is off by default, rejects duplicates, and requires a duration
of at least four seconds. After three unique accepted surfaces, the existing UI
timer requests alternating native window sizes of 960×600 and 1100×700 about every
150 ms for two seconds (at most 14 requests). Producer/source allocation remains
fixed—use `--render-size 1920x1080` for the HD probe. The summary distinguishes
`resize_requests` from `resize_observed_changes`, measured using actual viewport
changes. Stress mode requires at least two of each, otherwise it exits nonzero
with `stress_not_exercised`; compositor-ignored requests cannot pass. This is a
swapchain configure/resize lifecycle regression probe, not producer-buffer resize
qualification or proof of physical presentation.

## Isolation and maintenance

Checked-in files under this directory are the source of truth. `prepare.py` uses
root `cargo metadata --locked` to locate **exact registry version 0.3.9** of
`gpui-pre`, `gpui-pre-wgpu`, `gpui-pre-apple` and `gpui-pre-windows`, plus exact
`wgpu-hal` 29.0.4. These five local-copy patches are also used by the real-app
wrapper. The fifth Windows adapter patch is implemented with owner-reported
feature-on/off cross-check/Clippy passes, not native Windows runtime qualification. See the
[patch documentation](patches/README.md) for current interfaces and limits.
Preparation copies their complete source trees (including
licenses) to `target/gpui-mpv-gpu/patched/`, then applies sorted checked-in
`patches/*.patch` inside the corresponding copied crate. Each patch filename
identifies its exact crate/version, and its paths use `a/src/...`. Registry sources
are **never edited** and full vendor trees
are not checked in. Patch/source-location fingerprints avoid unnecessary copies.
Missing patches fail closed. Before copying, the script verifies per-file SHA256
values from `.cargo-checksum.json` and matches its package checksum to root
`Cargo.lock`. Normal Cargo registry trees lack that vendor checksum file: there
it authenticates the cached `.crate` archive against the root locked checksum,
then compares every extracted file to the archive. Mutated/extra files fail closed.
No registry file is created or changed.

Generated build/source directories carry `.kinema-experiment-owned` markers.
Preparation refuses preexisting unmarked or symlinked directories rather than
replacing unknown contents. Do not manually mark unrelated directories as owned.
Safety policy tests run without preparing a build:

```sh
experiments/gpui-mpv-gpu/run.sh prepare --self-test
```

The generated manifest is its own edition-2024/resolver-3 workspace. Dependency
versions come from root `[workspace.dependencies]`; its patches are local to that
manifest. Its binary references checked-in `src/main.rs`. It has a separate
checked-in `Cargo.lock`, copied to the generated workspace before each command.
All normal checks/runs use `--locked`. To intentionally refresh only the experiment
lockfile after dependency/patch maintenance:

```sh
experiments/gpui-mpv-gpu/run.sh prepare --update-lock
```

Review the resulting `experiments/gpui-mpv-gpu/Cargo.lock` diff. The root lockfile
must remain unchanged. The launcher uses root `target/` for build artifacts and
matching dev dependency profiles (opt-level 2, debug 1) to reuse existing GPUI
builds; generated manifests, source copies and fixtures remain in its subdirectory.
Do not delete user/generated files elsewhere or alter `reference/cpp`.

## What this proves — and does not

The app obtains GPUI's actual WGPU context **after opening its window**. The
producer uses that same device/queue. GPUI accepts an ordinary WGPU texture view
and retains an `Arc` frame owner; it does not own Vulkan/GL import policy. A
16 ms nonblocking UI timer takes the latest produced frame and renders it with
`ObjectFit::Contain`, plus numeric sequence, render dimensions and context label.
The producer's render allocation defaults to 640×360 and is fixed at the selected
`--render-size` for that run; window resizing exercises layout/object-fit, not
native producer allocation resizing.

The intended pipeline is libmpv GL rendering → external Vulkan-backed pixel-pack
buffer → GPU copy into a normal WGPU texture → GPUI surface. This is **not
zero-copy**: GPU copies are allowed. No CPU mapping/readback/re-upload of rendered
video frames belongs in playback. Software decoding and decoder CPU→GPU uploads
may still occur on the unchanged software default. Explicit `--hwdec vaapi`
passed the narrowly scoped Intel Mesa 1080p/4K SDR qualification in the report;
that does not qualify every hardware-decoding path. `auto` remains rejected:
prior automatic probing selected `vulkan-copy` and emitted decoder-path Vulkan
validation errors. The old environment-based automatic selector is not an
alternative CLI path. Software decoding may still upload decoded pixels;
only CPU readback/re-upload of **rendered frames** is excluded. Any synthetic
validation-only readback must be explicitly labeled and excluded from playback.

Missing required GL/Vulkan external-memory and external-semaphore FD capabilities
must fail closed, without falling back to CPU frames. Two-direction external
binary semaphores and external queue-ownership barriers are required for GL↔Vulkan
memory visibility: `glFinish` plus a CPU wait alone is **not** the memory-dependency
contract. The local HAL patch enables the optional external-semaphore FD extension.
GPUI submit/present and producer raw Vulkan submissions share a queue mutex.
Worker GPU/fence waits remain outside that gate. **Lifecycle exception:** patched
GPUI surface configure/resize/drop holds the same gate across existing GPUI/WGPU
device-idle waits, including the existing UI-thread wait on resize. This prevents
concurrent queue access, but is a blocking-UI limitation of this proof—not a
nonblocking production design. This is not a production synchronization/throughput
design, broad-driver compatibility promise, provider
integration, or end-to-end zero-copy claim. Passing unit/build checks alone is
not evidence of actual native playback; report native run evidence separately.

### Failure and completion policy

The bridge's explicit Vulkan, WGPU and GL completion waits have five-second
limits and run only on the worker, outside the queue gate. This does not bound or
remove the existing GPUI lifecycle/UI wait described above. The backend tracks raw submissions
and semaphore phases. If completion is uncertain, it quarantines native objects,
their libraries and the device until process exit rather than freeing potentially
in-flight resources. This deliberately favors safety over reclamation on failure.
Native FFI/driver calls are not universally interruptible; these bounded explicit
waits do **not** provide GPU-hang recovery or guarantee bounded process teardown.

### Validation diagnostics

A minimal `log::Log` sink is installed before GPUI initializes. It enables Warn
and Error only, avoiding per-frame Debug/Trace formatting overhead. WGPU and GPUI
render Error records, plus any VUID-bearing enabled record, increment a fatal
counter; non-VUID warnings are counted separately. Output contains only a fixed
target category and a bounded, validated VUID identifier, never raw arguments,
targets or media paths. A separate `diagnostic` field can identify only the fixed
`UNASSIGNED-Threading-MultipleThreads-Write` or `Read` tokens; these also count as
fatal diagnostics, including at warning severity. `gpu_validation_errors > 0` makes the app fail. Logger
installation conflicts also fail closed. This is not a general logging framework.

```sh
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/gpui-mpv-gpu/run.sh run --seconds 5
```

### Lead-verified standalone 1080p/4K VAAPI qualification (2026-10-25)

- Experimental Rust unit tests: 23 passed, 11 native tests ignored by default.
  All eleven explicit native tests passed: the software baseline plus all ten
  codec/size color/orientation cases using **test-only CPU readback**. Qualification
  runner Python tests: 13 passed; preparation safety tests: 5 passed.
- Both ten-second GUI stress matrices passed: H.264 8-bit, HEVC 8-bit, HEVC 10-bit,
  AV1 8-bit and AV1 10-bit, each at 1920×1080 and 3840×2160. Every case reported
  actual VAAPI decoding, VAAPI decoder/output pixel formats, NV12/P010 underlying
  storage, zero playback pixel maps, zero captured GPU errors/VUIDs, and 13
  requested/13 observed resizes. Source and producer-render allocations matched;
  window resizing did not change the producer allocation.
- Owned evidence directories:
  `target/gpui-mpv-gpu/qualification/run-7ep33973/` and
  `target/gpui-mpv-gpu/qualification/run-6gd1a0u7/`.
  See [QUALIFICATION.md](QUALIFICATION.md) for the ten qualified rows and detailed
  evidence. Scope is the tested Intel Mesa SDR BT.709 24-fps path only: ten seconds
  is not a long soak, and accepted surfaces are not physical-present counts.
  HDR remains deferred.
- These runs qualified the standalone harness only, before the newly authorized
  fourth-option integration. They do not validate the port's multiwindow behavior,
  control/audio/session lifecycle or current platform implementations.

### Real-app integration — scoped Linux validation; other OS runtime deferred

The root `kinema-mpv` integration extends the existing `Player` and libmpv client,
control/audio workers and session lifecycle with a separate bounded GPU mailbox.
Linux ports this bridge; Windows implements ANGLE + D3D11On12 on GPUI's actual
D3D12 device/queue with GPU copying; macOS implements legacy CGL +
IOSurface/CVPixelBuffer ownership into GPUI's Apple BGRA surface path. Native
interop/unsafe remains backend-owned. There is no rendered-frame CPU
readback/re-upload fallback or automatic alternate-player fallback.

The isolated [real-app wrapper](../kinema-gpu-app/README.md) uses local copies of
five patched GPUI/HAL/platform crates, including multiwindow/shared-queue-gate
fixes. The implemented fifth `gpui-pre-windows` patch adds an opt-in WGPU
HWND/D3D12 adapter to GPUI's ordinarily custom native D3D11 platform. Its feature
is activated only in the generated workspace; ordinary root GPUI/default players
are unchanged. Native D3D11 remains for glyph rasterization, not CPU video pixel
mapping.
Follow its owner's current build/run/check/test and safe-probe contract; do not
launch ordinary Kinema against user data/credentials for fixture validation.
Root manifests/lockfiles remain lead-coordinated; no production root Cargo patch
or registry edits are required.

The lead's latest native real-Player backend rerun at `203640b` with all five
patches passed **8 tests in 48.43 seconds**, zero failed, 77 filtered. Its raw scan
found zero VUID/validation/threading matches. The earlier corrected run passed in
47.89 seconds; the initial all-eight failure was a test viewport left zero
(rendering suspended), not a pass or a demonstrated backend capability failure.
Coverage includes five 1080p codec/control cases, H.264 1080p/4K source switches,
three-frame backpressure/release, retained-frame lifetime past Player, concurrent
experimental players on one WGPU device/queue, and ordinary EGL GPU-readback
coexistence/close-survivor behavior. These backend tests do not open GPUI windows.

The lead separately passed three actual PlayerWindow probes under Vulkan
validation, each with an **8-second total budget (7 playback + 1 cleanup)**:

| Case | Source = render | GPU surfaces / UI acceptances |
| --- | --- | --- |
| H.264 8-bit | 1920×1080 | 167 / 165 |
| HEVC 10-bit | 3840×2160 | 166 / 164 |
| AV1 10-bit | 3840×2160 | 165 / 163 |

All verified pause/seek/resume/speed/volume control phase 5, zero CPU readback
bytes/upload estimate, 2 requested/2 observed player resizes, and two painted
auxiliary GPUI windows, including resize and close/reopen while playing. The
three-window blocker is fixed/tested in this scope. Presentation owners released
cleanly, background Player join was observed, no late frames arrived, and GPU
errors/panics were zero; raw VUID/validation/threading scans remained zero through
all window teardown. Evidence: `target/kinema-gpu-app/gui-smoke-report.json`.
See [detailed app evidence](../../docs/development.md#experimental-gpu-player-integration--validation-pending)
for output artifacts and source checkpoints `203640b`/`ebfbb78`/`cc424ff`.

Lead final checks passed: root fmt/strict locked Clippy/tests **575 passed,
12 ignored**; experimental workspace fmt/strict locked Clippy/tests **598 passed,
20 ignored**; backend feature tests **69 passed, 16 ignored**; app/shared prepare
self-tests **8/6 passed**; standalone **23 passed, 11 ignored**; qualifier **13 passed**.
No DB/settings/accounts/providers were opened. These offline probes do not verify
managed downloads/providers, audible audio (fixtures have none), physical presents
or long-soak performance. Device-loss recovery is unsupported; cooperative audio
pause is best-effort and uncertain completion uses quarantine, not hang recovery.

Windows/macOS backend and GPUI Windows HWND/macOS BGRA/Metal source cross-checks
passed; Windows adapter feature-on/off and other four-patch Rust checks also
passed as reported by their owners. This is compile-only, not native OS support.
Native Windows/macOS linking/runtime tests, native Metal shader execution and HDR
remain explicitly user-deferred. Earlier
[native interop probes](../mpv-platform-probes/README.md) likewise did not qualify
those platforms. The ordinary root embedded default and other players remain
unchanged; selecting the fourth option without its compiled backend explains the
required isolated build rather than falling back.

### Observed stage-1 software validation (2026-10-24)

- Lead-reported sustained baseline: 60 seconds / 1786 accepted GPU surfaces /
  1790 GPU copies / 60033 ms elapsed; 120 seconds / 3588 accepted / 3590 copies /
  120028 ms elapsed / exit 0. Both reported zero playback pixel maps, maximum 3
  live frames and decoder `no`. The user confirmed a moving pattern and clean
  resizing, with no flicker/corruption reported. Five FD samples, four seconds
  apart, stayed at 154; RSS changed from 310756 to 310880 KiB over 16 seconds,
  not exhaustive leak proof. Both runs requested `VK_INSTANCE_LAYERS`, but precede
  the logger: absence of log-routed validation errors cannot be inferred.
- Lead independently passed the full final validation chain: root formatting,
  locked strict workspace Clippy, and 568 root tests (12 ignored); experimental
  formatting, locked strict all-targets Clippy, and 14 tests (1 ignored); all 5
  Python safety tests; and the explicit logger-backed native orientation test
  (1 passed). That native test uses **test-only CPU readback** to verify
  top-red/bottom-blue orientation after the external-memory/semaphore bridge.
  Root `Cargo.lock` remains unchanged.
- Logger-backed five-second smoke evidence (this agent): a
  native run requesting `VK_LAYER_KHRONOS_validation` exited 0: 139 accepted GPU
  surfaces, 140 GPU copies, zero playback pixel maps, maximum 3 live frames,
  5025 ms elapsed, decoder `no`, **`gpu_validation_errors=0`**, and 4 nonfatal
  GPU warnings. This establishes no captured Error/VUID diagnostics during that
  run, not absence of every possible driver defect. Alternate-device EGL probe
  warnings were still emitted directly by Mesa.

No result qualifies the rejected `auto` decoder path or an entirely CPU-free
pipeline. The ordinary embedded-player default remains unchanged; experimental
real-app integration has its own scoped Linux validation record above.

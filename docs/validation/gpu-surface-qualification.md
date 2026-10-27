# Archived GPU-surface qualification — retired harness

The experiment harnesses were retired after GPU promotion commit `75748c9`
(the pre-retirement revision). The original report was
`experiments/gpui-mpv-gpu/QUALIFICATION.md`; supplementary notes below came from
`experiments/gpui-mpv-gpu/README.md`, `experiments/kinema-gpu-app/README.md`, and
`experiments/mpv-platform-probes/README.md`. The former
`experiments/gpui-mpv-gpu/patches/README.md` is retained as
[patch API/synchronization notes](../../vendor/patches/README.md).

The report below preserves **historical evidence**, including unchanged evidence
matrices and then-current defaults, experimental feature names, and status
statements. Relative Markdown links have been retargeted to maintained records.
Every reproduction command below is **historical, not currently runnable**:
the standalone harness and wrappers have been deleted. Recover the harness from
Git history at `75748c9` in a separate checkout if historical reproduction is
needed; do not run those commands against the current source tree. “Latest” and “current” within the report refer to
its original checkpoints, not today's root application.

The current build entry point is `cargo run -p kinema-desktop --locked`, using
sole/default embedded GPU playback via default-on `gpu-surface` and durable
[vendor inputs](../../vendor/README.md). For current validation commands and
safe offline-probe requirements, see [development](../development.md); for current
status see [architecture](../architecture.md). Patch API and synchronization
constraints are retained in [the patch notes](../../vendor/patches/README.md).
Retirement does not rerun or extend qualification: native Windows/macOS runtime,
linking, native Metal shader execution, and HDR remain unqualified. Counts and
artifact paths below describe historical observations; local generated evidence
and conversation artifacts are not portable checked-in test results.

---

# Explicit Intel Mesa VAAPI qualification (experimental only)

This report qualifies the **standalone Linux harness**, not the real-app port.
The original qualification-before-integration stage is complete for the ten SDR
rows below. The user has since authorized the fourth `exp-embedded-mpv` option
and Windows/macOS implementation. **Latest native Linux real-Player backend tests,
three offline PlayerWindow/multiwindow probes and final root/app checks passed**. See the [archived real-app evidence](#final-integrated-playerwindow-linux-probes--separate-scoped-evidence) and
[current architecture status](../architecture.md).
The ordinary embedded default and external mpv/VLC behavior remain unchanged.
The fourth stable setting is visible in all builds and fails clearly without the
opt-in `experimental-gpu` backend, never silently falling back. HDR and native
Windows/macOS runtime tests are explicitly deferred by user decision; neither
platform is runtime-qualified by this report.

Normal Kinema's default and this harness's `hwdec=no` default remain unchanged.
`auto`, `vaapi-copy`, software fallback, and other hardware decoders are not
qualification candidates. The earlier `auto` run
selected `vulkan-copy` and emitted Vulkan VUID errors; it is not a passing result.

## Historical reproduction — requires harness recovered from Git

From the repository root on the native Linux graphical session:

```sh
python3 experiments/gpui-mpv-gpu/qualify.py --self-test
# Prepare/build first, outside the bounded playback run if necessary:
experiments/gpui-mpv-gpu/run.sh prepare
experiments/gpui-mpv-gpu/run.sh check
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 5
# Explicit legacy codec selection (same as the default):
python3 experiments/gpui-mpv-gpu/qualify.py run --cases h2648 hevc8 hevc10
# New AV1 SDR cases, never silently substituted or skipped:
python3 experiments/gpui-mpv-gpu/qualify.py run --cases av18 av110 --seconds 5
# Explicit high-resolution selections (both source AND producer allocation):
python3 experiments/gpui-mpv-gpu/qualify.py run --4k --cases h2648 hevc8 hevc10 --seconds 5
python3 experiments/gpui-mpv-gpu/qualify.py run --4k --cases av18 av110 --seconds 5
# Safe to run without a graphical session; no native app or GPU decode:
python3 experiments/gpui-mpv-gpu/qualify.py run --fixtures-only --cases av18 av110 --4k --seconds 4
```

Prerequisites: ffmpeg with the selected encoders (libx264/libx265 for the legacy
default, libaom-av1 for AV1), ffprobe, libmpv, GPUI experiment build
prerequisites, a working graphical session, Intel Mesa VAAPI, and the Khronos Vulkan
validation layer. No account, network source, database, credentials, or existing
media file is used. `--seconds` accepts 4–10 (default 5). `--app-timeout` accepts
30–900 seconds (default 300, including launcher preparation/build). Encoding has a
180-second timeout per fixture; capability/metadata probes have 20-second limits.
Timeout kills the subprocess group; a timeout or unavailable required component is
failure, not a skipped/pass result. This is not general GPU-hang recovery.

The default remains the original three local 24-fps SDR fixtures: H.264 High
8-bit, HEVC 8-bit, and HEVC 10-bit. `--cases` accepts one or more fixed labels:
`h2648 hevc8 hevc10 av18 av110`. AV1 cases use libaom-av1, 8-bit `yuv420p` or
10-bit `yuv420p10le`, respectively; no automatic alternate encoder is selected.
Case order is preserved and duplicate/unknown labels are rejected. `--4k` applies
to every explicitly selected case (or the legacy three when omitted). This keeps
historical command codec sets reproducible while allowing individual 4K rows.
All fixtures carry explicit limited-range BT.709 SDR tags. Each has a static red top, blue bottom, and white
center marker. The lavfi pattern's BT.601 YUV colors are converted to BT.709,
with frame-level color parameters as well as encoder/container tags. ffmpeg is limited to two encoder/filter threads (x265 also has a
two-thread pool). AV1 uses `-cpu-used 8 -row-mt 0 -threads 2 -crf 40 -b:v 0`;
its encoder is still software fixture generation, never a playback fallback. Source **and producer render** dimensions are 1920×1080 by default
or 3840×2160 with `--4k`; this is not merely window resizing. ffprobe validates the
codec, software fixture pixel format, frame rate, dimensions, limited range, and color metadata.
It does not establish the decoder's runtime hardware format.

`--fixtures-only` performs only encoder enumeration, generation, and ffprobe checks;
it never invokes mpv, vainfo, or the native renderer. Its successful report/rows
say **`fixture-pass-not-native-qualified`**, with resize stress false. This does
not claim hardware decoder, storage, rendering, resize, or pixel evidence. Missing
selected encoders produce `unavailable-encoder` and a failed report/nonzero exit;
encode/probe failures and timeouts likewise fail rather than skip. An encoder
advertisement does not prove that its selected bit depth works: generation and
metadata validation must still pass.

Each renderer invocation is an argument list equivalent to:

```sh
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/gpui-mpv-gpu/run.sh run --media /absolute/owned/fixture.mkv \
  --seconds 5 --render-size 1920x1080 --hwdec vaapi --stress-resize
```

Every renderer matrix invocation includes the probe-only `--stress-resize`
flag. The harness default remains off for ordinary experimental runs; the runner
always enables it for regression coverage. It repeatedly resizes the **GPUI window
and swapchain/layout only**, while source dimensions and producer render allocation
remain fixed at 1920×1080 (or 3840×2160 for the optional matrix). This is not dynamic
producer-buffer resizing. A passing row must report at least two resize requests
**and at least two actually observed window-size changes**; sending requests alone
is insufficient. Existing evidence without these counters cannot pass this updated
gate and needs a fresh stress-enabled run.

`KINEMA_GPU_EXPERIMENT_HWDEC` must be unset or `no`; the runner removes it from the
child environment, and selects VAAPI only via the explicit CLI. The producer must
force the `vaapi` GL interop and disable software fallback, not merely request an
opportunistic decoder. The runner requests the validation layer on every child;
requesting it alone does not prove the layer was successfully loaded. Native layer
availability and the logger-backed pixel tests are part of the overall gate.

## Evidence and ownership

All generated evidence lives in unique, marked
`target/gpui-mpv-gpu/qualification/run-*/` directories: fixture files, encode logs,
ffprobe JSON, native renderer logs, capability probe logs, and `report.json`.
The parent uses the existing experiment ownership marker; qualification directories
use their own token. Existing unmarked paths and symlinked paths/ancestors/markers
are refused. The runner never removes or reuses previous files. Logs are created
exclusively, and subprocesses receive argument lists, never shell commands.

`ffmpeg -encoders` is required and recorded, with selected-case availability in
`report.json`. `ffmpeg -version`, `mpv --version`, and DRM `vainfo` probes are captured when
available. Their optional reporting failures do not replace the mandatory native
decoder checks. The mpv executable's version is not proof of the linked libmpv's
version. Inspect native logs and capabilities on the machine being qualified; do
not infer support on another device from this document.

The runner stops at the first failure and exits nonzero. `report.json` is written
also on failure, with only completed case evidence; unexecuted cases are not
passes. Each renderer row also records partial `observed_evidence`, even after a
nonzero child exit or timeout: only unambiguous, syntax-validated whitelisted
summary fields, including numeric `gpu_validation_errors`. Missing, conflicting,
or malformed fields are omitted from this diagnostic view. This is not acceptance:
the original exit/timeout failure code is preserved, the row/report remain failed,
and the complete strict summary parser and all gates still apply before any pass.
Historical reports are not rewritten.

Raw subprocess output stays in owned logs; structured summaries only parse
known exact key/value fields, without dumping environment variables or inspecting
settings, credentials, databases, or user media.

## Summary contract and acceptance

The native app must emit whitespace-separated `key=value` tokens across its final
summary lines. Missing, malformed, or conflicting required fields fail clearly.
Unknown fields are ignored. The currently supported contract is:

| Field | Required value |
| --- | --- |
| `request_hwdec` (alias `requested_hwdec`) | `vaapi` |
| `actual_decoder` (aliases `hwdec-current`, `hwdec_current`) | `vaapi`, never `vaapi-copy` or `no` |
| `codec` | `h264`, `hevc`, or `av1`, matching the fixture |
| `decoder_pixel_format` | `vaapi` from `video-dec-params/pixelformat` |
| `hw_pixel_format` | `nv12` for 8-bit, `p010` for 10-bit |
| `output_pixel_format`, if reported | `vaapi` from `video-out-params/pixelformat` |
| `source_width`, `source_height` | fixture dimensions |
| `render_width`, `render_height` | requested producer allocation dimensions |
| `accepted_gpu_surfaces` | integer ≥3 |
| `cpu_pixel_maps` | integer 0 |
| `gpu_validation_errors` | integer 0 (includes captured Error/VUID diagnostics) |
| `resize_requests` | integer ≥2 |
| `resize_observed_changes` | integer ≥2; actual window-size changes, not requests |

Any reported `error` token must be `false`, and the native process must exit 0.
The complete captured native stdout/stderr is also scanned case-insensitively for
`VUID-` tokens, `Validation Error`, `UNASSIGNED-Threading...` (including
`UNASSIGNED-Threading-MultipleThreads-Write`), and `THREADING ERROR`; any match
fails even when the Rust counter is zero. `native_vuid_tokens`,
`native_validation_errors`, and `native_threading_errors` are recorded as numeric
counts, including when the renderer exits nonzero. Nonfatal alternate-device Mesa
EGL probe warnings alone do not fail this scan. Terminal/report failure messages
use fixed gate codes, not raw backend diagnostic text or exception paths.
`accepted_gpu_surfaces` counts UI acceptance, not proof of physical presentation.

**Important mpv distinction:** `video-dec-params/pixelformat` identifies the hardware
image format (`vaapi`). `video-dec-params/hw-pixelformat` describes the underlying
software format (`nv12`/`p010`), **not** `vaapi`. Missing hardware properties are a
qualification failure, not permission to substitute ffprobe metadata or fall back
to software. See mpv's `player/command.c` image-parameter property implementation:
<https://raw.githubusercontent.com/mpv-player/mpv/master/player/command.c>.

## Pixel correctness is a separate mandatory gate

A successful runner reports **`renderer-pass-pixel-tests-required`**, not complete
qualification. It demonstrates actual-decoder metadata, source/render allocation,
UI acceptance of the full-path candidate, and zero captured GPU Error/VUID records.
It does **not** read back or compare pixels and cannot establish color correctness
from metadata, a visible window, or nonzero frame counts.

The producer owner's explicit native pixel-readback tests must independently verify
red-top/blue-bottom orientation and SDR colors through the hardware-decoded path,
including the high-resolution allocation when claiming that matrix row. Readback
is **test-only CPU mapping**, excluded from playback and from the runner's zero-map
counter requirement. Record the exact pixel-test command, codec/format/dimensions,
results, and corresponding renderer evidence before declaring a row qualified.
The historical native synthetic orientation test is useful bridge evidence but
alone does not prove these hardware-decoded fixture colors. Use the producer
owner's current test name/arguments rather than inventing a test invocation here.

## Latest evidence matrix (2026-10-25)

Lead-reported prerequisites, **not independently remeasured by this runner**:
Intel iHD 26.3.5 on `/dev/dri/renderD128`; `vainfo` advertises H.264 Main/High,
HEVC Main/Main10 VLD; mpv 0.41 exposes `vaapi` in `--gpu-hwdec-interop=help`, plus
`vaapi-device` and `hwdec-software-fallback=no`. The existing interop probe matched
GL/Vulkan device UUIDs. These are necessary capabilities, not a decode success.

| Gate | Renderer result | Hardware-decoded pixel result | Qualification |
| --- | --- | --- | --- |
| 1080p24 H.264 High 8-bit / NV12 | 10-second stress pass; 232 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 HEVC Main 8-bit / NV12 | 10-second stress pass; 233 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 HEVC Main10 SDR / P010 | 10-second stress pass; 231 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 AV1 8-bit SDR / NV12 | 10-second stress pass; 237 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 AV1 10-bit SDR / P010 | 10-second stress pass; 237 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 4K24 H.264 8-bit / NV12 | 10-second stress pass; 231 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 4K24 HEVC 8-bit / NV12 | 10-second stress pass; 235 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 4K24 HEVC 10-bit SDR / P010 | 10-second stress pass; 236 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 4K24 AV1 8-bit SDR / NV12 | 10-second stress pass; 233 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |
| 4K24 AV1 10-bit SDR / P010 | 10-second stress pass; 231 accepted surfaces | Passed (lead) | **Qualified: scoped combined evidence** |

### Latest combined native evidence

The lead independently ran both five-case GUI matrices. This documentation owner
also inspected their `report.json` files:
- 1080p: `target/gpui-mpv-gpu/qualification/run-7ep33973/report.json`
- 4K: `target/gpui-mpv-gpu/qualification/run-6gd1a0u7/report.json`

All ten rows report actual decoder `vaapi`, decoder/output pixel format `vaapi`,
matching NV12/P010 underlying storage, and source dimensions equal to producer
render dimensions (1920×1080 or 3840×2160). Every row has 13 resize requests and
13 observed changes, zero playback CPU pixel maps, zero Rust GPU errors, and zero
native VUID, validation-error, and threading-error matches. These are 24-fps,
limited-range BT.709 SDR fixtures; each GUI run lasted 10 seconds. This is short
resize-stress evidence, not a long soak or proof of physical presentation.

The lead separately ran all 11 ignored `native_` tests with the Khronos validation
layer: **11 passed in 22.14 seconds**, comprising the software baseline and ten
hardware cases matching the matrix. The lead reports exact source/render sizes,
actual VAAPI and NV12/P010 storage, correct SDR colors/orientation, and zero logger
problems through teardown. Native output is retained in conversation tool artifact
`01M5QMASCP16ZXFE2R6PTVVWNS/files/output.txt` (not a repository artifact); generated
fixture temporary directories were cleaned by the tests. The pixel-test results
are attributed to the lead, not independently rerun by this documentation owner.

Reproduce exclusively, never concurrently with another native/GUI run:

```sh
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/gpui-mpv-gpu/run.sh test native_ -- --ignored --nocapture --test-threads=1
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 10 --cases h2648 hevc8 hevc10 av18 av110
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 10 --4k --cases h2648 hevc8 hevc10 av18 av110
```

The renderer reports intentionally remain `renderer-pass-pixel-tests-required`;
no generated report is rewritten into an automatic full-qualification result.
The ten **scoped combined qualifications** above rely on both those renderer
reports and the lead's separate native hardware pixel tests, solely on the tested
Linux Intel RPLS/Mesa configuration.

Lead-reported final checks passed: experiment formatting and strict all-target
Clippy; 23 ordinary experimental Rust tests passed with 11 native tests ignored,
then all 11 native tests passed explicitly; 13 qualifier self-tests and 5 prepare
tests passed; root formatting, strict workspace Clippy, and all 568 root tests
passed. These checks were not rerun for this documentation-only results update.

HDR remains deferred by user decision. Earlier Windows/macOS native interop
probes had cross-compilation checks only, before the current backend integration
changes. The user now authorizes Windows ANGLE/D3D11On12 on GPUI's actual D3D12
device/queue and macOS CGL/IOSurface/CVPixelBuffer into GPUI Apple BGRA
implementation, while explicitly deferring native Windows/macOS runtime tests.
Backend Windows/macOS cross-target Clippy has since passed, compile-only. The
implemented fifth `gpui-pre-windows` patch adds an opt-in WGPU HWND/D3D12 adapter
to Windows GPUI's ordinarily custom native D3D11 platform. Feature activation is
confined to the generated workspace: ordinary root GPUI/default players stay
unchanged. D3D11 remains for glyph rasterization, not video CPU pixel mapping.
The adapter owner reports feature-on/off cross-check and strict Clippy passes;
the other four patches passed Linux/Windows/macOS Rust checks including
feature-off. Windows/macOS backend and GPUI Windows HWND/macOS BGRA/Metal source
cross-checks passed, compile-only, not cross-OS qualification. The later Linux
GUI/root/app results are recorded separately below. Native Metal shader execution
was not performed; Windows/macOS native
linking/runtime execution and HDR remain explicitly user-deferred. This standalone
evidence makes no cross-platform or real-app GUI acceptance claim; the fourth
option is not a change to the ordinary embedded-player default. Native
interop/unsafe stays in the backend; the real app reuses its existing
Player/client/control/audio/session lifecycle with a separate GPU mailbox, not
this standalone Producer.

### Later native real-Player backend checkpoint — not GUI app acceptance

The lead independently passed **all 8 ignored Linux backend native tests in
47.89 seconds**, then reran all eight successfully on latest `203640b` with all
five patches in **48.43 seconds** (0 failed, 77 filtered), after fixing test API usage to set a nonzero viewport before
load. The first attempt failed all eight with viewport zero (rendering suspended),
without GPU errors; it was neither a passing attempt nor proof of missing backend
capabilities. Source checkpoints: `9f72211` backend, `3b6426b` paused-evidence fix,
`73c9912` test viewport.

Passing coverage uses the actual existing single Player/client: controls on all
five 1080p codecs; H.264 1080p/4K source switching; three-frame backpressure and
publication resuming after release; retained frame outliving Player; concurrent
experimental players sharing one supplied WGPU device/queue; and ordinary EGL
GPU-readback coexistence/close-survivor behavior. The native test context is not a
GPUI window; separate scoped GUI evidence follows, and full managed-session/lease
workflows remain unverified.
Experimental playback's zero-readback contract is separate from explicit test-only
pixel readback and the ordinary player's intentional readback path.

The lead reports zero uncaptured WGPU errors and zero raw VUID, validation-error
and UNASSIGNED-threading matches. Output artifact:
`01M5QQT7D34GGF6PGFYDKF0MSW/files/output.txt` in conversation, not a repository
artifact. Latest rerun output:
`01M5QSATYJ7J4JVSA35Q2160H8/files/output.txt`; the lead raw scan again found zero
VUID, Validation Error and threading matches. Backend feature tests passed 69,
with 16 ignored. These results are lead-attributed, not rerun by this documentation
owner. Only offline fixtures/probes were used; no ordinary user app/accounts were
opened.

### Final integrated PlayerWindow Linux probes — separate scoped evidence

On committed integration checkpoints `203640b`/`ebfbb78`/`cc424ff`, the lead passed
three actual five-patch PlayerWindow probes under Vulkan validation. Each had an
**8-second total budget: 7 playback + 1 cleanup**.

| Case | Source = render | Backend GPU surfaces | UI acceptances |
| --- | --- | --- | --- |
| H.264 8-bit SDR | 1920×1080 | 167 | 165 |
| HEVC 10-bit SDR | 3840×2160 | 166 | 164 |
| AV1 10-bit SDR | 3840×2160 | 165 | 163 |

All three verified control phase 5 (pause/seek/resume/speed/volume), zero CPU
readback bytes/upload estimate, two requested and two observed player resizes,
and two painted auxiliary GPUI windows. One auxiliary resized; the other closed,
reopened and painted during playback. Thus the multiwindow blocker is fixed and
three-window behavior tested in this scope. Presentation owners released cleanly,
background Player join was observed, no late frames arrived, and GPU errors/panics
were zero. The lead raw scan through all window teardown found zero VUID,
Validation Error and UNASSIGNED-threading matches.

Report: `target/kinema-gpu-app/gui-smoke-report.json` (summary inspected by this
documentation owner). Detailed output is lead-attributed in conversation artifact
`01M5QS7ZHBW2RTC47G5EMRK4R9/files/output.txt`. All three latest-code UI probes were
healthy. Final checks passed: root fmt/strict locked Clippy/tests **575 passed,
12 ignored** (42 core + 482 desktop + 51 mpv); experimental workspace fmt/strict
locked Clippy/tests **598 passed, 20 ignored**; app/shared prepare self-tests
**8/6 passed**; standalone Rust **23 passed, 11 ignored**; qualifier self-tests
**13 passed**. The earlier totals above remain historical for standalone checks.

No DB/settings/accounts/providers were opened. These are offline real-PlayerWindow
probes, not managed-download/provider workflow acceptance, audible audio verification
(no audio fixtures), physical-present measurements or long-soak evidence. Device-loss
recovery is unsupported; cooperative audio pause is best-effort and uncertain
completion uses quarantine, not GPU-hang recovery. Windows/macOS native linking/
runtime tests, native shader execution and HDR remain explicitly user-deferred.
User opt-in real-app launch is `experiments/kinema-gpu-app/run.sh run`; automated
validation uses only `run --probe`. The ordinary embedded default remains unchanged;
uncompiled fourth-option selection explains the required isolated build, never
silently falling back. The standalone ten-row matrix above remains its own
qualification record rather than a claim that the real-app GUI tested all ten rows.

### Historical HD and fixture evidence (2026-10-24)

Fixture-only validation passed for all three 1080p24 encodes and ffprobe checks:
`target/gpui-mpv-gpu/qualification/fixture-check-uc1gp5dd/`. An earlier fixture
probe correctly failed when the encoder omitted frame-level transfer/primary
metadata; that failed evidence was preserved, and generation now explicitly sets
frame-level parameters before encoding.

### Historical HD qualification: combined native evidence

The post-fix renderer report was independently inspected at
`target/gpui-mpv-gpu/qualification/run-dpwbpl8_/report.json`; its sibling renderer
logs retain the individual runs. All three rows passed, each with actual decoder
`vaapi`, decoder/output pixel format `vaapi`, matching NV12/P010 underlying format,
source/render 1920×1080, zero playback CPU pixel maps, zero Rust GPU Error records,
zero native VUID tokens, and zero native validation-error matches. Each row recorded
13 resize requests and 13 observed changes, exercising GPUI swapchain/layout while
keeping producer allocation fixed. Historical accepted surface counts were
112 H.264, 112 HEVC 8-bit, and 117 HEVC 10-bit; the latest matrix above supersedes
these counts without replacing this historical evidence.
All fixtures are 24-fps BT.709 SDR; this is not HDR evidence.

The report deliberately retains `renderer-pass-pixel-tests-required`. It is not
rewritten into an automatic full-qualification result. The lead's independent
post-fix native run passed all four tests: software baseline and hardware H.264,
HEVC 8-bit, and HEVC 10-bit at 1920×1080. Those tests verified orientation and
primaries, hardware decoder/output formats `vaapi` and NV12/P010 where applicable,
and zero captured problems through teardown with the validation layer and logger.
Combined with the renderer stress report, this completes the **scoped HD gate on
this tested Linux Intel Mesa configuration**, not universal support or proof of
physical presentation.

Reproduce the two mandatory native gates separately (run native tests exclusively,
not concurrently with the renderer matrix):

```sh
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/gpui-mpv-gpu/run.sh test native_ -- --ignored --nocapture --test-threads=1
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 5
```

The four native test names in that historical run were `native_synthetic_orientation_readback`,
`native_vaapi_h264_1080p`, `native_vaapi_hevc_1080p`, and
`native_vaapi_hevc10_1080p`. The lead's native-test/validation output is retained in
conversation tool output `01M5PJE9V676R3QTBR76F7AP50` (not a repository artifact).
Lead-reported canonical checks also passed: experimental formatting and strict
all-target Clippy; 21 experimental tests passed, 4 native tests ignored in that
ordinary run; all 4 native tests then passed explicitly; 10 qualifier tests;
5 preparation tests; root formatting, strict workspace Clippy, and 568 root tests.
The lead reports the root lockfile unchanged. These check results are attributed
to the lead, not independently rerun as part of this documentation-only update.

At that historical stage, 4K, AV1, and other platforms were pending/unqualified.
The latest combined evidence above now qualifies the ten Linux SDR cases only.
HDR is deferred, not a candidate in this SDR matrix. At that historical stage,
this gate added no fourth player option or settings/backend/default change. The
user's subsequent fourth-option authorization and separate scoped Linux app
validation are described above; the historical standalone results alone do not
validate that integration.

### Historical first native matrix attempt: failed, not qualified

Evidence: `target/gpui-mpv-gpu/qualification/run-pb6bephv/`. The H.264 renderer
exited 1, and the runner correctly stopped without running HEVC renderer rows.
Inspection of `h2648-renderer.log` confirms actual decoder `vaapi`, decoder/output
pixel formats `vaapi`, underlying format `nv12`, source/render 1920×1080, 117
accepted surfaces, 118 GPU copies, zero CPU pixel maps, and **8 captured WGPU Error
records** (`gpu_validation_errors=8`). These are log-record counts, not necessarily
eight distinct violations: message/object-line pairs can yield eight records for
four violations. Each diagnostic was categorized `wgpu` with `vuid=unavailable`.
The four warning records were lead-classified as benign validation-layer loader
notices about `VK_INSTANCE_LAYERS` adding Khronos validation (message/object-line
pairs), not shader or copy warnings. The fatal error counter remains unchanged.
Native-text scan counts were both zero; absence of VUID tokens
does not override the fatal Rust counter or nonzero process exit.

The lead separately reports all four native pixel tests (software baseline,
H.264, HEVC 8-bit, HEVC 10-bit) passed with Vulkan validation enabled. That is
pixel-path evidence, not a substitute for a clean full GPUI renderer run. The
post-fix combined evidence and reproducible commands are recorded above.

The lead reviewed the owned synthetic raw diagnostic log
`target/gpui-mpv-gpu-patch-work/native-diagnostic/raw-gpu.log` and confirmed
**`UNASSIGNED-Threading-MultipleThreads-Write`**, with worker `vkQueueSubmit`
concurrent with UI `vkDeviceWaitIdle`. This is a confirmed Vulkan queue external-
synchronization failure, not merely the earlier surface-configuration hypothesis.
The GPUI patch owner subsequently resolved queue-gate coverage using the surface
wrapper for configure/resize/drop. The clean stress-enabled rerun is recorded
above; this failed run remains historical evidence, not a pass. Raw-message and
warning classification are lead-reported from that log; the eight sanitized Error
records in the renderer evidence above were independently inspected.

The mandatory `--stress-resize` matrix exercises this race-prone swapchain path
reproducibly rather than relying on startup timing. It is regression pressure, not
a guarantee that every possible interleaving occurs. Do not suppress non-VUID
Errors, weaken any gate, or declare qualification from the failed run. The full
native matrix was repeated after correction with observed resize changes;
this failed evidence is preserved.

The earlier baseline unit/safety checks alone did not establish hardware
qualification; the HD rows above rely on the later combined native evidence.
No result establishes zero-copy (GPU copies are
allowed), an entirely CPU-free decode pipeline, physical presentation, general
Intel/Mesa compatibility, Windows/macOS support, or a production-ready backend.


## SDR matrix extension (2026-10-25; preliminary runner-only evidence)

Local inspection before changing fixture generation found ffmpeg encoders
`libx264`, `libx265`, `libaom-av1`, `libsvtav1`, and `librav1e`. The deterministic
runner uses only libaom-av1 for AV1. DRM `vainfo --display drm --device
/dev/dri/renderD128` succeeded with Intel iHD 26.3.5 / VA-API 1.24 and advertised
`VAProfileAV1Profile0 : VAEntrypointVLD`, as well as H.264 High and HEVC Main/Main10.
That AV1 profile advertisement is not independent 8/10-bit decode proof. Runtime
VAAPI, underlying NV12/P010 storage, zero CPU pixel maps/errors, exact source/render
size, strict summary and native scanner, and observed resize gates remain mandatory.

The runner's 13 stdlib self-tests passed, including AV1 8/10-bit 4K acceptance and
fallback/storage/size rejection, bad AV1 fixture metadata, unavailable encoder,
encode failure/timeout reports, preserved threading evidence on child failure,
owned paths, exclusive logs, and process timeout behavior. Unknown and duplicate
case labels were also manually confirmed to exit 2. `git diff --check` passed.
Fixture-only generation and ffprobe validation passed all five selected cases at
both 1920×1080 and 3840×2160 (`--seconds 4`, six-second fixture duration), each
within the unchanged per-fixture 180-second limit. Reports:
- 1080p: `target/gpui-mpv-gpu/qualification/run-z3q4xw75/report.json`
- 4K: `target/gpui-mpv-gpu/qualification/run-xgbrby9r/report.json`

Exact fixture-only commands run:

```sh
python3 experiments/gpui-mpv-gpu/qualify.py run --fixtures-only --seconds 4 --cases h2648 hevc8 hevc10 av18 av110
python3 experiments/gpui-mpv-gpu/qualify.py run --fixtures-only --4k --seconds 4 --cases h2648 hevc8 hevc10 av18 av110
```

This proves local AV1 8/10-bit **encoding** availability, not VAAPI decode support.

No GUI/native runs were performed by the runner owner for this extension. The
lead subsequently completed the exclusive renderer runs and matching AV1/4K
hardware pixel tests recorded in the latest combined evidence above. Encoder
availability, fixture-only passes, and historical 1080p H.264/HEVC results alone
do not qualify AV1 or 4K. Unsupported selected decode fails; it is not a software
fallback or pass.
HDR remains deferred in favor of the user-authorized Windows/macOS GPU-backend
implementation. This Linux runner qualifies neither those platforms nor the
real-app GUI integration; the later native backend and three PlayerWindow probe
results are separately recorded above, with their own limits.
The ordinary embedded default remains unchanged.


---

## Additional retired README evidence and limitations

These notes preserve useful information from the retired standalone, real-app
wrapper, and native-platform probe READMEs. They are historical, not new tests.
The real-app GUI results and limitations are already retained in the report above.

### Earlier standalone software baseline (2026-10-24)

Lead-reported software runs lasted 60 seconds (1786 accepted surfaces, 1790 GPU
copies, 60033 ms) and 120 seconds (3588 accepted, 3590 copies, 120028 ms, exit 0).
Both reported zero playback pixel maps, maximum three live frames, decoder `no`.
The user reported a moving pattern and clean resizing without flicker/corruption.
Five FD samples four seconds apart stayed at 154; RSS changed from 310756 to
310880 KiB over 16 seconds. This was not exhaustive leak proof. Those runs
requested the Vulkan validation layer but preceded the logger, so do not prove
zero log-routed validation errors. A later logger-backed five-second software
smoke exited 0 with 139 accepted surfaces, 140 copies, zero pixel maps, maximum
three live frames, 5025 ms, zero captured GPU errors and four nonfatal warnings.
None of this qualifies the rejected automatic decoder path or a CPU-free pipeline.

### Retired Windows/macOS native texture probes

These were **test-readback-only** probes, not libmpv playback, GPUI/WGPU handoff,
hardware-decoder support, presentation, HDR, or whole-pipeline zero-copy proof.
Neither native path was executed on its native host in the recorded validation.
Linux checks covered cross-target compilation/Clippy, preparation safety and pure
layout/channel/orientation/identity policies; they did not establish native SDK,
linker or GPU availability. The macOS `block 0.1.6` future-incompatibility warning
was neither native success nor failure evidence.

- Windows selected hardware DXGI (no WARP), one D3D12 device/direct queue and a
  BGRA8 texture shared through D3D11On12 and executable-adjacent trusted ANGLE.
  Canonical COM identity had to match. An explicit barrier established
  RENDER_TARGET; AcquireWrappedResources alone was not a state transition.
  GLES clear/flush/unbind, ReleaseWrappedResources and D3D11 Flush returned the
  texture in COMMON, followed by D3D12 COPY_SOURCE/readback and fence completion.
  Test code checked exact BGRA/alpha/orientation, not physical presentation.
  DLL loading used absolute executable-adjacent names and
  DLL_LOAD_DIR | DEFAULT_DIRS, not PATH changes or backend fallback.
  **No D3D12 debug layer or message collection was enabled**: a pixel pass would
  not have established zero debug errors.
- macOS used accelerated legacy CGL 3.2, IOSurface BGRA8 GL rectangle/FBO clears,
  worker GLsync completion, then same-IOSurface Metal import/copy to test memory.
  CGL registry-ID properties had to uniquely match an enumerated Metal device;
  missing/ambiguous identity failed rather than guessing a default device.
- Both used a non-symmetric 4×4 pattern, expecting native row zero to contain
  GL y=0; they did not accept either orientation or promise UI coordinates.
  Retention covered their own test, not eventual GPUI sampling/completion.
  Native calls/destruction ran on workers with five-second polling deadlines.
  Uncertain completion/device loss/panic after submission quarantined the whole
  native ownership graph through process exit. This was a one-shot probe, not
  production recovery or a universal bound on native driver calls/teardown.

Generated `target/` evidence and ignored caches were intentionally left in place
when tracked harness sources were retired; generated workspaces that referenced
those sources are historical artifacts, not maintained build entry points.

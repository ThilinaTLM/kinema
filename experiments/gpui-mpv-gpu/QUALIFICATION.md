# Explicit Intel Mesa VAAPI qualification (experimental only)

This gate precedes any fourth-player integration. It adds **no settings, player
choice, production backend, or default change**. Normal Kinema and the experiment's
`hwdec=no` default remain unchanged. `auto`, `vaapi-copy`, software fallback, and
other hardware decoders are not qualification candidates. The earlier `auto` run
selected `vulkan-copy` and emitted Vulkan VUID errors; it is not a passing result.

## Reproduce

From the repository root on the native Linux graphical session:

```sh
python3 experiments/gpui-mpv-gpu/qualify.py --self-test
# Prepare/build first, outside the bounded playback run if necessary:
experiments/gpui-mpv-gpu/run.sh prepare
experiments/gpui-mpv-gpu/run.sh check
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 5
# Separate, optional high-resolution allocation gate:
python3 experiments/gpui-mpv-gpu/qualify.py run --seconds 5 --4k
```

Prerequisites: ffmpeg with libx264/libx265, ffprobe, libmpv, GPUI experiment build
prerequisites, a working graphical session, Intel Mesa VAAPI, and the Khronos Vulkan
validation layer. No account, network source, database, credentials, or existing
media file is used. `--seconds` accepts 4–10 (default 5). `--app-timeout` accepts
30–900 seconds (default 300, including launcher preparation/build). Encoding has a
180-second timeout per fixture; capability/metadata probes have 20-second limits.
Timeout kills the subprocess group; a timeout or unavailable required component is
failure, not a skipped/pass result. This is not general GPU-hang recovery.

The runner creates three local 24-fps SDR fixtures: H.264 High 8-bit, HEVC 8-bit, and HEVC
10-bit, with explicit BT.709 tags. Each has a static red top, blue bottom, and white
center marker. The lavfi pattern's BT.601 YUV colors are converted to BT.709,
with frame-level color parameters as well as encoder/container tags. ffmpeg is limited to two encoder/filter threads (x265 also has a
two-thread pool). Source **and producer render** dimensions are 1920×1080 by default
or 3840×2160 with `--4k`; this is not merely window resizing. ffprobe validates the
codec, software fixture pixel format, frame rate, dimensions, and color metadata.
It does not establish the decoder's runtime hardware format.

Each renderer invocation is an argument list equivalent to:

```sh
VK_INSTANCE_LAYERS=VK_LAYER_KHRONOS_validation \
  experiments/gpui-mpv-gpu/run.sh run --media /absolute/owned/fixture.mkv \
  --seconds 5 --render-size 1920x1080 --hwdec vaapi --stress-resize
```

Every qualification matrix invocation includes the probe-only `--stress-resize`
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

`ffmpeg -version`, `mpv --version`, and DRM `vainfo` probes are captured when
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
| `codec` | `h264` or `hevc`, matching the fixture |
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
`VUID-` tokens and `Validation Error`; either fails even when the Rust counter is
zero. `native_vuid_tokens` and `native_validation_errors` are recorded as numeric
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

## Evidence matrix (2026-10-24)

Lead-reported prerequisites, **not independently remeasured by this runner**:
Intel iHD 26.3.5 on `/dev/dri/renderD128`; `vainfo` advertises H.264 Main/High,
HEVC Main/Main10 VLD; mpv 0.41 exposes `vaapi` in `--gpu-hwdec-interop=help`, plus
`vaapi-device` and `hwdec-software-fallback=no`. The existing interop probe matched
GL/Vulkan device UUIDs. These are necessary capabilities, not a decode success.

| Gate | Renderer result | Hardware-decoded pixel result | Qualification |
| --- | --- | --- | --- |
| 1080p24 H.264 High 8-bit / NV12 | Stress pass; 112 accepted surfaces | Native hardware pixel test passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 HEVC Main 8-bit / NV12 | Stress pass; 112 accepted surfaces | Native hardware pixel test passed (lead) | **Qualified: scoped combined evidence** |
| 1080p24 HEVC Main10 SDR / P010 | Stress pass; 117 accepted surfaces | Native hardware pixel test passed (lead) | **Qualified: scoped combined evidence** |
| Optional 4K24 H.264 8-bit / NV12 | Pending | Pending | **Pending** |
| Optional 4K24 HEVC 8-bit / NV12 | Pending | Pending | **Pending** |
| Optional 4K24 HEVC 10-bit SDR / P010 | Pending | Pending | **Pending** |
| Runner parser/ownership stdlib tests | 10 passed (`--self-test`) | No GPU involved | Runner only |

Fixture-only validation passed for all three 1080p24 encodes and ffprobe checks:
`target/gpui-mpv-gpu/qualification/fixture-check-uc1gp5dd/`. An earlier fixture
probe correctly failed when the encoder omitted frame-level transfer/primary
metadata; that failed evidence was preserved, and generation now explicitly sets
frame-level parameters before encoding.

### Final HD qualification: combined native evidence

The post-fix renderer report was independently inspected at
`target/gpui-mpv-gpu/qualification/run-dpwbpl8_/report.json`; its sibling renderer
logs retain the individual runs. All three rows passed, each with actual decoder
`vaapi`, decoder/output pixel format `vaapi`, matching NV12/P010 underlying format,
source/render 1920×1080, zero playback CPU pixel maps, zero Rust GPU Error records,
zero native VUID tokens, and zero native validation-error matches. Each row recorded
13 resize requests and 13 observed changes, exercising GPUI swapchain/layout while
keeping producer allocation fixed. Accepted surface counts are listed above.
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

The four native test names are `native_synthetic_orientation_readback`,
`native_vaapi_h264_1080p`, `native_vaapi_hevc_1080p`, and
`native_vaapi_hevc10_1080p`. The lead's native-test/validation output is retained in
conversation tool output `01M5PJE9V676R3QTBR76F7AP50` (not a repository artifact).
Lead-reported canonical checks also passed: experimental formatting and strict
all-target Clippy; 21 experimental tests passed, 4 native tests ignored in that
ordinary run; all 4 native tests then passed explicitly; 10 qualifier tests;
5 preparation tests; root formatting, strict workspace Clippy, and 568 root tests.
The lead reports the root lockfile unchanged. These check results are attributed
to the lead, not independently rerun as part of this documentation-only update.

4K, HDR, AV1, and other platforms remain pending/unqualified. This hardware-first
gate does **not** add or authorize an implemented fourth player option: no such
integration, settings choice, production backend, or default change is made here.

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

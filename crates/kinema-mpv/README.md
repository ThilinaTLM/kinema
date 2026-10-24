# kinema-mpv

Backend-only Apache-2.0 extraction of the `mpv-gpui` experiment; see
[PROVENANCE.md](PROVENANCE.md) and [LICENSE](LICENSE). No GPUI dependency.
This crate intentionally does not inherit the workspace unsafe-code prohibition:
it contains the native libmpv, GL, EGL/CGL FFI boundary. Core and desktop retain
that prohibition.

## Runtime prerequisites

Builds and pure tests do not need libmpv headers or shared libraries. Playback
needs a trusted architecture-matching libmpv shared library with client API major
2 and its codec/audio dependencies. An external `mpv` executable is not enough.
Use `Config.library_path`, then `MPV_LIBRARY_PATH` (a full library filename), or
normal OS loader discovery: `libmpv.so.2`/`libmpv.so`,
`libmpv.2.dylib`/`libmpv.dylib`, or `mpv-2.dll`/`libmpv-2.dll`.
Typical Linux package: `libmpv2`; typical macOS installation: Homebrew `mpv`.
Nothing is downloaded or bundled automatically. Loading a library executes native
code; do not accept untrusted library paths.

Default `gpu-surface` (which enables `gpu-render`) uses worker-owned GL and
GPU-resident SDR surfaces on the host presentation device. Supply
`Config.gpu_surface: Some(GpuSurfaceContext)` and consume `take_gpu_frame()`
(`GpuSurfaceFrame`). Default `Auto` without a context fails actionably before
loading libmpv; it never silently selects readback. Desktop supplies GPUI's
actual device/queue (or Metal registry ID). There is no CPU/copy-back fallback.
Auto decoding is required; HDR and native Windows/macOS remain unqualified.

Explicit low-level `RenderBackend::Cpu` or `Gpu` without a context retains RGBA
readback support for backend consumers/tests, not desktop presentation. Building
with `--no-default-features` also opts out of surfaces; enabling only `gpu-render`
retains the older Auto GL-to-CPU initialization fallback. Linux needs working
EGL/OpenGL drivers (Wayland, X11 or surfaceless); Windows needs compatible ANGLE
EGL/GLES DLLs; macOS uses CGL. Optional Config EGL/GLES paths select trusted
libraries. Inspect diagnostics rather than assuming hardware decoding or zero-copy.

## Host lifecycle contract

Call `Player::new` and drop Player on a background thread: initialization and
worker joins block. Commands are bounded, asynchronous, and return acceptance,
not proof of successful playback. Snapshots are observed state. Before native
initialization, the backend explicitly sets `config=no`, `load-scripts=no`,
`osc=no`, `osd-level=0` and disables default input bindings; managed presentation
must not rely on libmpv version-specific script/OSD defaults. Missing
`load-scripts`/`osc` options (`MPV_ERROR_OPTION_NOT_FOUND`) are accepted for builds
without the corresponding scripting capability; all other disable failures remain
errors. Upstream `options/options.c` conditionally registers `load-scripts` under
`HAVE_LUA || HAVE_JAVASCRIPT || HAVE_CPLUGINS` and `osc` under `HAVE_LUA`
(reviewed 2026-10-22). The installed `client.h` embedding-default documentation
and `mpv --no-config --show-profile=libmpv` confirm `config=no`/`osc=no` defaults,
but do not explicitly establish script-autoload/OSD suppression. Only the latest
frame is retained. `Frame::into_bgra_pixels()` optionally consumes the RGBA frame,
swizzling in place without a second allocation; capture dimensions before calling.
The root dev profile explicitly optimizes this workspace backend at level 2,
including packing, alpha and swizzle loops. Readback/upload costs still require
release profiling; this is not a zero-copy or realtime guarantee.
A zero viewport suspends rendering. Drop joins both workers;
render-context owners keep the native client and libraries alive until after
render resources are freed.

On exit, the existing control worker makes one best-effort nonblocking
`mpv_command_async` request to `set pause yes`, even after a fatal shutdown.
It does not wait for a reply or issue stop/reload/render cleanup. This matters
when uncertain GPU completion quarantines the full native/client owner graph
instead of destroying it. Failed enqueue sets the fixed
`RenderDiagnostics.shutdown_pause_enqueue_failed` category without replacing
fatal state or logging command/media data. Enqueue success is not confirmation:
a hung libmpv core or driver may never process the pause, and already-buffered
audio/network work may continue. This is cooperative mitigation, not guaranteed
silence, shutdown, or driver-hang recovery; restarting the application may be
necessary. Quarantined resources remain retained for safety.

`PlaybackState.stopped` / `Player::is_stopped()` indicate permanent worker
termination; recreate Player. `fatal_error` preserves terminal backend failure,
while `error` also reports recoverable media/command failures. `media_stopped`
identifies initial/no-media and END_FILE states; `ended` identifies EOF only.
`paused_for_cache` distinguishes buffering from user pause. No source URL is
logged by this backend; hosts must not log capability URLs or command arguments.

## Privacy-reviewed playback diagnostics

The backend emits structured `tracing` events through the host's subscriber;
`RUST_LOG=kinema_mpv=info` enables lifecycle/evidence events when the host uses an
environment filter (the desktop's default INFO logging includes these events).
For targeted diagnostic verbosity without blanket dependency tracing:

```sh
RUST_LOG=info,kinema_mpv=debug cargo run -p kinema-desktop --locked
```

The backend does not install a subscriber or log writer. A local
monotonic `player_id` correlates startup, control/render workers, media lifecycle,
failures and shutdown without identifying a media URL or account. Configuration
separates `hardware_policy` from the exact `effective_hwdec_request` sent to mpv.

Decode evidence uses a strict fixed vocabulary for actual decoder, pixel/storage
formats, transfer and optional codec, with explicit unavailable booleans and
numeric source/render dimensions. Unknown values become `other`, never sanitized
native text. Only changed snapshots are logged, with at most 32 per Player.
`decoded_transfer` (also the legacy `color_transfer` field) comes from
`video-dec-params/gamma`, before user overrides. `effective_transfer` comes from
`video-params/gamma`, mpv's effective input parameters including its resolution
of unspecified metadata (mpv 0.41 property documentation: decoder parameters
exclude overrides; `video-params` describes effective filter input). The backend
sets SDR target options, not an input gamma override. Neither property is a
measurement of displayed pixels or proof of output color correctness.
`output_pixfmt` is `video-out-params/pixelformat` after filters, **not** actual VO
output; `video-target-params` is the separate VO target and is not used to
qualify input SDR.

GPU surface qualification requires an allowlisted effective SDR transfer
(bt.1886, bt.709, srgb, gamma2.2 or gamma2.8) and direct hardware evidence.
Decoded auto/unknown/unavailable is not SDR by itself; it may qualify only once
mpv supplies effective SDR. Explicit PQ/HLG (including known aliases) in either
transfer rejects immediately, even if the other is SDR. Explicit unsupported
transfers reject rather than being guessed. Missing/auto/unknown effective
transfer stays pending even with decoded SDR. Persistent missing decode/color
evidence fails after five seconds of render-worker observation for that pipeline
generation; a new generation resets the wait and stale snapshots are discarded.
This is not a deadline for a blocked native call or proof of source tagging.
Effective-property changes force requalification/redraw even while paused without
another frame notification. Zero source dimensions still prevent publication but
do not bypass the observed-evidence timeout.

`mpv GPU surface metadata qualification` emits fixed `qualification` categories
(`waiting-decode`, `waiting-effective-transfer`, `ready`, or safe failure codes),
correlated by `player_id` and numeric `generation`. Only transitions are emitted,
at most 32 per renderer; decode/effective values remain allowlisted evidence
fields, never raw native text. These pure policy tests do not establish native
playback success for untagged files, HDR display safety, or platform qualification.

Libmpv warning capture is best effort; `available=false` does not prevent playback.
Native messages become reviewed diagnostic/module/severity enums, with at most
eight distinct tuples per Player. Raw module names, messages, errors, URLs, paths,
titles and credentials are never forwarded. Final summaries count suppressed
messages and evidence transitions; these are deliberately not full mpv logs.

Use `PlaybackState.failure_reason` or `Error::playback_failure()` for fixed safe
codes/messages. Raw `error` and `fatal_error` can contain sensitive details and
must not be displayed or logged. Typed libmpv failures include numeric
`mpv_error` when available, a fixed `error_kind`, and an allowlisted operation
category (`configure`, `initialize`, `render-create`, `render`, `command`,
`observe`, `load`, or `other`); raw operation/error strings are never forwarded.
For a failed reproduction, share the `mpv playback failure` line and nearby
`mpv evidence changed` / `mpv native diagnostic` lines with the same `player_id`.
These logs cannot retroactively recover an older uninstrumented failure.
Unqualified/missing color metadata is distinct
from recognized HDR; a diagnostic does not relax the GPU surface SDR/direct
hardware requirements or establish the cause of a previous uninstrumented run.

`workers-stopped` has `scope=workers`: joins do not prove native destruction or
recovery. GPU surface `native_cleanup=complete` follows successful teardown;
`native_cleanup=quarantined` means uncertain owners remain deliberately retained.
Shutdown pause `enqueued=true` is explicitly a request only, not proof of silence.

## Cinema Dock observations and commands

`PlaybackState.chapters` contains ordered `Chapter { time, title }` values;
`tracks` contains `Track { id, kind, title, lang, selected }`, where `TrackKind`
is Video, Audio, or Subtitle. `chapters_available` and `tracks_available` are true
only for successfully parsed observations, including empty lists. Both default
and reset to false; unavailable/malformed properties clear the corresponding Vec
and availability flag. Hosts must check availability before presenting a definitive
“No chapters/tracks” result. `speed` is the observed playback multiplier.
`buffered_ranges: Option<Vec<BufferedRange>>` copies actual demuxer-cache
`seekable-ranges` (`start`/`end` seconds); None means unavailable, not zero
buffering. These ranges need not cover every byte downloaded or every decoder
buffer. Media metadata is cleared on accepted load, START_FILE and END_FILE.
Speed is session-level and is not reset by media replacement.

`seek_absolute(seconds)` accepts finite nonnegative seconds;
`set_speed(rate)` accepts 0.01..=100; `select_audio(Some(id))` and
`select_subtitle(Some(id))` select observed track IDs, while None means Off.
`set_subtitle_position(percent: u8)` enqueues the presentation-only `sub-pos`
property: 0..=100 (0 top, 100 default bottom). It persists across media loads
on the same Player; the backend does not reset it. Hosts can raise subtitles
while chrome is visible and restore 100 when hidden;
subtitle styling/format can affect placement, so this is not a guaranteed safe zone.
Commands only enqueue and never optimistically change observed selections.
Seek chapters by their time; no thumbnail extraction is implemented.
Event-owned nodes are copied on the control worker before the next event wait;
parsing is nonrecursive and bounded to 4096 entries, 128 fields per map and
4096 bytes per string. Unavailable/malformed metadata clears its observation.

## Validation / opt-in smoke tests

From the workspace root:

```sh
cargo test -p kinema-mpv --locked
cargo test -p kinema-mpv --locked real_cinema_dock_metadata -- --ignored --nocapture
cargo clippy -p kinema-mpv --all-targets --locked -- -D warnings
cargo test -p kinema-mpv --no-default-features --locked
# List native smoke tests (ignored by default):
cargo test -p kinema-mpv --locked -- --list --ignored
# Installed libmpv + ffmpeg, software lifecycle:
cargo test -p kinema-mpv --locked real_cpu_player_media_lifecycle -- --ignored --nocapture
# Run a selected native test by its listed name; GPU tests need real GL:
cargo test -p kinema-mpv --locked native_readback_smoke -- --ignored --nocapture
```

Use local synthetic/non-sensitive media for smoke testing. Native tests require
ffmpeg where indicated and some use temporary media files. Passing pure tests or
compilation does not qualify actual device/audio/provider playback.

### Native backend validation: Linux, 2026-10-22

Each command used `cargo test -p kinema-mpv --locked <name> -- --ignored --nocapture`
and isolated ffmpeg-generated temporary fixtures, not user media/config/database:

| Test name | Result | Test duration |
| --- | --- | --- |
| `real_mpv_render_and_teardown` | passed | 2.00 s |
| `real_cpu_player_media_lifecycle` | passed | 0.92 s |
| `real_gpu_player_media_lifecycle` | passed | 1.50 s |
| `real_source_native_4k` | passed | 0.24 s |

CPU lifecycle reported active Cpu. Forced GPU lifecycle reported active Gpu,
no fallback or optional-option warnings: Intel, `Mesa Intel(R) Graphics (RPL-S)`,
`4.6 (Core Profile) Mesa 26.2.5-arch1.1`. Hardware decoding was not active for
these synthetic fixtures (`hardware_decoder: None`); this does not qualify
hardware decode support. The bounded 4K test received a native 3840×2160 RGBA
CPU frame despite a 640×360 viewport. These short backend lifecycle tests do
not establish sustained 4K performance, GPUI presentation, audio-device output,
provider playback, or Windows/macOS qualification.

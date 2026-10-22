# Player — independent native playback window

**Status: source implemented; scoped validation and Linux presentation reviewed by the lead.**
The backend import, embedded default/settings choices, managed window, original
player per-session replay, close/lease release, retry, and stale-event logic are
implemented in source. Root checks and scoped Linux GPUI presentation passed as
reported below, including final backend option hardening and verification reruns. Design targets below are not blanket
implementation or acceptance claims. [Desktop v3](desktop-v3.md) supplies the sole Cinema
visual system; [architecture](../architecture.md#approved-embedded-player-direction)
sets the backend boundary and lifetime policy.

## Scope and composition

Embedded mpv is the default choice. Play opens an independent Kinema-managed
GPUI window; the browse/detail window remains usable. Retain external mpv and VLC
as explicit alternatives, not silent fallbacks. Use only pinned `gpui-pre =0.3.9`
and `gpui_platform`, with in-house desktop widgets. Backend functionality has been
imported into `kinema-mpv` from `/home/tlm/Projects/Random/mpv-gpui`; the experiment
is unchanged and its GPUI 0.2.2 view is not imported. No HTML player or second UI
engine. Settings provide three primary choices: embedded mpv (default), external
mpv, and VLC.

The backend owns libmpv commands/events and decoding/render workers. Desktop owns
window, view, controls, focus, and session coordination. Rendering must not block
on I/O, decoder commands, or worker shutdown. Bound frame/event handoff, cancel
obsolete work, and reject stale events by window/session generation. A frame from
a closed/replaced session must never update another player.

Implemented control source scope: play/pause with reload at EOF, ±10-second seek,
click-seek timeline, mute and volume decrement/increment, fullscreen, Escape to
leave fullscreen (never close), keyboard focus, and chrome auto-hide. Native
window close releases the lease. This source status does not establish actual
full GPUI interaction acceptance. Audio/subtitle
track selection, More menus, drag-seek preview, and custom pointer hiding are
**design follow-ups**, not required first-increment controls; expose no placeholder
buttons for them unless implementation and review confirm they work.

## Compact Cinema chrome

Video is the dominant surface, aspect-fit with near-black letterboxing; never
stretch or crop by default. Keep native window resizing, move, minimize/maximize,
and OS close affordances. Fullscreen hides ordinary window chrome, not escape
routes. No browse rail, poster dashboard, decorative glass, or large floating
transport pills.

Text wireframe (labels describe accessible icon controls):

```text
┌ Kinema · Title / S01E02                                [OS controls] ┐
│ [Title / episode, truncated]                                        │
│                                                                    │
│                          VIDEO                                     │
│                    [Buffering… if stalled]                         │
│                                                                    │
│  00:42:18 ─────────────●────────────────────────────── 01:48:06       │
│  [Play/Pause] [−10s] [+10s]       [Mute] [−] 80% [+] [Fullscreen]     │
└────────────────────────────────────────────────────────────────────┘
```

- Use existing token roles, not a separate palette: `bg.canvas` `#0B0C0E`,
  `bg.surface` `#131418`, raised/overlay surfaces, `text.primary` `#F2F3F5`,
  secondary/muted text, and scarce warm `accent` `#FF7A45`. Use `accent.text` /
  `focus.ring` `#FF9B72` for selected labels and the 2 px focus ring.
- Text and controls over arbitrary video need `scrim.max`-strength backing under
  their full bounds; a decorative gradient alone is insufficient contrast.
  Menus use opaque `bg.overlay`. Cinema dark video overlays stay dark in Light
  theme; surrounding native/themed chrome follows desktop-v3.
- Reuse compact desktop type/spacing, 4/6/8/12 px radii and subtle hairlines.
  Aim for 32 px icon controls with clear hit regions, a compact top title row,
  and a two-row bottom transport. Use tabular time numerals. Narrow windows
  truncate titles while retaining play, seek, volume, and a fullscreen escape
  route. A responsive More menu is a follow-up, not a required placeholder.
- Progress is warm accent; any buffered/available-range indication is neutral
  and separately labelled. Decoder buffer is not the managed download's
  completion percentage. Unknown duration shows `—`, not a fabricated total;
  disable unsupported seek operations visibly.
- Follow-up only: audio/subtitle menus would list observed tracks, selection,
  subtitle Off, and explicit empty states; More could expose these when space is
  tight. Do not show unimplemented track, More, speed/HDR/equalizer controls.

## Controls and focus

All icon buttons have accessible names and hover tooltips including shortcuts.
Focus-visible uses the shared ring; selected, disabled, hover and pressed states
remain distinct. Follow-up menus should support arrows, Enter, Escape,
Menu key / Shift+F10.
Keyboard shortcuts are scoped to the focused player and do not override a text
input or open menu's editing/navigation keys.

Shortcut design contract (exact mapping still subject to native review):

| Input | Proposed player action |
| --- | --- |
| Space or K | Toggle user play/pause intent |
| Left / Right | Seek −/+10 seconds, clamped to available timeline bounds |
| Up / Down | Volume +/−5 percentage points, clamped 0–100 |
| M | Mute/unmute without losing the prior volume |
| F or double-click video | Toggle fullscreen |
| Escape | Leave fullscreen; never close playback (dismiss an open menu first if menus are later added) |
| Tab / Shift+Tab | Reveal chrome and move through enabled controls |
| Enter / Space on focused control | Activate that control, not a second playback toggle |
| Native OS close shortcut/button | Close this player window and release its lease |

Clicking the seek rail requests that time. Drag-seek preview and commit-on-release
are design follow-ups, not first-increment requirements. Seeking missing managed bytes requests priority
ranges through the server; it must not claim those bytes are already downloaded.
Single-click video reveals chrome; use the explicit button/keyboard to pause so
fullscreen double-click does not accidentally toggle playback twice.

## Visibility and playback states

**Proposed interaction defaults:** reveal chrome on pointer motion, pointer entry,
keyboard input, or a state change needing attention. After 2.5 seconds of
inactivity while playing, fade chrome. Custom pointer hiding over video is a
design follow-up, separate from required chrome auto-hide. Never hide
while a control is focused/hovered, seeking/dragging, a menu is open, user-paused,
buffering, loading, ended, or showing an error. Restore immediately on input;
use the shared short fade timing and instant transitions for reduced motion.
Do not steal keyboard focus on reveal. Do not leave an invisible focused control.

| State | Presentation / behavior |
| --- | --- |
| Opening | Quiet loading label; controls visible; close remains available |
| Playing | Pause action reflects user intent; chrome may auto-hide |
| User-paused | Play action, chrome stays visible, last frame retained |
| Buffering / seeking | Label `Buffering…` / `Seeking…`; retain last frame where possible; preserve user intent |
| Ended | Replay action and visible controls; no automatic history/resume write |
| Failed | Redacted actionable error with explicit retry/close; no infinite spinner or silent external launch |

Buffering is observed inability to advance media, not a user-pause command.
Recovery resumes only if the user still intends to play; a late ready event must
not override pause. Pause affects the decoder only: download pause and prefetch
suspension are deferred. Neither paused nor buffered playback implies that the
managed acquisition stopped. Minimize/occlusion render-work suppression must be
validated separately and must not imply download suspension. Minimize/occlusion
suppression is not automatic in the current increment.

## Window, lease, and session ownership

- One embedded window per lease. Replaying the same lease focuses its existing
  window rather than creating another embedded owner; different leases can have
  independent windows. A media session is a lease; multiple leases can share one
  asset. External replay means one lease need not imply only one player instance.
- External app replay can share a lease. This embedded one-window-per-lease rule
  neither rewrites external replay ownership nor makes an embedded close a reason
  to terminate an external process. **Implemented replay guard:** source retains
  the original player choice per session; preferences apply to new sessions,
  while replay retains the original backend. Never switch an externally used lease
  to embedded ownership merely because the global preference changed: closing the
  managed window could revoke the external reader. A separate new-lease switching
  alternative is not claimed as implemented.
- Close stops/tears down that embedded player, cancels its view tasks, invalidates
  late events, and releases its owned lease exactly once. Window/init/load failure
  paths must also release any lease they acquired. Keep libmpv loaded until all
  handles, callbacks, and workers using it are gone.
- Release cancels acquisition only when it removes the asset's **final lease**.
  Other leases/sessions remain usable. Closing a player is not delete-cache,
  delete-original-file, or a global stop-download command.
- Browse navigation never closes players. Main-window close/application quit
  retains the active-work confirmation policy; Cancel preserves players/leases.
  Confirmed quit tears down embedded players and stops the server. External
  players remain open and lose their stream, as before; there is no daemon and
  no external-close detection inferred from absent HTTP traffic.
- No tracked persistent resume position, viewing history, watched state, or
  personalized-library writes in this increment. Transient timeline state and
  Replay are transport UI, not watch tracking.

## Native requirements and acceptance

The separate embedded window is still **in-process**: a native libmpv, codec, or
driver crash can terminate Kinema. Worker error handling covers Rust/reported
failures, not native process crash isolation. External mpv/VLC retain their
separate-process boundary; no helper-process renderer is added in this increment.

Final backend review added explicit `load-scripts=no`, `osc=no`, and
`osd-level=0` to disable user-autoloaded scripts and native mpv chrome. In builds
without the scripting/OSC engine, only `OptionNotFound` is accepted for those
missing options; other disable failures fail startup. This is configuration
hardening, not a sandbox or crash-isolation boundary. The backend owner reviewed
upstream `options/options.c` and installed `client.h` option evidence and added
one focused test. Final root/native reruns passed. Missing-option classification
is pure-test coverage only; native no-Lua builds are not runtime-qualified.

Playback requires separately supplied native **libmpv**, not merely the mpv
executable. Dynamic loading, dependency discovery, trust, licensing, and the
validation matrix are covered in [development](../development.md#embedded-mpv-prerequisites-and-validation-plan).
The experiment's frame-copy pipeline is not a zero-copy, HDR, or performance
promise for Kinema. CPU fallback, hardware decode, renderer choice, native
resolution, and color behavior require explicit observation rather than inferred
support from successful compilation.

### Validation evidence (lead-reviewed)

- Backend deterministic tests: default **37 passed**, CPU-only **26 passed**.
- All four native synthetic tests passed again after option hardening: `real_mpv_render_and_teardown`,
  `real_cpu_player_media_lifecycle`, `real_gpu_player_media_lifecycle`, and
  `real_source_native_4k`, on local Linux Intel RPL-S, Mesa 26.2.5, OpenGL 4.6.
  These fixtures used software decoding; GPU rendering is not hardware-decoding
  evidence, and native 4K backend output is not proof of GPUI 4K presentation.
- Scoped integration tests: app **35 passed**, settings **13 passed**, and
  `ui-probe-test` `embedded_` **4 passed**.
- Final root fmt, strict locked workspace Clippy, and locked tests passed:
  **477 passed, 11 ignored** (40 core + 400 desktop + 37 backend). Full desktop `ui-probe-test`
  strict Clippy and tests passed: **447 passed, 4 ignored**.
- Production GPUI native Wayland probe: generated 1280×720 clip rendered into a
  physical 1100×680 viewport; **172 GPU-consumed frames**, background shutdown
  `true`, error `false`.
- Owned isolated Xvfb probe with real generated media: CPU fallback (expected DRI3
  warnings), **22 frames**, position **15/30 seconds**, paused/muted `true`,
  shutdown `true`. Space/M/Right/Left/Escape and timeline click were exercised;
  this is not fullscreen or audio acceptance.
- Lead visually inspected `player-light.png` (synthetic fixture, no libmpv) and
  `player-native-paused.png` (real generated media). Cinema chrome was consistent;
  video overlays intentionally stay dark under Light.

These results were independently inspected by the lead, not rerun by the docs
owner. Ignored local [artifacts and probe commands](../../target/ui-probe/player/README.md)
include `final-validation.log` (lead inspected, ending `TOTAL_FAILED_COMMANDS:0`),
`native-wayland.log`, `keyboard.log`, `workspace-validation.log`, and
`ui-validation.log`; they are not distributed documentation assets. See
[development](../development.md#embedded-mpv-prerequisites-and-validation-plan)
for reproducible probe entry points. Viewport resolution is deliberate, with no
hidden 720p cap. This establishes scoped Linux presentation, not cross-platform,
full-provider, fullscreen/audio, or sustained-performance acceptance.

Remaining acceptance plan (do not interpret the list as completed results):

1. Deterministic backend/load failures, event ordering, stale-generation rejection,
   bounded frame delivery, worker teardown, and missing/invalid library handling.
2. Native window open/replay focus/resize/fullscreen/close, first-increment controls
   and keyboard,
   focus/auto-hide/reduced motion, contrast over bright video, and actionable errors.
3. Local file and loopback range playback, missing-range seek, buffering recovery,
   pause during buffering, EOF/replay, and close during initialization/seek/failure.
4. Two leases sharing one asset: close one player and prove the other survives;
   close the final lease and prove acquisition cancellation. Exercise external
   replay sharing and confirm external-player close/quit policy is unchanged.
5. Quit Cancel/confirm, cleanup safety, minimized rendering behavior, sustained
   frame/memory/latency measurements, and platform-specific library/graphics/audio
   behavior on Windows, macOS, Linux Wayland and Linux X11.

Follow-up acceptance, only when implemented: audio/subtitle track selection,
More/menu keyboard behavior, drag-seek preview, and custom pointer hiding.

**Open implementation assumptions:** the shortcut mapping, 2.5-second hide delay,
narrow-window layout, and minimize/occlusion optimization above remain proposed
interaction details until native review. Library lookup/configuration and redistribution packaging must be
confirmed by the implementation owner. Evidence above is scoped; remaining
acceptance and any post-review reruns must be recorded separately.

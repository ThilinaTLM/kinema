# Player — independent native playback window

**Status: Cinema Dock source implemented; scoped Linux synthetic-media presentation reviewed.**
The earlier embedded-player increment implemented the backend import, embedded
default/settings choices, managed window, original player per-session replay,
close/lease release, retry, and stale-event logic. Its root checks and scoped Linux
presentation evidence are retained below as historical results, not validation of
the new controls. Design targets and source implementation are not blanket
implementation or acceptance claims. [Desktop v3](desktop-v3.md) supplies the sole Cinema
visual system; [architecture](../architecture.md#approved-embedded-player-direction)
sets the backend boundary and lifetime policy.

## Scope and composition

Embedded MPV uses the GPU-surface path as its sole/default renderer, enabled by
default Cargo feature `gpu-surface`. The former fourth experimental choice is
not a separate player. Current Rust settings normalize `exp-embedded-mpv` to
Embedded MPV on load, preserving other preferences; explicit saves write
`embedded-mpv`, with no automatic disk rewrite on load or legacy C++ migration.
Root `cargo run -p kinema-desktop --locked` uses durable
vendored patches, not a generated wrapper workspace. This promotion is not new
qualification: prior Linux evidence remains scoped; native Windows/macOS tests
and HDR remain deferred and unqualified. Play opens an independent Kinema-managed
GPUI window; the browse/detail window remains usable. Retain external mpv and VLC
as explicit alternatives, not silent fallbacks. Use only pinned `gpui-pre =0.3.9`
and `gpui_platform`, with in-house desktop widgets. Backend functionality has been
imported into `kinema-mpv` from `/home/tlm/Projects/Random/mpv-gpui`; the experiment
is unchanged and its GPUI 0.2.2 view is not imported. No HTML player or second UI
engine. The approved [Cinema Dock wireframe](player-controls-wireframes.html) is
an HTML **design artifact exception only**, not shipped player UI or a second
engine. Its illustrative media, metadata, buffering, thumbnails, and demo actions
are not evidence of native capabilities. Settings provide three primary choices:
Embedded MPV (GPU-surface, default), external mpv, and VLC.

The backend owns libmpv commands/events and decoding/render workers. Desktop owns
window, view, controls, focus, and session coordination. Rendering must not block
on I/O, decoder commands, or worker shutdown. Bound frame/event handoff, cancel
obsolete work, and reject stale events by window/session generation. A frame from
a closed/replaced session must never update another player.

The current native source adds chapter-aware drag/hover seeking,
vertical volume and keyboard adjustment, and real audio/subtitle/chapter/speed
menus to play/pause with reload at EOF, ±10-second seek, mute, fullscreen, focus,
and chrome auto-hide. Escape leaves fullscreen (never closes playback), after
dismissing an open menu. Native window close releases the lease. These controls
are no longer deferred first-increment follow-ups; their current source and
validation status are recorded separately below. Expose no fake options or
placeholder actions. Actual decoded thumbnail generation is explicitly deferred:
there is no thumbnail API, and no fixture or repeated video frame may impersonate
a seek thumbnail. Custom pointer hiding remains a separate follow-up.

## Current implementation and scoped validation

- Native source implements the Cinema Dock: chapter-marked 12 px seek rail,
  hover/drag time/chapter preview and release-to-seek, same-size ghost transport,
  middle-right vertical volume with mute/percentage and keyboard adjustment, and
  real audio/subtitle/chapter/speed menus. Utility controls use definite-width
  slots; popups anchor above the dock in viewport coordinates. The 100 px volume
  rail is offset upward for minimum-window clearance. Timeline and volume have
  explicit accessibility labels.
- Backend source supplies observed chapters/tracks/speed/buffered ranges, bounded
  metadata parsing, and real seek/selection/speed commands. Availability flags
  distinguish unknown metadata from known-empty lists; disabled tooltips explain
  the difference.
- Subtitle placement uses `set_subtitle_position(u8)`, measured dock height and a
  conservative fallback to move text above visible chrome and restore the default
  when hidden. Updates are deduplicated, reapplied after load, and do not reset
  the idle timer. Styled ASS subtitles may ignore `sub-pos`; universal subtitle
  clearance is not established. Backend restoration is tested; hidden-chrome
  subtitle restoration has not been observed in the native UI.
- Lead final checks passed: `cargo fmt --all -- --check`, strict locked workspace
  all-target Clippy, and locked workspace tests. Strict `ui-probe-test` feature
  Clippy and tests passed (**459 desktop tests passed, 4 ignored**); CPU-only
  backend tests passed (**35 passed, 4 ignored**). The lead independently reran
  native `real_cinema_dock_metadata` and `real_cpu_player_media_lifecycle` tests
  successfully.
- The lead personally reviewed final Linux synthetic-media captures at 1100×680
  and 720×440: all tools visible, menus above the right dock, minimum-window
  volume clearance, and plain SRT subtitles clear of the dock. The validator
  reviewed earlier seek-preview, drag, and keyboard captures. Ignored local
  evidence is in the ignored local capture directory
  `target/ui-probe/cinema-dock-20261022-final2-stable-oc4uyz/`, including
  `subtitles-1100x680-selected.png`, `subtitles-720x440-selected.png`,
  `speed-720x440-selected-menu.png`, and `volume-720x440-drag.png`; these are not
  distributed documentation assets.
- This establishes scoped Linux synthetic-media review, not blanket UI or
  accessibility acceptance. Audible audio, hardware decoding, sustained
  performance, and cross-platform behavior remain unqualified.
- Native reload coverage uses stop-and-settle before reload. An exploratory
  immediate same-file replacement while paused, without stop, could leave
  `source_size`/`render_size` missing; provenance and broader EOF-replay impact
  remain unverified. No claim is made that this is preexisting, introduced, or fixed.
- Decoded thumbnail generation is explicitly deferred: previews are time/chapter
  text only. No thumbnail pipeline, HTML runtime, or second engine is added.

## Compact Cinema chrome

Video is the dominant surface, aspect-fit with near-black letterboxing; never
stretch or crop by default. Keep native window resizing, move, minimize/maximize,
and OS close affordances. Fullscreen hides ordinary window chrome, not escape
routes. No browse rail, poster dashboard, decorative glass, or large floating
transport pills.

Use the approved Cinema Dock artifact for composition, with native GPUI controls:

- Compact title at the top; video remains dominant.
- Bottom dock: elapsed/remaining time and current chapter above a bold **12 px**
  seek rail inside a **32 px** interaction target. Chapter marks come from mpv.
- Below the rail, play/pause and ±10-second transport on the left use the same-size
  ghost buttons, not an oversized filled play pill. Chapter, subtitle, audio,
  speed, and fullscreen controls occupy the right.
- A separate **middle-right vertical volume slider** increases upward, with mute
  and percentage together on a contrast-safe backing. This replaces the old
  bottom-row volume −/+ buttons.
- Hover/drag preview floats above the rail with time and the observed chapter
  label when available. It is text-only until a real thumbnail pipeline exists.

Do not copy fixture-only back navigation, quality/HDR badges, settings actions,
buffer ranges, or subtitle samples into production as invented capabilities.

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
  route. A responsive More menu is optional; it must contain only working actions.
- Progress is warm accent; any buffered/available-range indication is neutral
  and separately labelled. Decoder buffer is not the managed download's
  completion percentage. Unknown duration shows `—`, not a fabricated total;
  disable unsupported seek operations visibly.
- Audio/subtitle menus list observed tracks and selection, subtitle Off, and
  explicit empty/unavailable states. Chapters use observed titles and start times;
  speed choices issue real mpv commands and selection reflects observed speed.
  Requested values are not proof of backend success. More may expose working
  actions when space is tight; do not invent HDR/equalizer/settings controls.
- Show buffered ranges only when genuinely observed, never a decorative fixture.
  Missing chapter metadata means no fabricated chapter ticks or names.

## Controls and focus

All icon buttons have accessible names and hover tooltips including shortcuts.
Focus-visible uses the shared ring; selected, disabled, hover and pressed states
remain distinct. Native menus should support arrows, Enter, Escape,
Menu key / Shift+F10.
Keyboard shortcuts are scoped to the focused player and do not override a text
input or open menu's editing/navigation keys.

Shortcut design contract (current implementation requires native review):

| Input | Player action target |
| --- | --- |
| Space or K | Toggle user play/pause intent |
| Left / Right | Seek −/+10 seconds, clamped to available timeline bounds |
| Up / Down | Volume +/−5 percentage points, clamped 0–100 |
| M | Mute/unmute without losing the prior volume |
| F or double-click video | Toggle fullscreen |
| Escape | Dismiss an open menu first, otherwise leave fullscreen; never close playback |
| Tab / Shift+Tab | Reveal chrome and move through enabled controls |
| Enter / Space on focused control | Activate that control, not a second playback toggle |
| Native OS close shortcut/button | Close this player window and release its lease |

Clicking the seek rail requests that time. Dragging updates the thumb and
text-only time/chapter preview; commit the seek on release, keeping the requested
position distinct from observed playback until reconciled. Hover previews do not
seek. Clamp to valid duration and suppress seeking when duration is unavailable.
Seeking missing managed bytes requests priority ranges through the server; it
must not claim those bytes are already downloaded.
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
one focused test. Earlier-increment final root/native reruns passed (see historical
evidence below). Missing-option classification
is pure-test coverage only; native no-Lua builds are not runtime-qualified.

Playback requires separately supplied native **libmpv**, not merely the mpv
executable. Dynamic loading, dependency discovery, trust, licensing, and the
validation matrix are covered in [development](../development.md#embedded-mpv-prerequisites-and-validation-plan).
GPU-surface playback must fail explicitly when required native capabilities are
missing, not fall back to rendered-frame CPU readback/re-upload or another player.
GPU copies are allowed; this is not a zero-copy, HDR, or performance promise.
Hardware decode, native resolution, and color behavior require explicit
observation rather than inferred support from successful compilation.

### Historical validation evidence (earlier increment; lead-reviewed)

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

These earlier-increment results were independently inspected by the lead, not
rerun by the docs owner. They do **not** validate the Cinema Dock changes or its
new metadata/command paths. Ignored local [artifacts and probe commands](../../target/ui-probe/player/README.md)
include `final-validation.log` (lead inspected, ending `TOTAL_FAILED_COMMANDS:0`),
`native-wayland.log`, `keyboard.log`, `workspace-validation.log`, and
`ui-validation.log`; they are not distributed documentation assets. See
[development](../development.md#embedded-mpv-prerequisites-and-validation-plan)
for reproducible probe entry points. Viewport resolution is deliberate, with no
hidden 720p cap. This establishes scoped Linux presentation, not cross-platform,
full-provider, fullscreen/audio, or sustained-performance acceptance.

Remaining acceptance plan (including the current changes; not completed results):

1. Deterministic backend/load failures, event ordering, stale-generation rejection,
   bounded frame delivery, worker teardown, and missing/invalid library handling.
2. Native window open/replay focus/resize/fullscreen/close; Cinema Dock layout,
   chapter marks, hover/drag preview and release commit, vertical volume and
   keyboard control; observed audio/subtitle/chapter/speed menus, command failure
   and empty states; focus/menu navigation/auto-hide/reduced motion, contrast over
   bright video, and actionable errors.
3. Local file and loopback range playback, missing-range seek, buffering recovery,
   pause during buffering, EOF/replay, and close during initialization/seek/failure.
4. Two leases sharing one asset: close one player and prove the other survives;
   close the final lease and prove acquisition cancellation. Exercise external
   replay sharing and confirm external-player close/quit policy is unchanged.
5. Quit Cancel/confirm, cleanup safety, minimized rendering behavior, sustained
   frame/memory/latency measurements, and platform-specific library/graphics/audio
   behavior on Windows, macOS, Linux Wayland and Linux X11.

Deferred acceptance: decoded thumbnail generation/cache and custom pointer
hiding, only once implemented. Real track/chapter/speed menus and drag/hover
text previews belong to the current acceptance scope, not this deferred list.

**Open implementation assumptions:** the shortcut mapping, 2.5-second hide delay,
narrow-window layout, and minimize/occlusion optimization above remain proposed
interaction details until native review. Library lookup/configuration and redistribution packaging must be
confirmed by the implementation owner. Evidence above is scoped; remaining
acceptance and any post-review reruns must be recorded separately.

## Cinema Dock presentation correction

The bottom chrome is not a card. The approved wireframe's `.dock` is only a
content position (28 px sides, 23 px bottom); it has no background, border or
radius. Native implementation: a full-width, edge-attached `scrim_max` backing
behind the time row, timeline and controls (so text stays contrast-safe), with a
72 px transparent-to-`scrim_max` fade above it that ends at the same colour. The
vertical volume panel intentionally remains a floating panel at middle right.
Menu anchoring uses the measured controls height plus the 23 px bottom inset.
The subtitle safe zone additionally reserves the 72 px fade (plus 8 px
clearance) so burned-in subtitles are not dimmed by it; it is applied only while
the chrome is visible.

Validation: root formatting, strict locked workspace Clippy, and locked workspace
tests passed. Lead-reviewed isolated Linux bright-media captures at 1100×680 and
720×440 confirm the edge-to-edge backing, upward fade, readable time/chapter row,
and subtitles above the fade. Menu placement and pointer-click mute also passed.
Local evidence is under ignored `target/ui-probe/cinema-dock-edge-8qtV89/`
(layout/menu), `cinema-dock-edge-final-nZmh2N/` (pointer mute), and
`cinema-dock-edge-subtitles-PwoEbX/` (final subtitle clearance). This is scoped
presentation evidence, not audio, hardware-decoding, or cross-platform acceptance.

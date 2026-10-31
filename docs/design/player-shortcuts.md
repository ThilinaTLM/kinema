# Embedded player keyboard and mouse implementation plan

**Status (2026-10-31): source implemented; automated and scoped Linux synthetic-media/input checks passed.**
The user approved broad mpv-style controls, video double-left-click fullscreen,
video single-right-click play/pause, and a Settings reference page. The source
implements this expansion; the validation section below distinguishes automated
and synthetic-media evidence from unqualified platform/media behavior.
This extends [Player](player.md); external mpv/VLC behavior is unchanged.

## Research findings

- `crates/kinema-desktop/src/screens/player_window.rs`: `bind_keys` already binds
  Space/K, arrows, M, F, Escape and Tab traversal. Space uses the surface context;
  other actions use the player context. Root handlers defer while menus are open.
  Surface pointer handlers, control tooltips, focus traversal and command helpers
  are the integration points. Do not create another input engine.
- `screens/player_controls.rs` has existing control policies and unit tests.
- `crates/kinema-mpv/src/mpv/player.rs` provides asynchronous exact seeks, speed,
  audio/subtitle selection and subtitle positioning. `state.rs` exposes observed
  tracks, chapters, speed and seekability. Visibility, delay and frame stepping
  need typed APIs; subtitle visibility/delay need observations.
- `screens/settings_window.rs`: extend `Section`, `Section::ALL`, labels and the
  content match. Existing sidebar navigation and scrollable content can be reused.
  Extend colocated navigation tests and `design_lab.rs` settings fixture routing.
- Existing design describes double-click fullscreen, but the inspected surface
  handler does not establish implementation. Verify the complete event path;
  do not assume the gesture works because it is documented.
- Upstream [mpv stable manual](https://mpv.io/manual/stable/) documents
  `frame-step`, `frame-back-step`, `sub-visibility` and `sub-delay`. Reverse stepping
  is seek-based and can be slow/inaccurate; audio-only stepping is unsupported.
  Verify command compatibility with the project's supported native libmpv version,
  rather than assuming the latest manual matches every installed runtime.

## Default input contract

| Input | Action |
| --- | --- |
| Space / K | Toggle play/pause; Space activates a focused control instead |
| Left / Right | Seek -/+10 seconds |
| Shift+Left / Right | Seek -/+60 seconds |
| Ctrl+Left / Right | Seek -/+1 second (literal Ctrl, including macOS) |
| Up / Down | Volume +/-5 percentage points, capped to 0–150 |
| M | Mute without losing volume |
| F | Toggle fullscreen |
| Escape | Dismiss help/menu first; otherwise cancel drag, then exit fullscreen; never close playback |
| [ / ] | Previous/next speed preset |
| Backspace | Reset speed to 1× |
| , / . | Previous/next video frame, leaving user intent paused |
| Page Up / Page Down | Previous/next chapter |
| J / Shift+J | Next/previous subtitle track, including Off |
| V | Toggle subtitle visibility without discarding selected track |
| # | Next audio track (does not include Off) |
| Z / X | Subtitle delay -/+0.1 seconds |
| ? | Open shortcut help |
| Tab / Shift+Tab | Reveal controls and traverse enabled controls |
| Enter / Space on control | Activate that control once |
| Double left-click on video surface | Toggle fullscreen once |
| Single right-click on video surface | Toggle play/pause once |
| Single left-click on video surface | Focus surface and reveal chrome; no playback toggle |

Native OS close shortcuts remain native. No Q-to-close binding is added.

### Behavior details

- Reuse current speed presets: 0.5, 0.75, 1, 1.25, 1.5, 1.75, 2×. Select the next
  strictly lower/higher preset for an intermediate observed speed; stop at bounds.
- Chapter navigation chooses the adjacent chapter relative to the current chapter
  (last start at or before position). Do not wrap or invent chapters. Before the
  first chapter, Next selects the first; Previous does nothing.
- Track cycling follows observed list order, filters by track kind, wraps, and
  uses actual IDs, never indices as IDs. Subtitle Off is a distinct selection.
  Selecting a subtitle explicitly makes it visible; visibility toggle with no
  selected subtitle reports unavailable rather than silently choosing a track.
- Positive subtitle delay means subtitles appear later. Use finite bounded delay
  values (initial UI safety range -60 to +60 seconds). Reset delay to zero and
  visibility to true on media replacement; preserve existing speed lifetime policy.
  Reset must be worker-ordered with load so old queued work cannot affect new media.
- Seeking uses existing capability/readiness and clamping rules. Unsupported or
  unknown state is not a successful empty result. Frame stepping requires loaded
  video; reverse additionally requires seekability. Set user pause intent and
  serialize pause/step as one accepted worker operation, not two independently
  droppable submissions. Report failures without claiming a frame advanced.
- Allow held-key repeats for seeks, volume, speed and delay. Suppress repeats for
  toggles, cycling, chapters, help and frame stepping (especially expensive reverse
  stepping). Bound/coalesce pending adjustments so repeated input does not flood
  the worker or lose increments by repeatedly reading stale observed state.
- Text editing, menu/help navigation and focused widget semantics take precedence.
  Unrelated windows never receive player actions. Help restores previous valid
  focus on dismissal and prevents chrome auto-hide while open.
- Mouse gestures belong to the video presentation surface, including letterboxing,
  not the window root. Controls, sliders, menus and help exclude these gestures.
  Use native click counts/timing; never build a double-click timer. Handle right
  click at one event phase only; suppress a surface context menu for this gesture.
- Feedback for adjustments must distinguish requests from observations and errors.
  Commands remain asynchronous; no blocking work in render/input handlers.

## Shared catalog and Settings

Add a small desktop-owned `player_shortcuts` module (exact location following
existing module organization). It defines action IDs, binding/context descriptors,
repeat policy, group, display labels and mouse gesture descriptions. Generate key
registration and help rows from the same definitions, with a typed action adapter;
avoid an independent table of help strings that can drift from live bindings.
GPUI key syntax and presentation labels belong in desktop, not `kinema-core`.
Reusable domain policies, if extracted, belong in core without GPUI dependencies.

Add **Settings → Keyboard & Mouse**, grouped by Playback & Seeking, Volume & Audio,
Subtitles, Speed & Frames, Chapters & Navigation, and Mouse. Clearly label these
as embedded-player controls, not global or external-player shortcuts. Include
focus rules and unsupported-media notes. Show visible action/key text with
accessible headings, readable narrow-window wrapping and existing scrolling.
No settings persistence or preference schema changes are needed for a reference.

Player `?` help and tooltips use the same catalog. Include a discoverable help
entry/button so symbol-key layout difficulties do not make help inaccessible.
Rebinding, conflict detection, unbinding and reset-to-defaults remain a subsequent
phase; no mpv input.conf import or arbitrary-command editor is included.

## Implementation sequence and ownership

1. **Contract/catalog (desktop owner):** establish catalog, typed action mapping,
   repeat/focus policy and pure policy tests. Verify GPUI logical symbol matching
   for `?`, `#`, brackets and Shift+J; do not assume US physical key positions.
2. **Backend (backend owner):** typed frame/visibility/delay operations, observed
   values, validation, ordered lifecycle resets, command and failure-path tests.
   Keep native work on existing workers and reject obsolete-session results.
3. **Player integration (desktop owner):** existing actions plus new controls,
   mouse hit-testing, help, feedback and catalog-backed tooltips. Coordinate new
   backend API names before editing call sites.
4. **Settings (settings owner):** new reference section and fixture; can proceed
   once the catalog interface is stable. Do not concurrently edit player files.
5. **Integration (lead):** reconcile player design/TODO with implemented scope,
   run combined checks, inspect native evidence and record remaining limits.

No new dependency, legacy migration, renderer change or database change is needed.
A minimal durable GPUI patch (`vendor/patches/0012-gpui-pre-0.3.9.patch`)
exposes native `is_held` before action dispatch, avoiding unreliable layout-dependent
key-up latches. Its source/patch inventories and offline reproduction are verified.

## Acceptance and validation

- Catalog tests: unique context/key bindings, registered actions represented in
  help, shared tooltip labels, and explicit mouse-only entries.
- Policy tests: modifier distinctions, repeat policy, speed edges, chapter
  boundaries, unavailable/empty metadata, track IDs and Off, delay bounds/reset.
- Backend tests: exact command arguments, invalid values, queue-full/disconnect,
  ordered pause-and-step/load behavior, observed property updates and stale work.
- GPUI interaction tests where supported: Space on surface versus button, focused
  text input, menu/help precedence, Escape ordering, separate windows, repeats,
  control/overlay exclusion, double-click single toggle and right-click single
  toggle. First establish the test harness; existing pure tests are not proof of
  event routing. Use native probes/manual scenarios where synthesis is unavailable.
- Settings: navigation wraparound, complete reference groups, dark/light and
  minimum-window visual review; help opens/dismisses and restores focus.
- Native synthetic media: seeking, pause/frame behavior, audio/subtitle cycling,
  delay/visibility observations, EOF, replacement/retry and unsupported audio-only
  behavior. Include non-US layout and Windows/macOS shortcut checks separately;
  Ctrl+arrow may be intercepted by macOS system shortcuts. Record conflicts rather
  than claiming cross-platform support from Linux-only tests.
- Run from root on Rust 1.97.1:
  `cargo fmt --all -- --check`,
  `cargo clippy --workspace --all-targets --locked -- -D warnings`,
  `cargo test --workspace --locked`.

## Implementation evidence and remaining limits

- Final lead checks passed: root format check, strict locked workspace all-target
  Clippy, and locked workspace tests (69 core + 661 desktop + 116 mpv passed;
  27 ignored). Tests used `TMPDIR="$PWD/target/shortcut-test-tmp"` after an initial
  run encountered explicit `/tmp` quota errors in storage/reveal tests; no test
  was disabled to obtain the pass.
- Lead independently passed strict `ui-probe-test` all-target Clippy and the
  feature-enabled `real_dispatch_repeat_help_focus_and_surface_hit_testing` test.
  This dispatches actual mocked GPUI key/mouse events for held/released toggles,
  symbol keys, help/dismiss focus, surface gestures and overlay exclusions. It
  is not native window-manager or real-player gesture qualification.
- Shared catalog: `crates/kinema-desktop/src/player_shortcuts.rs`; Settings, player
  help, bindings and shortcut-bearing tooltips consume it. Rebinding remains deferred.
- The lead independently ran the ignored native test
  `cargo test -p kinema-mpv --locked real_player_shortcuts_subtitles_and_frames -- --ignored --nocapture`:
  passed, observed synthetic CPU frame positions 1.0 → 1.1 → 1.0 seconds, paused
  stepping, subtitle visibility/delay and replacement-load reset.
- The lead built `ui-probe` with `--features ui-probe --locked`, captured the
  `settings-keyboard-mouse` scene in isolated Xvfb at 960×600 dark/light, and
  personally reviewed the first visible groups. Local ignored captures:
  `target/ui-probe/shortcuts-settings-{dark,light}/native-first-frame.png`.
  This is not a visual review of every scrolled row or native OS accessibility.
- Native frame accuracy for arbitrary media, symbol/layout routing and OS-reserved
  keys remain validation risks. Windows/macOS, HDR and real providers remain
  unqualified; no change to external players or standalone libmpv input handling.
- Independent source review found and prompted a fix for load-operation enqueue
  failures leaving startup marked loading. Failure injection now covers each
  command in the reset/reset/load operation and obsolete-generation protection.
  Desktop source review found no concrete defect, but subsequent native X11 smoke
  exposed a shifted-symbol key-up mismatch that left help latched. The final fix
  uses native repeat state instead of key-name latches, with a captured-event
  regression and durable vendored GPUI patch. Lead verified the source inventory
  and zero-fuzz offline reproduction (`python3 -B vendor/verify.py` and
  `--reproduce target/shortcut-vendor-reproduce-final`).
- After rebuilding, the lead ran the isolated X11 fixture at 720×440 and personally
  reviewed help reopening after Shift+/ → Escape → Shift+/, then closing with
  Tab/Enter. Captures/script: `target/ui-probe/shortcuts-input/final/` and
  `target/ui-probe/shortcuts-input/final-smoke.sh`. Earlier captures also establish
  the visible dismiss-button focus ring; help includes a scroll cue.
- Native GPU playback could not start in this isolated Xvfb environment
  (`gpu-device-unsupported`), and no window manager was available to qualify
  native fullscreen. Mouse event routing is covered by GPUI dispatch tests, not
  claimed as real native GPU playback/fullscreen acceptance.
- The bounded worker queue rejects saturation; requested speed/delay/volume/seek
  targets are reconciled with observations. Exhaustive repeat-flood/coalescing
  qualification remains separate from normal key-repeat tests.


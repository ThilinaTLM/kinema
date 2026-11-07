# Playback

There are three player choices in Settings: **Embedded MPV** (the default),
**external mpv** and **VLC**. Every player receives the same thing, which is a
loopback URL for one lease (see [media](media.md)). This page covers the
embedded player, external launching, progress and resume, and OS integration.

## Embedded player architecture

```mermaid
flowchart LR
  subgraph desktop["kinema-desktop"]
    Win["Player window<br/>ui/views/player"] --> Reg["Player registry<br/>(session → window, generations)"]
    Win --> Prog["controllers::playback_progress"]
    Desk["desktop_integration<br/>(OS media session, inhibitor, tray)"] --> Reg
  end
  subgraph mpv["kinema-mpv"]
    Player["Player"] --> Ctl["Control worker<br/>(bounded command queue)"]
    Player --> Rend["Render worker<br/>(GPU surface, latest frame only)"]
    Ctl --> Lib["libmpv (dynamically loaded)"]
  end
  Win -- "commands / observed state / GpuSurfaceFrame" --> Player
```

- **Split of responsibility.** `kinema-mpv` owns libmpv loading, commands,
  events, decode and render workers, GPU interop and teardown. It knows nothing
  about the Library, SQL or GPUI views. `kinema-desktop` owns the window,
  chrome, focus, sessions and progress. **All `unsafe` and FFI code lives in
  `kinema-mpv`.**
- **The player is an independent window** with one window per lease. It is not
  a pane in the browser and not a separate mpv process.
- **Commands return acceptance, not success.** Only observed mpv state is truth.
  A requested seek, speed, volume or track is not proof that it happened.
  Unknown metadata is different from known-empty metadata (`*_available`
  flags).
- **Rendering never blocks** on I/O, decoder commands or worker shutdown. Frame
  handoff is bounded and keeps only the latest frame. Retired images use a
  bounded two-slot pipeline.
- **Every async result carries a window, session and generation tag.** A frame
  or event from a closed or replaced session must never update another player.
- **mpv is locked down.** Kinema sets `config=no`, `load-scripts=no`, `osc=no`
  and `osd-level=0`, and turns off the default input bindings, so it never
  inherits the user's mpv config, scripts or OSC. External mpv, by contrast,
  keeps the user's configuration, and Kinema never copies or edits it.
- Player init and worker joins block, so `Player::new` runs off the UI thread.
  Native cleanup that might hang is *quarantined*: its owners are retained
  rather than torn down unsafely.

> **Known debt:** libmpv, codecs and GPU drivers run inside the Kinema process,
> so a native crash takes down the whole app, including downloads and the
> Library. Fix: [player process isolation](../proposals/player-process-isolation.md).

### GPU-surface rendering

The only embedded renderer is the **GPU-surface** path, enabled by the default
Cargo feature `gpu-surface`. Each OS path hands mpv's output to GPUI's own GPU
device without a CPU copy:

| OS | Path |
| --- | --- |
| Linux | OpenGL render into Vulkan buffers shared through external memory, on GPUI's WGPU (Vulkan) device and queue. It uses explicit `GL_EXT_semaphore_fd` ↔ `VK_KHR_external_semaphore_fd` sync. `glFinish` or CPU waits are not enough |
| Windows | ANGLE GL through D3D11On12 on GPUI's D3D12 device and queue. WGPU is DX12-only here. It needs the ANGLE `libEGL.dll`/`libGLESv2.dll` next to the executable, with the device-creation and D3D-texture extensions |
| macOS | Legacy CGL rendering into IOSurface-backed `CVPixelBuffer`s, presented through GPUI's Apple BGRA surface path |

Rules:

- **MUST: fail closed.** A missing compiled feature, GPU capability or hardware-decode
  proof, or an HDR/PQ/HLG transfer, produces an explicit failure
  (`hdr-unsupported`, `gpu-device-unsupported`, …). Never fall back to CPU
  readback, and never switch to another player automatically.
- **Use only GPUI's device.** Never create a private second device to work
  around a missing adapter. With multiple windows, the GPU context accessors
  return `None` when live renderers use different devices.
- **Gate the queue.** Every raw producer submit and every normal WGPU submit
  holds the same queue gate. A poisoned gate is fatal.
- Device-loss recovery is not supported. **Only SDR** is accepted (bt.1886,
  bt.709, sRGB, gamma 2.2 and 2.8). See the [HDR proposal](../proposals/hdr-playback.md).

GPUI 0.3.9 has no API for externally produced GPU textures, and its Windows
backend constructs its own DirectX renderer. So the root workspace patches
`gpui-pre`, `gpui-pre-wgpu`, `gpui-pre-apple`, `gpui-pre-windows` and `wgpu-hal`
through [vendored sources](development.md#vendored-patches). This is a durable
fork with reproducible patches, not edited registry sources.

### Native libmpv

libmpv is **not bundled**. Embedded playback needs a trusted libmpv with client
API major version 2. Lookup order is `Config.library_path`, then
`MPV_LIBRARY_PATH`, then the OS loader (`libmpv.so.2`, `libmpv.2.dylib`,
`mpv-2.dll`/`libmpv-2.dll`). An installed `mpv` executable alone is not enough.
Loading a library runs native code, so never discover one from media folders or
an untrusted working directory. Dynamic loading does not waive the GPL/LGPL
obligations of libmpv or FFmpeg. Distribution is an open question (see the
[desktop releases proposal](../proposals/desktop-releases.md)).

## Window, lease and session ownership

- **One embedded window per lease.** Replaying a lease focuses its existing
  window instead of creating a second owner. Different leases can have their own
  windows at the same time.
- **MUST: closing a player** stops its decoder, flushes progress and releases its
  lease **exactly once**. It is not "delete cache" and not "stop download".
- **Replay keeps the original backend.** A changed player preference applies to
  new sessions only. A lease that was used externally is never converted to
  embedded ownership.
- **Pause is decoder intent**, not a download pause. **Buffering** is observed
  starvation, not a user pause. Recovery resumes only if the user still intends
  to play.
- **Startup readiness:** the initial load has a 60 s budget, and seek
  confirmation has 10 s. A launch with no checkpoint that fails says "Playback
  could not start". It never says "Saved position cannot be resumed".

### External players

External mpv and VLC are explicit choices, launched with **argument lists**
(custom paths must be absolute executables, never shell commands). Kinema never
kills them and never infers their state from traffic. Launching one counts as
"launch requested", never as "playing". On quit, Kinema's server stops and the
external player loses its stream.

## Player chrome and input

The chrome is a **Cinema Dock** built from design-system tokens on
`Palette::cinema()` (always dark):

- The video is aspect-fit with near-black letterboxing and is never stretched or
  cropped. A compact title sits at the top.
- An edge-attached bottom scrim (`scrim`, with a fade above it, not a card)
  holds the time and chapter row, a 12 px seek rail inside a 32 px target, ghost
  transport buttons on the left (play/pause, ±10 s), and chapter, subtitle,
  audio, speed and fullscreen controls on the right. Volume (0–150) is a
  vertical panel at middle-right.
- The chrome auto-hides after `CHROME_IDLE` while playing. It never hides while
  a control has focus or hover, during a drag, while a menu is open, or when
  paused, buffering, loading, ended or failed. The subtitle safe zone moves
  `sub-pos` only while the chrome is visible.
- **Show only observed capabilities.** Don't invent chapters, tracks, buffer
  ranges or HDR badges. An unknown duration shows `—`. Seek previews are
  text-only (time and chapter); thumbnails are a
  [proposal](../proposals/player-enhancements.md).

### External subtitles

The embedded player can add subtitle files the user already has. Online search
is a [proposal](../proposals/external-subtitles.md).

- **Two entry points:** **Add file** in the subtitle menu (native dialog,
  several files allowed) and dropping files anywhere on the player window. The
  subtitle menu opens whenever track metadata is known, even with no tracks,
  so **Add file** is always reachable once the media is loaded.
- **Admission** (`kinema_core::subtitles`) runs off the UI thread: a supported
  extension (SRT, ASS, SSA, VTT, SUB, SUP), a regular non-empty file up to
  64 MiB, and an absolute UTF-8 path. At most `MAX_SUBTITLE_FILES` (8) per
  request; the rest are counted as skipped. The dialog has no type filter on
  any platform, so admission is the filter.
- **MUST: files are read in place.** Kinema never copies, moves or deletes a
  user's subtitle file, and never logs its path or name; logs carry counts and
  outcome categories only.
- **Loading** is one generation-tagged `Player::add_subtitles` operation
  (`sub-add … auto` for each file, `select` for the last). A racing load makes
  it stale. mpv may load the files concurrently, so track order need not follow
  request order; the last requested file is still the one selected. External tracks belong to one load: mpv drops them on reload or
  replay, and they are not remembered across sessions.
- **Outcome is observed, not assumed.** Success is a new track with
  `Track::external`; an mpv error, or no new track within
  `SUBTITLE_LOAD_TIMEOUT`, reports "Subtitle failed". Each outcome is a short
  dock status for `PLAYER_NOTICE` ("Subtitle added", "Not a subtitle",
  "Player not ready" …), with the full wording in the status tooltip.
- While files are dragged over the window a full-window hint names what a drop
  will do: "Drop to add", "Not a subtitle" (judged by name only) or "Player not
  ready". Drop support is code-read for X11, Wayland, Windows and macOS and
  tested through GPUI dispatch, not yet by hand on each platform.

### Track labels

Audio and subtitle menus use `kinema_core::tracks`, never raw metadata:

- **Language first**, as a name: `en-IN` → "English (India)", `eng` →
  "English". Unknown codes fall back to the code in capitals; `und`/`mul` mean
  no language.
- **Titles only when meaningful.** Web addresses and watermarks
  ("www.…", "site.org"), and titles that repeat the language, are dropped.
  Real titles stay ("English · Commentary").
- **Added files drop what is playing from their name.** A leading title
  (catalog name, or the playing file's stem), then any year, episode tag
  (`S01E04`, `1x04`) and episode name are removed, as is a trailing language
  tag: "Immortal.2026.1080p.WEB-DL.en.srt" reads "English · 1080p WEB-DL".
  Matching ignores case and punctuation and stops at word boundaries; if
  nothing matches or nothing is left, the whole name is kept. This is display
  only and never identity. Dots become spaces except in "5.1" and "H.264";
  names longer than 40 characters end at a word with "…".
- **Tags** from observed track flags: Forced, SDH; audio adds the layout
  (Mono, Stereo, 5.1, 7.1).
- **Source in the dimmed right column:** "Built-in" or "File · SRT" for
  subtitles, the codec (AAC, E-AC-3 …) for audio.
- Rows that would read the same are numbered in order ("English 1",
  "English 2"). Identity is still the mpv track ID; labels never select.

**Shortcuts** come from one catalog, `player_shortcuts.rs`. The player's `?`
help, Settings → Keyboard & Mouse, and the tooltips all read from it, so add or
change a binding there; it is the authority for the binding list.
**Default: Escape never closes playback.** It closes help or a menu, then
cancels a drag, then exits fullscreen. *Why:* a stray key must not end a film. Shortcuts are scoped to the focused player and never take
keys away from a text input or an open menu. Key repeat uses GPUI's
`KeystrokeEvent::is_held` (vendored patch 0012), not latches keyed on the key
name.

## Progress and resume

Progress is a separate personal fact, distinct from watched and from Library
membership (see [data](data.md#personal-records)).

| Action | Watched | Saved progress |
| --- | --- | --- |
| Mark watched | Set | Kept |
| Mark unwatched | Cleared | Kept. Auto-marking is suppressed for the rest of this run |
| Clear saved progress | Unchanged | Deleted (epoch bumped) |
| Play from beginning | Unchanged | Reset only once the new playback is ready |
| Seek backward | Unchanged | Saves the smaller position |
| Confirmed position ≥ 90% of a finite duration | Set (one-way) | Kept |

- **The identity is typed:** `PlaybackSubject::Movie | Episode | Untracked`.
  Local files with no catalog identity play normally, but do not save progress.
  Never derive identity from file names.
- **Run fencing:** each run increments a durable per-item epoch. Writes carry
  `(subject, epoch, run, sequence)`, and stale writes are rejected inside the
  transaction. The latest run owns writes. Never merge positions by taking the
  `max`.
- **Resume sequence:** load paused, wait for seekability, seek precisely, wait
  for the observed position, then apply the user's play/pause intent. A position
  of zero during loading never overwrites a checkpoint. If the new source's
  duration differs by more than max(30 s, 2%), ask the user instead of remapping.
- **Cadence:** checkpoints are coalesced to at most one every 5 s, and flushed
  on pause, seek, close, source replacement and quit. A crash can lose the last
  interval.
- **UI:** an unwatched item with progress shows **Resume · mm:ss** (offered only
  for the embedded player, which performs the seek) and **Play from beginning**.
  Watched items hide catalog progress bars.
- The pure policies live in `kinema-core::watch_progress`: eligibility, the 90%
  rule and next-up.

## OS integration

`desktop_integration/` is owned by the app, not by Main.

- **OS media controls** (souvlaki, vendored) route to **one selected player**:
  the focused or explicitly chosen player, otherwise the most recently started
  one. They send the same commands as the Cinema Dock, not commands to a second
  controller. Only supported actions are advertised, so Next and Previous stay
  disabled. Only a safe title and description, the position and artwork are
  published, never URLs or paths. Untrusted OpenUri requests are ignored.
- **The sleep and idle inhibitor** (keepawake) is process-wide. It is held while
  *any* embedded video is actively playing, with a 30 s buffering grace. A
  denied inhibitor is a nonfatal capability failure, and Kinema never overrides
  an explicit lock or suspend.
- **The tray** (tray-icon, KSNI on Linux, with no GTK or AppIndicator) has only
  Show/Hide, Settings and Quit. Its behavior is described in
  [runtime](runtime.md#main-window-tray-and-quit). Hiding the main window uses
  the vendored `Window::{hide_window, show_window}` patches. Wayland has no safe
  hide, so it falls back to minimize.

## Change risk

- **High: GPU-surface interop, vendored patches and `unsafe` FFI.** Mistakes
  cause crashes or GPU hangs that are hard to test. Read the
  [patch notes](../../vendor/patches/README.md) first, and test on real hardware.
- **High: progress fencing.** It protects personal data.
- **Medium: lease and window ownership.**
- **Low: chrome layout and shortcut bindings** (keep the single catalog).

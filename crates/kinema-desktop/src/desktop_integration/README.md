# Desktop integration runtime and qualification

Research/implementation date: 2026-10-28. Rust **1.97.1**, edition 2024.
These are concrete adapters and owned workers, **not native acceptance evidence**.

## Dependency/API evidence

`cargo info`, actual fetched source inspection and compilation confirmed:

| Dependency | Configuration | Evidence |
| --- | --- | --- |
| souvlaki =0.9.3 | repository-owned exact-release patch, `use_zbus`, no defaults | public `ActionCapabilities`; real registration, metadata/state publication and callbacks |
| keepawake =0.6.1 | library only | RAII native handles, blocking acquisition/release |
| tray-icon =0.26.1 | no defaults; Linux `ksni` | non-GTK D-Bus tray service; native Windows/macOS elsewhere |
| ksni 0.3.7 | transitive | worker-owned D-Bus service; no GTK loop |
| zbus 5.x | lockfile 5.19.0; blocking-api + async-io | independent Linux host-presence/owner probe |

`cargo tree -p kinema-desktop --locked -e features` confirms KSNI/muda snapshot
without GTK/AppIndicator. `org.kde.StatusNotifierWatcher` is a cross-desktop protocol
name, not a KDE library dependency. Souvlaki 0.9.6 was queried and does not exist;
0.9.3 is the actual release used. Its upstream hardcoded queue/seek advertisement
is fixed by `vendor/souvlaki`, not by changing registry sources or silently
ignoring callbacks. See `vendor/souvlaki/KINEMA-PATCH.md` for source evidence.

- <https://docs.rs/souvlaki/0.9.3/souvlaki/>
- <https://docs.rs/keepawake/0.6.1/keepawake/>
- <https://docs.rs/tray-icon/0.26.1/tray_icon/>

## Coordinator/lifecycle API

```rust,ignore
let (tx, rx) = desktop_integration::command_channel(64);
// Only after GPUI's native loop and a native window are ready:
let mut runtime = Runtime::start_after_native_ready(tx, RuntimeConfig::default())?;
runtime.update(RuntimeSnapshot { players, selected, activities });
let status = runtime.poll();
let can_hide_main = status.tray_usable_at(Instant::now());
```

Keep Runtime in the application owner, **not Main**. It owns a Linux tray/media
worker plus an independent inhibitor worker. Windows/macOS native tray/media
remain in the UI-owned Runtime; start/poll/drop on GPUI's running UI thread.
Runtime is deliberately `!Send`. Icon data is RGBA in memory; the default is a
small K glyph, with no path or temporary-file lifetime dependency. Artwork is
omitted (souvlaki's macOS URL artwork loader is synchronous).

`update` replaces a single latest-value mailbox; it never queues unbounded state.
Workers wake on updates and at least once/second for host health/buffering expiry.
Failed registrations are retried independently every five seconds, preserving
working registrations. Poll from the owned GPUI coordination task at least twice
per second. Windows/macOS polling applies native updates on the UI thread; Linux
polling only reads shared status. OS errors are nonfatal capability failures in
`native_error` / `inhibition_error`; coordinator should display a given failure
once rather than log/spam on every sample.

`RuntimeStatus` reports media registration, inhibitor acquisition and tray health.
`tray_usable_at(now)` includes its explicit freshness deadline and shutdown state.
A cached `tray_usable` bool alone must never authorize hiding Main. A stalled or
failed Linux worker causes health to expire and restores Main. Restore on
`Command::TrayHostLost` too; the bounded channel is best effort, so its event
cannot replace the independent health deadline. Missing tray retains Main and
uses the existing Minimize / Quit / Cancel fallback.

### Commands and generations

Drain `CommandReceiver::try_recv()` in that same owned GPUI task. Native callbacks
only enqueue bounded typed commands and never call GPUI or mpv. Overflow drops a
command, never waits. Drop the receiver first on final shutdown to revoke clones;
do not dispatch any commands after quitting begins.

`Selection::{started, focus, closed, selected}` implements sticky explicit focus,
including paused players, otherwise the most recently started live player. New
generations replace old generations of a reused session ID. Progress samples
must not call started/focus. Runtime consumes the resulting selected target;
inhibition always consumes the complete separate all-player activity list.

For transport, call `command_matches_selection` then validate the actual live
player generation. ShowPlayer/SelectPlayer also require live-generation checks.
Use the *same* player commands as the UI, not a second mpv controller. Session IDs
in this adapter contract are application-assigned numeric routing IDs; if the
player registry uses strings, retain a stable application-owned mapping and never
reassign an ID to a different player without changing generation.

Volume is normalized 0..1, finite and clamped; map to mpv percent units. Apply
volume deltas/mute against observed player state. Absolute and relative seeks
require the selected player to be seekable. Publish resulting state only after
observing mpv. Next/Previous/shuffle/OpenUri are not implemented and are not
advertised; incoming unsupported events are ignored. Raise/Quit use existing
restore/confirmation paths, never arbitrary URL launches. Media snapshots contain
public catalog titles/descriptions only, never source URLs, local filenames,
provider/account details or credentials. Duration/position are typed Duration.

### Windows retained media host

By default Runtime retains a clone of tray-icon's own native handle and uses its
HWND. tray-icon documents that HWND as valid for the icon's lifetime; the retained
clone therefore owns its lifetime independently of Main. No new host-window
subsystem is needed. `RuntimeConfig::media_host` accepts an optional
`Rc<dyn WindowsMediaHost>` override. That guard exposes `hwnd() -> NonZeroIsize`
and must own the lifetime of a **live native host**, not just its numeric handle.
The media adapter retains it and detaches before releasing it. If neither an
explicit host nor a valid retained tray HWND exists, registration returns a
specific media capability error; inhibition and any usable tray still run. Never use a destroyed Main HWND after
close-to-tray. Retaining a hidden, non-destroyed Main or a dedicated host is an
application ownership decision. GPUI Window implements HasWindowHandle, making
extraction feasible; it does not prove native lifetime qualification.

## Tray health and OS limits

KSNI's constructor uses `assume_sni_available(true)`, so Linux creation alone does
not prove visibility. Runtime separately probes IsStatusNotifierHostRegistered and
the watcher bus owner. Missing host/query failure/stale health revokes usability;
watcher-owner change drops/recreates tray registration. Health expires after five
seconds. The Linux adapter's service is non-GTK on X11 and Wayland, but actual
shell behavior on either session remains unqualified.

On Windows/macOS a successful native construction marks tray usable. tray-icon
has no comparable qualified host-loss notification/probe there, so this is weaker
than Linux host health and cannot claim shell-restart recovery. Runtime refreshes
the health lease while its UI owner is polling. Native creation/drop still require
a running Windows message loop / macOS main loop. No undocumented GPUI last-window
or Dock restoration behavior is assumed by this module.

## Inhibition and shutdown

The dedicated worker creates/acquires/releases keepawake on the same thread,
including Windows thread-scoped power state. It considers **all** embedded video
players; one paused player cannot release another playing player's inhibitor.
Paused/stopped/EOF/failure are Inactive. Buffering retains the original interval
start and expires after 30 seconds even without new observations. External
players are excluded. `display(true).idle(true).sleep(false)` requests automatic
idle/display protection without keepawake's explicit-suspend inhibition. Denial
is an error, never claimed protection; managed policies/explicit lock may win.

Call `runtime.shutdown()` before quitting, then poll `shutdown_complete()` from
the existing bounded application quit gate. Cancellation wakes both workers;
completed JoinHandles are joined without blocking the poll. Thread-affine native
handles are dropped on their owning thread. Native libraries/DBus calls already
in progress cannot be interrupted. Runtime Drop requests cancellation and joins
only completed workers; it never adds an unbounded UI-thread join after the quit
gate's deadline. If a native call outlives that deadline, the final owner drop
detaches its already-cancelled worker. That worker holds no GPUI entities and
releases its native handles on its creation thread when the call returns (or the
process exits). Normal shutdown retains/joins every handle via shutdown_complete.
No guaranteed native release at the deadline or hard OS-call latency is claimed.

## Validation and remaining native gates

Adapter tests cover selected routing/sticky focus/reused generation, invalid
volume, seeks, unsupported URI/events, bounded queue/revocation, all-player
inhibition and grace expiry, fake tray-host loss/restart/staleness, runtime health
expiry/shutdown and coalesced mailboxes. Vendor zbus tests cover actual advertised
properties, dynamic seek, pre-registration configuration, duration overflow and
registration failure on an invalid bus in an isolated child.

These tests do not qualify actual media keys, shell seek/app volume availability,
explicit lock/suspend behavior, tray unavailable/restarting, Windows HWND/COM,
macOS main-thread lifetime, or GPUI main-close while multiple players keep playing.
Record real OS/desktop/session versions and scenarios before claiming those.
Windows/macOS playback and HDR qualification is unchanged.

### Opt-in isolated Linux MPRIS protocol smoke

Reproduce from the repository root (requires `dbus-run-session` and coreutils
`timeout` on PATH):

```sh
cargo test -p kinema-desktop --lib --locked \
  desktop_integration::media_session::linux_smoke::isolated_mpris_session \
  -- --ignored --exact --nocapture
```

The ignored test self-spawns its own test executable under `dbus-run-session`,
with a 30-second external deadline. It changes only the subprocess environment,
never the user's session bus. Assertions read actual server properties with
property caching disabled; commands are actual MPRIS method calls. Detach waits
for the bus-owner-loss signal, not sleeps or polling.

Observed on 2026-10-28 using Rust 1.97.1 and D-Bus Message Bus Daemon 1.16.2:

- Registered `org.mpris.MediaPlayer2.kinema`; Identity was `Kinema`.
- Public fixture title/duration, position `72500000` microseconds, paused state
  and normalized volume `0.37` matched publication. No artwork URL was present.
- Next/Previous were false throughout; Seek changed from false to true when the
  selected player changed. Second metadata/position/playback also matched.
- Play delivered exactly target `41/3`; after selection, Pause delivered exactly
  target `72/9`. Requests did not optimistically alter published observed state.
- Empty selection cleared metadata/position and disabled controls; untrusted Play
  with no selected player delivered no command.
- Drop emitted name-owner loss, removed the MPRIS bus name, rejected later Play,
  and disconnected the callback channel with no outstanding command.

This is successful isolated D-Bus protocol evidence only. It exercises no real
provider, mpv playback, desktop shell/media keys, tray host, inhibitor, Windows or
macOS backend and changes none of their native qualification status.

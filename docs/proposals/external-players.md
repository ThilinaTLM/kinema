# External player discovery and reconnects

**Status:** Draft · **Area:** playback

## Problem

Players installed outside `PATH` and the fixed standard locations (for example
through a Windows registry entry, or a custom prefix) still need a typed
absolute path, and the user never sees which executable Kinema chose. When an
external player reconnects late, for example after a long pause, its lease may
already be gone.

## Current state

- Players are launched with argument lists. Custom paths must be absolute
  executable paths. Lookup checks `PATH`, then fixed standard install
  locations, silently at launch (see
  [playback](../architecture/playback.md#external-players)).
- Kinema never kills external players and never infers their state (see
  [playback](../architecture/playback.md#external-players)).
- The lease ends when the user stops the session or quits Kinema.

## Options

1. **Visible discovery:** add the Windows registry (App Paths, uninstall
   entries), and show the found executables in Settings so the user can
   confirm or pick one. Never execute anything found in untrusted directories.
2. **Reconnect grace:** after the last request, keep a lease alive for N minutes
   so the player can seek or resume, then release it.
3. **More players** (IINA, MPC-HC, Infuse via URL scheme), each with its own
   argument template.

## Open questions

- How long should the grace period be? Should it be different for local files
  and remote acquisitions?
- Should discovery happen on every launch, or only from Settings?

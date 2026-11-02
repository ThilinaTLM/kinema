# External player discovery and reconnects

**Status:** Draft · **Area:** playback

## Problem

External mpv and VLC must be installed on `PATH`, or the user has to type an
absolute path. When an external player reconnects late, for example after a long
pause, its lease may already be gone.

## Current state

- Players are launched with argument lists. Custom paths must be absolute
  executable paths. There is no automatic discovery.
- Kinema never kills external players and never infers their state (see
  [playback](../architecture/playback.md#external-players)).
- The lease ends when the user stops the session or quits Kinema.

## Options

1. **Discovery:** search the platform's standard install locations (the Windows
   registry and Program Files, `/Applications`, `PATH`) and offer the results
   to the user, who confirms them. Never execute anything found in untrusted
   directories.
2. **Reconnect grace:** after the last request, keep a lease alive for N minutes
   so the player can seek or resume, then release it.
3. **More players** (IINA, MPC-HC, Infuse via URL scheme), each with its own
   argument template.

## Open questions

- How long should the grace period be? Should it be different for local files
  and remote acquisitions?
- Should discovery happen on every launch, or only from Settings?

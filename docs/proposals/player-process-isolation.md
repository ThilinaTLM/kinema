# Player process isolation

**Status:** Draft · **Area:** playback

## Problem

libmpv, codecs and GPU drivers run inside the Kinema process. A native crash
(a bad file, a driver bug) takes down the whole app, including other players,
downloads and pending progress writes. Fixes the
[known debt in playback](../architecture/playback.md#embedded-player-architecture).

## Current state

`kinema-mpv` is a library in the same process. GPU-surface rendering shares
GPUI's device and queue, so frames reach the window without being copied.

## Options

1. **A separate player helper process** for each player window. It renders into
   a shared GPU texture (DXGI shared handle, IOSurface, dmabuf or Vulkan
   external memory) that GPUI imports. It needs cross-process synchronization,
   and new vendored GPUI import APIs.
2. **A helper process with its own native window,** embedded in or placed next
   to the Kinema window. This is simpler for interop but harder for chrome,
   focus and platform consistency.
3. **Keep in-process,** but harden it: run a risky probe of the media in a
   short-lived helper before loading it, and checkpoint progress more often so
   that a crash loses less.

## Open questions

- Is shared-texture import across processes feasible within GPUI's renderer on
  all three OSes?
- How does a crashed helper report its failure, and should Kinema offer to
  restart it?

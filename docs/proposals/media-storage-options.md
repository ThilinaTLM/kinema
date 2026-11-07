# Media storage options

**Status:** Draft · **Area:** media

## Problem

The media cache sits in a fixed location. Every file reserves its full size
against the quota. Acquisition keeps going while playback is paused.

## Current state

The design is in [media](../architecture/media.md): one payload and manifest
per asset, full-size reservation, manual cleanup of inactive assets only, and
no eviction. Pausing playback pauses the decoder only.

## Options

1. **Custom cache location.** The user picks a root folder, and Kinema creates
   and owns a dedicated subdirectory inside it, never adopting files that are
   already there. Moving assets between roots needs a rule: either move them or
   leave them and start fresh.
2. **Configurable checkpoint interval.** Expose the interval, or tune it per
   platform, trading durability against write load.
3. **Sliding-window caching for files larger than the quota.** Keep a pinned
   window around the play position, evict outside it, and fetch again on
   seek. This needs pinning, eviction, refetch and resume semantics. The RAM
   proof of concept did not show that this works for a disk cache.
4. **Download pause and prefetch suspension.** Pause or stop acquisition when
   the user pauses, or when the player is minimized or hidden. This is separate
   from decoder pause, and the UI must never suggest that pausing playback stops
   the download unless it really does.

## Open questions

- Should "Keep downloaded" (retained media that cleanup skips) exist? It is
  currently not planned.
- How should usage be shown when a custom root sits on a nearly full disk?

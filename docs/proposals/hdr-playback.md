# HDR playback

**Status:** Draft · **Area:** playback

## Problem

HDR sources (PQ/HLG) are rejected with `hdr-unsupported`. Users with HDR
content cannot play it in the embedded player.

## Current state

The GPU-surface path accepts SDR transfers only (see
[playback](../architecture/playback.md#gpu-surface-rendering)). GPUI presents
SDR surfaces. There is no tone mapping and no HDR swapchain configuration.

## Options

1. **Tone-map to SDR** in mpv (`tone-mapping`, target `bt.1886`). This needs no
   GPUI change and gives correct-looking SDR output.
2. **True HDR output.** HDR swapchain formats and metadata on each OS (DXGI
   HDR10, EDR on macOS, Wayland color management). This needs vendored GPUI
   changes and per-display capability detection.
3. Keep rejecting HDR, and offer to open the file in external mpv as an
   **explicit** user action.

## Open questions

- Is option 1 good enough as a first step?
- How should the app detect whether the display can show HDR, and when should
  it fall back?

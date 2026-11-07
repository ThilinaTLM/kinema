# Cross-platform qualification

**Status:** Draft · **Area:** platform

## Problem

Windows and macOS are first-release targets. So far they only compile and pass
unit tests in CI, and nobody has run the GUI, playback or OS integration on them.
Also fixes the [known debt on settings durability](../architecture/development.md#platform-status).

## Current state

See the [platform status](../architecture/development.md#platform-status).
Known gaps:

- **GPU-surface playback:** the Windows path (ANGLE D3D11On12, patch 0005) and
  the macOS path (CGL, IOSurface, runtime Metal shaders) have never run.
- **Keyring:** Windows Credential Manager and the macOS Keychain are untested.
- **Desktop integration:** the tray, media keys (with the HWND lifetime) and
  inhibition are untested on both.
- **Settings durability:** directory sync is a no-op on non-Unix systems.
- **Storage crash and power-loss behavior:** delete-while-open and atomic
  replace are untested.
- **Shortcuts on non-US layouts:** untested.
- **Linux:** upstream `LinuxPlatform::activate` is a no-op, and Wayland cannot
  hide the main window.

## Recommendation

1. **A per-OS runbook** that someone follows by hand and records results for:
   launch, browse, credentials, playback (1080p H.264/HEVC/AV1), seek into
   missing bytes, close and quit, tray, media keys, sleep inhibition, Unicode
   paths and cache loss.
2. **Fix the known gaps** that the runbook exposes, such as a Windows directory
   sync replacement and activation behavior.
3. **Later:** scheduled native workflows on self-hosted GPU runners.

## Open questions

- Who owns Windows and macOS hardware for testing?
- What are the minimum OS versions and architectures (x86_64, arm64)?

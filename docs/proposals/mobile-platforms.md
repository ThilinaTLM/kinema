# Mobile platforms

**Status:** Draft · **Area:** platform

## Problem

Users might want Kinema on Android or iOS. Kinema is a desktop app today.

## Current state

The UI is GPUI, which has no supported mobile backend. Embedded playback
depends on desktop GPU interop (see [playback](../architecture/playback.md)).
`kinema-core` is pure Rust and could be reused.

## Options

1. **Investigate only:** find out which UI framework would work, whether
   embedded playback is feasible on each platform, and how touch layouts would
   need to change. No support claim until it is validated on real devices.
2. **A separate mobile client** that reuses `kinema-core` and the provider
   adapters behind a different UI.
3. **Don't do it.** Move the item to "Not planned".

## Open questions

- Is a mobile app a goal at all, or only a nice-to-have?

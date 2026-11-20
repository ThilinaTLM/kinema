# Title backdrop on Browse

**Status:** Draft · **Area:** UI

## Problem

Browse uses a fixed brand-wash backdrop, while the approved visual language
calls for a blurred backdrop that follows the focused title.

## Current state

- Discover and Library use the fixed brand wash (see
  [UI › Browse](../architecture/ui.md)). Detail uses artwork-derived tints and
  a blurred hero image from `artwork::backdrop*`.
- The tokens and the `CROSSFADE` preset already exist.
- Next-page skeletons and smooth wheel scrolling have shipped (see
  [UI](../architecture/ui.md) and the
  [design system](../architecture/design-system.md)).

## Recommendation

Decode, downsample, blur and darken the focused title's backdrop off-thread,
rejecting stale results by generation. Crossfade between backdrops. Fall back
to the brand wash. Do not start any work on hover until the hover-intent delay
has passed.

## Open questions

- Should the brand wash stay as the identity of Discover and Library, with the
  title backdrop shown only when keyboard focus is on a card?
- Is blur affordable on low-end GPUs? Pre-blur at a small size and cache the
  result?

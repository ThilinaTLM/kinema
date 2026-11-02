# Browse loading and backdrop

**Status:** Draft · **Area:** UI

## Problem

When the next page loads, the grid can jump, because nothing reserves space
for the incoming cards. Browse uses a fixed brand-wash backdrop, while the
approved visual language calls for a blurred backdrop that follows the focused
title. Scrolling is not smooth.

## Current state

- Skeleton cards appear on first load only.
- Discover and Library use the fixed brand wash. Detail uses artwork-derived
  tints and a hero image.
- The tokens and the `CROSSFADE` preset already exist.

## Recommendation

1. **Next-page skeletons.** Keep the existing cards and append skeleton cards
   with the same column geometry in the next rows. Reserve their space so that
   scroll position and focus never jump when the page arrives. Skeletons are not
   interactive, and they are static under reduced motion. A failed next page
   becomes a retry row. This is different from a successful empty page.
2. **Title-responsive backdrop.** Decode, downsample, blur and darken the
   focused title's backdrop off-thread, rejecting stale results by generation.
   Crossfade between backdrops. Fall back to the brand wash. Do not start any
   work on hover until the hover-intent delay has passed.
3. **Smooth scrolling.** Use kinetic or interpolated wheel scrolling through a
   `motion::Tween`. Reduced motion turns it off.

## Open questions

- Should the brand wash stay as the identity of Discover and Library, with the
  title backdrop shown only when keyboard focus is on a card?
- Is blur affordable on low-end GPUs? Pre-blur at a small size and cache the
  result?

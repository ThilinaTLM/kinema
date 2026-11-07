# Visual baselines

**Status:** Draft · **Area:** tooling

## Problem

The design lab can capture scenes, but those captures are reviewed by eye only.
There are no stored baselines, so visual regressions are caught only when
someone happens to notice them.

## Current state

`ui-probe` and `scripts/ui-probe/native-verify.cjs` capture deterministic
scenes in a private Xvfb, and `pixels.cjs` and `compare.cjs` exist. Captures go
to `target/ui-probe/` and are treated as candidates only (see
[development](../architecture/development.md#ui-probe-and-design-lab)).

## Recommendation

- Baseline a small, stable set of scenes in Dark and Light, plus
  reduced-motion and focus variants: `tokens`, `components`, `browse`,
  `detail`, `series` and `states`. Use 960×600 and 1280×800.
- Run comparisons in Linux CI with a pinned software Vulkan driver (lavapipe),
  with tolerances and a diff artifact on failure.
- Update baselines only through an explicit command, and only after someone has
  looked at the new images.

## Open questions

- How stable is font rasterization across runner images?
- Should baselines live in git, in LFS, or as CI artifacts?

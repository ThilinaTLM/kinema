# Performance budgets

**Status:** Draft · **Area:** all

## Problem

Kinema has performance targets but no measured budgets, so changes that add
complexity "for performance" cannot be judged.

## Current state

- **Targets:** p95 frame time ≤ 16.7 ms and input-to-response ≤ 100 ms
  (see [UI](../architecture/ui.md#rendering-performance)).
- **Opt-in diagnostics** (`KINEMA_PLAYER_DIAGNOSTICS=1`) report acquisition
  and render cadence. Earlier investigations found occasional dips in the rate
  of accepted images and stalls from the cache pausing. These were not
  correlated with seeks, resizes or UI stalls.
- **Detail scrolling** was improved by deriving view models once per revision.
  The next step would be episode rows that are aware of the viewport.
- **CI** caching (rust-cache + sccache) has not been measured.

## Recommendation

1. **Define representative workloads:** a large grid, a long series detail,
   1080p and 4K playback, and a cold start with a 10k-title catalog.
2. **Set numeric budgets** for frame time, input latency, startup, time to first
   frame and seek recovery, plus memory and disk ceilings.
3. **Add measurement:** a repeatable local harness first, and CI trend tracking
   later.
4. **Investigate** the playback dips by correlating them with cadence events.
5. **Measure CI cold and warm durations** and the sccache hit rate. Keep a cache
   layer only if it pays for itself.

## Open questions

- Which hardware tier counts as the minimum?

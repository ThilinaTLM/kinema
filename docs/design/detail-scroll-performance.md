# Detail scrolling: scoped audit and optimization

## Findings

- GPUI's `Div::paint_scroll_listener` notifies the viewport's current view on a wheel offset change. The overview viewport belongs to `Detail`, so both movie and series scrolling repeat overview construction.
- `facts_view` previously derived and cloned provider facts twice per overview build (hero and facts section). Series also repeatedly cloned, filtered and sorted the selected season's episodes.
- Artwork cache access is in-memory. Disk access, decode, downscaling and backdrop blur already run on workers; they are not evidence of blocking I/O in detail rendering.
- The coordinator observer invalidates the entire detail screen. This is conservative but ensures metadata, personal state and failure states remain fresh.

## Implemented scope

Cache the derived facts model for the current coordinator notification revision, and the episode list for that revision plus selected season. Share results using `Rc`; retain the existing element builders, layout, controls and artwork completion behavior. Every coordinator notification invalidates these models, including updates within the same resource request generation. This deliberately does not use request generation alone as a data revision.

Simply moving builders into `RenderOnce` helpers or child entities does not prevent their construction. GPUI's explicit view cache needs definite dimensions and includes bounds/content masks in its key; scrolling content changes these. A generic caching layer or a speculative fixed-height rewrite was therefore not introduced.

## Regression checks

From the repository root:

```sh
cargo test -p kinema-desktop --features ui-probe-test --locked --lib wheel_scroll_does_not_recompute -- --nocapture
cargo test -p kinema-desktop --features ui-probe-test --locked --lib derived_models_present_new_values
```

The wheel tests dispatch ten real GPUI wheel events for movie and series fixtures, verify the scroll offset changed, and require zero facts/episode-list derivations after initial layout. Test-only counters also expose overview and episode-card construction; these remain nonzero. Invalidation coverage changes actual metadata and episode names and checks the new values are presented; a direct season-switch test checks the episode memo follows the selection.

Use a project-local `TMPDIR` if the host's `/tmp` quota prevents storage tests from running. Native offline comparison scenes are `detail-live-tmdb-movie` and `detail-live-tmdb-series` (see `ui-probe.md`).

## Reviewed evidence (2026-10-31)

- Root formatting, strict workspace Clippy and workspace tests passed. Workspace tests used `TMPDIR=$PWD/target/detail-scroll-tmp` after the initial run exhausted the host `/tmp` quota.
- Desktop strict Clippy with `ui-probe-test` passed; its full test run passed (776 library tests and 5 probe-binary tests; 5 ignored).
- Lead reran the detail interaction/domain subset with `ui-probe-test`: 67 passed. Ten wheel events plus explicit test draws produced movie counts `[20, 0, 0, 0]` and series counts `[20, 0, 0, 160]` for `[overview builds, facts derivations, episode-list derivations, episode cards]`. The extra test draws mean these are not production frame counts.
- Built native reduced-motion 1280×800 fixtures before and after the change. Captures are under `target/ui-probe/detail-scroll-{before,after}/{movie,series}/native-first-frame.png` (local artifacts, not committed). Movie captures are pixel-identical. Series captures differ only inside synthetic artwork rectangles; `design_lab.rs` assigns artwork colors by enumerating hash-map values. Outside those rectangles, pixels are identical. This checks initial fixture appearance, not every scroll position or interaction.

## Remaining work and limits

This reduces repeated model work; it does not establish a frame-time improvement or eliminate element construction/layout. The episode grid still builds every selected-season card. There is no real-provider or native Windows/macOS performance qualification here.

Before further decomposition, profile representative long seasons and artwork-heavy pages on a real display. Measure element construction, layout and paint independently. If card work dominates, investigate viewport-aware episode rows while retaining keyboard focus, season navigation and context menus. Narrow coordinator notifications only after mapping all metadata, personal-state, settings and error dependencies; missing an update would trade jank for stale UI. A component split should follow independently changing state and measured work, not merely file size.

## Series snapshot and per-section artwork inputs (2026-10-31)

The user reported smoother movies but continued TV scrolling lag after the first optimization. Counters (`work.rs`, `scroll_cost_tests.rs`) identified substantial repeated personal-state derivation per card, per tile and in the hero, in addition to card element construction:

- `episode_status` ×3 per card plus `episode_known_watched`/`saved_progress` per card and per menu.
- `next_up`/`episode_marks` (hero and episodes section) each scanned every coordinate of the whole show; `season_years` cloned `all_episodes` per season tile; the Local `all_episodes` rescanned the detail rows per season.

`snapshot.rs` now derives, once per coordinator revision and season-option set, pure values only: per-episode `{status, known_watched, saved, visible}` (Loading/Failed/Known{pending} preserved, row-less Failed counts unwatched only with a known show identity, Local rows are the fallback, indexed once), marks, continue target, season years/posters, embedded flag and show-identity readiness. No elements, handlers or menus are cached. Cards, menus and handlers are rebuilt from these values each draw. Artwork is not cached (completion does not bump the revision); only shared inputs (season poster, backdrops) are gathered once per section, and fixture keys are built lazily.

Deterministic counts per wheel step (a step is one wheel event, which triggers a draw, plus one explicit draw: two overview builds). Columns: episode cards built, `episode_status`, `episode_known_watched`, `episode_coordinates`, `all_episodes`, `saved_progress`. Before counts are agent-reported from the instrumented pre-snapshot code (the original console log was not retained); after is zero for the last five, with no snapshot rebuild, and `scroll_cost_tests` asserts it.

| shape | before | after |
|---|---|---|
| 3 seasons × 8 | 16 / 400 / 336 / 12 / 20 / 160 | 16 / 0 / 0 / 0 / 0 / 0 |
| 3 seasons × 24 | 48 / 1200 / 1008 / 12 / 20 / 480 | 48 / 0 / 0 / 0 / 0 / 0 |
| 20 seasons × 24 | 48 / 6096 / 5904 / 12 / 54 / 2112 | 48 / 0 / 0 / 0 / 0 / 0 |
| 20 seasons, season 1 = 150 | 300 / 9372 / 8172 / 12 / 54 / 3624 | 300 / 0 / 0 / 0 / 0 / 0 |

Wall-clock timing, current code only, debug test build (µs per wheel step; one lead-run result shown, with observed run-to-run variability). Columns: whole step (timer starts before the wheel event) / explicit draw only / episodes section / card loop / season selector / hero. The section, card, selector and hero timers accumulate over both builds in a step, so compare them with the whole step.

| shape | step / explicit draw / section / cards / selector / hero |
|---|---|
| 3 seasons × 8 | 6607 / 3302 / 424 / 261 / 94 / 135 |
| 3 seasons × 24 | 10170 / 5075 / 904 / 702 / 117 / 154 |
| 20 seasons × 24 | 12010 / 6011 / 1291 / 758 / 431 / 214 |
| 20 seasons, season 1 = 150 | 45163 / 22407 / 5095 / 4425 / 455 / 295 |

In the 150-episode case the card loop is about 10% of a step and the section about 11%; the remainder of the step (outside these timers) is not attributed here. An earlier draft compared draw timings before and after the change; that baseline timed only the explicit draw while its section timers covered two builds, so it is withdrawn. There is no valid before timing; only the counts above compare before and after.

Reproduce: `TMPDIR=$PWD/target/detail-scroll-tmp cargo test -p kinema-desktop --features ui-probe-test --locked --lib scroll_cost_report -- --nocapture --test-threads=1`.

Lead validation of the follow-up: root formatting, strict workspace Clippy and workspace tests passed. Desktop `ui-probe-test` strict Clippy and full tests passed (781 library tests, 5 probe-binary tests; 5 ignored). After tightening zero-work assertions to check raw totals rather than rounded per-step averages, the 5 scroll/snapshot tests and strict feature-enabled Clippy passed again. Native screenshots under `target/ui-probe/series-scroll-{followup,local}-{before,after}` were inspected: the local resume page is pixel-identical; the scrolled TMDB episode grid differs only inside synthetic artwork rectangles (fixture hash-map color assignment). Outside those artwork rectangles it is pixel-identical. An independent source review found no concrete snapshot recursion or stale-state regression. These checks are not a real-provider/GPU scrolling qualification.

Remaining: all selected-season cards are still built. Culling needs a separate design because every card owns a focus handle and tab stop. Artwork lookup counts are zero in fixture mode (offline fixtures bypass the artwork cache), so artwork lookup cost is not measured here.

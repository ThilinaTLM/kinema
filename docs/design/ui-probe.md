# Native v3 UI lab

The offline native v3 lab is the only probe. The Kit counter, Graphite gallery,
browser/WASM runner and their exclusive tests have been removed. The default
`ui-probe` entry point now opens the lab; `--lab` remains an explicit lab flag.
No production app, user database, credentials or real player is opened.
Production-screen fixtures inject offline state; they do not prove live workflows.

## Native v3 design lab

`--lab` bootstraps direct GPUI + `widgets` (no Kit initialization or legacy
screens). Scenes: `components` (default), `browse`, `menus`, `sheet`, `sheet-ready`,
`detail`, `series`, `series-streams`, `states`.
All content is synthetic/in-memory; buttons never start services or players.
`--light` and `--reduce-motion` select lab theme/motion preferences. `--size WxH`
(logical pixels, default 1280x800; also accepted by `native-verify.cjs`, whose Xvfb
screen is 2560x2400) opens the window at a given size for responsive review.
Production detail fixtures: `detail-live-tmdb-movie`, `detail-live-tmdb-movie-streams`,
`detail-live-tmdb-series`, `detail-live-tmdb-series-streams` (full `TitleFacts`,
long overview, 20 similar titles per page, long release names).

```sh
cargo build -p kinema-desktop --bin ui-probe --features ui-probe --locked
cargo run -p kinema-desktop --bin ui-probe --features ui-probe --locked -- --lab --scene browse
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/components-dark --lab
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/browse-light --lab --scene browse --light
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/browse-hover --lab --scene browse --hover 115,190
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/browse-focus --lab --scene browse --focus 115,190
```

Use the Xvfb/Vulkan fallback below if needed. Lab `--focus` uses keyboard Tab
presses (default eight; `--tabs N` accepts integers 1–40), not a card click. Focus
is also permitted for `library` and `-live` scenes. X,Y is retained for CLI
compatibility, not used as a keyboard oracle. Review `native-focused.png`; a Tab
count alone is not proof of which control has focus. Lead-reviewed Library,
movie and series focus captures are listed in
[development](../development.md#library-and-manual-watch-tracking-validation-pending).
Earlier failed focus captures are not evidence of successful focus.
The menus scene mounts a public `Menu` for the context specimen. Because Select
and Popover have no public initial-open API, an owned fixture startup task uses
their ordinary focus/Enter paths to open both after first layout. This is a
special simultaneous-overlay specimen, not validation of the single-menu policy.
The local PosterCard is a fixture prototype, not the production virtualized
grid/navigation system.

### Initial lab review evidence (2026-10-18)

Candidates under `target/ui-probe/lab/`: `{components,browse,menus,sheet,detail}-dark/`,
`{components,browse}-light/` (`native-first-frame.png`), `browse-hover/native-hover.png`,
`browse-focus/native-focused.png`, and `browse-reduced/native-hover.png`. Each was
image-reviewed; focus uses keyboard input with the pointer outside cards. This
host rendered with its available Vulkan runtime, without an ICD override; adapter
selection/performance is not independently established. These are candidates,
not accepted pixel baselines or production workflow/cross-platform evidence.

Initial widget observations (historical; widget owners have since revised busy
markers, Switch, and duplicate context-menu tab stops; this lab task never modifies widgets):

- `widgets/menu.rs`: `ContextMenuExt` adds a separate tab-stop/focus handle around
  an already focusable card. The wrapper is reached before the card, without the
  card's disclosure/ring; nine Tabs are needed for the reviewed first-card state.
  A shared target focus-handle API would avoid the duplicate stop.
- `widgets/button.rs` / `progress.rs`: busy markers are 6 px corner squares,
  difficult to distinguish at 1× (see both components captures), not missing font
  glyphs. Consider a clearer busy affordance without changing button width.
- `widgets/switch.rs`: Switch renders the same checked square as Checkbox, so the
  two controls are visually indistinguishable in the components captures.

Lab limits: fixed fixtures, no real provider/player, no shared-poster flight,
virtualization, infinite paging or complete production grid keyboard model. The
Play sheet fixtures do not verify real preparation or file-choice policy. Menus
intentionally coexist for inspection.

### Lead-review revisions

The current dark candidates (`native-first-frame.png`) are in
`target/ui-probe/lab/{components,browse,menus,sheet,sheet-ready,detail,series,series-streams,states}-dark/`;
light candidates are in `browse-light/` and `detail-light/`. Detail uses full-bleed
artwork with directional/bottom scrims, top-aligned hero content and compact facts.
`sheet` has two unselected Radio choices and disabled Play; `sheet-ready` starts
with the main feature selected. Selecting either radio updates the fixture only.
`series` shows eight episodes; clicking one opens the episode streams column.
`states` is a two-column, three-row specimen of §4.3 collection states.

```sh
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/sheet-ready-dark --lab --scene sheet-ready
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/series-dark --lab --scene series
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/series-streams-dark --lab --scene series-streams
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/states-dark --lab --scene states
node scripts/ui-probe/native-verify.cjs target/ui-probe/lab/detail-light --lab --scene detail --light
```

Lab hover capture now saves `browse-hover/native-hover-mid.png` after a nominal
80 ms and `native-hover.png` after at least 300 ms (capture timing is not a frame
acknowledgment). `report.json` compares an artwork-only first-card ROI to rest,
excluding chips/disclosure, to confirm visible zoom independently of panel repaint.


## Validation and capture prerequisites

```sh
cargo clippy -p kinema-desktop --all-targets --features ui-probe-test --locked -- -D warnings
cargo test -p kinema-desktop --features ui-probe-test --locked
node --test scripts/ui-probe/pixels.test.cjs
```

Linux captures require Xvfb, xdotool, ImageMagick `import`, fonts and a working
Vulkan implementation. The runner owns an isolated X display and temporary HOME,
never captures the personal desktop, and removes only its own temporary directory.
If needed, select a software Vulkan ICD with `KINEMA_UI_PROBE_VULKAN_ICD`.
Captures under ignored `target/ui-probe/` are candidates requiring image review,
not accepted baselines, native performance or cross-platform acceptance.

`--click X,Y` performs one primary click after the first frame and saves
`native-clicked.png` (e.g. to review header popovers); `browse-live-tmdb` makes
TMDB the primary provider so the Discover Filters popover shows provider facets.

The lab also includes production-screen fixtures (`browse-live`, detail-live,
play-live, transfers-live, quit-live and settings scenes); consult the scene list
in `src/design_lab.rs` for exact names. These retain synthetic/offline data.

### Detail responsive review (breakpoints)

```sh
for sz in 960x600 1280x800 1920x1080 2560x1440; do
  node scripts/ui-probe/native-verify.cjs target/ui-probe/shots/movie-$sz --lab --scene detail-live-tmdb-movie --size $sz
done   # likewise ...-movie-streams, ...-series, ...-series-streams
```

Use a tall size such as `1920x2000` to see the page below the fold. Captures are
candidates for image review, not baselines.

## Visual-baseline review requirements

These retained acceptance requirements are not a claim that a baseline-upload
pipeline or cross-platform visual qualification exists.

- Keep baselines separate by OS/backend, renderer/driver, fonts, scale, viewport,
  theme, fixture version and toolchain; record logical and physical dimensions.
  HTML previews are not GPUI pixel baselines.
- Assert fixture readiness, drive native input, check semantics, capture the
  client area and compare within the same environment, then inspect the images.
  Measure nondeterminism before approving tolerances. Masks must be narrow and
  reviewed, never hiding focus rings, text or artwork to force a pass.
- A missing baseline requires review, not a pass. Never silently regenerate
  approved references. Submit changes with fixture ID, reason, before/after/diff
  images and environment details for reviewer approval.
- On failure retain available images, semantic snapshot, failed step, bounded
  logs, build identity/dirty status, fixture/seed, environment and exit reason.
  Startup failure may have only logs; never fall back to capturing the user's
  desktop. Upload only fixture artifacts with bounded retention, not secrets,
  user databases, home directories, environment dumps or arbitrary core dumps.
- Inspect clipping, hierarchy, focus visibility, hover/keyboard parity, long
  titles, missing/error states, both themes, scale and reduced motion. Image diffs
  detect changes; they do not decide usability or prove native performance.

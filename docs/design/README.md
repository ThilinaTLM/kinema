# Graphite v2 desktop review guide

Open [desktop-wireframes.html](desktop-wireframes.html) directly in a browser
(`file://`). This self-contained HTML is a **design-only, not-connected review
artifact**, not Kinema's runtime, a framework choice, or proof of native behavior.
The [Graphite design system](design-system.md) defines the visual/state contract,
composition, verified pinned Kit mappings, and future performance requirements.
**The user approved native Graphite implementation.** See the
[native implementation status](native-implementation.md) for implemented component
boundaries, safe gallery commands and remaining native acceptance.
See [UI verification research](ui-verification.md) for the strategy and the
[isolated GPUI probe](ui-probe.md) for implemented checks, commands and current limitations.

Navigation is decided: **permanent 56 px left icon-only rail**, with Browse,
Transfers, Settings and About. There is no expanded/top navigation choice,
layout switch, or automatic expansion. The old three-navigation review is obsolete.

## What to review

Graphite uses neutral compact desktop surfaces, restrained blue primary/focus
accents, and semantic inks for important status/error cues. Yellow `IMDb` labels
are not the global accent. Inspect both themes, controls, information density and
keyboard focus—not only attractive poster screenshots.

- **Browse/search:** movie/series filters, grid/list, poster-size control, IMDb
  sorting, and title details. Grid title/year/IMDb rating remain visible at rest.
  Missing IMDb rating is `—`; known `0.0` is valid. Rating and IMDb-ID mapping are
  independent: unknown score does not disable sources when the ID is known.
- **Hover/focus details:** the intended contract is a bounded opaque-neutral
  disclosure inside the poster footprint: runtime or reported season count,
  up to two genres, up to two synopsis lines, plus Details/source actions. No
  layout growth, poster lift, invented facts, or hover-triggered metadata request.
  Keyboard focus must reveal the same details; Enter opens Details.
- **Components gallery:** Review → Components gallery shows shared button states,
  inputs/selection controls, semantic notices, ratings, tokens and media-card
  specimens (rest, disclosure, selected, loading, failed art, unknown score,
  known zero, missing IMDb mapping and long title). Static specimens are not a
  second fully interactive form or proof of native accessibility.
- **Details:** synopsis, labelled ratings, facts and Details & provenance.
  Series add season/episode selection. Cast shown in fixtures is a design concept,
  not a promise that the current normalized metadata model supplies it.
- **Sources:** one selected indexer at a time, filters/sort and release inspector.
  Torrentio/Peerflix results are not automatically merged or requested together.
  Missing mapping, missing credentials, empty results and service failure differ.
- **Prepare:** request files → explicit file choice → download/serve → external
  player launch. The fixture's **5% cached** readiness rule is illustrative only;
  it is not an approved universal readiness policy or automatic file choice.
- **Local files:** Browse's **Open local file…** opens a simulated chooser with
  invented entries. Select one to simulate serving/launch and a session. There is
  **no OS file dialog, file input, file read, copy, or actual player launch** here.
  The real MVP retains native file choice and loopback serving of existing files.
- **Transfers:** simulated downloads, serving sessions, quota and manual
  inactive-only cleanup. In-use files are protected; original local files are
  not cache entries to delete. Quota-full can be explored by removing inactive
  fixture files or increasing the fixture limit, then checking again.
- **Settings:** provider/account status flags, default indexer, external player,
  media limit, theme, density and reduced-motion preview. Credential presence
  is not successful service validation. Test connection/launch are unavailable.
- **About and quit:** attribution; persistent session indicator; simulated close/
  quit confirmation with Cancel first. Navigation keeps sessions alive. Quitting
  stops Kinema's server, not the external player; no background daemon remains.

Sessions say **Launch requested** and explain that playback state is not tracked.
Do not interpret download progress, HTTP serving or launch as watched/playing/
paused state. No history, personalized library, resume, embedded player, new
providers, torrent engine or LAN scope is introduced.

## Suggested review journey

1. Browse in dark and light themes. Compare a known rating, unknown rating and
   valid zero in the gallery. Hover, then keyboard-focus, a card; inspect actual
   facts as well as actions. Open Details with Enter.
2. Choose a movie, or a series season/episode; find sources. Select one release
   and prepare through the chosen debrid service. Choose the file explicitly,
   then simulate download/serve and external launch.
3. Navigate elsewhere: the session indicator remains. In Transfers, inspect stop/
   cleanup safeguards, then Review → Window-close / quit request… and cancel.
4. Try Open local file… and inspect the simulated local session separately from
   managed-media cache usage.
5. Review Components and Performance preview, then forced failure/empty states.
   Repeat with keyboard only, reduced motion, long titles and the light theme.

Review's forced-state menu uses these source-defined names:

| Screen | `state` values |
| --- | --- |
| Browse | `loading`, `stale`, `noresults`, `tmdbCred`, `connError` |
| Detail | `loading`, `noImdb`, `stale` |
| Sources | `loading`, `sourceEmpty`, `filtered`, `debridCred`, `connError`, `stale` |
| Prepare | `ambiguous`, `quotaFull`, `connError`, `success` |
| Transfers | `session`, `quotaFull` |

## Performance preview: 1,000 fixtures, not a native benchmark

Open Review → Performance preview or
[the direct preview](desktop-wireframes.html?screen=perf). Controls include poster
size, Top/Middle/End jumps, toolbar search, keyboard movement, and **Stress: 60
queries + jumps**. The current source windows grid rows with two overscan rows on
each side and permits an extra row pinned by keyboard focus/modal return.

Artwork work is **simulated timers**, not network fetch or actual decoding. One
global queue has four active slots across navigation generations; obsolete queued
work is dropped and stale completions rejected while active slots remain occupied
until completion. The panel reports mounted DOM cards/bound, active/peak jobs,
queued/dropped/stale jobs, simulated failures, row-mount time and layout-shift data
where supported. On the frozen revision, the reported maximum was **72 mounted
cards within its bound of 72** across 1,000 fixtures, and **4 active mock jobs**;
all 60 stress iterations completed. These are HTML-only measurements, not native
performance results.

DOM/job/application-state counters do **not** measure browser resource retention,
image-cache/GPU memory, process RSS, native GPUI frame timing, real task cancellation
or production memory bounds. This mock has no real decoded-image cache. The design
system's native artwork limits now have an [implemented bounded pipeline](native-implementation.md#artwork-bounds-are-implemented-constants-not-ram-measurements);
its constants still require profiling and are not measured RAM/GPU guarantees or
user-facing application settings. Artwork
memory is separate from the **20 GiB default managed-media quota**. No new data TTL,
automatic provider enrichment, or automatic media eviction is adopted.

## Direct review URLs

Use `?screen=browse|detail|sources|prepare|transfers|settings|about|gallery|perf`.
Source-defined optional flags:

| Flag | Meaning |
| --- | --- |
| `state=<name>` | Forced state from the table above |
| `theme=light` | Initial light theme (otherwise dark) |
| `motion=reduce` | Instant/reduced-motion preview |
| `view=list` | Browse list view |
| `item=<fixture ID>` | Selected title; use an ID from the current fixtures |
| `season=<n>&ep=<n>` | Series season/episode for detail/source/prepare routes |
| `src=<fixture ID>` | Selected source on source/prepare routes |
| `sec=providers|player|storage|appearance` | Initial Settings section |
| `stage=1` | Skip initial simulated Prepare wait |
| `menu=1` | Open Review menu |
| `scroll=0.5` | Initial fractional scroll position on Performance preview |

Examples: [gallery, light](desktop-wireframes.html?screen=gallery&theme=light),
[missing mapping](desktop-wireframes.html?screen=detail&item=1&state=noImdb),
[performance, middle/reduced motion](desktop-wireframes.html?screen=perf&scroll=0.5&motion=reduce).
There is no `nav` flag or navigation-layout control in the new design.

## Keyboard, fonts and motion

The source offers `/` or Ctrl/Cmd+K search, Alt+1–4 destinations, Alt+Left back,
Esc to dismiss/back, arrows/Home/End/Page keys for collection movement, Enter to
open/select, Tab traversal, and `?` for shortcut help. Dialogs implement focus
containment/return; browser focus/return guards passed the frozen-revision checks
below, but this does not establish full accessibility or native Kit behavior. Rail labels appear on hover/focus;
accessible names must not depend on visible labels or pointer hover.

The CSS uses a system-first OS/native-font fallback stack (`system-ui` then
platform/fallback families): Segoe UI variants on Windows, system UI on macOS,
Noto Sans/system sans on Linux where available. No downloaded font
or redistribution license is promised. Native GPUI must resolve one available
family after initialization (or retain the Kit default), **not** pass a CSS
comma-separated stack as its font family. Body/label/metadata/heading targets are
13/12/11/18 px; ordinary button labels are 13 px (compact 12), field labels
12 px, and group captions 11 px semibold. Display/text scaling needs separate
native OS testing.

Feedback uses 100–180 ms opacity/fill changes, not decorative chrome gradients,
poster lift or large navigation movement. OS reduced-motion preference or the
Settings preview removes transitions/loops; functional values still update.
Artwork lighting is not chrome, but overlay text requires opaque contrast-safe
backing. Test both themes at 900, 1280 and 1600 logical px widths, plus display/text
scaling. Normal text targets ≥4.5:1, essential UI/rings ≥3:1. See the design-system
acceptance checklist; source colors and screenshots alone do not prove compliance.

## Honesty, validation and review gate

All titles, ratings, releases, local paths, files and ports are invented fixtures.
Artwork is inline SVG/CSS. There is no server/dependency/CDN, credential field,
secure-store/database access, persisted state or connected provider/player. Token
dialogs flip in-memory status flags only; reload resets fixture state. Do not enter
secrets. HTML is not a substitute for native GUI, authenticated provider, secure
storage or external-playback acceptance on Windows, macOS or Linux.

### Frozen v2 evidence

The validated HTML is **1,201 lines**, SHA-256:

`c686a3f290c40faad42ebbc1402dcea95c295c553dd1d22ff93846e80b875dd7`

The HTML owner's six hardened browser scripts passed **159 checks**, with real
exit-code failures and a tested negative control. The lead reviewed the logs and
final hover/gallery/settings screenshots. This documentation pass independently
checked the saved hash, script-summary/check counts, card-layout/metrics logs,
source tokens and routes, local links/anchors, fences, whitespace, embedded logo
bytes and exact notice. The token pass verified 48 dark/light color assignments
and an explicit semantic alias for all 64 declared CSS variables. It did not
rerun the browser scripts or native application.

| Browser check group | Passing checks |
| --- | ---: |
| Metrics, focus and overflow | 23 |
| Journey, states and navigation | 24 |
| Forced-state parity and disabled controls | 50 |
| Performance focus/stress observer | 9 |
| Immediate navigation | 10 |
| Card layout | 43 |
| **Total** | **159** |

Reported coverage: **64 viewport/theme/screen combinations** at 900, 1280, 1600
and 2000 px widths, dark/light, eight screens; no page errors or overflow. Focus
resize/filter/modal-return guards passed. Card checks confirmed the opaque themed
96 px details panel stayed inside the poster, the fixed 76 px two-line-title
footer did not shift on hover, actions fitted, and the outer focus indicator was
2 px with a 2 px offset. The global mock-job peak remained 4 across generations;
60/60 stress iterations completed and maximum mounted cards stayed 72/72.

Run materials/logs/screenshots were retained at
`/tmp/kinema-design-v2-verify/{run.sh,scripts,logs,screenshots}` for this session;
these are temporary local evidence, not committed tests or portable CI artifacts.
The earlier lead browser inspection preceded the final card correction and is
not represented as an independent run against this final hash. Prior prototype
results and older HTML hashes are not evidence for this freeze.

Static inspection found no network/storage API calls or file input; this is not
a security audit. The exact CSS semantic-alias table and disclosed HTML/native
variances are in the [design system](design-system.md#frozen-html-v2-semantic-aliases).
Sampled token contrast is calculated there; the scripts/screenshots do not prove
all composited-state contrast, screen-reader conformance or OS behavior. Real
images, providers, secure storage, native font/DPI rendering, Windows/macOS/Linux
GUI support, native memory/frame profiling and playback remain unverified by this
artifact. No new native runtime/support result is claimed.

Verified official Kit 0.7.3 API mappings and unresolved runtime concerns live in
[the design system](design-system.md#pinned-kit-api-evidence-and-remaining-verification),
not a duplicate API table here. Use official reexports/components/assets/init/
open_window only. No Rust, production GPUI layout, backend/database, credential or
media-cache behavior is changed by these review artifacts. Native implementation
is now explicitly user-approved and tracked [separately](native-implementation.md);
the frozen HTML evidence is not native acceptance.

## TMDB attribution

About shows the exact notice:

> This product uses the TMDB API but is not endorsed or certified by TMDB.

The approved logo is the unmodified
`crates/kinema-desktop/assets/tmdb-logo.svg`, embedded byte-for-byte as a base64
`data:` URI so the artifact stays self-contained, and smaller than Kinema branding.
The logo remains under TMDB's branding terms, not the project license. Re-embed
and recheck the official asset if it changes. Nothing here implies TMDB endorsement.

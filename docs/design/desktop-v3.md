# Desktop v3 — native media UI redesign

**Playback-progress extension (2026-10-28):** the approved
[progress and desktop integration contract](playback-progress-desktop.md) supersedes
the manual-only/no-resume and Main-close-always-quits rules below. Source is
implemented with [scoped validation](../validation/playback-progress-desktop.md),
not full native desktop/provider qualification.

**Status: approved sole/default UI; native implementation in progress.** The user
approved complete removal of the old UI, version switches and gpui-kit. Production
now bootstraps direct GPUI v3 only. The native lab remains available for offline
captures. Specification and phase checklists below are targets, not proof of full
workflow, provider, playback or cross-platform acceptance.

**2026-10 UI revision (user-approved, source implemented; native visual QA and
live-provider checks pending).** The approved wireframe
[ui-revision-2026-10.html](ui-revision-2026-10.html) supersedes the conflicting
parts of §§3.1, 4.1, 4.2, 4.5, 4.6 and 6.1 below:

- Action labels never end in "…" (status text such as "Loading…" keeps it). The
  shared `Menu` highlights only the pointed row; pointer-opened menus start with
  no highlight, keyboard-opened ones highlight the first enabled row. Menus with
  any leading icon give every row an icon.
- Toolbar trailing group: Open local file · Refresh | Transfers status button
  ("No transfers" / "N serving" / speed + "N active · x / y" + 2 px bar; speed only
  below 1100 px; rate sampled in the coordinator) | Settings · More (About, Quit).
- Detail: 2400 px max frame; hero = poster · story/actions · "At a glance" panel
  (IMDb/TMDB tiles + key facts); grouped fact cards (Crew / Production / Release or
  Broadcast); cast as a wrapping photo grid (2 rows + Show all). No carousels.
  Series order: hero → Episodes → About the series → Cast → Similar series (TMDB TV).
- Episodes: wrapping season tiles with progress, collapsed to 2 rows around the
  next-up season; tri-state `WatchedToggle` for movie, season and series (applies
  immediately, no confirm/Undo); next-up episode highlight and hero
  "Continue · SxEy"; default season = season of next-up.
- Streams: no sidebar; compact header + control bar (provider, quality, filter,
  sort, "In my debrid account"); provider/quality/sort/account-only remembered
  globally; column table with parsed tags; selection drawer ≥ 1280 px. Debrid shows
  only *account presence* from one read-only listing per configured service —
  RD/AllDebrid instant-availability is unavailable, and no magnets are uploaded.
- Header chips: an open popover chip (Filters) keeps the shared glass edge and only
  gains the hover fill, matching other header controls. The Discover/Library chip
  is a one-click switch (no popover, no chevron): it shows the current scope and
  each click / Enter / Space flips to the other; tooltip "Switch to …" names the
  target. `ChoiceCard` is no longer used for scope.
- Hero actions, seasons and episode cards follow
  [episodes-revision-2026-10.html](episodes-revision-2026-10.html): one 32 px / `radius::MD`
  `Button` family (Watch and Library toggles use the selected state; no pill
  radii), a shared primary-colored `WatchMark` (no green for watched), `Thumb` with
  Ready / Loading{fallback} / Fallback / Missing (skeleton only after 150 ms),
  `OverlayBadge`/`OverlayButton` on artwork, and `MediaCard` episode cards sharing
  the poster-card hover lift and shadow. Artwork fallbacks: episode still → season
  backdrop → series backdrop → season poster; season poster → series poster.
  Definitive artwork misses persist for 24 h (see [cache policy](../cache-policy.md)).
- Discover/Library: fixed brand wash backdrop, never artwork-derived. Card hover is
  interruptible: `HOVER` lift, `CARD_OVERLAY` scrim and rows after the 60 ms intent
  delay with a 30 ms row stagger, `CARD_OVERLAY_OUT` exit (see
  [design system §3](design-system.md#3-transition-presets-uimotiontransition)).

References: [architecture](../architecture.md), [current implementation](../features.md),
[design index](README.md), [design system](design-system.md),
[isolated UI probe](ui-probe.md).

Approved playback extension: [independent embedded-mpv player](player.md), with
Cinema chrome and separate window/lease lifetime rules. Embedded is the approved
default; external mpv/VLC remain alternatives. Backend/window/control source is
implemented, with lead-reviewed root/feature checks and scoped native Linux GPUI
presentation evidence. Final backend option hardening passed root/native reruns;
this remains scoped validation, not full acceptance. This introduces no second GPUI engine or UI version.

Approved extension, **source implemented and reviewed; scoped validation passed**:
[Library membership and manual watch tracking](library.md). Its control placements
supersede the earlier Saved badge/dropdown proposal: always-visible top-right
poster `bookmark-off` / `bookmark`, with SERIES below; movie hero membership and
`circle` / `circle-check` watch controls after existing actions; show hero
membership and episode-still top-right watch controls. No episode membership or
whole-show watch flag. Detail actions are identical from every entry point;
child toggles never navigate. The landing scope menu selects Discover / Library.
The linked contract governs independent personal states, conservative summaries,
Undo and loading/errors. Lead-reviewed checks and Linux synthetic captures
(including movie, hero membership and episode watched keyboard focus) are recorded
in [development](../development.md#library-and-manual-watch-tracking-validation-pending),
not full MVP or cross-platform acceptance. This is manual tracking only,
not history, resume or automatic playback completion.

## 1. Product character and approved directions

Kinema is a **media browser**, not a dashboard. The posters and backdrops are the
interface; chrome recedes. Reference points are Stremio, Apple TV and Infuse for
the cinematic dark canvas and poster-first browsing, combined with desktop-tool
discipline: dense, keyboard-complete, restrained rounding and subtle elevation,
not oversized web-style cards or pills.

Design principles, in priority order:

1. **Artwork first.** Chrome is near-black and quiet so posters carry the color.
2. **One screen, one job.** Browse grid → title Detail → pick a stream → play.
   No dashboards, no tab bars, no left rail.
3. **Strong defaults, few knobs.** Every setting must earn its place.
4. **Desktop native.** Compact rounded geometry, 1 px hairlines, hover + right-click +
   keyboard everywhere, compact density, separate Settings window.
5. **Motion explains space.** Animations show where things come from and go;
   never decorative loops. Reduced motion is honored instantly.

### User-approved directions

- Drop **gpui-kit entirely**; depend directly on GPUI and build all components
  in-house, including a custom text input and context menus.
- One main window: **single-row toolbar + poster grid**. Detail replaces the grid
  in place; Back returns to the exact saved browse context.
- Transfers/sessions live behind a toolbar indicator and popover.
- Settings and About share **one separate OS window** (single instance).
- Cards are **poster-only**, no labels below. At rest: rating chip top-left and a
  `SERIES` marker on TV only. Hover/keyboard focus reveals title, year, runtime or
  season count, up to two genres and two synopsis lines. Click/Enter opens Detail.
- **Context menus** on cards, episodes, streams, transfer rows and text inputs
  (right-click, Menu key, Shift+F10).
- Text wireframes first (this document), then a **native GPUI design lab** with
  fixtures and screenshots through the ui-probe Xvfb pipeline. **No HTML.**
- Motion: named presets (`HOVER` lift, `CROSSFADE`, grid → Detail `FLIGHT` shared
  poster flight then `CONTENT` fade; Back reverses); reduced motion is instant.
  Values in [design system §3](design-system.md#3-transition-presets-uimotiontransition).
- Warm amber/coral brand (`primary`); artwork-derived secondary washes on Detail.
- Radius **4 / 6 / 8 / 12 px**, low-alpha borders and soft shadows only on
  raised surfaces; compact desktop density remains unchanged.

### Lead decisions (user may override)

- **GPUI crate: `gpui-pre =0.3.9`** (weekly snapshot of Zed GPUI, `zed@93d7456`,
  Apache-2.0, already the engine under Kit 0.7.3). Zed's crates.io `gpui` 0.2.2 is
  a year older and lacks `with_spring`, focus-visible, headless and popover work.
  Fallback: a git pin to the same Zed revision. Never combine engines.
- **Dark is the default theme**, regardless of OS appearance; Light is available.
  Media browsing is a dark-room activity and posters read best on near-black.
- **Bundled fonts** (Inter + JetBrains Mono) for identical rendering on all OSes.
- **Provider priority (default Cinemeta, TMDB) lives in Settings** as an ordered
  list. The first enabled, active provider is primary; others enrich. Missing
  required credentials show an inactive provider that is skipped, not an error.
  The header Filters popover picks sort and facets within the primary provider.
- **Detail is a scrolling page; streams get their own page.** The detail page holds
  a compact hero, facts, episodes (series) or similar titles (movies) and
  provenance. Play (movies) or an episode card (series) navigates to a full-width
  Streams page; Back returns to the detail page unchanged. There are no side
  panels. Clicking a stream prepares it in a modal progress sheet and plays the
  file chosen by the selection policy; there is no file picker.
- **Unknown rating hides the chip and hover rating**; no `IMDb —` placeholder.
  Known `0.0` is shown. Never substitute a TMDB score for IMDb.

### MVP flow preservation map

| Current capability | v3 home |
| --- | --- |
| Cinemeta/TMDB browse + search | Toolbar + grid use the primary provider from Settings priority; visible cards receive bounded missing-field enrichment |
| Movie/series filter; sort | Toolbar segmented control + collection/sort menu; preserved on Back |
| Title details; refresh | In-place Detail hero; inline Refresh icon button / F5; freshness in provenance |
| Series seasons/episodes | Detail page body: season tabs + responsive episode card grid |
| Torrentio/Peerflix sources | Streams page list, **one active indexer**, explicit switch |
| Debrid preparation + playback | Modal *Play* sheet: prepare → auto-play the file the selection policy identifies, or "Couldn't identify the file…" |
| Managed download/serve + player | Session coordinator outlives routes; transfers popover |
| Local file open | App menu / toolbar overflow `Open local file…` (Ctrl/Cmd+O) |
| Cache quota + cleanup | Popover summary + Settings › Storage; 20 GiB default, inactive-only cleanup |
| Settings / credentials | Settings window; secrets only in OS secure storage |
| About | Settings window › About |
| Quit / main-window close | Confirmation when sessions/launch pending; Cancel first |

Every `app.rs::AppAction` intent survives unchanged: settings load/save,
credential set/delete/test (incl. TMDB), `Prepare`, `PlayFile` (explicit file),
`PlayLocal`, `ReleaseSession`, `CleanupUnused`. Domain policy stays in
`kinema-core`; this redesign is presentation only.

## 2. Visual language

All sizes are logical px. The **[design system](design-system.md)** is the
reference for every token (color, typography, space, size, radius, elevation,
opacity, layout, motion values and presets), the layer rules and the enforcement
test. Tokens live in `crates/kinema-desktop/src/ui/theme/` and the motion runtime
in `ui/motion/`; views, blocks and primitives never use raw values. This section
keeps the *product* rules that the tokens serve. The blurred-backdrop pipeline is
in progress, not accepted end-to-end rendering behavior.

Naming: `primary` is the brand orange, `accent` is the neutral hover/highlight
fill, `selection` is the brand-tinted selected fill.

### 2.1 Color — "Cinema" dark (default)

Neutral near-black chrome with layered depth. The brand color `primary` is warm
amber/coral (`#FF7A45` in Dark), with a darker AA-safe Light variant. Artwork
colors provide secondary washes, never replace brand focus or primary actions.
Desktop brand assets use an `#FF7A45` tile and white K/play glyph; the player
variant inverts it onto a `#382218` ink tile with a seek bar, and the symbolic
variant remains monochrome. [Brand guidelines](brand.md) define geometry,
colours and usage. Editable SVGs and generated PNG/ICO/ICNS files live in
`crates/kinema-desktop/assets/brand/`; root `packaging/` owns generation and Linux
integration (see [packaging notes](../../packaging/README.md)). The application
ID remains `dev.tlmtech.kinema`; Windows EXE resources and macOS bundles are not
implemented by these assets.
Ratings and status still require explicit glyphs/labels, not color alone.
Ratios use WCAG 2.x relative luminance, without rounding to pass thresholds.

The 27-token palette with Dark and Light hex values, and the derived hover and
pressed mixes, are in [design system §2.1](design-system.md#21-color-themecolorpalette).
Summary: `background < surface < card < popover` layers; `foreground`,
`muted_foreground`, `subtle_foreground` and `primary_text` for ink; `primary`,
`secondary`, `accent`, `selection` for actions; one `border` plus the essential
`input` boundary and `ring`; `success`/`warning`/`destructive`; `shadow`,
`overlay`; and the media tokens `scrim`, `chip_on_art`, `poster_edge`, `rating`,
`rating_on_art`.

Contrast evidence (opaque, untinted surfaces; surfaces are `background` /
`surface` / `card` / `popover`):

| Pair | Dark ratio | Light ratio |
| --- | --- | --- |
| `foreground` on the four surfaces | 17.62 / 16.58 / 15.19 / 14.32 | 16.90 / 18.73 / 18.73 / 18.73 |
| `muted_foreground` on the four surfaces | 9.82 / 9.24 / 8.47 / 7.98 | 7.30 / 8.09 / 8.09 / 8.09 |
| `primary_foreground` on `primary` / hover / pressed (derived) | 6.98 / 7.81 / 4.61 | 5.62 / 6.93 / 7.76 |
| `primary_text` (minimum across these surfaces) | 7.71 | 5.74 |
| `ring` (minimum across these surfaces) | 7.71 | 5.07 |
| `success` on background / surface / card | 9.30 / 8.75 / 8.02 | 5.49 / 6.09 / 6.09 |
| `warning` on background / surface / card | 9.39 / 8.83 / 8.09 | 5.20 / 5.77 / 5.77 |
| `destructive` on background / surface / card | 7.05 / 6.63 / 6.08 | 5.88 / 6.52 / 6.52 |

Unit tests also check text on the `muted` placeholder surface, all primary
states, radius ordering, and sampled extreme/intermediate artwork colors. Status
inks must each meet 4.5:1 on the four surfaces, including artwork-tinted
palettes, and on a 10 % ink wash over the untinted surfaces (status-badge
backings). The Light success green was darkened from `#1E7A4C` to `#1B7045`, and
error red from `#C02B2B` to `#B32626`, to preserve caption AA with those washes.
Warning and all Dark status values are unchanged.

Rules:

- `primary` is **scarce**: primary action, selection marker, focus, active segment.
  Never fill large areas, toolbars or headings with it.
- Overlays on artwork (card hover panel, detail hero text) always sit on
  `scrim`-strength backing *under the text*; poster pixels never decide
  contrast. Checked: `foreground` on a 92 % scrim over pure white = 14.8:1.
- Artwork overlays keep the dark scrim in Light theme too (cinema convention);
  chrome around them follows the theme.
- Selection generally uses the `selection` fill **plus** a 2 px `primary` marker
  (left edge on list rows, bottom edge on segments), never color alone. Menus
  instead use an inset `radius::MD` `selection` selected pill with no left bar;
  preserve keyboard focus and semantic selection independently.
- Disabled: `subtle_foreground` on an unchanged fill, no whole-control opacity fade.
- Semantic inks are local cues (icon + text), never panel floods.

### 2.2 Subtle elevation and artwork tint

Layers are separated first by tone: `background < surface < card < popover`.
Use a 1 px low-alpha `border` edge on raised surfaces; essential control
boundaries still use `input`. Soft shadows belong only on raised things:
`elevation::sm` (resting raised), `md` (card hover), `lg` (menus, popovers) and
`xl` (sheets). All have zero spread and use `Palette::shadow`; do not shadow every
row or resting card. OS window shadows remain native. No decorative glow or glass
chrome.

Detail uses `Palette::tinted(Option<ArtworkPalette>, Mode)` for low-strength
artwork-primary washes on background/surface/card/popover (14 % Dark, 7 % Light).
Clamp HSL lightness to at most 0.13 in Dark and at least 0.94 in Light. The
`selection` fill derives from the artwork secondary color (saturation at most 0.65,
lightness 0.76 Dark / 0.24 Light). Primary/secondary text remains AA-safe.
`primary`, its derived states, `primary_foreground`, `primary_text` and `ring`
stay brand-colored. Pass the matching palette mode; `None` returns the original
palette unchanged. Missing/failed extraction stays neutral.
This pure token API and the existing ambient gradient are implemented; screen
adoption and backdrop blur/crossfade are in progress.

Blurred artwork is a content backdrop, not glass UI. Browse targets a blurred,
darkened full-canvas backdrop that crossfades with the hovered/keyboard-focused
or selected title (selected title is the fallback). Detail targets a full-bleed
hero backdrop with a fade into the tinted body. Keep decoding, downsampling and
blur off the render/UI path, own/cancel requests and reject stale generations.
Never depend on arbitrary pixels or `overlay` alone for readable text:
retain contrast-safe local scrims/opaque surfaces and neutral failure fallbacks.

### 2.3 Typography

Bundle **Inter** (OFL 1.1; Regular 400, Medium 500, SemiBold 600, Bold 700) as the
UI family and **JetBrains Mono** (OFL 1.1; Regular) for file names, paths and
hashes. Stream release names use the UI face. Register them at startup
through GPUI's text system; ship license texts and credit them in About. Reasons:
identical metrics on Windows/macOS/Linux, excellent small-size legibility,
tabular figures (`tnum`) for ratings, sizes and runtimes, and a neutral modern
voice that does not compete with poster typography. Fall back to the system UI
font only if bundled registration fails (log once, no user-visible error).

The scale is `TextSize::{Xs, Sm, Base, Md, Lg, Xl, Xxl}` (11 to 30 px) with
`Weight::{Regular, Medium, Semibold, Bold}`; see
[design system §2.2](design-system.md#22-typography-themetypography). Display
(`Xxl`, Bold) is the Detail title; `Xl` titles dialogs and sheets; `Lg` headings;
`Base` is body; `Md` prose; `Sm` facts and mono paths; `Xs` chips.

Use tabular numbers (`.tabular()`) for every numeric fact. Uppercase only for
`SERIES` and quality chips (`2160P`, `HDR`). Never shrink text to fit fixed
geometry; wrap, truncate with tooltip, or scroll.

### 2.4 Geometry and spacing

Spacing, size and radius scales are in
[design system §2.3](design-system.md#23-space-size-radius). Product rules:
compact desktop density; radii 4 / 6 / 8 / 12 px (`SM` chips and badges, `MD`
controls and inputs, `LG` cards and posters, `XL` sheets, popovers and menus;
focus rings follow their host); 1 px hairlines; a 20 px grid inset with 12 px
gaps; a 52 px floating toolbar (20 px top inset + 32 px controls); 32 px rows
(56 px for two-line episode and stream rows); 16 px icons in 28 px hit targets; a
2 px focus ring, inset on artwork and outside on controls.

Icons: one monochrome 16 px line icon set (Lucide, ISC license, SVGs embedded
via the app asset source), 1.5 px stroke, `muted_foreground` at rest and
`foreground` on hover. No emoji, no multicolor icons.

### 2.5 Imagery

- Posters 2:3, `ObjectFit::Cover`, clipped to `radius::LG`. A 1 px inner hairline
  (`poster_edge`, white @ 6 % in Dark) keeps dark posters from dissolving into
  the canvas.
- Missing/failed art: `muted` with the title (`TextSize::Lg`, centered,
  up to 3 lines) and year in `subtle_foreground`. Because cards have no labels, the
  placeholder **must** identify the title.
- Loaded art fades in with the `CROSSFADE` preset (instant in reduced
  motion). Skeletons pulse on an approximately 1.6 s loop
  (`timing::PULSE_PERIOD`), static under reduced motion; no moving shimmer.
- Browse targets blurred/darkened, title-responsive backdrops; Detail targets a
  full-bleed hero. This pipeline and screen redesign are in progress. Failure
  leaves a neutral theme canvas, not a fabricated successful image.

### 2.6 Motion

Motion is expressed only as named presets from `ui::motion` (three durations:
90 / 180 / 280 ms; easings `Linear`, `Out`, `InOut`); the full preset table is in
[design system §3](design-system.md#3-transition-presets-uimotiontransition), the
runtime (`AnimateExt`, `Presence`, `state_fill`) in
[§6](design-system.md#6-motion-runtime-uimotion). Mapping of product moments:

| Moment | Preset |
| --- | --- |
| Hover/pressed fills, toggles | `FEEDBACK` |
| Card lift (4 px, no artwork scaling or layout shift) and state layers | `HOVER` |
| Poster-card scrim and text rows | `CARD_OVERLAY` in (after hover intent, 30 ms row stagger) · `CARD_OVERLAY_OUT` out |
| Existing-panel disclosure | `DISCLOSE` |
| Backdrop changes, skeleton/placeholder → content | `CROSSFADE` |
| Shared poster flight, both directions | `FLIGHT` |
| Detail content landing (fade + 8 px rise) | `CONTENT` |
| Detail ⇄ Streams page slide | `COLUMN` |
| Menus, context menus, popovers, tooltips | `POPUP_IN` / `POPUP_OUT` (opacity + 4 px travel) |
| Modal sheet and scrim (fade + 12 px rise) | `SHEET` |

Popup entry uses the same overshoot-free cubic settle as Sheet; no bounce on text.
Honor reduced motion: every preset resolves to zero duration. The target is OS
reduced motion **or** the Settings override, whichever requests reduce;
GPUI 0.3.9 currently exposes no OS reduced-motion preference, so System resolves
to false and Always is the implemented reduction path. In either case,
switching mid-animation settles immediately. Interruptible transitions (hover
in/out, flight reversal, `Presence` reopen during exit) start from the current
interpolated value, not jump (`state_fill` springs, `Tween`, `Presence`).
Interaction timings (tooltip delay, search debounce, scrollbar idle, type-ahead)
are not animations and live in `theme::motion::timing`.

## 3. In-house component inventory

The component inventory, layers (`ui/primitives` → `ui/blocks` → `ui/views`),
variants, sizes, tones and the accessibility checklist are in the
[design system](design-system.md#4-component-conventions). Build only what views
use. All interactive components share states: rest, hover, pressed,
focus-visible, disabled, busy where relevant. Focus and selection are independent.

Keep components clean, composable and reusable, with explicit inputs/events and
low coupling to application services or individual views. Use shared theme
tokens rather than ad-hoc colors, spacing and typography. Compose workflows in
focused view submodules; keep rendering, presentation state and service
coordination (`controllers/`) separated without unused framework layers. Domain
policies remain in `kinema-core`; components must not own provider, storage or
playback policy.

Behavioral contracts that are product decisions (not visible from the API):

- `Button`: busy keeps width and label and blocks repeats; icon-only buttons need
  a tooltip or explicit name; selected toggles use `selection`.
- `Segmented`: arrows/Home/End; optional icons and counts; `toolbar()` glass variant.
- `TextInput`: caret, selection, word/grapheme motion, clipboard, undo/redo, IME
  composition with candidate positioning, secret mode, context menu.
  `SearchField` adds a search icon and clear, a debounced live query (300 ms),
  Enter commits, Esc clears then blurs.
- `Menu` / `ContextMenu` / `Select`: shared item model (§3.1); keyboard
  navigation; one-level submenus; `Select` adds a check marker and type-ahead.
- `Popover`: anchored non-modal panel; Esc/outside click dismiss; repositions
  within the window; content can dismiss itself via `PopoverHandle`.
- `Tooltip`: 500 ms delay, text only.
- `Switch` only for immediate boolean settings.
- `Badge`: non-interactive chip for rating (`★ 8.1`), `SERIES`, quality (`2160P`,
  `HDR`) and status.
- `Progress` / spinner: determinate 2 px bar; indeterminate sweep (static in reduce).
- `Alert`: inline info/success/warning/error/empty with icon, text and one action.
- `ScrollArea`: overlay thumb 4 px → 6 px on hover; fades to 35 % after 600 ms
  idle; always visible under reduced motion; keyboard scrolling.
- `Sheet`: centered 560 px `popover` surface, `radius::XL`, `elevation::xl`, title +
  body + footer actions; focus trap and return.
- `Toolbar`: floating 52 px header row (20 px top padding + 32 px controls) with
  leading / center / trailing slots; owns pointer hits across its height (clicks
  never reach the page beneath) while the wheel still scrolls the page.
  `ToolbarGroup` is one glass capsule of equal icon targets with optional hairline
  dividers; `ToolbarChip` is the glass popover-trigger face (icon, label,
  active-count badge, chevron; icon-only below 1100 px).
- `ChoiceCard`: large icon-led choice (title, description, check) for popover
  pickers such as Discover / Library. `FilterPanel`: popover body with title +
  Reset and labelled sections of choice chips; choices apply immediately.
- `SidebarNav`: Settings window section list only. `PosterCard`: §4.2.

### 3.1 Context menus

Right-click, the Menu key and Shift+F10 open the context menu for the focused or
pointed element. Right-clicking an unfocused card focuses it first. Menus render
in-app (`deferred` + `anchored` at the pointer, flipped to stay within the
window), `popover`, `radius::XL`, `border` + `elevation::lg`, 32 px rows, label left, shortcut right in
`subtle_foreground`, separators as hairlines, disabled items muted with a reason tooltip.
The selected item uses an inset `radius::MD` `selection` pill, not a left bar.
Keyboard: arrows, Home/End, type-ahead, Enter, Esc closes and returns focus;
Right/Left opens/closes a submenu. One menu open at a time; opening another
closes the first. Actions run through the same commands as buttons; the menu
never offers something the screen cannot do.

| Target | Items |
| --- | --- |
| Poster card | Open details · Find streams (movie) · Copy title · Copy IMDb ID · Open on IMDb (external browser) · Refresh metadata |
| Episode card | Find streams · Copy episode title · Copy `SxxEyy` |
| Stream row | Play… (prepares and plays) · Show details · Copy release name |
| Transfer row | Open in player again · Copy local URL · Show file in folder · Stop serving… |
| Text input | Undo · Redo · Cut · Copy · Paste · Delete · Select all |

Copied values never include credentials, debrid URLs or authorized links. "Show
file in folder" uses the platform opener with argument lists only.

## 4. Text wireframes

Fixture data only. Primary review size 1280 × 800; minimum 960 × 600.

### 4.1 Main window — browse

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ (◎ Discover ▾) [🎞 Movies│📺 Series]  [⌕ Search movies…  Ctrl K]  (☰ Filters 2 ▾) (⇅2│⚙ ⋯) │
├──────────────────────────────────────────────────────────────────────────────┤ 48
│                                                                              │
│  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐           │
│  │★ 8.1   │ │        │ │★ 7.4   │ │★ 8.3   │ │★ 6.9   │ │★ 0.0   │           │
│  │        │ │        │ │        │ │        │ │        │ │        │           │
│  │ POSTER │ │ POSTER │ │ POSTER │ │ POSTER │ │ POSTER │ │ POSTER │           │
│  │        │ │        │ │        │ │        │ │        │ │        │           │
│  │        │ │        │ │        │ │        │ │        │ │        │           │
│  └────────┘ └────────┘ └────────┘ └────────┘ └────────┘ └────────┘           │
│      12 px gaps; 20 px inset; columns stretch to fill the full width         │
│  ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐ ┌────────┐           │
│  │★ 7.2   │ │★ 8.0   │ │ The    │ │★ 6.8   │ │★ 7.8   │ │★ 7.1   │           │
│  │        │ │        │ │ Quiet  │ │        │ │        │ │        │           │
│  │   …    │ │   …    │ │ Orbit  │ │   …    │ │   …    │ │   …    │  ← missing art shows title
└──────────────────────────────────────────────────────────────────────────────┘
```

The toolbar is a transparent overlay: no bar fill or bottom border; it floats
over the page (grid, or the detail hero art). Every main-window page uses one
20 px page inset: content starts 72 px down (toolbar height + 20 px), so the
header controls have 20 px above and 20 px below them at scroll top, and the
sides match the same 20 px; once content
scrolls beneath, a canvas-colored top scrim (≈92 % → 0, fades in over 24 px of
scroll) keeps the controls legible in both themes.

One header design language serves every main-window view; only slot contents
change. All header controls share one translucent **glass** material
(`popover` at 78 % with a `border` edge, `Palette::glass`), 32 px height and `radius::MD`,
so they stay legible over posters, hero art and both themes. Primitives live in
`ui/blocks/toolbar.rs` (`Toolbar`, `ToolbarGroup`, `ToolbarChip`, `glass`).

| Slot | Browse (Discover / Library) | Detail and Streams |
| --- | --- | --- |
| Leading | Scope chip (`◎ Discover ▾` / `Library ▾`); kind segmented with icons (Discover: Movies / Series; Library: All / Movies / Series) | Back capsule |
| Center | Search (flexible, 180–440 px; placeholder follows scope and kind) | — |
| Trailing | `Filters` chip with active-count badge; global capsule | Global capsule |

- **Scope chip** opens a popover of two `ChoiceCard`s (icon, title, one-line
  description, check on the current one); choosing one switches scope and closes.
- **Kind** stays a visible segmented control: it is a frequent two/three-way
  toggle, so a popover would add a click.
- **Filters** is the single home for advanced options, so the header never grows.
  Library: watch status and sort. Discover with TMDB primary: provider-backed
  sort (Popular, Top rated, Newest, Title), genres (match all), release decade
  and minimum TMDB score, requested from TMDB `/discover` (default facets keep
  `/popular`). While searching, or when the primary provider has no faceted
  Discover, only a sort of the loaded results is offered, with a footer saying
  why the other facets are unavailable; facets are never faked. Choices apply
  immediately; Reset is enabled only when something differs from the default.
- **Global capsule** (transfers indicator with count · Settings gear · overflow
  `⋯`: Open local file…, Refresh, About Kinema, Quit) sits at the trailing edge
  in every view; the overflow menu anchors to it.
- On macOS the toolbar sits in a transparent titlebar beside the traffic lights;
  on Windows/Linux the native frame stays above it in P1 (integrated client-side
  titlebar is a later, optional refinement).

Freshness is quiet: no "Updated 12:40" in the toolbar. Stale or failed refresh
shows a one-line `Alert` strip under the toolbar only while relevant.

#### Incremental loading

First-load skeleton poster cards already exist. **Follow-up required:** while
loading the next page, retain existing cards and append skeleton poster cards in
the upcoming rows, using the same dimensions, gaps and column geometry. Reserve
their footprint so completion does not jump the layout or reset scroll/focus.
Skeletons are noninteractive, pulse on an approximately 1.6 s loop, and remain
static with reduced motion; no whole-grid replacement or moving shimmer is
required. Empty/failure handling must preserve loaded cards and distinguish a failed next-page request from a successful empty page.
This incremental skeleton requirement is approved, not yet implemented.

### 4.2 Poster card

```text
REST (series)                     HOVER / FOCUS (same footprint)
┌──────────────────────┐         ┏━━━━━━━━━━━━━━━━━━━━━━┓  ← 2 px focus ring (focus only)
│▐★ 8.1▌      ▐SERIES▌ │         ┃▐★ 8.1▌      ▐SERIES▌ ┃
│                      │         ┃                      ┃  whole card lifts 3 px; no artwork zoom
│                      │         ┃        POSTER        ┃  (rounded image, no reflow)
│        POSTER        │         ┃░░░░░░░░░░░░░░░░░░░░░░┃  scrim: transparent → 92 %
│                      │         ┃▓ North Harbor        ┃  TextSize::Lg, ≤2 lines
│                      │         ┃▓ 2024 · ★ 8.1        ┃  facts | year · rating; TextSize::Sm
│                      │         ┃▓ Drama · Mystery     ┃
│                      │         ┃▓ A visitor uncovers  ┃  ≤2 lines synopsis
│                      │         ┃▓ a secret by the…    ┃  (dropped first when small)
└──────────────────────┘         ┗━━━━━━━━━━━━━━━━━━━━━━┛
```

- Chips: 20 px high, 6 px horizontal padding, `TextSize::Xs`, `chip_on_art`
  backing (`#0B0C0E` @ 80 %), 8 px from the poster edges. Rating chip: amber `★` + score
  (`foreground`). `SERIES` chip: `muted_foreground`. Movies have no type chip.
  Unknown rating: no chip or hover rating; no `IMDb —` placeholder.
- Hover panel: bottom-anchored, bottom-rounded to `radius::LG`; a
  transparent→scrim gradient info panel slides up and fades in. Content is title,
  metadata (`facts | year · ★ rating`), genres and synopsis. Movies show runtime
  instead of season count; unknown facts and ratings are omitted. Preserve
  contrast-safe `scrim` backing under text.
- Hover/focus lifts the card 4 px with the `HOVER` preset and
  `elevation::md`; neighboring cards do not move and hover triggers no
  requests. Artwork does not zoom: GPUI clips containers only as rectangles, so
  the image itself uses `img().rounded(radius::LG)` rather than relying on a
  rounded parent to clip an enlarged image.
- Artwork fades in with `CROSSFADE`; skeletons pulse on an approximately
  1.6 s loop and are static under reduced motion.
- Pressed: artwork brightness unchanged, panel stays; activation on mouse-up.
- Selected/return target after Back: focus ring only while focus-visible.

### 4.3 Collection states

The grid area hosts one centered state block (max 420 px wide) when there are no
cards; with cards, problems use the notice strip under the toolbar.

```text
Loading (first)   pulsing skeletons fill the grid; static under reduced motion
Empty (success)   ⌕  No titles match "harbor".             [Clear search]
Failure           ⚠  Couldn't reach Cinemeta — timed out.   [Retry]
Missing creds     🔑 TMDB needs an API token.               [Open Settings…]
Store failure     ⚠  Secure storage unavailable: <reason>.  [Retry]
Stale (strip)     Showing saved results from yesterday · refresh failed  [Retry]
```

Storage/database failures are errors, never "no results". Saved content is kept
visible when a refresh fails.

### 4.4 Transfers popover (anchored to `⇅`)

```text
                                           ⇅2
                    ┌─────────────────────────────────────────┐ 380 wide
                    │ Transfers                               │
                    ├─────────────────────────────────────────┤
                    │ North Harbor · S01E02                   │
                    │ Downloading 1.4 / 4.2 GiB · serving     │
                    │ ▬▬▬▬▬▬▬▬▬▬▬▬▬▬▬───────────────────────── │ 2 px
                    │ Launch requested in mpv          [■]    │ stop (confirm)
                    ├─────────────────────────────────────────┤
                    │ sample.mkv · local file · serving  [■]  │
                    ├─────────────────────────────────────────┤
                    │ Cache 8.1 of 20 GiB      [Clean unused] │
                    └─────────────────────────────────────────┘
```

Zero sessions: "Nothing playing or downloading" plus cache line. "Launch
requested" is the truth; never "Playing". Stop releases the session and warns the
player may lose access; it does not kill the player. Cleanup reports freed bytes
and protected files, never deletes original local files. Rows have context menus.

### 4.5 Detail page (movie and series)

Implemented in `ui/views/detail*`. One scrolling page (`ScrollArea`). The hero and
every body section share **one frame**: the shared 20 px page inset (same as
Browse) and a 2400 px maximum content width,
centred only beyond it, so everything has the same left edge. Poster width and
overview measure scale with the window (140–232 px, 560–920 px).

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ (←)                                                              (⇅2│⚙ ⋯)      │ toolbar
├──────────────────────────────────────────────────────────────────────────────┤
│ compact hero: backdrop as ambient art fading into the canvas                 │
│ ┌────────┐  The Quiet Orbit                                                  │
│ │ POSTER │  2024 · 118 min · Drama · ★ 8.1                                   │
│ │168×252 │  synopsis (3 lines; "More" wraps within a 640 px measure)         │
│ └────────┘  [▶ Play] [⋯]            (series: [Episodes] scrolls to the grid) │
├──────────────────────────────────────────────────────────────────────────────┤
│ ★ 8.1 /10      Released · Runtime/Seasons · Episodes · Genres   (facts grid)  │
│ Cast row (rendered when the provider supplies cast)                          │
│ Episodes (series): [Season 1│Season 2…]  card grid: still, E02, title,       │
│   runtime · air date, 2-line overview — click/Enter opens Streams            │
│ Similar movies (movies): wrapping poster grid, two rows, [Show more]         │
│ Metadata · Identifiers (muted footer)                                        │
└──────────────────────────────────────────────────────────────────────────────┘
```

- The hero sizes to its content; there is no fixed-height band. The overview
  shows in full (typically 3–5 lines; no More/Less toggle) with a safety clamp of
  8 lines and an ellipsis (`clamp_lines`; a bare `line_clamp` leaves the tail
  unwrapped and clipped mid-word). There is no overflow menu: an icon-only
  Copy title button sits beside the title (turns into a check / "Copied" briefly),
  and icon-only Refresh and Open on IMDb (only with an IMDb id) buttons follow
  the Play button, each with a tooltip and accessible label.
- Streams load only when the Streams page opens, for the selected indexer.
  Without an IMDb mapping the Play button is disabled with a reason tooltip.
- **Series:** season default = first non-special season with episodes (or the last
  selected). Season change cancels obsolete requests. The episode grid is
  responsive (cards ≥ 260 px). Episode cards offer Find streams / Copy episode
  title / Copy `SxxEyy` in a context menu.
- **Grids fill the width.** Column count comes from the available width and each
  item's min/max width (`fit_grid`): posters 140–200 px (2:3), episode cards
  260–360 px (16:9 still), facts cells 200–300 px, cast 104–136 px (one row).
  The facts block leads with prominent rating tiles (IMDb, TMDB + votes), then the
  tagline and a multi-column facts grid.
- **Similar movies (TMDB only):** a wrapping poster grid limited to
  two rows at the computed column count. "Show more" reveals two more rows and, when the
  loaded titles run out, requests the next catalog page (pages accumulate;
  refresh restarts at page 1).
- The facts block is built from fields the providers expose today (rating,
  release, runtime/seasons, episodes, genres). Votes, certification, creators,
  studios and cast are *not yet wired*; the cast row and ratings tiles are laid
  out and render once data is supplied.

Details show the primary result first, then asynchronously enrich fully from all
active enrichment providers. Failures stay quiet and non-fatal; obsolete work is
cancelled and generation-checked. A footer lists only actual contributors:
“Data from TMDB and Cinemeta” or “Data from Cinemeta”. No per-field provenance UI;
TMDB scores remain labelled TMDB, never IMDb. TMDB's required About notice stays.
This approved provider revision is in progress, not live-provider verification.

### 4.6 Streams page

Approved revision: [streams-revision-2026-10.html](streams-revision-2026-10.html)
(supersedes the earlier Streams sections of the UI revision). Reached from Play
(movie) or an episode card (series). Same route as Detail: Esc / Back / toolbar
arrow return to the detail page, which keeps its scroll position, season and focus.

```text
│ backdrop fading into the canvas (MediaHero, as Detail)                       │
│ ┌──────┐ CHOOSE A STREAM                                                     │
│ │POSTER│ North Harbor                                       (TextSize::Xxl)   │
│ │104 px│ [▭ S01E02] The Signal [2024]                                        │
│ └──────┘ [Torrentio 5│Peerflix 1] [All 5│4K 1│1080p 3] │ [⌕ Filter] [Sort ▾] [⚡] [⟳] │
│ Streams 5 of 5                       remembered note │ ┌ inspector ───────┐ │
│ ▌[4K ] Release name (2 lines)        3.9 GiB  ↑412    │ │ badges, name     │ │
│  [HDR] DTS-HD 5.1 · Torrentio  [⚡ AllDebrid]          │ │ Size│Seeders│Audio│ │
│  [1080p] …                                            │ │ accounts, file   │ │
│                                                       │ ├──────────────────┤ │
│                                                       │ │ Play with [RD│AD]│ │
│                                                       │ │ [▶ Play        ] │ │
```

- **Header:** `MediaHero` with eyebrow, 104 px poster (80 px narrow), episode
  marker badge + name, fact chips. Filters sit in the hero action row as 32 px
  glass controls: provider and quality `Segmented` with counts, `SearchField`,
  sort `Select` (icon + "Sort" prefix), "In my account" `Button` toggle
  (only with a debrid token) and Refresh.
- **List:** `StreamRow` pattern — `QualityTile`, release name in the regular UI
  face (2 lines), `TagList` (codec, audio, languages, tracker) and presence
  badges, right-aligned size and seeders. No table header and no resting fill;
  selection is the `selection` fill plus a 2 px leading marker.
- **Inspector** (frame ≥ 1040 px, 340–400 px, sticky): badges, cleaned name,
  origin, three `StatTile`s, debrid account presence (presence only, never
  cache/readiness), file-name disclosure, then the play footer.
- **Narrow** (< 1040 px): the inspector is replaced by a floating play dock with
  the selected release and the same play footer.
- Single click selects; double-click, Enter, the row menu or the footer's Play
  start playback. Remembered provider/quality/sort/account filter, loading,
  empty, error (with retry) and provider-limit states are unchanged.

### 4.7 Inline play session (no modal)

The former Play sheet is a non-modal `PlaySession` rendered in the inspector
footer (or dock): "Preparing with Real-Debrid…" → "Starting playback in
<player>" steps with an indeterminate bar and Cancel; failures show a notice
with Try again / Dismiss; missing credentials show Open Settings / Dismiss; the
cache-limit failure links to Storage settings. Playback start, Cancel, Dismiss,
leaving the page, or playing another row drop the session and cancel its owned
work. A row being prepared shows a "Preparing…" badge; selecting another row
shows a one-line "Preparing another stream" banner. There is no file picker:
`FileSelection::Unique(i)` plays `prepared.files[i]` immediately; anything else
shows `FileSelection::error_message()`. Players receive local server URLs only,
launched as executable + argument list.

### 4.8 Settings window (single instance, ~860 × 600)

```text
OS frame: Kinema Settings
┌────────────────────┬─────────────────────────────────────────────────────────┐
│ General            │ Accounts                                                │
│▌Accounts           │                                                         │
│ Playback           │ Metadata    [Cinemeta ▾]                                │
│ Storage            │ TMDB token  ● Saved (not verified)  [Replace] [Remove] [Test]
│ Appearance         │ Streams     [Torrentio ▾]                               │
│ About              │ Debrid      [Real-Debrid ▾]                             │
│                    │ Real-Debrid ● Saved · verified 12:40  [Replace] [Remove]│
│                    │ AllDebrid   ○ Not set                 [Add…]            │
└────────────────────┴─────────────────────────────────────────────────────────┘
```

- **General:** startup collection, search scope note. Kept minimal.
- **Accounts:** ordered metadata-provider priority (default Cinemeta, TMDB),
  enablement, indexer/debrid choices and credentials. Replace/reset the old single
  metadata choice without migration; skip inactive providers lacking credentials.
  Missing, saved, verified,
  rejected, service failure and secure-store unavailable are distinct states.
- **Playback:** external player (mpv/VLC/custom path) with argument preview.
- **Storage:** cache used/limit (20 GiB default), Clean unused, location.
- **Appearance:** Theme (Dark default / Light / System), poster size S/M/L,
  reduce motion (System / Always).
- **About:** version, pre-1.0 notice, licenses (Inter, JetBrains Mono, Lucide,
  GPUI) and TMDB logo + "This product uses the TMDB API but is not endorsed or
  certified by TMDB."

Settings apply **immediately** (appearance, poster size, provider priority/indexer
selection) with field-level error display — no Save/Revert bar, matching desktop
preference-window conventions. Credentials are separate secure-store operations
with their own outcomes; never in settings files, fixtures, logs or SQLite.
Opening Settings again focuses the existing window (optionally at a section);
closing it never stops sessions or quits.

### 4.9 Quit confirmation (sheet)

```text
┌──────────────────────────────────────────────────┐
│ Quit Kinema?                                     │
│ 2 sessions are active. Quitting stops downloads  │
│ and the local server; your player may lose media.│
│                          [Cancel] [Quit]         │
└──────────────────────────────────────────────────┘
```

Cancel has initial focus; Esc cancels. Covers main-window close and app Quit
(also from Settings). No prompt when nothing is active; closing Settings never asks.

## 5. Grid layout and keyboard rules

- Preferred poster width `P`: **S = 140, M = 172 (default), L = 212**. Inner width
  `W` = window − 2 × 20 inset (minus scrollbar gutter if not overlay); gap `g = 12`.
  Columns `C = max(2, floor((W + g) / (P + g)))`; card width `(W − (C−1)·g) / C`;
  height = width × 1.5. At 1280 px: 6 columns of ≈ 197 px.
- Row virtualization (`uniform_list` rows of `C` cards): visible rows + 2 overscan
  each side. Stable media keys own focus/selection; resize keeps the anchor title.
- Keyboard: arrows move by cell/row; Home/End row edges; Ctrl/Cmd+Home/End loaded
  edges; PageUp/Down by viewport; Enter opens; Menu/Shift+F10 context menu;
  typing printable characters focuses search and inserts them.
- Infinite scroll requests the next page when within 2 rows of the end; a failed
  page shows a single retry row, keeping loaded cards.
- Back restores kind, collection, query, sort, scroll anchor and focused title.
- Sorts: Provider order, Title, Year, IMDb rating (unknown last; 0.0 numeric). Sort
  applies to loaded results unless the provider supports it server-side; label it.
- Unchanged backend rules: bounded artwork jobs/cache, generation-based stale
  rejection, no I/O or decode during render.

## 6. Transition choreography

### 6.1 Card hover / focus

Implemented PosterCard hover/focus lifts the whole card 4 px with
the `HOVER` preset (180 ms) and `elevation::md`, without changing the grid
footprint. The bottom-rounded gradient info panel slides up and fades in,
showing title, metadata (`facts | year · ★ rating`), genres and synopsis; omit
unknown facts/ratings rather than showing `IMDb —`.

There is no 1.05 artwork zoom. GPUI container clipping is rectangular, so the
image itself uses `img().rounded(radius::LG)` to retain rounded poster corners.
On leave, reverse from the current progress without snapping. Chips stay anchored
to the card. Focus-visible adds the ring immediately (no ring animation).
Artwork fades in with `CROSSFADE` (280 ms). Skeleton pulse loops at
approximately 1.6 s; reduced motion makes the pulse static and transitions instant.

Scroll thumbs widen from 4 px to 6 px on hover and fade to 35 % after 600 ms idle;
under reduced motion they remain always visible. Menus use `popover`,
`radius::XL`, `border` and `elevation::lg`, with an inset `radius::MD`
`selection` selected pill and no left selection bar.

### 6.2 Shared poster flight (grid ⇄ Detail)

```mermaid
sequenceDiagram
    participant G as Grid
    participant N as Navigator (transition owner)
    participant O as Overlay poster
    participant D as Detail
    G->>N: open(id, cell bounds, artwork handle)
    N->>D: mount; content opacity 0; hero slot reserved
    D-->>N: hero slot bounds (after layout, same generation)
    N->>O: place at cell bounds; hide cell + hero images
    par 320 ms
      O->>O: interpolate left/top/width/height
      N->>G: grid fades out (160 ms)
    end
    O->>D: land; show hero image; drop overlay
    D->>D: content fade + 8 px rise (160 ms, 40 ms delay)
    Note over D,G: Back: content fades out (120 ms), restore grid scroll anchor,<br/>measure cell, fly hero → cell, grid fades in, focus the card
```

- Backdrop fades in independently when it arrives; the flight never waits for
  network. Detail data loading starts at `open`, not after landing.
- Fallbacks → plain 240 ms cross-fade: artwork not loaded, source cell not
  measurable after restoring the anchor, or reduced motion (instant).
- Resize, a second navigation, or stale bounds settle immediately to the target
  route and release the overlay. One overlay, one transition generation.

### 6.3 Other transitions

Detail ⇄ Streams page: `COLUMN` (180 ms, 12 px horizontal slide). Menus/popovers:
`POPUP_IN` (180 ms fade + 4 px travel from their anchor), `POPUP_OUT` (90 ms).
Sheets: `SHEET` (scrim fade + 12 px rise, 280 ms). Focus moves
immediately; animations never delay input.

## 7. Accessibility and interaction

- Keyboard focus reveals the same card facts as hover; accessible card names
  include title, year, kind and rating (`IMDb unknown` when missing).
- Esc: closes topmost menu/popover/sheet; otherwise Back from Detail. IME
  composition owns Esc/Enter while active.
- Shortcuts: Ctrl/Cmd+F search · Alt+Left / Backspace (outside inputs) Back ·
  Ctrl/Cmd+, Settings · Ctrl/Cmd+O Open local file · F5 / Ctrl/Cmd+R Refresh ·
  Ctrl/Cmd+Q Quit · Menu/Shift+F10 context menu.
- Menus restore focus to their trigger/target; sheets trap focus.
- Contrast targets text ≥ 4.5:1, essential UI and focus ≥ 3:1 — met by §2.1 tokens.
- Validate 100–200 % scale, 960/1280/1600 widths, both themes, reduced motion.
  These are acceptance targets, not claims of current cross-OS support.

### Retained collection and artwork acceptance requirements

These requirements survive removal of the v2 design docs; they are acceptance
targets, not new runtime qualification or changes to the approved visual revisions.

- Collection focus belongs to a stable canonical identity, not a recycled cell.
  Scroll/mount the target before restoring focus; recycled cells must not inherit
  another title's state or callbacks. Resolve identity remaps on modal return;
  if filtering/removal hides the target, choose a predictable surviving neighbor
  or results heading rather than losing focus.
- Bound demand-driven page requests, retained pages and queues; coalesce matching
  demand, stop at known end, and require explicit retry after paging failure.
- Own artwork work in an application-wide bounded pool. Obsolete blocking jobs
  retain their active slots until completion; navigation must not reset global
  accounting. Cancel obsolete subscriptions and reject stale keys/generations.
  Prioritize visible artwork before overscan; do not crawl the catalog.
- Enforce encoded-byte, dimension, pixel and decoder-allocation limits before
  large allocations and during decoding. Budget decoded cache bytes separately
  from compressed buffers, temporary allocations and GPU residency. Evict unused
  handles; reduce resolution/defer overscan when pinned images exceed budget.
  Store quotas are not process-memory guarantees or part of the media quota.
  Missing, pending and failed artwork stay distinct; never retry every render.
- Profile native scrolling, input latency and repeated route/query/resize changes
  with slow/failing providers and large synthetic catalogs. Record hardware,
  OS/backend, scale, data/image sizes, cold/warm cache, p50/p95 frame time and
  worst stalls. Historical targets of p95 ≤16.7 ms and input feedback ≤100 ms
  require measurement; fixtures and HTML counters do not establish them.

## 8. Superseded directions

Superseded v2 documents and prototypes have been removed. `AGENTS.md` and the
design index identify v3 as the sole/default UI:

- Permanent 56 px left rail → toolbar-only main window.
- Footer labels at rest → poster-only cards with chips; facts on hover/focus.
- In-app Settings/About pages, Save/Revert bar → separate window, immediate apply.
- v2 Graphite blue-gray palette and system font stack → Cinema palette, Inter.
- Provider picker in browse chrome → ordered priority list in Settings › Accounts.
- Release inspector pane, inline prepare panel → stream rows + Play sheet.
- Graphite HTML prototype → this document + native design lab.
- gpui-kit facade/components → direct `gpui-pre` + in-house components.

## 9. Delivery phases and gates

| Phase | Deliverable / gate |
| --- | --- |
| **P0** | Use `gpui-pre =0.3.9` as the direct engine; own assets (Inter, JetBrains Mono, Lucide icons, TMDB logo), tokens, motion module and primitives (Button, Segmented, TextInput, SearchField, Menu/ContextMenu, Select, Popover, Tooltip, Badge, Progress, Alert, ScrollArea, Sheet; originally `src/widgets/`, now `src/ui/{theme,motion,primitives}`); `AGENTS.md` updated; design lab gallery screenshots. The transitional Kit path has been removed. |
| **P1** | Toolbar + virtualized poster grid + PosterCard (hover/focus/context menu), collection states, keyboard model, S/M/L. |
| **P2** | Detail hero + action column (movie streams, series seasons/episodes → streams), Play sheet, transfers popover, local file, quit sheet. |
| **P3** | Shared poster flight with reverse, fallbacks, cancellation and reduced motion; start/mid/end captures. |
| **P4** | Settings window (single instance, sections, immediate apply, accounts states). |
| **P5** | Remove gpui-kit and all obsolete screens/assets, own bootstrap (`application`/window opening), reconcile docs, full validation and screenshots review. |

The native design lab is the sole remaining ui-probe (fixture-only: no network, database,
keyring, media or player access). Every phase runs from the repository root:
`cargo fmt --all -- --check`,
`cargo clippy --workspace --all-targets --locked -- -D warnings`,
`cargo test --workspace --locked`, plus probe captures of touched screens.
Linux fixture pixels do not prove Windows/macOS support or real provider/playback.

## 10. Open decisions

1. **Search scope:** kind-scoped (current behavior, kept by default) vs. both kinds
   with grouped results.
2. **Integrated titlebar on Windows/Linux** (client-side decorations): deferred
   until after P2.
3. **Episode stills** in episode rows if the metadata model supplies them; rows
   are text-only until then.
4. **Provider collections:** TMDB Discover now accepts facets (sort, genres,
   release years, minimum score); Cinemeta catalogues still browse by kind and
   offset only, so with Cinemeta primary the Filters popover offers a sort of
   loaded results only (labelled as such). Named catalogue entries (e.g.
   Trending, Top 250) are added with backend support, never faked.

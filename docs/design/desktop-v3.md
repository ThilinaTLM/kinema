# Desktop v3 — native media UI redesign

**Status: approved sole/default UI; native implementation in progress.** The user
approved complete removal of the old UI, version switches and gpui-kit. Production
now bootstraps direct GPUI v3 only. The native lab remains available for offline
captures. Specification and phase checklists below are targets, not proof of full
workflow, provider, playback or cross-platform acceptance.

References: [architecture](../architecture.md), [historical feature inventory](../features.md),
[v2 review guide](README.md), [v2 design system](design-system.md),
[native implementation](native-implementation.md), [isolated UI probe](ui-probe.md).

## 1. Product character and approved directions

Kinema is a **media browser**, not a dashboard. The posters and backdrops are the
interface; chrome recedes. Reference points are Stremio, Apple TV and Infuse for
the cinematic dark canvas and poster-first browsing, combined with desktop-tool
discipline: dense, keyboard-complete, square, flat, no web-style cards or pills.

Design principles, in priority order:

1. **Artwork first.** Chrome is near-black and quiet so posters carry the color.
2. **One screen, one job.** Browse grid → title Detail → pick a stream → play.
   No dashboards, no tab bars, no left rail.
3. **Strong defaults, few knobs.** Every setting must earn its place.
4. **Desktop native.** Square geometry, 1 px hairlines, hover + right-click +
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
- Motion: hover **160 ms**, grid → Detail **320 ms shared poster flight** then
  content fade; Back reverses; reduced motion is instant.
- Flat, **0-radius**, 1 px hairlines, no drop shadows, one accent.

### Lead decisions (user may override)

- **GPUI crate: `gpui-pre =0.3.9`** (weekly snapshot of Zed GPUI, `zed@93d7456`,
  Apache-2.0, already the engine under Kit 0.7.3). Zed's crates.io `gpui` 0.2.2 is
  a year older and lacks `with_spring`, focus-visible, headless and popover work.
  Fallback: a git pin to the same Zed revision. Never combine engines.
- **Dark is the default theme**, regardless of OS appearance; Light is available.
  Media browsing is a dark-room activity and posters read best on near-black.
- **Bundled fonts** (Inter + JetBrains Mono) for identical rendering on all OSes.
- **Provider choice (Cinemeta/TMDB) moves to Settings**; the toolbar only picks a
  catalog/collection and sort within the active provider.
- **Detail uses a right-hand action column** (sources for movies; seasons →
  episodes → sources for series). Preparation and file choice run in a modal sheet.
- **Unknown rating hides the chip at rest**; the hover panel states `IMDb —`.
  Known `0.0` is shown. Never substitute a TMDB score for IMDb.

### MVP flow preservation map

| Current capability | v3 home |
| --- | --- |
| Cinemeta/TMDB browse + search | Toolbar + grid; provider selected in Settings; no automatic fallback |
| Movie/series filter; sort | Toolbar segmented control + collection/sort menu; preserved on Back |
| Title details; refresh | In-place Detail hero; Refresh in context menu / F5; freshness in provenance |
| Series seasons/episodes | Detail action column: season selector + episode list |
| Torrentio/Peerflix sources | Action column stream list, **one active indexer**, explicit switch |
| Debrid preparation + file selection | Modal *Play* sheet: prepare → explicit file choice → launch |
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

All sizes are logical px. One semantic token module in `kinema-desktop/src/design`
is the only source of color, type, spacing and motion; screens never use raw values.

### 2.1 Color — "Cinema" dark (default)

Neutral near-black with a faint cool bias, so poster color is never tinted by
chrome. Accent is an electric indigo-violet: distinctive for a media app, clearly
separate from IMDb gold (ratings), amber (warning) and red (error).
All text pairs below were contrast-checked (WCAG 2.x ratio in brackets).

| Token | Dark | Light | Use |
| --- | --- | --- | --- |
| `bg.canvas` | `#0B0C0E` | `#F3F3F5` | Window background, grid, detail body |
| `bg.surface` | `#131418` | `#FFFFFF` | Toolbar, action column, settings sidebar |
| `bg.raised` | `#1B1D22` | `#FFFFFF` | Menus, popovers, dialogs, tooltips (+ hairline) |
| `bg.placeholder` | `#1B1D22` | `#E4E5EA` | Poster placeholder / missing art |
| `ctl.rest` | `#24272D` | `#EAEAEE` | Secondary button, input fill |
| `ctl.hover` | `#2D3037` | `#E1E2E7` | Hover fill (rows, buttons, menu items) |
| `ctl.pressed` | `#373B43` | `#D7D9DF` | Pressed fill |
| `line.hairline` | `#25282E` | `#DCDDE3` | Decorative dividers |
| `line.control` | `#646A75` [3.1 on raised] | `#868C97` [3.05] | Essential control boundaries |
| `text.primary` | `#F2F3F5` [16.6] | `#111215` [18.7] | Titles, body |
| `text.secondary` | `#B3B8C1` [9.2] | `#4B505A` [7.3] | Facts, metadata |
| `text.muted` | `#8C929D` [5.4 on raised] | `#5E646F` [5.0] | Hints, captions, disabled labels |
| `accent` | `#6B4EF0` (white 5.3) | `#5B3FE0` (white 6.5) | Primary buttons, selection marker |
| `accent.hover` / `.pressed` | `#5D41E2` / `#4F34CF` | `#4F34CF` / `#4429BB` | Primary states |
| `accent.text` | `#A996FF` [7.4] | `#5B3FE0` [5.9] | Links, selected labels, active segment ink |
| `accent.tint` | `#6B4EF0` @ 16 % | `#5B3FE0` @ 10 % | Selected row / segment fill |
| `focus.ring` | `#B4A6FF` [8.6] | `#5B3FE0` [6.5] | 2 px focus ring |
| `rating` | `#F5C451` [11.3] | `#8A5D00` [5.2] | Star glyph + score in chips |
| `status.ok` | `#46C98A` | `#1E7A4C` | Ready, cached, serving |
| `status.warn` | `#F2A33A` | `#9A5400` | Quota, stale, partial |
| `status.error` | `#FF6B6B` | `#C02B2B` | Failures, danger outline |
| `scrim.max` | `#0B0C0E` @ 92 % | `#0B0C0E` @ 92 % | Text-bearing scrim over artwork (both themes) |
| `dialog.scrim` | `#000000` @ 60 % | `#0B0C0E` @ 40 % | Modal backdrop |

Rules:

- Accent is **scarce**: primary action, selection marker, focus, active segment.
  Never accent-fill large areas, toolbars or headings.
- Overlays on artwork (card hover panel, detail hero text) always sit on
  `scrim.max`-strength backing *under the text*; poster pixels never decide
  contrast. Checked: `text.primary` on 92 % scrim over pure white = 14.8:1.
- Artwork overlays keep the dark scrim in Light theme too (cinema convention);
  chrome around them follows the theme.
- Selection = `accent.tint` fill **plus** a 2 px accent marker (left edge on rows,
  bottom edge on segments), never color alone.
- Disabled: `text.muted` on unchanged fill, no whole-control opacity fade.
- Semantic inks are local cues (icon + text), never panel floods.

### 2.2 Elevation without shadows

Layers are separated by tone and a hairline, not blur or drop shadow:
canvas < surface < raised. Every raised layer (menu, popover, dialog, tooltip) has
a 1 px `line.hairline` border. **Functional scrims** over artwork preserve
contrast (card hover panel, detail backdrop fade into canvas). The user also
approves restrained **artwork-derived ambient gradients on Detail** and subtle
grid styling. Keep these content-led, low-contrast and theme-token controlled,
with neutral fallbacks for absent/failed artwork; they must not obscure text or
loading/focus states. Decorative gradient chrome, glow, glass and blur remain
out of scope. Window shadows remain OS-native. This is approval, not a claim
that ambient extraction or all grid styling is implemented.

### 2.3 Typography

Bundle **Inter** (OFL 1.1; Regular 400, Medium 500, SemiBold 600, Bold 700) as the
UI family and **JetBrains Mono** (OFL 1.1; Regular) for release names, file names,
paths, hashes and byte sizes inside stream/file lists. Register them at startup
through GPUI's text system; ship license texts and credit them in About. Reasons:
identical metrics on Windows/macOS/Linux, excellent small-size legibility,
tabular figures (`tnum`) for ratings, sizes and runtimes, and a neutral modern
voice that does not compete with poster typography. Fall back to the system UI
font only if bundled registration fails (log once, no user-visible error).

| Token | Size / line | Weight | Use |
| --- | --- | --- | --- |
| `type.display` | 30 / 36 | 700 | Detail title |
| `type.title` | 18 / 24 | 600 | Dialog/sheet titles, settings section heading |
| `type.heading` | 15 / 20 | 600 | Card hover title, column headings |
| `type.body` | 13 / 18 | 400 | Default text, synopsis (detail 14 / 21) |
| `type.body.strong` | 13 / 18 | 600 | Row titles, button labels |
| `type.small` | 12 / 16 | 400–500 | Facts, metadata rows, menu shortcuts |
| `type.caption` | 11 / 14 | 600 | Chips (rating, SERIES, quality), counters |
| `type.mono` | 12 / 16 | 400 | Release/file names, sizes in stream lists |

Use tabular numbers for every numeric fact. Uppercase only for `SERIES` and
quality chips (`2160P`, `HDR`). Never shrink text to fit fixed geometry; wrap,
truncate with tooltip, or scroll.

### 2.4 Geometry and spacing

| Token | Value |
| --- | --- |
| Spacing scale | 2, 4, 6, 8, 12, 16, 20, 24, 32, 48 |
| Radius | **0 everywhere** (cards, chips, controls, menus, dialogs, focus ring) |
| Toolbar height | 48 (single row) |
| Control height | 28 (compact), 32 (primary sheet actions) |
| Icon size / icon hit target | 16 / 28 × 28 |
| Row height | 32 (menu, list), 56 (episode / stream rows, 2 lines) |
| Grid inset / gap | 20 / 12 |
| Hairline / focus ring | 1 px / 2 px ring inset 0 px on artwork, 2 px outside on controls |

Icons: one monochrome 16 px line icon set (Lucide, ISC license, SVGs embedded
via the app asset source), 1.5 px stroke, `text.secondary` at rest and
`text.primary` on hover. No emoji, no multicolor icons.

### 2.5 Imagery

- Posters 2:3, `ObjectFit::Cover`, clipped square. A 1 px inner hairline at
  `#FFFFFF` 6 % keeps dark posters from dissolving into the canvas (dark theme).
- Missing/failed art: `bg.placeholder` with the title (`type.heading`, centered,
  up to 3 lines) and year in `text.muted`. Because cards have no labels, the
  placeholder **must** identify the title.
- Loaded art fades in over 160 ms (instant in reduced motion). Placeholders are
  static: no shimmer.
- Backdrops render only in Detail; failure leaves the canvas plain.

### 2.6 Motion tokens

| Token | Duration / easing | Purpose |
| --- | --- | --- |
| `motion.feedback` | 80 ms, linear | Hover/pressed fills, borders |
| `motion.disclose` | **160 ms**, ease-out cubic | Card hover panel, artwork zoom, art fade-in |
| `motion.flight` | **320 ms**, ease-in-out cubic (spring optional) | Shared poster flight, both directions |
| `motion.content` | 160 ms, ease-out, 40 ms delay after landing | Detail content fade + 8 px rise |
| `motion.column` | 200 ms, ease-out | Action column slide episodes ↔ streams |
| `motion.popup` | 100 ms in / 70 ms out, ease-out | Menus, context menus, popovers: opacity + 4 px travel |
| `motion.sheet` | 160 ms, ease-out | Modal sheet opacity + 12 px rise; scrim fade |
| `motion.reduce` | 0 ms | Everything instant; progress still updates |

Honor OS reduced motion **or** the Settings override, whichever requests reduce;
switching mid-animation settles immediately. Use `with_animation` keyed by stable
element IDs; interruptible transitions (hover in/out, flight reversal) must start
from the current interpolated value, not jump (use `with_spring` or store the
progress value in state).

## 3. In-house component inventory

Build only what screens use. All share states: rest, hover, pressed, focus-visible,
disabled, busy where relevant. Focus and selection are independent.

Keep widgets clean, composable and reusable, with explicit inputs/events and
low coupling to application services or individual screens. Use shared theme
tokens rather than ad-hoc colors, spacing and typography. Compose workflows in
focused screen submodules; keep rendering, presentation state and service
coordination separated without unused framework layers. Domain policies remain
in `kinema-core`; widgets must not own provider, storage or playback policy.

| Component | Contract |
| --- | --- |
| `Button` | Primary (accent), Secondary (`ctl.rest`), Ghost, Danger (error outline). Busy keeps width/label, blocks repeats |
| `IconButton` | Ghost 28×28, tooltip + accessible name; optional badge count (transfers) |
| `Segmented` | Movies / Series; accent ink + 2 px bottom marker; arrows/Home/End |
| `TextInput` | Caret, selection, mouse/keyboard selection, word/grapheme motion, clipboard, undo/redo, horizontal scroll, IME composition with candidate positioning, secret mode; context menu |
| `SearchField` | TextInput + search icon + clear; debounced live query (300 ms) + Enter commits; Esc clears then blurs |
| `Menu` / `ContextMenu` | Shared item model (see §3.1); anchored or at-pointer; keyboard navigation; submenus one level max |
| `Select` | Trigger + Menu with check marker; type-ahead |
| `Popover` | Anchored nonmodal panel; Esc/outside click dismiss; repositions within window |
| `Tooltip` | 500 ms delay, instant hide; text only |
| `Checkbox` / `Switch` | Switch only for immediate boolean settings |
| `Chip` | Rating (`★ 8.1`), `SERIES`, quality (`2160P`, `HDR`), status; non-interactive |
| `Progress` / `Spinner` | Determinate 2 px bar; indeterminate bar sweep (static in reduce) |
| `Notice` | Inline info/warn/error/empty with icon, text and one action |
| `ScrollArea` | Overlay 6 px scrollbar, widens to 10 px on hover; keyboard scrolling |
| `Sheet` (modal dialog) | Centered 560 px, `bg.raised`, title + body + footer actions; focus trap/return |
| `SidebarNav` | Settings window section list only |
| `PosterCard` | §4.2 |

### 3.1 Context menus

Right-click, the Menu key and Shift+F10 open the context menu for the focused or
pointed element. Right-clicking an unfocused card focuses it first. Menus render
in-app (`deferred` + `anchored` at the pointer, flipped to stay within the
window), `bg.raised`, hairline border, 32 px rows, label left, shortcut right in
`text.muted`, separators as hairlines, disabled items muted with a reason tooltip.
Keyboard: arrows, Home/End, type-ahead, Enter, Esc closes and returns focus;
Right/Left opens/closes a submenu. One menu open at a time; opening another
closes the first. Actions run through the same commands as buttons; the menu
never offers something the screen cannot do.

| Target | Items |
| --- | --- |
| Poster card | Open details · Find streams (movie) · Copy title · Copy IMDb ID · Open on IMDb (external browser) · Refresh metadata |
| Episode row | Find streams · Copy episode title · Copy `SxxEyy` |
| Stream row | Play… (opens sheet) · Show details · Copy release name |
| Transfer row | Open in player again · Copy local URL · Show file in folder · Stop serving… |
| Detail backdrop | Refresh · Copy title · Open on IMDb |
| Text input | Undo · Redo · Cut · Copy · Paste · Delete · Select all |

Copied values never include credentials, debrid URLs or authorized links. "Show
file in folder" uses the platform opener with argument lists only.

## 4. Text wireframes

Fixture data only. Primary review size 1280 × 800; minimum 960 × 600.

### 4.1 Main window — browse

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◆ Kinema [Movies│Series]  [⌕ Search movies…       Ctrl+F]  Popular ▾   ⇅2 ⚙ ⋯ │
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

Toolbar, left to right: app mark; Movies/Series segmented; search (flexible,
240–480 px, placeholder follows the active kind); collection menu (`Popular ▾`
lists the active provider's catalogs, then a separator and named sorts); flexible
space; transfers indicator (icon + count; hidden count when zero, icon remains);
Settings gear (opens the Settings window); overflow `⋯` (Open local file…,
Refresh, About Kinema, Quit). On macOS the toolbar sits in a transparent
titlebar beside the traffic lights; on Windows/Linux the native frame stays above
it in P1 (integrated client-side titlebar is a later, optional refinement).

Freshness is quiet: no "Updated 12:40" in the toolbar. Stale or failed refresh
shows a one-line `Notice` strip under the toolbar only while relevant.

#### Incremental loading

First-load skeleton poster cards already exist. **Follow-up required:** while
loading the next page, retain existing cards and append skeleton poster cards in
the upcoming rows, using the same dimensions, gaps and column geometry. Reserve
their footprint so completion does not jump the layout or reset scroll/focus.
Skeletons are noninteractive and static with reduced motion; no whole-grid
replacement or shimmer is required. Empty/failure handling must preserve loaded
cards and distinguish a failed next-page request from a successful empty page.
This incremental skeleton requirement is approved, not yet implemented.

### 4.2 Poster card

```text
REST (series)                     HOVER / FOCUS (same footprint)
┌──────────────────────┐         ┏━━━━━━━━━━━━━━━━━━━━━━┓  ← 2 px focus ring (focus only)
│▐★ 8.1▌      ▐SERIES▌ │         ┃▐★ 8.1▌      ▐SERIES▌ ┃
│                      │         ┃                      ┃  artwork zooms 1.00 → 1.05
│                      │         ┃        POSTER        ┃  (clipped, no reflow)
│        POSTER        │         ┃░░░░░░░░░░░░░░░░░░░░░░┃  scrim: transparent → 92 %
│                      │         ┃▓ North Harbor        ┃  type.heading, ≤2 lines
│                      │         ┃▓ 2024 · 3 seasons    ┃  type.small secondary
│                      │         ┃▓ Drama · Mystery     ┃
│                      │         ┃▓ A visitor uncovers  ┃  ≤2 lines synopsis
│                      │         ┃▓ a secret by the…    ┃  (dropped first when small)
└──────────────────────┘         ┗━━━━━━━━━━━━━━━━━━━━━━┛
```

- Chips: 20 px high, 6 px horizontal padding, `type.caption`, background
  `#0B0C0E` @ 80 %, 8 px from the poster edges. Rating chip: amber `★` + score
  (`text.primary`). `SERIES` chip: `text.secondary`. Movies have no type chip.
  Unknown rating: no chip at rest; hover shows `IMDb —`.
- Hover panel: bottom-anchored; a 40 px transparent→scrim gradient, then a solid
  `scrim.max` text block with 12 px padding. Panel content rises 8 px and fades
  in (160 ms). Movies show runtime instead of season count; unknown facts omitted.
- Hover does not move neighboring cards, lift the card, or trigger requests.
- Pressed: artwork brightness unchanged, panel stays; activation on mouse-up.
- Selected/return target after Back: focus ring only while focus-visible.

### 4.3 Collection states

The grid area hosts one centered state block (max 420 px wide) when there are no
cards; with cards, problems use the notice strip under the toolbar.

```text
Loading (first)   static placeholders fill the grid; no text
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

### 4.5 Detail — movie

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [←]  The Quiet Orbit                                            ⇅2  ⚙ ⋯      │ toolbar
├─────────────────────────────────────────────────────┬────────────────────────┤
│ BACKDROP (full bleed behind left area, fades to     │ Streams   Torrentio ▾  │
│ canvas at bottom and left via functional scrim)     │ [All │ 4K │ 1080p │ 720p]│
│                                                     ├────────────────────────┤
│  ┌──────────┐                                       │ Quiet.Orbit.2160p.HDR… │ mono
│  │          │  The Quiet Orbit                      │ 2160P HDR · 18.2 GiB   │ chips
│  │   HERO   │  2024 · 118 min · Drama · Sci-fi      │ ▸ 412 seeds            │
│  │  POSTER  │  ★ IMDb 8.1                           ├────────────────────────┤
│  │ 220×330  │                                       │▌Quiet.Orbit.1080p.Web… │ selected
│  │          │  A drifting station's last engineer…  │ 1080P · 4.2 GiB        │
│  └──────────┘  (synopsis, 4 lines, "More" expands)  ├────────────────────────┤
│                                                     │ Quiet.Orbit.720p…      │
│                Details & provenance ▾               │ 720P · 2.1 GiB         │
│                                                     │  …                     │
│                                                     │                        │
└─────────────────────────────────────────────────────┴────────────────────────┘
                                                        action column 400 px, bg.surface
```

- Hero: poster 220 × 330 at 32 px from the left/bottom of the hero band; title
  `type.display`; facts `type.small` secondary; synopsis `type.body` 14/21,
  max 640 px measure. Provenance (IDs, rating origin, provider, freshness)
  expands inline.
- Streams load automatically when Detail opens and an IMDb mapping exists (one
  selected indexer). No mapping: column shows "Streams unavailable — no IMDb ID
  for this title." Browsing still works.
- Stream rows: release name (mono, 1 line, full name in tooltip), chips from
  parsed hints (resolution, HDR/DV, codec, size, seeds) — hints, not verified
  facts; unknowns omitted. Quality filter shows only buckets present in results.
  Indexer switch (`Torrentio ▾`) replaces the list and rejects stale replies.
- Click / Enter on a stream opens the **Play sheet** (§4.7). "Show details"
  (context menu or `▸`) expands the row inline: full name, source identity, files
  count where known.
- Width < 1100: action column becomes 340 px; < 960 it moves below the hero.

### 4.6 Detail — series

```text
├─────────────────────────────────────────────────────┬────────────────────────┤
│                                                     │ Season [ 1 ▾ ]   12 ep │
│  (same hero as movie: poster, title, facts,         ├────────────────────────┤
│   2024 · 3 seasons · Drama, synopsis)               │ 01  Arrival     48 min │
│                                                     │     Aired 2024-03-02   │
│                                                     │▌02  The Signal  51 min │
│                                                     │ 03  Low Tide           │
│                                                     │  …                     │
├─────────────────────────────────────────────────────┴────────────────────────┤

after selecting an episode, the column slides left (motion.column):

│                                                     │ ← S01E02 · The Signal  │
│                                                     │ Streams   Torrentio ▾  │
│                                                     │ [All │ 1080p │ 720p]   │
│                                                     │ rows …                 │
```

Season defaults to the first non-special season with episodes (or the last
selected in this session). Season change cancels obsolete episode requests.
Selected episode identity is carried into the Play sheet; episode packs never use
largest/positional file guesses. No watched/progress/auto-next claims.

### 4.7 Play sheet (modal, 560 wide)

```text
┌──────────────────────────────────────────────────────────┐
│ Play · The Quiet Orbit                                   │
│ Quiet.Orbit.1080p.WEB…  ·  via Real-Debrid ▾             │
├──────────────────────────────────────────────────────────┤
│ ① Preparing with Real-Debrid…            ▬▬▬▬▬▬──────    │
│                                                          │
│ ② Choose a file                                          │
│   ○ Quiet.Orbit.mkv                          4.2 GiB     │ mono
│   ○ Extras/Interview.mkv                     0.4 GiB     │
│                                                          │
│ ⚠ File exceeds the 20 GiB cache limit. [Storage settings]│
├──────────────────────────────────────────────────────────┤
│ Player: mpv ▾                         [Cancel] [▶ Play]  │
└──────────────────────────────────────────────────────────┘
```

Steps reveal progressively; the file must be chosen explicitly even with one
candidate (Play disabled until chosen). Missing debrid credentials show a notice
with "Open Settings…" to that account. On launch the sheet closes and the
transfers indicator pulses once (static in reduce). Launch failure keeps the sheet
open with the reason and the file choice intact. Players receive local server
URLs only, launched as executable + argument list.

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
- **Accounts:** provider selections and credentials. Missing, saved, verified,
  rejected, service failure and secure-store unavailable are distinct states.
- **Playback:** external player (mpv/VLC/custom path) with argument preview.
- **Storage:** cache used/limit (20 GiB default), Clean unused, location.
- **Appearance:** Theme (Dark default / Light / System), poster size S/M/L,
  reduce motion (System / Always).
- **About:** version, pre-1.0 notice, licenses (Inter, JetBrains Mono, Lucide,
  GPUI) and TMDB logo + "This product uses the TMDB API but is not endorsed or
  certified by TMDB."

Settings apply **immediately** (appearance, poster size, provider/indexer
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

On enter: artwork bounds grow from 100 % to 105 % around the center inside the
clipped card (animated `left/top/width/height` of the image; GPUI transforms are
SVG-only), and the panel rises 8 px while fading 0 → 1, both 160 ms ease-out. On
leave: reverse from the current progress (no snap). Chips stay put. Focus-visible
adds the ring immediately (no animation on the ring).

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
- Fallbacks → plain 160 ms cross-fade: artwork not loaded, source cell not
  measurable after restoring the anchor, or reduced motion (instant).
- Resize, a second navigation, or stale bounds settle immediately to the target
  route and release the overlay. One overlay, one transition generation.

### 6.3 Other transitions

Action column episodes ⇄ streams: 200 ms horizontal slide with fade, the leaving
pane moving 24 px and fading out. Menus/popovers: 100 ms fade + 4 px travel from
their anchor; close 70 ms. Sheets: scrim fade + 12 px rise (160 ms). Focus moves
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

## 8. Superseded directions

Older v2 documents are retained and marked historical; `AGENTS.md` and the design
index now identify v3 as the sole/default UI:

- Permanent 56 px left rail → toolbar-only main window.
- Footer labels at rest → poster-only cards with chips; facts on hover/focus.
- In-app Settings/About pages, Save/Revert bar → separate window, immediate apply.
- v2 Graphite blue-gray palette and system font stack → Cinema palette, Inter.
- Provider picker in browse chrome → Settings › Accounts.
- Release inspector pane, inline prepare panel → stream rows + Play sheet.
- `desktop-wireframes.html` → this document + native design lab.
- gpui-kit facade/components → direct `gpui-pre` + in-house components.

## 9. Delivery phases and gates

| Phase | Deliverable / gate |
| --- | --- |
| **P0** | Use `gpui-pre =0.3.9` as the direct engine; own assets (Inter, JetBrains Mono, Lucide icons, TMDB logo), tokens, motion module and primitives (Button, IconButton, Segmented, TextInput, SearchField, Menu/ContextMenu, Select, Popover, Tooltip, Chip, Progress, Notice, ScrollArea, Sheet) in `src/widgets/`; `AGENTS.md` updated; design lab gallery screenshots. The transitional Kit path has been removed. |
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
4. **Provider collections:** the catalog worker currently browses by kind and
   offset only (`metadata_bridge::Ask::Browse { kind, offset }`). Until a
   collection request exists, the toolbar menu lists named sorts over loaded
   results only (labelled as such); collection entries are added with backend
   support, never faked.

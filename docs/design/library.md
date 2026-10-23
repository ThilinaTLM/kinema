# Library — UI wireframe proposal

**Status: proposal for review; not approved or implemented.**

Extends [Desktop v3](desktop-v3.md), retaining its single-row toolbar,
poster-first grid, in-place Detail, Cinema tokens, and native GPUI components.
This proposes bringing personalized library membership and manual watched status
forward from the architecture's “Later” list; it does not claim either exists.
All titles and counts below are fixtures. No application behavior changes here.

## 1. Recommended model

- **Discover** browses provider catalogs; **Library** browses titles explicitly
  saved by the user. Library means a personal collection, not downloaded files.
- Save a movie or an entire show with **Add to library**. Episodes belong to the
  saved show; there is no separate episode-library collection.
- Membership and watched status are independent. Marking watched does not add a
  title; removing it from Library does not erase its watched status.
- Movies have a manual Watched / Unwatched state. Episodes have their own manual
  state; a show's summary is derived from its episodes, not a separate checkbox.
- First iteration: manual tracking only. Playback launch, download completion,
  and playback failure never imply watched. Automatic completion thresholds,
  resume positions, watch dates, history, and sync are separate future decisions.

## 2. Landing page — Discover / Library scope

Use one compact scope menu, not a navigation rail or a new tab bar. It remains
visible on the main landing page, next to the media-kind control. “Discover” is
the provider-browse scope, not a library membership state.

```text
DISCOVER
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◆ [Discover ▾] [Movies|Series] [⌕ Search movies…     ] [Popular ▾]    ⇅ ⚙ ⋯    │
│                                                                              │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                     │
│   │ ★ 8.1    │  │ ★ 7.4    │  │   SERIES │  │ ★ 8.0    │                     │
│   │          │  │          │  │          │  │          │                     │
│   │  POSTER  │  │  POSTER  │  │  POSTER  │  │  POSTER  │                     │
│   │ ▣ Saved  │  │          │  │ ▣ Saved  │  │          │                     │
│   └──────────┘  └──────────┘  └──────────┘  └──────────┘                     │
└──────────────────────────────────────────────────────────────────────────────┘

SCOPE MENU                    LIBRARY FILTER / SORT MENU
┌──────────────────────┐      ┌────────────────────────────┐
│ ✓ Discover           │      │ Watched status             │
│   Library            │      │ ✓ Any status               │
└──────────────────────┘      │   Unwatched                │
                             │   In progress (shows only) │
                             │   Watched                  │
                             ├────────────────────────────┤
                             │ Sort                       │
                             │ ✓ Recently added           │
                             │   Title A–Z                │
                             │   Release year, newest     │
                             └────────────────────────────┘

LIBRARY (mixed movies and shows)
┌──────────────────────────────────────────────────────────────────────────────┐
│ ◆ [Library ▾] [All|Movies|Series] [⌕ Search library…  ] [Filters ▾ •]  ⇅ ⚙ ⋯    │
│                                                                              │
│   ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐                     │
│   │ ★ 8.1    │  │   SERIES │  │ ★ 7.4    │  │   SERIES │                     │
│   │          │  │          │  │          │  │          │                     │
│   │  POSTER  │  │  POSTER  │  │  POSTER  │  │  POSTER  │                     │
│   │ ✓ Watched│  │ 5/12 seen│  │          │  │ 2/8 seen │                     │
│   └──────────┘  └──────────┘  └──────────┘  └──────────┘                     │
└──────────────────────────────────────────────────────────────────────────────┘
```

### Scope and filter behavior

- Library is a complete local membership query, **not an intersection with the
  current Popular page or currently loaded provider results**. Saved titles
  remain available when a provider is unavailable. Missing metadata can be shown
  as unavailable without hiding the saved entry.
- Library defaults to All kinds, Any status, Recently added. Discover retains
  its Movies/Series control and provider collection menu. Library's Filters menu
  replaces that collection menu; it contains both status and sort controls.
- Switching scope restores each scope's search, filters, sort, scroll and focus
  within the window. Detail → Back restores that same context. Persisting this
  context across app launches is outside this proposal.
- Library search is local over saved titles; Discover search is provider search.
  Search placeholders and accessible labels make that difference explicit.
- For movies: Unwatched means not marked watched. For shows: Unwatched means no
  known episodes watched; In progress means some but not all known episodes
  watched; Watched means all known episodes watched with a nonempty, complete
  episode listing. A newly discovered unwatched episode can move a show back
  to In progress.
- Unknown or incomplete episode listings are not “0 episodes” or “all watched”.
  Show “Episode status unavailable” or “5 watched · total unknown”; do not classify
  the show under a status filter until the listing is complete. It remains in
  Any status. Specials count when supplied as canonical episodes by the catalog.
- An active-status dot is paired with a descriptive accessible label and a
  checked menu item, never color alone. “In progress” is disabled for Movies;
  switching to Movies while it is active resets status to Any.
- Keep the 48 px single-row toolbar. At the 960 px minimum, shorten search
  (minimum 160 px), use the app mark without the wordmark, and shorten Filters
  to an icon with accessible name and tooltip. Do not wrap into a second row.
  Native lab review must verify this fit; the text drawings are not pixel proof.

## 3. Add and manage titles

### Shared Detail actions — user-confirmed requirement

Movie and show Detail pages are shared destinations, regardless of whether they
were opened from Discover, Library, search, or another entry point. The entry
point affects only Back navigation and restored browse context, never which
library or watched-status actions are available.

- Every movie and show Detail exposes **Add to library / Remove from library**,
  based on current membership, not the route used to reach it.
- Movie Detail exposes **Mark watched / Mark unwatched**, based on current status.
- Show Detail exposes **Mark watched / Mark unwatched** for its episodes and
  confirmed season bulk actions, regardless of library membership or entry point.
  The show-level summary remains derived; a whole-show toggle is not proposed.
- These are common Detail controls, not Library-only controls. Discover Detail
  can remove an already-saved title or mark it unwatched; Library Detail can do
  the same. Watching an unsaved title does not silently add it to Library.
- After a successful mutation, all representations of that title share the same
  updated membership/status. Back restores the originating context and applies
  its filters to the updated data; it must not restore stale action states.

This entry-point-independent integration is confirmed by the user; the remaining
wireframe choices below are still proposals.

### Movie detail

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [←] The Quiet Orbit                                                 ⇅ ⚙ ⋯   │
│                                                                              │
│ ┌─────────┐  The Quiet Orbit                                                  │
│ │ POSTER  │  2024 · 118 min · Drama · ★ 8.1                                   │
│ │         │  Synopsis…                                                       │
│ └─────────┘  [▶ Play] [+ Add to library] [○ Mark watched] [⋯]                 │
│                                                                              │
│ After saving: [▶ Play] [✓ In library ▾] [○ Mark watched] [⋯]                 │
│ After watching:                        [✓ Watched ▾]                         │
└──────────────────────────────────────────────────────────────────────────────┘

[✓ In library ▾]                 [✓ Watched ▾]
┌─────────────────────────┐     ┌─────────────────────────┐
│ Remove from library     │     │ Mark unwatched          │
└─────────────────────────┘     └─────────────────────────┘
```

Play stays the only primary action. Library and status actions are secondary;
long titles/actions wrap inside the hero, never overflow it.

### Poster context menu (Discover and Library)

```text
┌─────────────────────────────┐
│ Open details                │
├─────────────────────────────┤
│ Add to library              │  or Remove from library
│ Mark watched                │  movie only; or Mark unwatched
└─────────────────────────────┘
```

- Detail has the always-visible Add action; card context menus offer the fast
  path without turning poster clicks into save toggles. Click/Enter still opens
  Detail. A show card offers membership management; episode status lives in Detail.
- In Discover, saved titles get a quiet bottom-edge “Saved” badge. In Library,
  membership is implicit: use that space for Watched or episode counts. Unwatched
  movies need no rest-state badge. Hover/focus exposes textual membership and
  status without replacing the existing title/facts overlay.
- Context menus support right-click, Menu key and Shift+F10. Badges have accessible
  text and contrast-safe backing, using existing v3 tokens.
- A save/removal only reports success after persistence succeeds. While pending,
  disable repeat activation of that action, not the whole page. On failure keep
  the previous state and show an inline error with Retry; never silently lose it.
- Removal is immediate with an **Undo** notice: “Removed from library. Watched
  status kept.” It never deletes media files. In a filtered grid, restore focus
  to the next item (previous if last), and restore the item on Undo without
  changing sort. The same rule applies when marking watched filters a card out.
- Saved membership and status must refer to canonical title/episode identity,
  not a provider page or poster card. Duplicate provider results must not create
  duplicate library entries. Identity/storage details require implementation design.

## 4. Show detail — episode tracking

```text
┌──────────────────────────────────────────────────────────────────────────────┐
│ [←] North Harbor                                                    ⇅ ⚙ ⋯   │
│ ┌─────────┐  North Harbor                                                    │
│ │ POSTER  │  2024 · 2 seasons · Drama                                        │
│ └─────────┘  [Episodes] [✓ In library ▾] [⋯]                                 │
│                                                                              │
│ Episodes          5 of 12 watched                                            │
│ [Season 1 ▾]  3 of 6 watched                      [Season actions ▾]        │
│                                                                              │
│ ┌──────────────────────────────┐  ┌──────────────────────────────┐           │
│ │ STILL                        │  │ STILL                        │           │
│ │ S01E01 · Arrival              │  │ S01E02 · Signals             │           │
│ │ 46 min          [✓ Watched ▾]│  │ 44 min      [○ Mark watched] │           │
│ └──────────────────────────────┘  └──────────────────────────────┘           │
└──────────────────────────────────────────────────────────────────────────────┘

SEASON ACTIONS
┌────────────────────────────────┐
│ Mark season watched…           │
│ Mark season unwatched…         │
└────────────────────────────────┘
```

- Activating the episode body still opens Streams; the separate status button
  does not navigate or start playback. It is independently keyboard focusable.
- Episode context menus also include Mark watched / Mark unwatched.
- Bulk actions confirm the loaded season and affected count: “Mark 6 episodes
  in Season 1 watched?” They operate atomically on that displayed episode set,
  not future episodes. Disable bulk actions when the season listing is incomplete.
- No whole-show “watched” toggle in the first iteration: explicit season actions
  avoid accidentally marking a long-running show complete.
- Counts update across Detail and Library after a successful save. Unknown totals
  are visibly unknown, never a false completion checkmark.

## 5. Empty and failure states

```text
Library empty
    Your library starts here.
    Save movies and shows to keep them together.             [Browse Discover]

No filter/search matches
    No library titles match these filters.                   [Clear filters]
    Clear filters resets search, kind and status; preserves sort.

Library load failure
    Couldn't load your library.                              [Retry]
    Never show this as an empty library; retain already loaded cards.

Write failure
    Couldn't save watched status. Your previous status is unchanged. [Retry]
```

Library membership and watched status are durable personal data, not disposable
provider/media cache. Refresh and cache cleanup must not remove either. Library
load/search must not require provider credentials; remote artwork/details may
still be unavailable offline. Offline native acceptance remains to be tested.

## 6. Review decisions before implementation

Recommended defaults to approve or revise:

1. **Discover / Library scope menu** on the landing page, with a mixed All view
   in Library; avoid a separate route or navigation rail.
2. **Manual watched status first**, independent of membership; episode-level
   tracking plus confirmed season bulk actions, not automatic playback tracking.
3. **Keep watched status on removal**, with Undo for membership removal.

After approval: resolve canonical identity, incomplete/changed episode listings,
durable storage, background writes and stale-result handling; then add native
GPUI lab fixtures/captures at 1280×800 and 960×600 before production wiring.
Validate identical Detail actions when entering from Discover, Library and search,
including removal and marking unwatched from Discover, and episode tracking for
unsaved shows. Also validate keyboard focus, filter transitions, failed writes,
persistence across restart, provider duplicates, offline listing, and
cache-cleanup safety. Domain
policies belong in kinema-core; presentation/storage adapters in kinema-desktop.

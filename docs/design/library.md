# Library — membership and manual watch tracking

**Status: source implemented and lead-reviewed; scoped validation passed.**
Storage, shared state and native UI are implemented, with reviewed root/feature
checks and Linux synthetic native captures, including movie, hero membership and
episode watched keyboard focus. This is not full MVP, live-provider/playback or
cross-platform acceptance. Exact evidence and remaining limits are recorded in
[development](../development.md#library-and-manual-watch-tracking-validation-pending).
Examples remain fixtures.

Extends [Desktop v3](desktop-v3.md), retaining direct GPUI, Cinema tokens, the
48 px single-row toolbar, poster-first grid and in-place Detail. This approved
contract replaces the earlier bottom-edge Saved badge and watched dropdown
proposal; those are not alternative layouts.

## 1. Independent personal states

- **Discover** browses provider catalogs; **Library** lists explicitly saved movies
  and shows, not downloaded files. Episodes have no separate membership.
- Membership and watched state are independent. Unsaved movies and episodes can
  be marked watched. Removing membership never deletes watch state or media files.
- Movies and episodes have manual watched/unwatched state. A show's summary is
  computed from canonical episodes, never an independent whole-show flag.
- No watch history, viewing dates, resume position, automatic playback completion,
  cloud sync, profiles, import/export or download changes in this increment.
  Playback launch/failure and download completion never mark anything watched.

## 2. Approved control placement

Use Lucide icons, not substitute glyphs or dropdown state controls:

| Surface | Placement and state |
| --- | --- |
| Poster cards (including Discover, Library and recommendations) | Always-visible top-right membership button: `bookmark-off` when unsaved, `bookmark` when saved. Place SERIES **below** this control, not overlapping it. |
| Movie Detail | Hero action row, after existing playback/utility actions: membership button and watched button (`circle` unwatched, `circle-check` watched). |
| Show Detail | Hero membership button; no whole-show watched toggle. |
| Episode still | Top-right watched button (`circle` / `circle-check`); no episode membership. |

Tooltips and accessible names describe the next action: Add to library / Remove
from library, Mark watched / Mark unwatched. Expose current state accessibly too.
Use 16 px glyphs in at least 28 px independently focusable targets, contrast-safe
backing, v3 radius/focus tokens, and pressed/disabled/pending states. Keep Play the
sole primary hero action; secondary actions wrap within the hero when necessary.

Click/Enter on a poster still opens Detail; the episode body still opens Streams.
Child toggle activation must not bubble into card/detail/Streams navigation or
playback. Include controls in keyboard focus traversal. Secondary/context menus
retain equivalent text commands (right-click, Menu key, Shift+F10); a show card
only manages membership, with episode tracking in Detail. No bottom Saved badge
is used as a replacement for the always-visible membership control.

### Entry-point-independent Detail

Movie and show Detail expose exactly the same personal actions whether reached
from Discover, Library, search or recommendations. Only Back context depends on
arrival. Discover Detail can remove a saved title or mark it unwatched; unsaved
shows still expose episode tracking and eligible season bulk actions. All instances
observe shared state after a committed change. Back reapplies current filters to
the originating context rather than restoring stale membership/watch snapshots.

## 3. Landing scope and local queries

One compact **Discover / Library** scope menu sits next to media kind; no rail or
new tab bar. Discover retains its provider kind/search/sort behavior.

Library offers:

- **All / Movies / Series** (default All).
- Local, literal case-insensitive title-name substring search over saved titles.
- **Any / Unwatched / In progress / Watched** (default Any). In progress is
  show-only; selecting Movies resets an active In progress filter to Any.
- **Recently added / Title A–Z / Newest release-year** (default Recently added).
  Status and sort live in Library's Filters menu, replacing provider sort.

Library begins at local membership, not an intersection with a provider page or
loaded cards. Apply kind/search/status before pagination, with stable ID tie
breakers. Do not fetch an entire show to answer a Library query. Saved entries
remain visible without credentials; canonical-only entries must open local Detail
without a usable provider route. Remote artwork/details can remain unavailable.

Restore search, filters, sort, scroll and focus independently per scope within
the window. Persist personal records across restart, not browse context. Detail
Back restores the originating view against updated personal state.

Keep the 48 px toolbar on one row. At 960 px, allow search to shrink to 160 px,
use the app mark without wordmark, and icon-only Filters with accessible name and
tooltip. Status indicators use checked menu items and descriptive labels, not
color alone. Native review must verify actual fit.

## 4. Conservative show completeness

Count canonical episodes including specials and announced episodes already in the
catalog. Known future episodes remain unwatched until manually marked. Newly
arriving episodes are never automatically marked; they can change a summary.

Completeness requires successfully acquired episode listings whose advertised
season/episode totals reconcile. Missing counts, missing/failed season acquisition,
contradictory counts or partial fetches mean **unknown**, based on locally persisted
acquisition evidence, not an eager network crawl.

- Complete, nonempty sets classify as Unwatched (none), In progress (some), or
  Watched (all).
- Empty sets and unknown completeness never imply Watched. Unknown summaries
  remain in Any only, not any of the three status-specific filters. Show
  “N watched · total unknown” or “Episode status unavailable”, not a false zero.
- Season bulk watched/unwatched actions require a complete loaded season.
  Confirm the exact season and count (for example “Mark 6 episodes in Season 1
  watched?”), then atomically change that displayed episode set. Episodes arriving
  after confirmation are excluded. There is no whole-show bulk toggle.
- Storage source now freezes verified title identity anchors, title/season IDs,
  season number and episode ID/number pairs. It rechecks these before the atomic
  bulk write, rejecting stale merged/reused identities instead of targeting
  different rows. Later arrivals do not expand the confirmed set. This source
  guard has scoped source-test coverage; see the schema contract and development
  evidence for validation limits.

## 5. Loading, writes, Undo and focus

Personal state has explicit **Loading / Ready / Failed** states. Unknown state is
not unsaved/unwatched: disable controls until known; offer Retry on failure.
Metadata remains visible even when personal-state reads fail.

Mutations express desired state, with unique request IDs and per-subject pending
guards. Report success only after commit; on failure retain previous state and
show actionable Retry. Disable repeat activation of that action, not the page.
Queue-full rejection leaves UI unchanged. Navigation/read cancellation cannot
cancel an accepted durable mutation. Shared snapshots update even if the initiating
Detail has closed; provider refresh must not overwrite personal state.

After committed removal offer **Undo**: “Removed from library. Watched status
kept.” Restore original added-at ordering only if no newer membership intent has
superseded removal. Undo reapplies the query without discarding sort. When a filter
removes the focused card, focus next (previous if last); empty results focus the
appropriate state action. Back must not resurrect removed items from saved views.

Distinguish:

- Empty Library: “Your library starts here.” → Browse Discover.
- No matches: “No library titles match these filters.” → Clear filters (reset
  search, kind and status, preserve sort).
- Load failure: “Couldn't load your library.” → Retry; retain existing cards,
  never substitute an empty success.
- Write failure: previous state unchanged → Retry.

## 6. Identity, durability and background ownership

Use canonical integer title/episode IDs resolved through verified external-ID
mappings at the SQLite boundary, never provider keys, names or artwork guesses.
If resolution fails, disable writes with a recoverable identity error. Preserve
provider navigation data only where metadata/source APIs require it.

The [source-implemented schema v7](../catalog-schema.md#personal-library-and-manual-watch-state-proposed-schema-v7)
adds membership, movie watch and episode watch tables to the **same**
`catalog.sqlite3`. They and referenced catalog rows are excluded from cache cleanup.
Removing membership does not erase watch records. Deleting the database **loses
these personal records**; no backup/export feature is promised here. No user DB
reset, legacy C++ import, credential storage or path migration is authorized.

Verified canonical graph merges transfer personal rows before deleting losers,
within the same transaction: union membership with earliest added-at; newer watch
mutation wins, ties with conflicting state prefer unwatched. Preserve explicit
unwatched rows and refresh remapped UI refs. Contradictions roll everything back.

Pure policies belong in `kinema-core`; SQL, workers and GPUI in desktop.
`CatalogWorker` remains the sole SQLite owner, with a distinct bounded local-work
queue: accepted mutations are FIFO, acknowledged and never coalesced away by
search/enrichment. Keep HTTP in the existing fetch pool and render free of I/O.
Batch personal reads and summaries rather than querying per card. AppCoordinator
owns shared canonical snapshots, pending/error state and outcomes. Reject stale
scope/detail generations and reads older than a committed personal-state revision.

## 7. Validation contract and scoped evidence

The visual validation contract calls for native fixtures
at **1280×800 and 960×600**, dark/light, keyboard focus and reduced motion:
Discover, Library, movie Detail and show/episodes. Check SERIES stacking, overlay,
title and tooltip collisions. Fixtures are not real-provider acceptance.

Required automated/native scenarios include migrations and restart durability;
FK/type constraints and version refusal; merge collisions/rollback; explicit
unwatched and timestamp ordering; query filtering before pagination; unknown/empty
completeness, specials and future arrivals; season snapshot boundaries; full queues,
failed commits, stale reads and navigation during writes; independent membership;
entry-point parity; child-event isolation; focus/Undo; credential-free local Detail
and cleanup exclusions. See [development validation requirements](../development.md#library-and-manual-watch-tracking-validation-pending).

Implementation is lead-reviewed with scoped checks and captures; the development
evidence lists the exact reviewed scenes rather than implying every matrix cell
was accepted. Compiling code or synthetic fixtures alone do not establish full
offline, provider, playback or cross-platform acceptance.

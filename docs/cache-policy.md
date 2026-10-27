# Cache policy and refresh

Catalog and indexer caching, visible update times, and force-refresh are agreed.
Backend-owner evidence reports v4 canonical metadata/indexer reads and writes,
with twelve normalized `STRICT` tables and seven retained legacy tables. The
[approved catalog specification](catalog-schema.md#target-catalog-schema) remains
the target authority; current post-fix root checks passed (295 tests passed,
3 ignored), while post-v4 authenticated-provider/debrid and new GUI acceptance
remain unverified. Existing GPUI projections do not render
per-field origins or the full rich domain API. The first-increment section below
is historical baseline evidence, not a claim that live paths still use v3 tables.
Automatic refresh timing, retained-entry limits, and offline use of stale results
remain open. No catalog metadata TTL is adopted; artwork misses have the approved
24-hour TTL described below.

The atomic v3 → v4 source upgrade preserves old tables without promising all old
entries are reusable offline: nonempty legacy Cinemeta exact-query searches are
not backfilled because shared title-field provenance is ambiguous, so the first
canonical query requires refresh/fetch. Known legacy detail fields retain `Detail`
provenance quality; only fetch-resource completeness stays `Summary` to request
richer details on demand. Unverified source-target mappings stay unmapped in retained
legacy rows. See [backfill limits](catalog-schema.md#conservative-v3-preservation-and-backfill-limits).
No user database/credentials were accessed; actual upgrade occurs on next open
with the new build.

## Implemented first increment

This section describes the historical v1–v3 Cinemeta exact-query flow. Additional saved
browse/detail/source and TMDB resources are listed in [the actual schema](catalog-schema.md#implemented-first-increment);
provider-specific UI flows now supplement it. Manual refresh and failure-versus-empty
semantics remain required; no automatic TTL or fallback has been adopted.

- Search reads the saved Cinemeta query first; only a miss triggers a remote fetch.
  A saved successful empty result is a cache hit, not a miss or service failure.
- Refresh of the active query reads saved results, then requests Cinemeta regardless
  of saved data. Equivalent pending work may coalesce. Refresh proceeds remotely
  even if reading saved results fails, retaining displayed data until success;
  ordinary Search reports a cache read error without fetching.
- Remote HTTP, validation, or persistence failure does not replace saved membership
  or advance its last-success timestamp. For the same active query the UI retains
  previous results and reports the error; changing query clears the old display.
- There is **no automatic TTL, stale-data refresh, provider fallback, or cache
  pruning** yet. Saved queries remain reusable until manually refreshed. Explicit
  Cinemeta selection describes the baseline; the new explicit Cinemeta/TMDB
  selection requirement is recorded in the [metadata contract](architecture.md#agreed-metadata-provider-contract),
  not claimed verified here.
- The UI labels saved versus fetched results and shows the last successful fetch
  as UTC Unix seconds, not a localized “Updated … ago” label or freshness deadline.

See [the actual schema](catalog-schema.md#implemented-first-increment) for keys and
[worker lifetime limits](architecture.md#implemented-worker-lifetime-limits) for
bounded blocking and stale-result handling.

## Implemented artwork cache

Artwork transport uses one lazily initialized HTTP client with connection pooling,
no proxies/credentials, 5-second connect and 15-second total timeouts. Current
Metahub posters can redirect to `live.metahub.space`: at most three redirects
are followed, only HTTPS-to-HTTPS and only when every target passes `ArtworkKey`
validation (no userinfo, query, fragment or non-HTTP schemes). Referer forwarding
is disabled; invalid redirects fail rather than bypass validation.
Up to eight background jobs run; browse replaces its queued window on each viewport
pass: visible rows in visual order, then two rows below, then two above. Obsolete
queued prefetch is dropped; already-running work retains its slot.

Decoded posters have a 192 MiB byte budget; backdrops retain 16 MiB, with a shared
256-entry ceiling (including failures). Episode stills use a separate 64 MiB /
128-entry allowance and 480×270 thumbnails, so browse posters cannot evict them.
These constants bound accounted decoded pixels, not total process/GPU memory or references held by mounted cards.

Validated static JPEG/PNG/WebP encoded bytes are also cached under
`AppPaths.cache/artwork/<sha256-of-exact-url>.img`, independently of managed media,
with a 256 MiB encoded-file budget. Background workers read/decode and atomically
publish via same-directory temporary files; process-local maintenance is serialized.
Last-use mtime is touched on successful reads and oldest mtimes are evicted first.
Only owned hash-named regular files are pruned; missing, oversized or corrupt files
are misses and reacquired. Error response bodies/invalid image bytes are never persisted.
Disk failures degrade to network use; eviction is best effort on unwritable/locked
files, not a reservation guarantee across multiple processes. Crash-left temporary
files are not counted or automatically removed. Encoded responses remain limited
to 4 MiB and images to 8 megapixels/8192 pixels per edge before thumbnail decoding.
Definitive misses (HTTP 404/410 or invalid/undecodable images) persist as tiny
`<sha256>.miss` timestamp markers for **24 hours** (at most 4096 markers after
maintenance, sharing the disk byte budget). Within that TTL workers skip
transport and views resolve the fallback chain; expired markers are removed and
retried on demand. Transient transport failures (including 5xx, offline and DNS)
remain memory-only until a route/refresh boundary. No-URL artwork makes no request.
Explicit detail Refresh clears persisted misses associated with that title,
including previously visited seasons and earlier mounts/app sessions. Markers
retain only hashed title associations alongside their timestamp. Workers perform marker I/O;
route changes do not remove persisted misses. Memory hits render synchronously;
episode loading shimmers wait 150 ms to avoid a flash on typical disk hits.
TMDB series-detail cache identities include a season-artwork revision: older
normalized records that omitted season summary artwork refresh once; a successful
response with explicit null artwork remains complete.
The fallback policy is primary → landscape (series backdrop when no season
backdrop exists) → season poster for episodes, and season poster → series poster
for tiles. Pending primary work remains Loading, not a promoted fallback.
No catalog metadata TTL or automatic positive-artwork refresh is introduced:
a changed URL changes the key; cache loss is recoverable, not guaranteed offline storage. No URLs are logged by
artwork timing events (`tracing::debug` fetch/decode milliseconds).

Reproduce the opt-in 30-poster network/decode comparison with
`cargo test -p kinema-desktop --lib --locked live_poster_timing -- --ignored --nocapture`
(and add `--release` for release). It compares fresh-client sequential requests
with eight pooled workers, bypassing application disk storage. Both arms use the
current profile; development dependencies are optimized while workspace crates
remain unoptimized. Sequential runs precede pooled runs, so CDN/network warming
can influence results; this is a smoke measurement, not a controlled benchmark.
If local system DNS exceeds the five-second connection timeout, set
`KINEMA_ARTWORK_BENCH_RESOLVE=1` to resolve the public hosts once before timing
(both arms then exclude DNS latency; TLS validation is unchanged). An optional
`KINEMA_ARTWORK_BENCH_CATALOG=/path/to/catalog.json` can use a catalog previously
retrieved from the public endpoint. Neither option affects application transport.

Local smoke measurement (2026-10-18, 30 current Cinemeta poster URLs, disk bypassed,
`KINEMA_ARTWORK_BENCH_RESOLVE=1` because local DNS took about 5.015 seconds):

| Profile | Fresh-client sequential wall | Pooled eight-worker wall | Decode sum, sequential / pooled |
| --- | ---: | ---: | ---: |
| Dev (optimized dependencies) | 22.074 s | 3.174 s | 251 / 252 ms |
| Release | 21.007 s | 1.842 s | 42 / 55 ms |

Both arms use the current validated redirect and decode policies; this compares
connection/scheduling strategies, not historical versus current complete UI builds.
Fetch-time sums were 21,716 / 20,605 ms (dev) and 20,862 / 10,959 ms (release).
No GUI scroll latency or cross-platform performance claim follows from these runs.

## Target cache coverage

- **Catalog metadata and query results:** SQLite is the target primary read model, with shared movie/series, season, and episode metadata independent of Cinemeta/TMDB. Retain rich descriptions, artwork references, external IDs, distinct rating sources, and provenance under the target schema's merge rules. Unknown is nullable, not zero or an invented empty value. Saved query/page membership remains provider-scoped; normalized titles do not imply provider-independent search results.
- **Torrentio / Peerflix results:** keep sanitized source snapshots in SQLite, with their last successful fetch time and fresh/stale state.
- **Debrid status, file lists, and resolved URLs:** use small, bounded in-memory caches, not SQLite tables. These have shorter validity than catalog data.
- **Media bytes:** keep in managed storage under the [storage layout](storage-layout.md). Their lifetime depends on file identity and media storage policy, not metadata TTLs.
- **Artwork:** use a separate bounded cache rather than the media budget.

Return usable local catalog data first. Own and bound demand-driven background
fetches of missing/new details; coalesce duplicate work and reject obsolete results.
Fetch only from the selected metadata provider, never both automatically and never
through automatic fallback. Explicit refresh remains available; this is not a
full-catalog crawler or an adopted periodic/TTL refresh policy. Exact offline and
retention rules remain open.

Response/page, indexer, and media cache cleanup must not implicitly delete canonical
catalog title/season/episode identities, ratings, or provenance. Catalog retention
and deletion are a separate open policy, not response/media eviction. Follow the
[authoritative target schema](catalog-schema.md#target-catalog-schema) for dependency
rules, including retaining lightweight page/fetch stubs where provenance references
require them; pruning a payload must not break provenance.

A provider's status or availability response describes that moment; it does not guarantee later playback. Neither source availability nor debrid readiness establishes watch progress.

## Proposed saved source results

Save normalized, versioned snapshots with useful release details, torrent/file identity hints, and availability hints—not raw provider responses.
Never persist signed access URLs, tokens, or credential-bearing responses in SQLite or media manifests.

If a candidate has only a temporary authorized URL, omit it from the saved snapshot and flag that a network refresh is needed for complete results. Do not invent a torrent identity or treat a stripped URL as playable. The live in-memory result may retain the temporary URL while valid.

Keys must distinguish the exact query, provider/endpoint, non-secret configuration, and an opaque account/configuration scope. Title or hash alone is insufficient. Never put credentials in keys, logs, or labels, or use a token hash as account isolation.
Account, credential, or provider changes invalidate affected entries. Local display filters can reuse snapshots; changes to server-side filters need a different key or scope.

Successful empty results can be cached. A failed request is not an empty result: retain the last successful snapshot and report the error.
Prune old snapshots rather than keeping an indefinite archive. See the [authoritative target catalog schema](catalog-schema.md#target-catalog-schema) for snapshot keys and metadata provenance/locale rules. Current source uses v4
canonical rows and passed current root checks; full rich/provenance UI presentation
and post-v4 authenticated-provider/debrid acceptance remain unverified.

## Target freshness and refresh policy

- Keep `updated_at` (stored metadata changed) distinct from `fetched_at` (last successful fetch for a provider/resource/locale). A successful unchanged fetch may advance only the latter; a local reread or failed fetch advances neither.
- Label visible timestamps accurately: metadata update and last successful fetch are not interchangeable. Show separate catalog, source, or provider times when one would mislead; localize display without changing UTC storage.
- Show saved data, refreshing, and refresh errors where relevant. Freshness deadlines and automatic TTL behavior are not adopted; do not infer expiry merely from elapsed time.

**Refresh** asks the remote service even when cached data is still fresh. Keep existing data visible and replace it only after success; failure must not erase it.
Equivalent refresh requests may share work, but a stale cache alone cannot satisfy force-refresh. Ignore late results from an older request or account/configuration scope.
Refresh still respects offline state, authentication, rate limits, and provider retry guidance. Renew debrid access through the provider's acquisition workflow rather than blindly repeating requests that change provider state.

## Proposed debrid validity

Keep status, file lists, and resolved access separate, scoped by provider, account, and stable asset/file identity. Clear account-specific state on logout or credential changes.
A resolved URL is usable only until the earlier of its real expiry and the local reuse deadline, allowing a safety margin. A TTL never extends real expiry.
When expiry is unknown, reuse conservatively; on expiry or a recognized access rejection, invalidate and resolve again with bounded retries.
Check access before fetching missing ranges. Expired status or URLs do not invalidate compatible bytes already stored locally; never mix bytes from incompatible files.

## Checks

Cover expired and unknown-expiry URLs, scope changes, empty results versus failures, failed force-refresh, and late responses. Start with one persistent source-snapshot table and small in-memory debrid caches; no general cache framework is needed.

## Detail facts payload (schema v5)

The v5 source upgrade adds nullable `metadata_fetches.facts_json`, a bounded,
validated normalized detail projection scoped to the provider/resource/locale
fetch row, not a raw provider response. It retains distinct TMDB score/votes and
IMDb score, runtime, dates/status, US certification, language, credits, studios,
networks and tagline where supplied. Cinemeta also retains its supplied writers,
countries and release-info text. Cast is limited to ten; unknown character names
remain null. Canonical scalar fields and ratings continue using existing rich
imports/provenance; the payload supplies detail-only collections and the selected
provider's facts without implicit cross-provider enrichment.

The atomic v4 → v5 upgrade preserves existing rows and timestamps; NULL facts
load as empty facts. Existing reduced snapshots omit the new optional payload
and also decode empty. On a complete detail fetch row, NULL means facts have
**not been acquired**, whereas a validated non-NULL payload records acquisition
even if every fact is unknown. Legacy summary-completeness backfill rows already
miss the complete-detail read and do not gain completeness from their payload.
Never use `TitleFacts::default()` equality to decide freshness: genuinely empty
provider metadata must not cause an endless refresh loop.

On a successful detail cache hit, Load must show the saved copy and fetch remotely
when `CatalogService::metadata_needs_refresh` returns true. This applies only to
the selected provider's unacquired detail resource, not browse/search/season/source
resources and not an age-based TTL. Emit the saved event with `refreshing=true`,
then the usual fetched/failed event. A failed request must retain the saved facts
and acquisition marker; a successful detail fetch writes a non-NULL facts payload
in the existing save transaction, clearing unknown values and the need to retry.
Explicit Refresh still always fetches. A missing or corrupt cache retains its
existing miss/error behavior. If the optional acquisition predicate itself fails,
keep the successful saved result and treat it as cache-only; log only a safe debug
category, never the backend error string. Worker cache routing now consumes this
predicate in Load mode and logs `refresh_reason="facts_unacquired"`. Fixture tests
cover the saved→fetched sequence, later cache-only loads (including legitimately
empty facts), and cancellation/generation replacement during the predicate.

This introduces no automatic fallback or credential storage. The adapter locale is currently fixed
to en-US, hence certification preference is US; user locale selection is not yet
implemented. Offline fixture tests do not establish real-provider/GUI acceptance.

# Cache policy and refresh

Catalog and indexer caching, visible update times, and force-refresh are agreed.
Only the Cinemeta search cache below is implemented. The broader cache coverage
and freshness policies that follow are targets/proposals; exact TTLs, retained-entry
limits, and offline fallback rules remain open.

## Implemented first increment

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
  Cinemeta selection for this slice does not settle global provider preference.
- The UI labels saved versus fetched results and shows the last successful fetch
  as UTC Unix seconds, not a localized “Updated … ago” label or freshness deadline.

See [the actual schema](catalog-schema.md#implemented-first-increment) for keys and
[worker lifetime limits](architecture.md#implemented-worker-lifetime-limits) for
bounded blocking and stale-result handling.

## Target cache coverage

- **Catalog metadata and query results:** keep in SQLite so local results remain available. Freshness depends on the resource, not one global TTL.
- **Torrentio / Peerflix results:** keep sanitized source snapshots in SQLite, with their last successful fetch time and fresh/stale state.
- **Debrid status, file lists, and resolved URLs:** use small, bounded in-memory caches, not SQLite tables. These have shorter validity than catalog data.
- **Media bytes:** keep in managed storage under the [storage layout](storage-layout.md). Their lifetime depends on file identity and media storage policy, not metadata TTLs.
- **Artwork:** use a separate bounded cache rather than the media budget.

A provider's status or availability response describes that moment; it does not guarantee later playback. Neither source availability nor debrid readiness establishes watch progress.

## Proposed saved source results

Save normalized, versioned snapshots with useful release details, torrent/file identity hints, and availability hints—not raw provider responses.
Never persist signed access URLs, tokens, or credential-bearing responses in SQLite or media manifests.

If a candidate has only a temporary authorized URL, omit it from the saved snapshot and flag that a network refresh is needed for complete results. Do not invent a torrent identity or treat a stripped URL as playable. The live in-memory result may retain the temporary URL while valid.

Keys must distinguish the exact query, provider/endpoint, non-secret configuration, and an opaque account/configuration scope. Title or hash alone is insufficient. Never put credentials in keys, logs, or labels, or use a token hash as account isolation.
Account, credential, or provider changes invalidate affected entries. Local display filters can reuse snapshots; changes to server-side filters need a different key or scope.

Successful empty results can be cached. A failed request is not an empty result: retain the last successful snapshot and report the error.
Prune old snapshots rather than keeping an indefinite archive. See the [proposed catalog schema](catalog-schema.md#proposed-seven-table-schema) for the tables and locale-aware metadata keys.

## Target freshness and refresh policy

- Record the last **successful** remote fetch in UTC; display “Updated … ago” in the user's locale.
- Track the freshness deadline and show cached/stale, refreshing, and refresh errors where relevant.
- Never advance “last updated” after a local reread or failed request. Show separate catalog, source, or provider timestamps when one timestamp would mislead.

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

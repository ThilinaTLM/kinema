# Cache policy and refresh

Catalog and indexer caching, visible update times, and force-refresh are agreed. The storage choices below are proposed; exact TTLs, cache sizes, and offline fallback rules remain open.

## What is cached

- **Catalog metadata and query results:** keep in SQLite so local results remain available. Freshness depends on the resource, not one global TTL.
- **Torrentio / Peerflix results:** keep sanitized source snapshots in SQLite, with their last successful fetch time and fresh/stale state.
- **Debrid status, file lists, and resolved URLs:** use small, bounded in-memory caches, not SQLite tables. These have shorter validity than catalog data.
- **Media bytes:** keep in managed storage under the [storage layout](storage-layout.md). Their lifetime depends on file identity and media storage policy, not metadata TTLs.
- **Artwork:** use a separate bounded cache rather than the media budget.

A provider's status or availability response describes that moment; it does not guarantee later playback. Neither source availability nor debrid readiness establishes watch progress.

## Saved source results

Save normalized, versioned snapshots with useful release details, torrent/file identity hints, and availability hints—not raw provider responses.
Never persist signed access URLs, tokens, or credential-bearing responses in SQLite or media manifests.

If a candidate has only a temporary authorized URL, omit it from the saved snapshot and flag that a network refresh is needed for complete results. Do not invent a torrent identity or treat a stripped URL as playable. The live in-memory result may retain the temporary URL while valid.

Keys must distinguish the exact query, provider/endpoint, non-secret configuration, and an opaque account/configuration scope. Title or hash alone is insufficient. Never put credentials in keys, logs, or labels, or use a token hash as account isolation.
Account, credential, or provider changes invalidate affected entries. Local display filters can reuse snapshots; changes to server-side filters need a different key or scope.

Successful empty results can be cached. A failed request is not an empty result: retain the last successful snapshot and report the error.
Prune old snapshots rather than keeping an indefinite archive. See [catalog schema](catalog-schema.md) for the tables and locale-aware metadata keys.

## Visible freshness and refresh

- Record the last **successful** remote fetch in UTC; display “Updated … ago” in the user's locale.
- Track the freshness deadline and show cached/stale, refreshing, and refresh errors where relevant.
- Never advance “last updated” after a local reread or failed request. Show separate catalog, source, or provider timestamps when one timestamp would mislead.

**Refresh** asks the remote service even when cached data is still fresh. Keep existing data visible and replace it only after success; failure must not erase it.
Equivalent refresh requests may share work, but a stale cache alone cannot satisfy force-refresh. Ignore late results from an older request or account/configuration scope.
Refresh still respects offline state, authentication, rate limits, and provider retry guidance. Renew debrid access through the provider's acquisition workflow rather than blindly repeating requests that change provider state.

## Debrid validity

Keep status, file lists, and resolved access separate, scoped by provider, account, and stable asset/file identity. Clear account-specific state on logout or credential changes.
A resolved URL is usable only until the earlier of its real expiry and the local reuse deadline, allowing a safety margin. A TTL never extends real expiry.
When expiry is unknown, reuse conservatively; on expiry or a recognized access rejection, invalidate and resolve again with bounded retries.
Check access before fetching missing ranges. Expired status or URLs do not invalidate compatible bytes already stored locally; never mix bytes from incompatible files.

## Checks

Cover expired and unknown-expiry URLs, scope changes, empty results versus failures, failed force-refresh, and late responses. Start with one persistent source-snapshot table and small in-memory debrid caches; no general cache framework is needed.

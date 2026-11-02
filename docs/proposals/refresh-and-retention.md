# Refresh, freshness and retention

**Status:** Draft · **Area:** data

## Problem

Saved metadata, pages and source results stay in place until the user refreshes
them by hand. Nothing ever expires, and nothing is ever pruned. Over time the
database grows without limit, and old data looks just as trustworthy as new data.

## Current state

- Refresh is explicit only: there is no TTL, polling or crawling (see
  [data](../architecture/data.md#catalog-local-first-read-model)).
  `updated_at` and `fetched_at` are already stored separately.
- `indexer_snapshots` store the latest indexer results. There is no pruning
  policy for snapshots, saved pages or old fetch rows.
- Cleanup must never delete canonical identities, ratings, provenance or
  personal records. It also must not remove the page and fetch stubs that
  provenance references.

## Options

1. **Keep explicit refresh, and show the data's age.** Show "saved N days ago"
   on stale strips and do nothing automatically. This is the simplest option and
   fits the local-first approach.
2. **Freshness deadlines for each kind of resource.** Pages, details and source
   snapshots each get a soft deadline, and a visit after the deadline triggers a
   background refresh. Stale data stays visible until that refresh succeeds.
   Needs per-provider rate limiting and backoff.
3. **Retention pruning.** Drop payloads (page membership, old indexer
   snapshots) after N days or beyond N entries, while keeping canonical rows and
   the stubs provenance needs. Combine this with 1 or 2.

## Related: saved source results

Indexer snapshots should be versioned, normalized rows, never raw responses.
Their keys must cover the exact query, the provider and endpoint, the non-secret
configuration and an opaque account scope. Strip temporary authorized URLs, and
flag that the user must refresh to get complete results. Never put credentials
or token hashes in keys. A successful empty result can be cached. A failure
keeps the last snapshot.

## Open questions

- Should stale data be usable offline, and how is that labelled?
- How many saved pages or snapshots should be kept per provider?
- Should a user action to "clear cached metadata" exist, and what exactly would
  it keep?

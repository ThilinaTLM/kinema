# Catalog and database

**Agreed target:** SQLite holds the local catalog and cached indexer results.
**Implemented in source:** the provider-specific cache below, now at SQLite
`user_version = 3`. **Proposed:** the normalized seven-table model later in this
document. The current seven provider/cache tables are not that proposed schema.

## Implemented first increment

The original exact-query cache remains the foundation. Version 2 adds
Cinemeta browse/details and source snapshots; version 3 adds TMDB resources.
These source-level migrations are not proof of final end-to-end acceptance.

The desktop adapter stores `catalog.sqlite3` under the build-mode location
specified in [storage layout](storage-layout.md#implemented-catalog-location):
development working-directory `./data/` or production native application data.
Version 1 introduced these tables, retained in version 3:

| Actual table | Stored data and key |
| --- | --- |
| `cinemeta_v3_searches` | `(kind, query)` primary key, last successful `fetched_at` in UTC Unix seconds, and `result_count` (including zero) |
| `cinemeta_v3_titles` | `(kind, imdb_id)` primary key, title name, optional overview |
| `cinemeta_v3_members` | Ordered membership keyed by `(kind, query, position)`, unique `(kind, query, imdb_id)`, foreign keys to searches and titles |

The later source migrations add:

| Actual table | Stored data and key |
| --- | --- |
| `cinemeta_v3_browse` | `(kind, offset)`, fetch time and normalized page payload |
| `cinemeta_v3_details` | `(kind, imdb_id)`, fetch time and normalized detail/episode payload |
| `indexer_sources_v1` | `(indexer, target, scope)`, fetch time and sanitized source payload |
| `tmdb_v1_cache` | `(resource, locale)`, fetch time and normalized page/detail/season payload; locale constrained to `en-US` |

TMDB resource keys bind operation, media kind, numeric ID or exact search text,
and page/season as applicable. Public cached metadata can be read without accessing
a token; remote requests require the selected provider's credentials. Source
snapshots contain stable torrent/file identities, not signed media URLs or tokens.

The original three-table namespace fixes the provider to Cinemeta v3. Query text is trimmed at
the edges, otherwise unchanged and case-sensitive; movie and series queries are
separate. Titles are shared across saved queries of the same kind, so refreshing
one query can update a title's display fields in another saved query. Fetch times
belong to query membership, not independent detail snapshots.

The adapter enables foreign keys, sets a 2-second SQLite busy timeout, and replaces
membership and upserts titles in one transaction after validating the whole remote
response. Network work occurs outside that transaction. Reads validate saved count,
ordered membership, and title values; corrupt data is an error, not an empty result.
Unknown newer versions and nonempty unversioned databases are rejected, not deleted
or silently reset. WAL is not explicitly enabled, and no pruning/reset UI or legacy
migration is implemented.

The original search cache stores exact-query membership, not arbitrary offline
text search. Browse/detail/episode/source and TMDB resource caches now supplement
it; they do not implement the normalized sync engine or multilingual storage
proposed below.
[Current refresh policy](cache-policy.md#implemented-first-increment) defines cache
hits, misses, manual refresh, and error preservation.

## Proposed fuller catalog sync

```mermaid
flowchart LR
    UI["GPUI"] --> Query["Local catalog queries"]
    Query --> DB[("SQLite")]
    Query -->|"Missing / stale data"| Sync["Sync engine"]
    Sync --> Providers["Metadata providers"]
    Sync -->|"Save successful results"| DB
    Sync -->|"Notify affected queries"| Query
```

Return locally usable data immediately. Fetch missing details and, as a proposed policy, refresh stale data in the background. After a successful write, notify the relevant query to read the updated result. SQL runs outside rendering; network calls run outside database transactions.

Start with a bounded in-memory queue that combines duplicate requests. Rebuild needed work from user demand and freshness after restart—no persisted job system or full-catalog crawler.

## Proposed seven-table schema

```mermaid
erDiagram
    TITLES ||--o{ TITLE_IDENTIFIERS : has
    TITLES ||--o{ EPISODES : contains
    TITLES ||--o{ METADATA_FETCHES : refreshed_by
    CATALOG_PAGES ||--o{ CATALOG_PAGE_ITEMS : contains
    TITLES ||--o{ CATALOG_PAGE_ITEMS : appears_in
    TITLES ||--o{ INDEXER_CACHE : has_source_snapshots

    TITLES {
        integer id PK
        text kind "movie or series"
        text name
        text original_name "nullable"
        text overview "nullable"
        text release_date "nullable ISO date"
        integer runtime_minutes "nullable"
        text poster_url "nullable"
        text backdrop_url "nullable"
        text locale
        text display_provider
        text detail_level "summary or detail"
        integer updated_at "UTC"
    }
    TITLE_IDENTIFIERS {
        integer title_id FK
        text namespace PK "tmdb or imdb"
        text media_kind PK,FK "movie or series"
        text external_id PK
    }
    EPISODES {
        integer id PK
        integer series_id FK
        integer season_number
        integer episode_number
        text name "nullable"
        text overview "nullable"
        text air_date "nullable ISO date"
        integer runtime_minutes "nullable"
        text still_url "nullable"
        text locale
        text display_provider
    }
    METADATA_FETCHES {
        integer title_id PK,FK
        text provider PK
        text resource PK "details or episodes"
        text locale PK
        integer fetched_at "last success, UTC"
        integer expires_at "UTC"
    }
    CATALOG_PAGES {
        integer id PK
        text provider
        text query_key "parameters and locale"
        integer page_number
        integer total_pages "nullable"
        integer fetched_at
        integer expires_at
    }
    CATALOG_PAGE_ITEMS {
        integer page_id PK,FK
        integer position PK
        integer title_id FK
    }
    INDEXER_CACHE {
        integer id PK
        integer title_id FK
        text indexer
        text query_key "movie or episode query"
        text cache_scope "opaque config/account revision"
        integer payload_version
        text sources_json "safe normalized candidates"
        boolean has_omitted_sources
        integer fetched_at
        integer expires_at
    }
```

## Proposed table responsibilities

| Table | Purpose and key rules |
| --- | --- |
| `titles` | A movie/series and its current display metadata. Local ID is independent of release, URL, or language. |
| `title_identifiers` | Maps external IDs to local titles. Key: `(namespace, media_kind, external_id)`. Cinemeta can supply an IMDb ID without becoming a separate ID scheme. |
| `episodes` | Episode metadata. Unique `(series_id, season_number, episode_number)`; only series may own episodes. No seasons table until season-specific data is needed. |
| `metadata_fetches` | Last successful fetch per `(title_id, provider, resource, locale)`. A successful empty response is different from never fetched. |
| `catalog_pages` | Search/browse/discovery membership and freshness. Unique `(provider, query_key, page_number)`; query key includes operation, filters, sort, and locale. |
| `catalog_page_items` | Ordered titles in a page. Key `(page_id, position)`; unique `(page_id, title_id)`. |
| `indexer_cache` | Versioned, sanitized source snapshots. Unique `(indexer, query_key, cache_scope)`; episode/configuration/account differences must not collide. See [cache policy](cache-policy.md). |

Caching details alone is insufficient: saved page membership lets the application reopen a collection without waiting for a remote list. Indexer snapshots use typed JSON because the MVP does not need SQL queries over individual candidate fields.

## Proposed integrity rules

- Constrain kind to movie/series, episode coordinates and positions to nonnegative values, and page numbers/payload versions to positive values. Validate typed snapshot JSON.
- Give `titles` a unique `(id, kind)` key. The identifiers' composite foreign key `(title_id, media_kind)` references it, preventing a TMDB movie ID from being mistaken for a TV ID. Never merge titles solely by name.
- Enforce series-only episode ownership in transactional writes. Check conflicting provider IDs/episode numbering rather than silently merging them.
- Enable foreign keys. Page deletion removes its items; title deletion removes its disposable identifiers, episodes, fetch state, page references, and indexer snapshots. Revisit deletion before adding personal records.
- Index child title references and indexer expiry. Replace page membership atomically; partial imports must not mark a resource fresh or downgrade full details to summaries.

## Proposed provider and language policy

The [agreed metadata contract](architecture.md#agreed-metadata-provider-contract)
requires explicit Cinemeta/TMDB selection without automatic fallback; it supersedes
the earlier preferred-provider/fallback proposal. The current TMDB adapter contract
uses fixed `en-US`, not multilingual snapshots. For the fuller proposed schema,
record display provider and completeness rather than merging fields implicitly.

In this proposed model, a fetch timestamp is usable only when the stored snapshot also matches the requested provider, locale, and completeness. Switching English → French → English requires fetching English again if its snapshot was overwritten, even if an old English timestamp is still fresh.

## Fuller database operating model

Start with one SQLite worker, foreign keys enabled, WAL where supported, short transactions, and versioned migrations using `user_version`. Store it in app data; see [storage layout](storage-layout.md).

Ordinary cleanup prunes cached records, not the database file. Full reset is a separate user action, never a fallback for an arbitrary SQL error. Future personal library/progress must survive cache cleanup, but needs no tables yet.

Keep old data on failed refresh and distinguish failure from an empty result. Test ID collisions, episode ownership, locale changes, atomic refresh, cleanup, and migration rollback. The target freshness policy is described in [caching](cache-policy.md); automatic TTLs remain open.

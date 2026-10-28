//! Normalized catalog DDL, applied inside the caller's migration transaction.
//!
//! This module neither imports legacy cache rows nor changes `user_version`.
//! Cross-row resource/identifier coherence, calendar/typed-JSON validation, and
//! available-page count/position validation belong to the validated write boundary.

use super::CatalogError;

pub(super) fn migrate(tx: &rusqlite::Transaction<'_>) -> Result<(), CatalogError> {
    tx.execute_batch(SCHEMA)?;
    Ok(())
}

// STRICT gives integer columns real integer semantics (not merely SQLite affinity).
// All references restrict deletion, except a page's disposable membership rows.
const SCHEMA: &str = r#"
CREATE TABLE titles (
    id INTEGER PRIMARY KEY,
    kind TEXT NOT NULL CHECK(kind IN ('movie', 'series')),
    name TEXT CHECK(name IS NULL OR length(trim(name)) > 0),
    original_name TEXT,
    tagline TEXT,
    overview TEXT,
    release_date TEXT CHECK(release_date IS NULL OR (length(release_date) = 10 AND release_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    release_year INTEGER CHECK(release_year BETWEEN 1 AND 9999),
    runtime_minutes INTEGER CHECK(runtime_minutes > 0),
    genres_json TEXT,
    poster_url TEXT,
    backdrop_url TEXT,
    logo_url TEXT,
    original_language TEXT,
    status TEXT,
    first_air_date TEXT CHECK(first_air_date IS NULL OR (length(first_air_date) = 10 AND first_air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    first_air_year INTEGER CHECK(first_air_year BETWEEN 1 AND 9999),
    last_air_date TEXT CHECK(last_air_date IS NULL OR (length(last_air_date) = 10 AND last_air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    last_air_year INTEGER CHECK(last_air_year BETWEEN 1 AND 9999),
    season_count INTEGER CHECK(season_count >= 0),
    episode_count INTEGER CHECK(episode_count >= 0),
    locale TEXT NOT NULL CHECK(locale = 'en-US'),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    UNIQUE(id, kind),
    CHECK(kind = 'series' OR (first_air_date IS NULL AND first_air_year IS NULL AND last_air_date IS NULL AND last_air_year IS NULL AND season_count IS NULL AND episode_count IS NULL)),
    CHECK(kind = 'movie' OR (release_date IS NULL AND release_year IS NULL)),
    CHECK(release_date IS NULL OR release_year IS NULL OR CAST(substr(release_date, 1, 4) AS INTEGER) = release_year),
    CHECK(first_air_date IS NULL OR first_air_year IS NULL OR CAST(substr(first_air_date, 1, 4) AS INTEGER) = first_air_year),
    CHECK(last_air_date IS NULL OR last_air_year IS NULL OR CAST(substr(last_air_date, 1, 4) AS INTEGER) = last_air_year)
) STRICT;
CREATE TABLE seasons (
    id INTEGER PRIMARY KEY,
    series_id INTEGER NOT NULL,
    series_kind TEXT NOT NULL CHECK(series_kind = 'series'),
    season_number INTEGER NOT NULL CHECK(season_number >= 0),
    name TEXT,
    overview TEXT,
    air_date TEXT CHECK(air_date IS NULL OR (length(air_date) = 10 AND air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    air_year INTEGER CHECK(air_year BETWEEN 1 AND 9999),
    poster_url TEXT,
    episode_count INTEGER CHECK(episode_count >= 0),
    locale TEXT NOT NULL CHECK(locale = 'en-US'),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    FOREIGN KEY(series_id, series_kind) REFERENCES titles(id, kind) ON DELETE RESTRICT,
    UNIQUE(series_id, season_number),
    CHECK(air_date IS NULL OR air_year IS NULL OR CAST(substr(air_date, 1, 4) AS INTEGER) = air_year)
) STRICT;
CREATE TABLE episodes (
    id INTEGER PRIMARY KEY,
    season_id INTEGER NOT NULL REFERENCES seasons(id) ON DELETE RESTRICT,
    episode_number INTEGER NOT NULL CHECK(episode_number > 0),
    name TEXT,
    overview TEXT,
    air_date TEXT CHECK(air_date IS NULL OR (length(air_date) = 10 AND air_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]')),
    runtime_minutes INTEGER CHECK(runtime_minutes > 0),
    production_code TEXT,
    still_url TEXT,
    locale TEXT NOT NULL CHECK(locale = 'en-US'),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    UNIQUE(season_id, episode_number)
) STRICT;
CREATE TABLE catalog_pages (
    id INTEGER PRIMARY KEY,
    provider TEXT NOT NULL CHECK(provider IN ('cinemeta', 'tmdb')),
    query_key TEXT NOT NULL CHECK(length(trim(query_key)) > 0),
    page_number INTEGER NOT NULL CHECK(page_number > 0),
    total_pages INTEGER CHECK(total_pages >= 0),
    result_count INTEGER CHECK(result_count >= 0),
    membership_available INTEGER NOT NULL CHECK(membership_available IN (0, 1)),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    UNIQUE(provider, query_key, page_number),
    CHECK(membership_available = 0 OR result_count IS NOT NULL)
) STRICT;
CREATE TABLE metadata_fetches (
    facts_json TEXT,
    id INTEGER PRIMARY KEY,
    title_id INTEGER REFERENCES titles(id) ON DELETE RESTRICT,
    season_id INTEGER REFERENCES seasons(id) ON DELETE RESTRICT,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE RESTRICT,
    page_id INTEGER REFERENCES catalog_pages(id) ON DELETE RESTRICT,
    provider TEXT NOT NULL CHECK(provider IN ('cinemeta', 'tmdb')),
    resource_key TEXT NOT NULL CHECK(length(trim(resource_key)) > 0),
    locale TEXT NOT NULL CHECK(locale = 'en-US'),
    fetched_at INTEGER CHECK(fetched_at >= 0),
    success_kind TEXT CHECK(success_kind IN ('data', 'empty')),
    completeness TEXT CHECK(completeness IN ('summary', 'detail')),
    last_attempt_at INTEGER CHECK(last_attempt_at >= 0),
    last_error_code TEXT CHECK(last_error_code IS NULL OR (length(last_error_code) BETWEEN 1 AND 64 AND last_error_code NOT GLOB '*[^a-z0-9_]*')),
    CHECK((title_id IS NOT NULL) + (season_id IS NOT NULL) + (episode_id IS NOT NULL) + (page_id IS NOT NULL) = 1),
    CHECK((fetched_at IS NULL AND success_kind IS NULL AND completeness IS NULL) OR (fetched_at IS NOT NULL AND success_kind IS NOT NULL AND completeness IS NOT NULL))
) STRICT;
CREATE TABLE title_identifiers (
    namespace TEXT NOT NULL CHECK(namespace IN ('imdb', 'tmdb')),
    media_kind TEXT NOT NULL CHECK(media_kind IN ('movie', 'series')),
    external_id TEXT NOT NULL CHECK(length(trim(external_id)) > 0),
    title_id INTEGER NOT NULL,
    fetch_state_id INTEGER NOT NULL REFERENCES metadata_fetches(id) ON DELETE RESTRICT,
    observed_at INTEGER NOT NULL CHECK(observed_at >= 0),
    PRIMARY KEY(namespace, media_kind, external_id),
    UNIQUE(title_id, namespace),
    FOREIGN KEY(title_id, media_kind) REFERENCES titles(id, kind) ON DELETE RESTRICT
) STRICT;
CREATE UNIQUE INDEX title_identifiers_imdb ON title_identifiers(external_id) WHERE namespace = 'imdb';
CREATE INDEX title_identifiers_fetch ON title_identifiers(fetch_state_id);
CREATE TABLE season_identifiers (
    namespace TEXT NOT NULL CHECK(namespace = 'tmdb'),
    external_id TEXT NOT NULL CHECK(length(trim(external_id)) > 0),
    season_id INTEGER NOT NULL REFERENCES seasons(id) ON DELETE RESTRICT,
    fetch_state_id INTEGER NOT NULL REFERENCES metadata_fetches(id) ON DELETE RESTRICT,
    observed_at INTEGER NOT NULL CHECK(observed_at >= 0),
    PRIMARY KEY(namespace, external_id),
    UNIQUE(season_id, namespace)
) STRICT;
CREATE INDEX season_identifiers_fetch ON season_identifiers(fetch_state_id);
CREATE TABLE episode_identifiers (
    namespace TEXT NOT NULL CHECK(namespace IN ('imdb', 'tmdb')),
    external_id TEXT NOT NULL CHECK(length(trim(external_id)) > 0),
    episode_id INTEGER NOT NULL REFERENCES episodes(id) ON DELETE RESTRICT,
    fetch_state_id INTEGER NOT NULL REFERENCES metadata_fetches(id) ON DELETE RESTRICT,
    observed_at INTEGER NOT NULL CHECK(observed_at >= 0),
    PRIMARY KEY(namespace, external_id),
    UNIQUE(episode_id, namespace)
) STRICT;
CREATE INDEX episode_identifiers_fetch ON episode_identifiers(fetch_state_id);
CREATE TABLE ratings (
    id INTEGER PRIMARY KEY,
    title_id INTEGER REFERENCES titles(id) ON DELETE RESTRICT,
    season_id INTEGER REFERENCES seasons(id) ON DELETE RESTRICT,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE RESTRICT,
    rating_source TEXT NOT NULL,
    metric TEXT NOT NULL,
    supplied_by TEXT NOT NULL CHECK(supplied_by IN ('cinemeta', 'tmdb')),
    value REAL,
    vote_count INTEGER CHECK(vote_count >= 0),
    source_updated_at INTEGER CHECK(source_updated_at >= 0),
    fetch_state_id INTEGER NOT NULL REFERENCES metadata_fetches(id) ON DELETE RESTRICT,
    observed_at INTEGER NOT NULL CHECK(observed_at >= 0),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    CHECK((title_id IS NOT NULL) + (season_id IS NOT NULL) + (episode_id IS NOT NULL) = 1),
    CHECK(value IS NOT NULL OR vote_count IS NOT NULL),
    CHECK((rating_source IN ('imdb', 'tmdb') AND metric = 'score' AND (value IS NULL OR value BETWEEN 0 AND 10)) OR (rating_source = 'rotten_tomatoes' AND metric IN ('critic_percent', 'audience_percent') AND (value IS NULL OR value BETWEEN 0 AND 100)))
) STRICT;
CREATE INDEX ratings_fetch ON ratings(fetch_state_id);
CREATE TABLE field_provenance (
    id INTEGER PRIMARY KEY,
    title_id INTEGER REFERENCES titles(id) ON DELETE RESTRICT,
    season_id INTEGER REFERENCES seasons(id) ON DELETE RESTRICT,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE RESTRICT,
    field_name TEXT NOT NULL CHECK(length(trim(field_name)) > 0),
    supplied_by TEXT NOT NULL CHECK(supplied_by IN ('cinemeta', 'tmdb')),
    quality TEXT NOT NULL CHECK(quality IN ('summary', 'detail')),
    fetch_state_id INTEGER NOT NULL REFERENCES metadata_fetches(id) ON DELETE RESTRICT,
    observed_at INTEGER NOT NULL CHECK(observed_at >= 0),
    CHECK((title_id IS NOT NULL) + (season_id IS NOT NULL) + (episode_id IS NOT NULL) = 1)
) STRICT;
CREATE INDEX field_provenance_fetch ON field_provenance(fetch_state_id);
CREATE TABLE catalog_page_items (
    page_id INTEGER NOT NULL REFERENCES catalog_pages(id) ON DELETE CASCADE,
    position INTEGER NOT NULL CHECK(position >= 0),
    title_id INTEGER NOT NULL REFERENCES titles(id) ON DELETE RESTRICT,
    PRIMARY KEY(page_id, position),
    UNIQUE(page_id, title_id)
) STRICT;
CREATE INDEX catalog_page_items_title ON catalog_page_items(title_id);
CREATE TABLE indexer_snapshots (
    id INTEGER PRIMARY KEY,
    movie_id INTEGER,
    movie_kind TEXT,
    episode_id INTEGER REFERENCES episodes(id) ON DELETE RESTRICT,
    indexer TEXT NOT NULL CHECK(length(trim(indexer)) > 0),
    query_key TEXT NOT NULL CHECK(length(trim(query_key)) > 0),
    cache_scope TEXT NOT NULL CHECK(length(trim(cache_scope)) > 0),
    payload_version INTEGER NOT NULL CHECK(payload_version = 1),
    sources_json TEXT NOT NULL,
    has_omitted_sources INTEGER NOT NULL CHECK(has_omitted_sources IN (0, 1)),
    fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
    updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
    FOREIGN KEY(movie_id, movie_kind) REFERENCES titles(id, kind) ON DELETE RESTRICT,
    CHECK((movie_id IS NOT NULL) + (episode_id IS NOT NULL) = 1),
    CHECK((movie_id IS NOT NULL AND movie_kind IS NOT NULL AND movie_kind = 'movie') OR (episode_id IS NOT NULL AND movie_kind IS NULL))
) STRICT;
CREATE UNIQUE INDEX ratings_title ON ratings(title_id, rating_source, metric, supplied_by) WHERE title_id IS NOT NULL;
CREATE UNIQUE INDEX ratings_season ON ratings(season_id, rating_source, metric, supplied_by) WHERE season_id IS NOT NULL;
CREATE UNIQUE INDEX ratings_episode ON ratings(episode_id, rating_source, metric, supplied_by) WHERE episode_id IS NOT NULL;
CREATE UNIQUE INDEX field_provenance_title ON field_provenance(title_id, field_name) WHERE title_id IS NOT NULL;
CREATE UNIQUE INDEX field_provenance_season ON field_provenance(season_id, field_name) WHERE season_id IS NOT NULL;
CREATE UNIQUE INDEX field_provenance_episode ON field_provenance(episode_id, field_name) WHERE episode_id IS NOT NULL;
CREATE UNIQUE INDEX metadata_fetches_title ON metadata_fetches(title_id, provider, resource_key, locale) WHERE title_id IS NOT NULL;
CREATE UNIQUE INDEX metadata_fetches_season ON metadata_fetches(season_id, provider, resource_key, locale) WHERE season_id IS NOT NULL;
CREATE UNIQUE INDEX metadata_fetches_episode ON metadata_fetches(episode_id, provider, resource_key, locale) WHERE episode_id IS NOT NULL;
CREATE UNIQUE INDEX metadata_fetches_page ON metadata_fetches(page_id, provider, resource_key, locale) WHERE page_id IS NOT NULL;
CREATE UNIQUE INDEX indexer_snapshots_movie ON indexer_snapshots(movie_id, indexer, query_key, cache_scope) WHERE movie_id IS NOT NULL;
CREATE UNIQUE INDEX indexer_snapshots_episode ON indexer_snapshots(episode_id, indexer, query_key, cache_scope) WHERE episode_id IS NOT NULL;
"#;

/// v7 only: called separately for existing normalized v4-v6 catalogs.
pub(super) fn migrate_library(tx: &rusqlite::Transaction<'_>) -> Result<(), CatalogError> {
    tx.execute_batch(
        r#"
CREATE TABLE library_entries (
 title_id INTEGER PRIMARY KEY REFERENCES titles(id) ON DELETE RESTRICT,
 added_at INTEGER NOT NULL CHECK(added_at >= 0)
) STRICT;
CREATE INDEX library_entries_added ON library_entries(added_at DESC, title_id);
CREATE TABLE movie_watch_state (
 title_id INTEGER PRIMARY KEY,
 title_kind TEXT NOT NULL DEFAULT 'movie' CHECK(title_kind = 'movie'),
 watched INTEGER NOT NULL CHECK(watched IN (0,1)),
 updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
 FOREIGN KEY(title_id,title_kind) REFERENCES titles(id,kind) ON DELETE RESTRICT
) STRICT;
CREATE INDEX movie_watch_state_updated ON movie_watch_state(updated_at DESC);
CREATE TABLE episode_watch_state (
 episode_id INTEGER PRIMARY KEY REFERENCES episodes(id) ON DELETE RESTRICT,
 watched INTEGER NOT NULL CHECK(watched IN (0,1)),
 updated_at INTEGER NOT NULL CHECK(updated_at >= 0)
) STRICT;
CREATE INDEX episode_watch_state_updated ON episode_watch_state(updated_at DESC);
"#,
    )?;
    Ok(())
}

/// v8: independent checkpoints and durable writer authorization; no fabricated progress.
pub(super) fn migrate_playback_progress(
    tx: &rusqlite::Transaction<'_>,
) -> Result<(), CatalogError> {
    for table in ["movie_watch_state", "episode_watch_state"] {
        tx.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN revision INTEGER NOT NULL DEFAULT 0 CHECK(revision >= 0);
            ALTER TABLE {table} ADD COLUMN active_run INTEGER NOT NULL DEFAULT 0 CHECK(active_run >= 0);
            ALTER TABLE {table} ADD COLUMN auto_mark_suppressed INTEGER NOT NULL DEFAULT 0 CHECK(auto_mark_suppressed IN (0,1));"))?;
    }
    tx.execute_batch(r#"
CREATE TABLE movie_playback_progress (
 title_id INTEGER PRIMARY KEY,
 title_kind TEXT NOT NULL DEFAULT 'movie' CHECK(title_kind = 'movie'),
 position_ms INTEGER NOT NULL CHECK(position_ms >= 0),
 duration_ms INTEGER CHECK(duration_ms > 0),
 updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
 revision INTEGER NOT NULL CHECK(revision >= 0),
 FOREIGN KEY(title_id,title_kind) REFERENCES titles(id,kind) ON DELETE RESTRICT
) STRICT;
CREATE INDEX movie_playback_progress_recency ON movie_playback_progress(updated_at DESC,title_id);
CREATE TABLE episode_playback_progress (
 episode_id INTEGER PRIMARY KEY REFERENCES episodes(id) ON DELETE RESTRICT,
 position_ms INTEGER NOT NULL CHECK(position_ms >= 0),
 duration_ms INTEGER CHECK(duration_ms > 0),
 updated_at INTEGER NOT NULL CHECK(updated_at >= 0),
 revision INTEGER NOT NULL CHECK(revision >= 0)
) STRICT;
CREATE INDEX episode_playback_progress_recency ON episode_playback_progress(updated_at DESC,episode_id);
"#)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{Connection, ErrorCode};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn connection() -> Result<Connection, Box<dyn std::error::Error>> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("PRAGMA foreign_keys = ON;")?;
        let tx = db.transaction()?;
        migrate(&tx)?;
        tx.commit()?;
        Ok(db)
    }

    fn seed(db: &Connection) -> TestResult {
        db.execute_batch(
            "INSERT INTO titles(id, kind, locale, updated_at) VALUES
                (1, 'movie', 'en-US', 1), (2, 'series', 'en-US', 1);
             INSERT INTO seasons(id, series_id, series_kind, season_number, locale, updated_at)
                VALUES(1, 2, 'series', 0, 'en-US', 1);
             INSERT INTO episodes(id, season_id, episode_number, locale, updated_at)
                VALUES(1, 1, 1, 'en-US', 1);
             INSERT INTO catalog_pages VALUES(1, 'tmdb', 'v1:synthetic', 1, NULL, NULL, 0, 1);
             INSERT INTO metadata_fetches(id, title_id, provider, resource_key, locale)
                VALUES(1, 1, 'tmdb', 'v1:synthetic', 'en-US');",
        )?;
        Ok(())
    }

    // Do not print statements or bound values on failure: fixtures stay synthetic,
    // and the helper remains safe if later tests add sensitive-looking inputs.
    fn rejects(db: &Connection, sql: &str) {
        assert_eq!(
            db.execute_batch(sql)
                .err()
                .and_then(|error| error.sqlite_error_code()),
            Some(ErrorCode::ConstraintViolation)
        );
    }

    #[test]
    fn synthetic_v3_rows_and_version_survive_and_caller_can_rollback() -> TestResult {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch(
            "PRAGMA foreign_keys = ON;
             CREATE TABLE cinemeta_v3_searches(kind TEXT, query TEXT, fetched_at INTEGER, result_count INTEGER, PRIMARY KEY(kind, query));
             CREATE TABLE cinemeta_v3_titles(kind TEXT, imdb_id TEXT, name TEXT, overview TEXT, PRIMARY KEY(kind, imdb_id));
             CREATE TABLE cinemeta_v3_members(kind TEXT, query TEXT, position INTEGER, imdb_id TEXT, PRIMARY KEY(kind, query, position), FOREIGN KEY(kind, query) REFERENCES cinemeta_v3_searches(kind, query), FOREIGN KEY(kind, imdb_id) REFERENCES cinemeta_v3_titles(kind, imdb_id));
             CREATE TABLE cinemeta_v3_browse(kind TEXT, offset INTEGER, fetched_at INTEGER, payload TEXT, PRIMARY KEY(kind, offset));
             CREATE TABLE cinemeta_v3_details(kind TEXT, imdb_id TEXT, fetched_at INTEGER, payload TEXT, PRIMARY KEY(kind, imdb_id));
             CREATE TABLE indexer_sources_v1(indexer TEXT, target TEXT, scope TEXT, fetched_at INTEGER, payload TEXT, PRIMARY KEY(indexer, target, scope));
             CREATE TABLE tmdb_v1_cache(resource TEXT, locale TEXT, fetched_at INTEGER, payload TEXT, PRIMARY KEY(resource, locale));
             INSERT INTO cinemeta_v3_searches VALUES('movie', 'synthetic', 1, 1);
             INSERT INTO cinemeta_v3_titles VALUES('movie', 'tt0000001', 'Synthetic', NULL);
             INSERT INTO cinemeta_v3_members VALUES('movie', 'synthetic', 0, 'tt0000001');
             INSERT INTO cinemeta_v3_browse VALUES('movie', 0, 1, '[]');
             INSERT INTO cinemeta_v3_details VALUES('movie', 'tt0000001', 1, '{}');
             INSERT INTO indexer_sources_v1 VALUES('synthetic', 'synthetic', 'synthetic', 1, '[]');
             INSERT INTO tmdb_v1_cache VALUES('v1:synthetic', 'en-US', 1, '{}');
             PRAGMA user_version = 3;",
        )
        ?;
        {
            let tx = db.transaction()?;
            migrate(&tx)?;
            // Rolling back the caller's transaction must roll back all DDL.
        }
        let count: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE name = 'titles'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        let tx = db.transaction()?;
        migrate(&tx)?;
        tx.commit()?;
        let count: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 19);
        for table in [
            "cinemeta_v3_searches",
            "cinemeta_v3_titles",
            "cinemeta_v3_members",
            "cinemeta_v3_browse",
            "cinemeta_v3_details",
            "indexer_sources_v1",
            "tmdb_v1_cache",
        ] {
            let count: i64 =
                db.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
            assert_eq!(count, 1);
        }
        let name: String = db.query_row("SELECT name FROM cinemeta_v3_titles", [], |r| r.get(0))?;
        assert_eq!(name, "Synthetic");
        let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        assert_eq!(version, 3);
        assert!(
            db.prepare("PRAGMA foreign_key_check")?
                .query([])?
                .next()?
                .is_none()
        );
        Ok(())
    }

    #[test]
    fn kind_ownership_coordinates_and_integer_checks() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        for sql in [
            "INSERT INTO seasons VALUES(2, 1, 'series', 1, NULL, NULL, NULL, NULL, NULL, NULL, 'en-US', 1)",
            "INSERT INTO seasons VALUES(2, 1, 'movie', 1, NULL, NULL, NULL, NULL, NULL, NULL, 'en-US', 1)",
            "INSERT INTO seasons VALUES(2, 2, 'series', 0, NULL, NULL, NULL, NULL, NULL, NULL, 'en-US', 1)",
            "UPDATE seasons SET season_number = -1",
            "UPDATE episodes SET episode_number = 0",
            "UPDATE episodes SET episode_number = 1.5",
            "UPDATE episodes SET season_id = 999",
            "UPDATE titles SET season_count = 0 WHERE id = 1",
            "UPDATE titles SET release_year = 2000 WHERE id = 2",
            "UPDATE titles SET release_date = '2001-01-01', release_year = 2000 WHERE id = 1",
            "UPDATE titles SET name = ' ' WHERE id = 1",
            "UPDATE titles SET locale = 'fr-FR'",
            "UPDATE titles SET runtime_minutes = 0",
            "UPDATE seasons SET episode_count = -1",
        ] {
            rejects(&db, sql);
        }
        Ok(())
    }

    #[test]
    fn exactly_one_owner_and_fetch_success_tuple() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        for sql in [
            "INSERT INTO metadata_fetches(provider, resource_key, locale) VALUES('tmdb', 'v1:other', 'en-US')",
            "UPDATE metadata_fetches SET season_id = 1",
            "UPDATE metadata_fetches SET fetched_at = 1",
            "UPDATE metadata_fetches SET success_kind = 'empty', completeness = 'detail'",
            "UPDATE metadata_fetches SET provider = 'unknown'",
            "UPDATE metadata_fetches SET resource_key = ''",
            "UPDATE metadata_fetches SET last_error_code = 'unsafe text'",
            "INSERT INTO ratings(rating_source, metric, supplied_by, value, fetch_state_id, observed_at, updated_at) VALUES('imdb', 'score', 'tmdb', 1, 1, 1, 1)",
            "INSERT INTO field_provenance(title_id, season_id, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 1, 'name', 'tmdb', 'detail', 1, 1)",
            "INSERT INTO field_provenance(field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES('name', 'tmdb', 'detail', 1, 1)",
            "INSERT INTO field_provenance(title_id, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 'name', 'tmdb', 'complete', 1, 1)",
            "INSERT INTO field_provenance(title_id, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 'name', 'imdb', 'detail', 1, 1)",
            "INSERT INTO field_provenance(title_id, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 'name', 'tmdb', 'detail', 999, 1)",
        ] {
            rejects(&db, sql);
        }
        db.execute_batch("UPDATE metadata_fetches SET fetched_at = 1, success_kind = 'empty', completeness = 'detail', last_error_code = 'network';")?;
        rejects(&db, "UPDATE metadata_fetches SET completeness = 'complete'");
        Ok(())
    }

    #[test]
    fn partial_uniques_apply_to_every_target_type() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        for target in ["title_id", "season_id", "episode_id"] {
            let rating = format!(
                "INSERT INTO ratings({target}, rating_source, metric, supplied_by, value, fetch_state_id, observed_at, updated_at) VALUES(1, 'imdb', 'score', 'tmdb', 0, 1, 1, 1)"
            );
            db.execute_batch(&rating)?;
            rejects(&db, &rating);
            let provenance = format!(
                "INSERT INTO field_provenance({target}, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 'name', 'tmdb', 'detail', 1, 1)"
            );
            db.execute_batch(&provenance)?;
            rejects(&db, &provenance);
        }
        for target in ["title_id", "season_id", "episode_id", "page_id"] {
            let fetch = format!(
                "INSERT INTO metadata_fetches({target}, provider, resource_key, locale) VALUES(1, 'cinemeta', 'v1:other', 'en-US')"
            );
            db.execute_batch(&fetch)?;
            rejects(&db, &fetch);
        }
        Ok(())
    }

    #[test]
    fn identifiers_have_real_owners_and_imdb_is_kind_global() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        db.execute_batch(
            "INSERT INTO title_identifiers VALUES('imdb', 'movie', 'tt0000001', 1, 1, 1)",
        )?;
        for sql in [
            "INSERT INTO title_identifiers VALUES('imdb', 'series', 'tt0000001', 2, 1, 1)",
            "INSERT INTO title_identifiers VALUES('imdb', 'movie', 'tt0000002', 1, 1, 1)",
            "INSERT INTO title_identifiers VALUES('tmdb', 'series', '1', 1, 1, 1)",
            "INSERT INTO title_identifiers VALUES('tmdb', 'movie', '', 1, 1, 1)",
            "INSERT INTO season_identifiers VALUES('imdb', 'tt0000003', 1, 1, 1)",
            "INSERT INTO episode_identifiers VALUES('tmdb', '1', 999, 1, 1)",
            "INSERT INTO season_identifiers VALUES('tmdb', '1', 1, 999, 1)",
        ] {
            rejects(&db, sql);
        }
        db.execute_batch("INSERT INTO title_identifiers VALUES('tmdb', 'movie', '1', 1, 1, 1); INSERT INTO title_identifiers VALUES('tmdb', 'series', '1', 2, 1, 1); INSERT INTO season_identifiers VALUES('tmdb', '1', 1, 1, 1); INSERT INTO episode_identifiers VALUES('tmdb', '1', 1, 1, 1);")?;
        rejects(
            &db,
            "INSERT INTO season_identifiers VALUES('tmdb', '2', 1, 1, 1)",
        );
        rejects(
            &db,
            "INSERT INTO episode_identifiers VALUES('tmdb', '2', 1, 1, 1)",
        );
        Ok(())
    }

    #[test]
    fn rating_scales_unknowns_and_supplier_enums() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        let insert = "INSERT INTO ratings(title_id, rating_source, metric, supplied_by, value, vote_count, fetch_state_id, observed_at, updated_at) VALUES(1, ?1, ?2, ?3, ?4, ?5, 1, 1, 1)";
        for (source, metric, value) in [
            ("imdb", "score", 10.0),
            ("tmdb", "score", 0.0),
            ("rotten_tomatoes", "critic_percent", 100.0),
            ("rotten_tomatoes", "audience_percent", 0.0),
        ] {
            db.execute(insert, rusqlite::params![source, metric, "tmdb", value, 0])?;
        }
        for sql in [
            "UPDATE ratings SET value = -1",
            "UPDATE ratings SET value = 10.1 WHERE rating_source = 'imdb'",
            "UPDATE ratings SET value = 100.1 WHERE rating_source = 'rotten_tomatoes'",
            "UPDATE ratings SET value = 1e999",
            "UPDATE ratings SET value = NULL, vote_count = NULL",
            "UPDATE ratings SET vote_count = -1",
            "UPDATE ratings SET metric = 'stars'",
            "UPDATE ratings SET supplied_by = 'imdb'",
            "UPDATE ratings SET season_id = 1",
            "UPDATE ratings SET fetch_state_id = 999",
        ] {
            rejects(&db, sql);
        }
        db.execute_batch("UPDATE ratings SET value = NULL, vote_count = 0; UPDATE ratings SET value = 0, vote_count = NULL;")?;
        assert_eq!(
            db.execute("UPDATE ratings SET value = ?1", [f64::NAN])
                .err()
                .and_then(|error| error.sqlite_error_code()),
            Some(ErrorCode::ConstraintViolation)
        );
        Ok(())
    }

    #[test]
    fn empty_page_pruned_page_and_restricted_stub_deletion() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        rejects(&db, "UPDATE catalog_pages SET membership_available = 1");
        db.execute_batch("UPDATE catalog_pages SET result_count = 0, membership_available = 1; INSERT INTO metadata_fetches(id, page_id, provider, resource_key, locale, fetched_at, success_kind, completeness) VALUES(2, 1, 'tmdb', 'v1:page', 'en-US', 1, 'empty', 'summary');")?;
        let available: bool =
            db.query_row("SELECT membership_available FROM catalog_pages", [], |r| {
                r.get(0)
            })?;
        assert!(available);
        db.execute_batch("UPDATE catalog_pages SET result_count = 1; INSERT INTO catalog_page_items VALUES(1, 0, 1);")?;
        rejects(&db, "INSERT INTO catalog_page_items VALUES(1, 1, 1)");
        rejects(&db, "INSERT INTO catalog_page_items VALUES(1, -1, 2)");
        rejects(&db, "DELETE FROM titles WHERE id = 1");
        db.execute_batch(
            "DELETE FROM catalog_page_items; UPDATE catalog_pages SET membership_available = 0;",
        )?;
        let state: (i64, bool) = db.query_row(
            "SELECT result_count, membership_available FROM catalog_pages",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        assert_eq!(state, (1, false));
        rejects(&db, "DELETE FROM catalog_pages");
        db.execute_batch("INSERT INTO field_provenance(title_id, field_name, supplied_by, quality, fetch_state_id, observed_at) VALUES(1, 'name', 'tmdb', 'detail', 2, 1)")?;
        rejects(&db, "DELETE FROM metadata_fetches WHERE id = 2");
        rejects(&db, "DELETE FROM titles WHERE id = 2");
        rejects(&db, "DELETE FROM seasons");
        db.execute_batch("INSERT INTO catalog_pages VALUES(2, 'tmdb', 'v1:disposable', 1, NULL, 1, 1, 1); INSERT INTO catalog_page_items VALUES(2, 0, 1); DELETE FROM catalog_pages WHERE id = 2;")?;
        let count: i64 = db.query_row(
            "SELECT count(*) FROM catalog_page_items WHERE page_id = 2",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        Ok(())
    }

    #[test]
    fn snapshots_cannot_bypass_movie_kind_or_owner_checks() -> TestResult {
        let db = connection()?;
        seed(&db)?;
        db.execute_batch("INSERT INTO indexer_snapshots VALUES(1, 1, 'movie', NULL, 'synthetic', 'v1:synthetic', 'synthetic', 1, '[]', 0, 1, 1); INSERT INTO indexer_snapshots VALUES(2, NULL, NULL, 1, 'synthetic', 'v1:synthetic', 'synthetic', 1, '[]', 1, 1, 1)")?;
        for sql in [
            "UPDATE indexer_snapshots SET movie_kind = NULL WHERE id = 1",
            "UPDATE indexer_snapshots SET movie_id = 2 WHERE id = 1",
            "UPDATE indexer_snapshots SET movie_kind = 'series' WHERE id = 1",
            "UPDATE indexer_snapshots SET movie_kind = 'movie' WHERE id = 2",
            "UPDATE indexer_snapshots SET episode_id = 1 WHERE id = 1",
            "UPDATE indexer_snapshots SET episode_id = NULL WHERE id = 2",
            "UPDATE indexer_snapshots SET payload_version = 2",
            "UPDATE indexer_snapshots SET has_omitted_sources = 2",
            "INSERT INTO indexer_snapshots SELECT 3, movie_id, movie_kind, episode_id, indexer, query_key, cache_scope, payload_version, sources_json, has_omitted_sources, fetched_at, updated_at FROM indexer_snapshots WHERE id = 1",
            "INSERT INTO indexer_snapshots SELECT 3, movie_id, movie_kind, episode_id, indexer, query_key, cache_scope, payload_version, sources_json, has_omitted_sources, fetched_at, updated_at FROM indexer_snapshots WHERE id = 2",
        ] {
            rejects(&db, sql);
        }
        Ok(())
    }
}

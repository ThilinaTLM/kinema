//! Blocking Cinemeta/SQLite adapter. Construct, use, and drop on one background
//! worker, never during rendering. The caller owns cancellation/queueing and
//! rejects stale generations; an in-flight request is bounded by HTTP timeouts.

use std::{
    collections::HashSet,
    error::Error,
    fmt,
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use kinema_core::{
    MediaKind,
    catalog::{CatalogEpisode, CatalogTitle, ImdbId, SearchQuery, TitleDetails},
    sources::{IndexerKind, PlaybackTarget, SourceCandidate},
};
use reqwest::{Url, blocking::Client, redirect::Policy};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

#[path = "catalog_metadata.rs"]
mod metadata;
use metadata::migrate_metadata;
pub(crate) use metadata::provider_cinemeta;
pub(crate) use normalized_tmdb::observation as provider_tmdb_observation;
#[path = "catalog_library.rs"]
pub(crate) mod library;
#[path = "catalog_normalized.rs"]
mod normalized;
#[path = "catalog_normalized_backfill.rs"]
mod normalized_backfill;
#[path = "catalog_normalized_fetches.rs"]
mod normalized_fetches;
#[path = "catalog_normalized_merge.rs"]
mod normalized_merge;
#[path = "catalog_normalized_pages.rs"]
mod normalized_pages;
#[path = "catalog_normalized_pairs.rs"]
mod normalized_pairs;
#[path = "catalog_normalized_read.rs"]
mod normalized_read;
#[path = "catalog_normalized_rich.rs"]
pub(crate) mod normalized_rich;
#[path = "catalog_normalized_tmdb.rs"]
mod normalized_tmdb;
#[path = "catalog_schema.rs"]
mod schema;

const ENDPOINT: &str = "https://v3-cinemeta.strem.io/";
const SCHEMA_VERSION: i64 = 7;
const MAX_EPISODES: usize = 20_000;
const MAX_URL_BYTES: usize = 4096;
const MAX_RESPONSE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RESULTS: usize = 500;
const MAX_ID_CHARS: usize = 64;
const MAX_NAME_CHARS: usize = 500;
const MAX_OVERVIEW_CHARS: usize = 20_000;

/// Messages deliberately contain no upstream URLs, queries, payloads or paths.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CatalogError {
    DataDirectory,
    Storage,
    NewerDatabase,
    InvalidCache,
    Network,
    HttpStatus,
    NotFound,
    InvalidResponse,
    InvalidMetadata,
    IdentityConflict,
    MissingIdentityMapping,
    ResponseTooLarge,
    Clock,
    Indexer,
    Canceled,
    Tmdb(crate::tmdb::TmdbError),
    TmdbMissingCredential,
    TmdbCredentialUnavailable,
    TmdbCredentialDenied,
}

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::DataDirectory => "The catalog data directory is unavailable.",
            Self::Storage => "The saved catalog could not be opened, read, or updated.",
            Self::NewerDatabase => "This catalog was created by a newer version of Kinema Next.",
            Self::InvalidCache => "The saved catalog contains invalid data.",
            Self::Network => "Cinemeta could not be reached or the request timed out.",
            Self::NotFound => "The provider has no matching title.",
            Self::HttpStatus => "Cinemeta returned an unsuccessful response.",
            Self::InvalidResponse => "Cinemeta returned an invalid catalog response.",
            Self::InvalidMetadata => "The provider returned invalid catalog metadata.",
            Self::MissingIdentityMapping => {
                "This action requires a verified catalog identity. Refresh title details and retry."
            }
            Self::IdentityConflict => "The metadata identifiers conflict with the saved catalog.",
            Self::ResponseTooLarge => "Cinemeta returned a catalog response that is too large.",
            Self::Clock => "The system clock cannot timestamp catalog results.",
            Self::Indexer => "The source indexer could not return valid results.",
            Self::Canceled => "The metadata request was canceled.",
            Self::Tmdb(error) => return error.fmt(f),
            Self::TmdbMissingCredential => {
                "Configure a TMDB API Read Access Token to fetch TMDB metadata."
            }
            Self::TmdbCredentialUnavailable => "TMDB secure credential storage is unavailable.",
            Self::TmdbCredentialDenied => "Access to the TMDB credential was denied.",
        })
    }
}

impl Error for CatalogError {}

impl From<rusqlite::Error> for CatalogError {
    fn from(error: rusqlite::Error) -> Self {
        tracing::warn!(category = "sqlite", sqlite_code = ?error.sqlite_error_code(), "Catalog SQL operation failed");
        Self::Storage
    }
}

// Never log SQL/error Display or raw cache keys; they can contain user input.
pub(crate) fn diagnostic_error(error: CatalogError) -> String {
    if error == CatalogError::Canceled {
        tracing::debug!(category = ?error, "Catalog operation canceled");
    } else {
        tracing::warn!(category = ?error, "Catalog operation failed");
    }
    error.to_string()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CachedSearch {
    pub titles: Vec<CatalogTitle>,
    /// Unix seconds, captured after successful download and validation.
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CachedDetails {
    pub details: TitleDetails,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CachedSources {
    pub candidates: Vec<SourceCandidate>,
    pub omitted: usize,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CachedTmdb {
    pub data: crate::tmdb::TmdbData,
    pub fetched_at: u64,
}

pub(crate) struct CatalogService {
    connection: Connection,
    provider_priority: kinema_core::provider::ProviderPriority,
    client: Client,
    endpoint: Url,
    indexers: Option<Arc<crate::sources::IndexerClient>>,
    tmdb: Option<(u64, Arc<crate::tmdb::TmdbClient>)>,
    tmdb_transport: Option<Client>,
    pub(super) cancellation: Option<Arc<AtomicBool>>,
}

fn retire_legacy_caches(tx: &rusqlite::Transaction<'_>) -> Result<(), CatalogError> {
    // Child first, with foreign keys enabled. IF EXISTS also supports catalogs
    // initialized directly with the normalized-only v5 schema.
    tx.execute_batch(
        "DROP TABLE IF EXISTS cinemeta_v3_members;
         DROP TABLE IF EXISTS cinemeta_v3_searches;
         DROP TABLE IF EXISTS cinemeta_v3_titles;
         DROP TABLE IF EXISTS cinemeta_v3_browse;
         DROP TABLE IF EXISTS cinemeta_v3_details;
         DROP TABLE IF EXISTS indexer_sources_v1;
         DROP TABLE IF EXISTS tmdb_v1_cache;",
    )?;
    Ok(())
}

fn initialize_catalog(connection: &mut Connection) -> Result<(), CatalogError> {
    let tx = connection.transaction()?;
    schema::migrate(&tx)?;
    schema::migrate_library(&tx)?;
    tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
    tx.commit()?;
    Ok(())
}

impl CatalogService {
    pub(super) fn ensure_active(&self) -> Result<(), CatalogError> {
        check_cancellation(self.cancellation.as_deref())
    }

    /// Resolve paths and perform all filesystem I/O on the owning worker.
    /// AppPaths owns development/native routing; no migration or fallback.
    pub(crate) fn open_default() -> Result<Self, CatalogError> {
        let paths = crate::paths::AppPaths::resolve().map_err(|_| CatalogError::DataDirectory)?;
        std::fs::create_dir_all(&paths.data).map_err(|error| {
            tracing::warn!(io_kind = ?error.kind(), "Catalog data directory unavailable");
            CatalogError::DataDirectory
        })?;
        Self::open(&paths.data.join("catalog.sqlite3"))
    }

    /// The parent directory must exist. Opening errors never reset/delete a DB.
    pub(crate) fn open(path: &Path) -> Result<Self, CatalogError> {
        let endpoint = Url::parse(ENDPOINT).map_err(|_| CatalogError::Network)?;
        Self::open_at(path, endpoint)
    }

    // Private endpoint injection is only used by local HTTP tests.
    fn open_at(path: &Path, endpoint: Url) -> Result<Self, CatalogError> {
        let client = Client::builder()
            // OS resolution can legitimately spend one 5-second DNS retry on
            // the catalog redirect host. Include it in a bounded connect budget;
            // the unchanged total request deadline still caps the entire fetch.
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::custom(|attempt| {
                if attempt.previous().len() < 3
                    && attempt.url().scheme() == "https"
                    && matches!(
                        attempt.url().host_str(),
                        Some("v3-cinemeta.strem.io" | "cinemeta-catalogs.strem.io")
                    )
                {
                    attempt.follow()
                } else {
                    attempt.stop()
                }
            }))
            .user_agent("Kinema-Next/0.1 catalog")
            .build()
            .map_err(|error| {
                tracing::warn!(
                    kind = crate::logging::http_error_kind(&error),
                    "Catalog HTTP client initialization failed"
                );
                CatalogError::Network
            })?;
        let mut connection = Connection::open(path)?;
        connection.busy_timeout(Duration::from_secs(2))?;
        connection.pragma_update(None, "foreign_keys", true)?;
        let version: i64 = connection.pragma_query_value(None, "user_version", |row| row.get(0))?;
        if version > SCHEMA_VERSION {
            return Err(CatalogError::NewerDatabase);
        }
        if version < 0 {
            return Err(CatalogError::InvalidCache);
        }
        let migration_started = std::time::Instant::now();
        if version < SCHEMA_VERSION {
            tracing::info!(
                from_version = version,
                to_version = SCHEMA_VERSION,
                "Catalog schema migration started"
            );
        }
        if version == 0 {
            let objects: i64 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if objects != 0 {
                return Err(CatalogError::InvalidCache);
            }
            // Fresh catalogs have no legacy records to preserve. Create only
            // the canonical schema, with its version in the same transaction.
            initialize_catalog(&mut connection)?;
        }
        if version == 1 {
            migrate_metadata(&mut connection)?;
        }
        if (1..3).contains(&version) {
            metadata::migrate_tmdb(&mut connection)?;
        }
        if (1..SCHEMA_VERSION).contains(&version) {
            let tx = connection.transaction()?;
            if version < 4 {
                schema::migrate(&tx)?;
                // Backfill may reconcile verified graphs, whose merge boundary
                // now preserves personal rows even though these start empty.
                schema::migrate_library(&tx)?;
                normalized_backfill::backfill(&tx)?;
            } else if version == 4 {
                tx.execute_batch("ALTER TABLE metadata_fetches ADD COLUMN facts_json TEXT;")?;
            }
            // v5 already has canonical data: never replay stale legacy snapshots.
            // The user explicitly authorized disposal of obsolete caches, even
            // old-only results that the conservative backfill cannot represent.
            retire_legacy_caches(&tx)?;
            if version >= 4 {
                schema::migrate_library(&tx)?;
            }
            tx.pragma_update(None, "user_version", SCHEMA_VERSION)?;
            tx.commit()?;
        }
        if version < SCHEMA_VERSION {
            tracing::info!(
                from_version = version,
                to_version = SCHEMA_VERSION,
                duration_ms = migration_started.elapsed().as_millis() as u64,
                "Catalog schema migration completed"
            );
        }
        Ok(Self {
            connection,
            client,
            endpoint,
            indexers: None,
            tmdb: None,
            tmdb_transport: None,
            cancellation: None,
            provider_priority: Default::default(),
        })
    }

    /// A saved empty result is Some, a cache miss is None, failures are Err.
    pub(crate) fn read(
        &mut self,
        query: &SearchQuery,
    ) -> Result<Option<CachedSearch>, CatalogError> {
        normalized_pages::read_cinemeta_page(
            self,
            query.kind(),
            &normalized_pages::cinemeta_search_key(query),
        )
    }

    /// Always fetch remote data, including when a saved snapshot exists. Every
    /// failure leaves the saved snapshot and its timestamp unchanged.
    pub(crate) fn refresh(&mut self, query: &SearchQuery) -> Result<CachedSearch, CatalogError> {
        let result = self.refresh_search_inner(query);
        if let Err(error) = result {
            normalized_fetches::search(self, query, error);
        }
        result
    }
    fn refresh_search_inner(&mut self, query: &SearchQuery) -> Result<CachedSearch, CatalogError> {
        let url = search_url(&self.endpoint, query)?;
        let raw: serde_json::Value = self.fetch_json(url)?;
        let (titles, rich) = parse_search(raw, query.kind())?;
        self.save_search(query, titles, rich)
    }

    fn save_search(
        &mut self,
        query: &SearchQuery,
        titles: Vec<CatalogTitle>,
        rich: Vec<normalized_rich::RichRecord>,
    ) -> Result<CachedSearch, CatalogError> {
        let fetched_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CatalogError::Clock)?
            .as_secs();
        let timestamp = i64::try_from(fetched_at).map_err(|_| CatalogError::Clock)?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        normalized_pages::write_page(
            &tx,
            "cinemeta",
            &normalized_pages::cinemeta_search_key(query),
            1,
            None,
            &titles
                .iter()
                .map(normalized::imdb_observation)
                .collect::<Vec<_>>(),
            timestamp,
            &self.provider_priority,
        )?;
        normalized_rich::write_page_rich(
            &tx,
            "cinemeta",
            &normalized_pages::cinemeta_search_key(query),
            1,
            query.kind(),
            &rich,
            timestamp,
            &self.provider_priority,
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        self.read(query)?.ok_or(CatalogError::InvalidCache)
    }
}

fn check_cancellation(flag: Option<&AtomicBool>) -> Result<(), CatalogError> {
    if flag.is_some_and(|flag| flag.load(Ordering::Acquire)) {
        Err(CatalogError::Canceled)
    } else {
        Ok(())
    }
}

fn kind_text(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Series => "series",
    }
}

fn search_url(endpoint: &Url, query: &SearchQuery) -> Result<Url, CatalogError> {
    let mut url = endpoint.clone();
    url.path_segments_mut()
        .map_err(|_| CatalogError::Network)?
        .pop_if_empty()
        .extend(["catalog", kind_text(query.kind()), "top"]);
    // Stremio treats '&' and '=' as extras delimiters even inside this path
    // segment. Encode the entire value, not just URL path-unsafe characters.
    // set_path preserves these percent escapes (push would double-encode '%').
    let value =
        percent_encoding::utf8_percent_encode(query.text(), percent_encoding::NON_ALPHANUMERIC);
    url.set_path(&format!("{}/search={value}.json", url.path()));
    Ok(url)
}

fn validate_title(
    id: String,
    kind: MediaKind,
    name: String,
    overview: Option<String>,
) -> Result<CatalogTitle, CatalogError> {
    if id.chars().count() > MAX_ID_CHARS
        || name.chars().count() > MAX_NAME_CHARS
        || name.contains('\0')
        || overview.as_ref().is_some_and(|text| text.contains('\0'))
        || overview
            .as_ref()
            .is_some_and(|text| text.chars().count() > MAX_OVERVIEW_CHARS)
    {
        return Err(CatalogError::InvalidResponse);
    }
    let id = ImdbId::new(id).map_err(|_| CatalogError::InvalidResponse)?;
    CatalogTitle::new(id, kind, name, overview).map_err(|_| CatalogError::InvalidResponse)
}

// No defaults on metas or required fields: a changed API/error object is not a
// successful empty search. Optional description is absent in current search data.
#[derive(Deserialize, Serialize)]
struct SearchResponse {
    metas: Vec<SearchMeta>,
}

#[derive(Deserialize, Serialize)]
struct SearchMeta {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
    description: Option<String>,
}

#[cfg(test)]
#[path = "catalog_normalized_acceptance_tests.rs"]
mod normalized_acceptance_tests;
#[cfg(test)]
#[path = "catalog_normalized_merge_tests.rs"]
mod normalized_merge_tests;
#[cfg(test)]
#[path = "catalog_normalized_tests.rs"]
mod normalized_tests;

#[cfg(feature = "ui-probe")]
pub(crate) fn decode_fixture_details(
    kind: MediaKind,
    id: &ImdbId,
    payload: &str,
) -> Result<TitleDetails, CatalogError> {
    metadata::decode_fixture_details(kind, id, payload)
}

fn parse_search(
    raw: serde_json::Value,
    kind: MediaKind,
) -> Result<(Vec<CatalogTitle>, Vec<normalized_rich::RichRecord>), CatalogError> {
    let rich = normalized_rich::cinemeta_summaries(&raw)?;
    let payload: SearchResponse =
        serde_json::from_value(raw).map_err(|_| CatalogError::InvalidResponse)?;
    if payload.metas.len() > MAX_RESULTS {
        return Err(CatalogError::InvalidResponse);
    }
    let mut titles = Vec::with_capacity(payload.metas.len());
    let mut seen = HashSet::new();
    for meta in payload.metas {
        if meta.kind != kind_text(kind) {
            return Err(CatalogError::InvalidResponse);
        }
        let title = validate_title(meta.id, kind, meta.name, meta.description)?;
        if !seen.insert(title.id().clone()) {
            return Err(CatalogError::InvalidResponse);
        }
        titles.push(title);
    }
    Ok((titles, rich))
}

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Read, Write},
        net::TcpListener,
        thread::{self, JoinHandle},
    };

    use super::*;

    #[test]
    fn enrichment_adapters_distinguish_not_found_from_failure() -> TestResult {
        use crate::metadata_providers::{MetadataSource, ProviderError};
        let mock = Mock::start(vec![
            ok(r#"{"meta":null}"#),
            (404, "{}".into()),
            (503, "{}".into()),
        ])?;
        let root = tempfile::tempdir()?;
        let service = CatalogService::open_at(&root.path().join("catalog"), mock.endpoint.clone())?;
        let source = crate::metadata_providers::cinemeta::Cinemeta {
            client: service.client.clone(),
            endpoint: mock.endpoint.clone(),
        };
        let id = ImdbId::new("tt0133093")?;
        let cancel = Arc::new(AtomicBool::new(false));
        assert!(
            source
                .details_by_imdb(MediaKind::Movie, &id, &cancel)
                .map_err(|_| "unexpected provider error")?
                .is_none()
        );
        assert!(
            source
                .details_by_imdb(MediaKind::Movie, &id, &cancel)
                .map_err(|_| "unexpected provider error")?
                .is_none()
        );
        assert!(matches!(
            source.details_by_imdb(MediaKind::Movie, &id, &cancel),
            Err(ProviderError::Failed(CatalogError::HttpStatus))
        ));
        assert_eq!(mock.finish()?.len(), 3);
        let tmdb = Mock::start(vec![ok(r#"{"movie_results":[],"tv_results":[]}"#)])?;
        let source = crate::metadata_providers::tmdb::Tmdb {
            client: Some(Arc::new(crate::tmdb::TmdbClient::at(
                crate::credentials::SecretString::new("fixture-token".into()),
                tmdb.endpoint.clone(),
            )?)),
        };
        assert!(
            source
                .details_by_imdb(MediaKind::Movie, &id, &cancel)
                .map_err(|_| "unexpected provider error")?
                .is_none()
        );
        assert_eq!(tmdb.finish()?.len(), 1);
        Ok(())
    }

    fn enrich_request(
        ids: Vec<ImdbId>,
        providers: Vec<kinema_core::provider::ProviderId>,
        mode: crate::catalog_worker::EnrichMode,
    ) -> crate::catalog_worker::Work {
        use crate::catalog_worker::*;
        Work::Metadata(MetadataRequest {
            generation: 1,
            mode: Mode::Refresh,
            action: MetadataAction::Enrich {
                kind: MediaKind::Movie,
                imdb: ids,
                providers,
                priority: kinema_core::provider::ProviderId::ALL.to_vec(),
                mode,
            },
        })
    }

    #[test]
    fn enrichment_lower_priority_fills_and_find_is_explicit() -> TestResult {
        let _guard = crate::credentials::METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        use crate::{
            catalog_worker::{EnrichMode, MetadataSnapshot, Refreshed},
            credentials::SecretString,
        };
        use kinema_core::provider::ProviderId;
        let cinemeta = Mock::start(vec![ok(
            r#"{"meta":{"id":"tt0133093","type":"movie","name":"Primary name"}}"#,
        )])?;
        let tmdb = Mock::start(vec![
            ok(r#"{"movie_results":[{"id":603}],"tv_results":[]}"#),
            ok(
                r#"{"id":603,"title":"Secondary name","imdb_id":"tt0133093","poster_path":"/poster.jpg","release_date":"1999-03-31","runtime":136,"genres":[{"id":1,"name":"Science Fiction"}],"vote_average":8.2,"vote_count":100}"#,
            ),
        ])?;
        let root = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&root.path().join("catalog"), cinemeta.endpoint.clone())?;
        let id = ImdbId::new("tt0133093")?;
        service.refresh_details(MediaKind::Movie, &id)?;
        service.tmdb = Some((
            crate::credentials::metadata_generation(),
            Arc::new(crate::tmdb::TmdbClient::at(
                SecretString::new("fixture-token".into()),
                tmdb.endpoint.clone(),
            )?),
        ));
        let work = enrich_request(
            vec![id.clone()],
            vec![ProviderId::Tmdb],
            EnrichMode::Details,
        );
        let persist = service.prepare_remote(work, Arc::new(AtomicBool::new(false)))?();
        let Refreshed::Metadata(snapshot) = persist(&mut service)? else {
            return Err("wrong result".into());
        };
        let MetadataSnapshot::Enrichment(snapshot) = *snapshot else {
            return Err("wrong snapshot".into());
        };
        assert!(snapshot.failed.is_empty());
        assert_eq!(snapshot.titles[0].title.name(), "Primary name");
        assert!(snapshot.titles[0].title.poster_url().is_some());
        assert_eq!(snapshot.titles[0].title.runtime_minutes(), Some(136));
        assert_eq!(
            snapshot.titles[0].contributors,
            vec![ProviderId::Cinemeta, ProviderId::Tmdb]
        );
        let requests = tmdb.finish()?;
        assert!(requests[0].starts_with("GET /find/tt0133093?"));
        assert!(requests[0].contains("external_source=imdb_id"));
        assert!(requests[1].starts_with("GET /movie/603?"));
        cinemeta.finish()?;
        Ok(())
    }

    #[test]
    fn enrichment_failure_is_quiet_and_canceled_import_is_rejected() -> TestResult {
        use crate::catalog_worker::{EnrichMode, MetadataSnapshot, Refreshed};
        use kinema_core::provider::ProviderId;
        let mock = Mock::start(vec![
            ok(r#"{"meta":{"id":"tt0133093","type":"movie","name":"Saved"}}"#),
            (503, "{}".into()),
        ])?;
        let root = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&root.path().join("catalog"), mock.endpoint.clone())?;
        let id = ImdbId::new("tt0133093")?;
        service.refresh_details(MediaKind::Movie, &id)?;
        let work = enrich_request(
            vec![id.clone()],
            vec![ProviderId::Cinemeta],
            EnrichMode::Details,
        );
        let persist = service.prepare_remote(work.clone(), Arc::new(AtomicBool::new(false)))?();
        let Refreshed::Metadata(snapshot) = persist(&mut service)? else {
            return Err("wrong result".into());
        };
        let MetadataSnapshot::Enrichment(snapshot) = *snapshot else {
            return Err("wrong snapshot".into());
        };
        assert_eq!(snapshot.failed, vec![ProviderId::Cinemeta]);
        assert_eq!(snapshot.titles[0].title.name(), "Saved");
        mock.finish()?;
        let cancel = Arc::new(AtomicBool::new(true));
        let persist = service.prepare_remote(work, cancel.clone())?();
        service.cancellation = Some(cancel);
        assert!(persist(&mut service).is_err());
        Ok(())
    }

    #[test]
    fn enrichment_batch_skips_complete_cards() -> TestResult {
        use crate::catalog_worker::{EnrichMode, MetadataSnapshot, Refreshed};
        use kinema_core::provider::ProviderId;
        let mock = Mock::start(vec![ok(
            r#"{"meta":{"id":"tt0133093","type":"movie","name":"Complete","poster":"https://example.com/p.jpg","releaseInfo":"1999","runtime":"136 min","genres":["Action"],"imdbRating":"8.1"}}"#,
        )])?;
        let root = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&root.path().join("catalog"), mock.endpoint.clone())?;
        let id = ImdbId::new("tt0133093")?;
        service.refresh_details(MediaKind::Movie, &id)?;
        // Shut down the server: any accidental enrichment HTTP call fails.
        assert_eq!(mock.finish()?.len(), 1);
        let work = enrich_request(vec![id], vec![ProviderId::Cinemeta], EnrichMode::CardFields);
        let persist = service.prepare_remote(work, Arc::new(AtomicBool::new(false)))?();
        let Refreshed::Metadata(snapshot) = persist(&mut service)? else {
            return Err("wrong result".into());
        };
        let MetadataSnapshot::Enrichment(snapshot) = *snapshot else {
            return Err("wrong snapshot".into());
        };
        assert!(snapshot.failed.is_empty());
        assert_eq!(snapshot.titles.len(), 1);
        Ok(())
    }

    type TestResult = Result<(), Box<dyn Error>>;
    #[test]
    fn catalog_migration_and_sql_diagnostics_are_sanitized() -> TestResult {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "catalog_migration_and_sql_diagnostics_are_sanitized",
        ) {
            return Ok(());
        }

        use crate::providers::test_logging::{capture, excludes};
        let root = tempfile::tempdir()?;
        let (result, output) = capture(|| {
            let result = CatalogService::open(&root.path().join("path-secret.sqlite3"));
            let _category = CatalogError::from(rusqlite::Error::InvalidParameterName(
                "sql-parameter-secret".into(),
            ));
            result
        });
        result?;
        assert!(
            output.contains("schema migration started"),
            "captured: {output}"
        );
        assert!(output.contains("schema migration completed"));
        excludes(&output, &["path-secret", "sql-parameter-secret"]);
        Ok(())
    }

    const MOVIE: &str = r#"{"metas":[{"id":"tt0133093","type":"movie","name":"The Matrix"}]}"#;

    struct Mock {
        endpoint: Url,
        thread: JoinHandle<io::Result<Vec<String>>>,
    }

    impl Mock {
        fn start(responses: Vec<(u16, String)>) -> Result<Self, Box<dyn Error>> {
            Self::start_canceling(responses, None)
        }

        fn start_canceling(
            responses: Vec<(u16, String)>,
            cancel: Option<(usize, Arc<AtomicBool>)>,
        ) -> Result<Self, Box<dyn Error>> {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let endpoint = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
            let thread = thread::spawn(move || {
                let mut requests = Vec::new();
                for (index, (status, body)) in responses.into_iter().enumerate() {
                    let (mut stream, _) = listener.accept()?;
                    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                    let mut request = Vec::new();
                    while !request.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        stream.read_exact(&mut byte)?;
                        request.push(byte[0]);
                        if request.len() > 8192 {
                            return Err(io::Error::other("test request too large"));
                        }
                    }
                    requests.push(String::from_utf8_lossy(&request).into_owned());
                    if let Some((cancel_index, flag)) = &cancel
                        && *cancel_index == index
                    {
                        flag.store(true, Ordering::Release);
                    }
                    // A too-large response may be rejected before its body is read.
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                }
                Ok(requests)
            });
            Ok(Self { endpoint, thread })
        }

        fn finish(self) -> Result<Vec<String>, Box<dyn Error>> {
            Ok(self
                .thread
                .join()
                .map_err(|_| io::Error::other("mock panicked"))??)
        }
    }

    fn ok(body: &str) -> (u16, String) {
        (200, body.to_owned())
    }

    #[test]
    fn tmdb_movie_and_series_remote_refresh_fill_unacquired_facts_end_to_end() -> TestResult {
        use crate::{
            catalog_worker::MetadataAction,
            credentials::{CredentialError, SecretString},
            tmdb::{TmdbClient, TmdbData, TmdbDetails, TmdbQuery, TmdbTitle},
        };
        let _guard = crate::credentials::METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        for (kind, id, response, append, certification, credit) in [
            (
                MediaKind::Movie,
                42,
                r#"{"id":42,"title":"Fixture movie","genres":[],"vote_average":8.4,"vote_count":1234,"runtime":121,"credits":{"crew":[{"name":"Fixture director","job":"Director"}],"cast":[{"name":"Fixture actor","character":"Hero"}]},"release_dates":{"results":[{"iso_3166_1":"US","release_dates":[{"certification":"PG-13"}]}]}}"#,
                "external_ids,credits,release_dates",
                "PG-13",
                "Hero",
            ),
            (
                MediaKind::Series,
                43,
                r#"{"id":43,"name":"Fixture series","genres":[],"vote_average":8.7,"vote_count":987,"episode_run_time":[45],"created_by":[{"name":"Fixture creator"}],"aggregate_credits":{"cast":[{"name":"Fixture actor","roles":[{"character":"Detective"}]}]},"content_ratings":{"results":[{"iso_3166_1":"US","rating":"TV-14"}]}}"#,
                "external_ids,aggregate_credits,content_ratings",
                "TV-14",
                "Detective",
            ),
        ] {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("catalog");
            let mut service = CatalogService::open(&path)?;
            let query = TmdbQuery::Details { kind, id };
            let action = MetadataAction::Tmdb(query.clone());
            service.save_tmdb(
                &query,
                TmdbData::Details(TmdbDetails {
                    facts: Default::default(),
                    backdrop_url: None,
                    title: TmdbTitle {
                        id,
                        kind,
                        name: "Old detail".into(),
                        overview: None,
                        release_info: None,
                        poster_url: None,
                        display: Default::default(),
                        imdb: None,
                    },
                    imdb: None,
                    genres: vec![],
                    seasons: vec![],
                }),
            )?;
            // Exactly the state left by the v4 -> v5 migration: valid details,
            // original success timestamp/completeness, but no facts acquisition.
            service
                .connection
                .execute("UPDATE metadata_fetches SET facts_json=NULL", [])?;
            assert!(service.metadata_needs_refresh(&action)?);
            let saved = service.read_tmdb(&query)?.ok_or("old detail missing")?;
            let TmdbData::Details(saved) = saved.data else {
                return Err("wrong old resource".into());
            };
            assert_eq!(saved.facts, Default::default());
            let mock = Mock::start(vec![ok(response)])?;
            service.tmdb = Some((
                crate::credentials::metadata_generation(),
                Arc::new(TmdbClient::at(
                    SecretString::new("fixture-only".into()),
                    mock.endpoint.clone(),
                )?),
            ));
            let refreshed =
                service.refresh_tmdb_with_credential(&query, || Err(CredentialError::Missing))?;
            let TmdbData::Details(refreshed) = refreshed.data else {
                return Err("wrong refreshed resource".into());
            };
            let score = refreshed.facts.tmdb_rating.ok_or("rating missing")?.value();
            assert_eq!(score, if kind == MediaKind::Movie { 8.4 } else { 8.7 });
            assert!(refreshed.facts.tmdb_vote_count.is_some_and(|n| n > 0));
            assert_eq!(
                refreshed.facts.certification.as_deref(),
                Some(certification)
            );
            assert_eq!(refreshed.facts.cast[0].name, "Fixture actor");
            assert_eq!(refreshed.facts.cast[0].character.as_deref(), Some(credit));
            assert_eq!(refreshed.facts.imdb_rating, None);
            if kind == MediaKind::Movie {
                assert_eq!(refreshed.facts.directors, ["Fixture director"]);
            } else {
                assert_eq!(refreshed.facts.creators, ["Fixture creator"]);
            }
            assert!(!service.metadata_needs_refresh(&action)?);
            drop(service);
            let mut service = CatalogService::open(&path)?;
            let reloaded = service
                .read_tmdb(&query)?
                .ok_or("refreshed cache missing")?;
            let TmdbData::Details(reloaded) = reloaded.data else {
                return Err("wrong reloaded resource".into());
            };
            assert_eq!(reloaded.facts, refreshed.facts);
            assert!(!service.metadata_needs_refresh(&action)?);
            let requests = mock.finish()?;
            assert_eq!(requests.len(), 1);
            let target = requests[0]
                .split_whitespace()
                .nth(1)
                .ok_or("request target missing")?;
            let url = Url::parse(&format!("http://fixture.invalid{target}"))?;
            assert!(
                url.query_pairs()
                    .any(|(k, v)| k == "append_to_response" && v == append)
            );
        }
        Ok(())
    }

    #[test]
    fn tmdb_remote_legitimately_empty_facts_records_acquisition() -> TestResult {
        use crate::{
            catalog_worker::MetadataAction,
            credentials::{CredentialError, SecretString},
            tmdb::{TmdbClient, TmdbData, TmdbQuery},
        };
        let _guard = crate::credentials::METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        let mock = Mock::start(vec![ok(r#"{"id":44,"title":"Unknown facts","genres":[]}"#)])?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let mut service = CatalogService::open(&path)?;
        let query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 44,
        };
        let action = MetadataAction::Tmdb(query.clone());
        service.tmdb = Some((
            crate::credentials::metadata_generation(),
            Arc::new(TmdbClient::at(
                SecretString::new("fixture-only".into()),
                mock.endpoint.clone(),
            )?),
        ));
        let fetched =
            service.refresh_tmdb_with_credential(&query, || Err(CredentialError::Missing))?;
        let TmdbData::Details(fetched) = fetched.data else {
            return Err("wrong refreshed resource".into());
        };
        assert_eq!(fetched.facts, Default::default());
        assert!(!service.metadata_needs_refresh(&action)?);
        drop(service);
        let mut service = CatalogService::open(&path)?;
        for _ in 0..2 {
            assert!(service.read_tmdb(&query)?.is_some());
            assert!(!service.metadata_needs_refresh(&action)?);
        }
        assert_eq!(mock.finish()?.len(), 1);
        Ok(())
    }

    #[test]
    fn cinemeta_remote_refresh_fills_imdb_rating_and_keeps_empty_acquisition_complete() -> TestResult
    {
        use crate::catalog_worker::MetadataAction;
        for response in [
            r#"{"meta":{"id":"tt1234567","type":"movie","name":"Rated","imdbRating":"8.1","runtime":"123 min","cast":["Fixture actor"]}}"#,
            r#"{"meta":{"id":"tt1234567","type":"movie","name":"Unknown facts"}}"#,
        ] {
            let mock = Mock::start(vec![ok(response)])?;
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("catalog");
            let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
            let id = ImdbId::new("tt1234567")?;
            let action = MetadataAction::Details {
                kind: MediaKind::Movie,
                id: id.clone(),
            };
            let old = TitleDetails {
                facts: Default::default(),
                title: CatalogTitle::new(id.clone(), MediaKind::Movie, "Old detail", None)?,
                poster_url: None,
                background_url: None,
                release_info: None,
                genres: vec![],
                episodes: vec![],
            };
            let tx = service.connection.transaction()?;
            normalized_pages::write_cinemeta_details(&tx, &old, 1, true, &Default::default())?;
            tx.execute("UPDATE metadata_fetches SET facts_json=NULL", [])?;
            tx.commit()?;
            assert!(service.metadata_needs_refresh(&action)?);
            assert_eq!(
                service
                    .read_details(MediaKind::Movie, &id)?
                    .ok_or("old cache missing")?
                    .details
                    .facts,
                Default::default()
            );
            let refreshed = service.refresh_details(MediaKind::Movie, &id)?;
            if response.contains("imdbRating") {
                assert_eq!(
                    refreshed
                        .details
                        .facts
                        .imdb_rating
                        .map(kinema_core::catalog::ImdbRating::value),
                    Some(8.1)
                );
                assert_eq!(refreshed.details.facts.cast[0].name, "Fixture actor");
            } else {
                assert_eq!(refreshed.details.facts, Default::default());
            }
            assert!(!service.metadata_needs_refresh(&action)?);
            drop(service);
            let mut service = CatalogService::open(&path)?;
            assert_eq!(
                service
                    .read_details(MediaKind::Movie, &id)?
                    .ok_or("new cache missing")?
                    .details
                    .facts,
                refreshed.details.facts
            );
            assert!(!service.metadata_needs_refresh(&action)?);
            assert_eq!(mock.finish()?.len(), 1);
        }
        Ok(())
    }

    #[test]
    fn worker_load_refreshes_null_facts_then_reuses_acquired_rated_or_empty_details() -> TestResult
    {
        use crate::{
            catalog_worker::{
                CatalogWorker, MetadataAction, MetadataEvent, MetadataRequest, MetadataSnapshot,
                Mode, Submit,
            },
            credentials::SecretString,
            tmdb::{TmdbClient, TmdbData, TmdbDetails, TmdbQuery, TmdbTitle},
        };
        use kinema_core::catalog::TitleFacts;
        fn event(worker: &CatalogWorker) -> Result<MetadataEvent, Box<dyn Error>> {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(event) = worker.poll_metadata() {
                    return Ok(event);
                }
                if std::time::Instant::now() >= deadline {
                    return Err("timed out waiting for metadata event".into());
                }
                thread::sleep(Duration::from_millis(2));
            }
        }
        fn facts(snapshot: &MetadataSnapshot) -> Result<&TitleFacts, Box<dyn Error>> {
            match snapshot {
                MetadataSnapshot::Details(saved) => Ok(&saved.details.facts),
                MetadataSnapshot::Tmdb(CachedTmdb {
                    data: TmdbData::Details(details),
                    ..
                }) => Ok(&details.facts),
                _ => Err("unexpected snapshot resource".into()),
            }
        }
        let _guard = crate::credentials::METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        for (tmdb, kind, response, has_rating) in [
            (
                true,
                MediaKind::Movie,
                r#"{"id":42,"title":"Rated movie","genres":[],"vote_average":8.4,"vote_count":120,"credits":{"cast":[{"name":"Actor","character":"Hero"}]},"release_dates":{"results":[{"iso_3166_1":"US","release_dates":[{"certification":"PG"}]}]}}"#,
                true,
            ),
            (
                true,
                MediaKind::Series,
                r#"{"id":42,"name":"Rated series","genres":[],"vote_average":8.4,"vote_count":120,"aggregate_credits":{"cast":[{"name":"Actor","roles":[{"character":"Hero"}]}]},"content_ratings":{"results":[{"iso_3166_1":"US","rating":"TV-14"}]}}"#,
                true,
            ),
            (
                false,
                MediaKind::Movie,
                r#"{"meta":{"id":"tt1234567","type":"movie","name":"Rated movie","imdbRating":"8.4","cast":["Actor"]}}"#,
                true,
            ),
            (
                true,
                MediaKind::Movie,
                r#"{"id":42,"title":"Unknown facts","genres":[]}"#,
                false,
            ),
            (
                false,
                MediaKind::Movie,
                r#"{"meta":{"id":"tt1234567","type":"movie","name":"Unknown facts"}}"#,
                false,
            ),
        ] {
            let mock = Mock::start(vec![ok(response)])?;
            let endpoint = mock.endpoint.clone();
            let action = if tmdb {
                MetadataAction::Tmdb(TmdbQuery::Details { kind, id: 42 })
            } else {
                MetadataAction::Details {
                    kind,
                    id: ImdbId::new("tt1234567")?,
                }
            };
            let seed_action = action.clone();
            let worker = CatalogWorker::spawn(move || {
                // In-memory SQLite prevents any file-lifetime/cleanup dependency
                // on the worker's intentionally asynchronous shutdown.
                let mut service = CatalogService::open_at(Path::new(":memory:"), endpoint.clone())
                    .map_err(|e| e.to_string())?;
                if let MetadataAction::Tmdb(query) = &seed_action {
                    service
                        .save_tmdb(
                            query,
                            TmdbData::Details(TmdbDetails {
                                facts: Default::default(),
                                backdrop_url: None,
                                title: TmdbTitle {
                                    id: 42,
                                    kind,
                                    name: "Old detail".into(),
                                    overview: None,
                                    release_info: None,
                                    poster_url: None,
                                    display: Default::default(),
                                    imdb: None,
                                },
                                imdb: None,
                                genres: vec![],
                                seasons: vec![],
                            }),
                        )
                        .map_err(|e| e.to_string())?;
                    let client =
                        TmdbClient::at(SecretString::new("fixture-only".into()), endpoint.clone())
                            .map_err(|e| e.to_string())?;
                    service.tmdb =
                        Some((crate::credentials::metadata_generation(), Arc::new(client)));
                } else {
                    let id = ImdbId::new("tt1234567").map_err(|e| e.to_string())?;
                    let old = TitleDetails {
                        facts: Default::default(),
                        title: CatalogTitle::new(id, kind, "Old detail", None)
                            .map_err(|e| e.to_string())?,
                        poster_url: None,
                        background_url: None,
                        release_info: None,
                        genres: vec![],
                        episodes: vec![],
                    };
                    let tx = service
                        .connection
                        .transaction()
                        .map_err(|_| "fixture transaction failed")?;
                    normalized_pages::write_cinemeta_details(
                        &tx,
                        &old,
                        1,
                        true,
                        &Default::default(),
                    )
                    .map_err(|e| e.to_string())?;
                    tx.commit().map_err(|_| "fixture commit failed")?;
                }
                service
                    .connection
                    .execute("UPDATE metadata_fetches SET facts_json=NULL", [])
                    .map_err(|_| "fixture NULL seed failed")?;
                Ok(service)
            })?;
            assert_eq!(
                worker.submit_metadata(MetadataRequest {
                    generation: 7,
                    action: action.clone(),
                    mode: Mode::Load
                }),
                Submit::Queued
            );
            let saved = event(&worker)?;
            assert!(saved.saved && saved.refreshing);
            assert_eq!(saved.generation, 7);
            assert_eq!(saved.action, action);
            assert_eq!(facts(&saved.result?)?, &TitleFacts::default());
            let fetched = event(&worker)?;
            assert!(!fetched.saved && !fetched.refreshing);
            assert_eq!(fetched.generation, 7);
            let snapshot = fetched.result?;
            let refreshed = facts(&snapshot)?;
            if has_rating {
                assert_eq!(
                    if tmdb {
                        refreshed.tmdb_rating
                    } else {
                        refreshed.imdb_rating
                    }
                    .map(kinema_core::catalog::ImdbRating::value),
                    Some(8.4)
                );
                assert_eq!(refreshed.cast[0].name, "Actor");
                if tmdb {
                    assert!(refreshed.certification.is_some());
                }
            } else {
                assert_eq!(refreshed, &TitleFacts::default());
            }
            // Both an acquired rich record and an acquired all-unknown record
            // must be terminal cache hits on later Load requests.
            assert_eq!(
                worker.submit_metadata(MetadataRequest {
                    generation: 8,
                    action: action.clone(),
                    mode: Mode::Load
                }),
                Submit::Queued
            );
            let reused = event(&worker)?;
            assert!(reused.saved && !reused.refreshing);
            assert_eq!(reused.generation, 8);
            assert_eq!(reused.result?, snapshot);
            assert!(worker.poll_metadata().is_none());
            drop(worker);
            assert_eq!(mock.finish()?.len(), 1);
        }
        Ok(())
    }

    #[test]
    fn saved_order_and_summaries_survive_reopen() -> TestResult {
        let mock = Mock::start(vec![ok(
            r#"{"metas":[{"id":"tt0234215","type":"movie","name":"Reloaded","description":"Summary"},{"id":"tt0133093","type":"movie","name":"The Matrix"}]}"#,
        )])?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog.sqlite3");
        let query = SearchQuery::new(MediaKind::Movie, "  Matrix  ")?;
        let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
        assert_eq!(service.read(&query)?, None);
        let before = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let snapshot = service.refresh(&query)?;
        assert!(snapshot.fetched_at >= before);
        assert_eq!(snapshot.titles[0].id().as_str(), "tt0234215");
        assert_eq!(snapshot.titles[0].overview(), Some("Summary"));
        assert_eq!(snapshot.titles[1].overview(), None);
        drop(service);
        let mut reopened = CatalogService::open(&path)?;
        assert_eq!(reopened.read(&query)?, Some(snapshot));
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn keys_distinguish_kind_and_case_but_trim_queries() -> TestResult {
        let mock = Mock::start(vec![
            ok(MOVIE),
            ok(r#"{"metas":[{"id":"tt0903747","type":"series","name":"Breaking Bad"}]}"#),
            ok(r#"{"metas":[]}"#),
        ])?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let movie = SearchQuery::new(MediaKind::Movie, "Matrix")?;
        let series = SearchQuery::new(MediaKind::Series, "Matrix")?;
        let lowercase = SearchQuery::new(MediaKind::Movie, "matrix")?;
        let m = service.refresh(&movie)?;
        let s = service.refresh(&series)?;
        let lower = service.refresh(&lowercase)?;
        assert!(lower.titles.is_empty());
        assert_eq!(service.read(&movie)?, Some(m.clone()));
        assert_eq!(service.read(&series)?, Some(s));
        assert_eq!(service.read(&lowercase)?, Some(lower));
        assert_eq!(
            service.read(&SearchQuery::new(MediaKind::Movie, " Matrix ")?)?,
            Some(m)
        );
        assert_eq!(
            service.read(&SearchQuery::new(MediaKind::Movie, "MATRIX")?)?,
            None
        );
        let requests = mock.finish()?;
        assert!(requests[1].starts_with("GET /catalog/series/top/search=Matrix.json "));
        Ok(())
    }

    #[test]
    fn explicit_refresh_replaces_saved_data_with_successful_empty() -> TestResult {
        let mock = Mock::start(vec![ok(MOVIE), ok(r#"{"metas":[]}"#)])?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let query = SearchQuery::new(MediaKind::Movie, "matrix")?;
        service.refresh(&query)?;
        let empty = service.refresh(&query)?;
        assert!(empty.titles.is_empty());
        assert_eq!(service.read(&query)?, Some(empty));
        assert_eq!(mock.finish()?.len(), 2);
        Ok(())
    }

    #[test]
    fn upstream_failures_and_invalid_payloads_preserve_snapshot_and_timestamp() -> TestResult {
        let mut responses = vec![ok(MOVIE)];
        let invalid = vec![
            (503, "private upstream message".into()),
            ok("not json"),
            ok("{}"),
            ok(r#"{"metas":null}"#),
            ok(r#"{"metas":{}}"#),
            ok(r#"{"metas":[{"id":"tt0133093","type":"series","name":"Wrong kind"}]}"#),
            ok(r#"{"metas":[{"id":"bad","type":"movie","name":"Bad id"}]}"#),
            ok(r#"{"metas":[{"id":"tt0133093","type":"movie","name":" "}]}"#),
            ok(r#"{"metas":[{"id":"tt0133093","type":"movie","name":"Good","description":5}]}"#),
            ok(
                r#"{"metas":[{"id":"tt0133093","type":"movie","name":"One"},{"id":"tt0133093","type":"movie","name":"Duplicate"}]}"#,
            ),
            ok(&format!(
                r#"{{"metas":[{{"id":"tt0133093","type":"movie","name":"{}"}}]}}"#,
                "n".repeat(MAX_NAME_CHARS + 1)
            )),
            ok(&format!(
                r#"{{"metas":[{}]}}"#,
                vec![r#"{"id":"tt0133093","type":"movie","name":"One"}"#; MAX_RESULTS + 1]
                    .join(",")
            )),
            ok(&"x".repeat(MAX_RESPONSE_BYTES as usize + 1)),
        ];
        let failures = invalid.len();
        responses.extend(invalid);
        let mock = Mock::start(responses)?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let query = SearchQuery::new(MediaKind::Movie, "matrix")?;
        service.refresh(&query)?;
        service
            .connection
            .execute("UPDATE metadata_fetches SET fetched_at = 123", [])?;
        let saved = service.read(&query)?;
        for _ in 0..failures {
            assert!(service.refresh(&query).is_err());
            assert_eq!(service.read(&query)?, saved);
        }
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn mid_write_sql_failure_rolls_back_header_members_and_shared_titles() -> TestResult {
        let mock = Mock::start(vec![
            ok(MOVIE),
            ok(
                r#"{"metas":[{"id":"tt0133093","type":"movie","name":"Changed"},{"id":"tt0234215","type":"movie","name":"New"}]}"#,
            ),
        ])?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
        let query = SearchQuery::new(MediaKind::Movie, "matrix")?;
        service.refresh(&query)?;
        service.connection.execute_batch(
            "UPDATE metadata_fetches SET fetched_at = 123;
             CREATE TRIGGER fail_second_member BEFORE INSERT ON catalog_page_items
             WHEN NEW.position = 1 BEGIN SELECT RAISE(ABORT, 'forced test failure'); END;",
        )?;
        let saved = service.read(&query)?;
        assert_eq!(service.refresh(&query), Err(CatalogError::Storage));
        assert_eq!(service.read(&query)?, saved);
        let title_count: i64 =
            service
                .connection
                .query_row("SELECT count(*) FROM titles", [], |row| row.get(0))?;
        assert_eq!(title_count, 1);
        drop(service);
        assert_eq!(CatalogService::open(&path)?.read(&query)?, saved);
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn corrupt_saved_data_is_an_error_not_empty() -> TestResult {
        let corruptions = [
            "PRAGMA ignore_check_constraints = ON; UPDATE titles SET name = ' '",
            "PRAGMA ignore_check_constraints = ON; UPDATE titles SET name = replace(hex(zeroblob(501)), '0', 'x')",
            "UPDATE titles SET overview = replace(hex(zeroblob(20001)), '0', 'x')",
            "PRAGMA foreign_keys = OFF; UPDATE title_identifiers SET external_id = 'bad'",
            "DELETE FROM catalog_page_items",
            "PRAGMA ignore_check_constraints = ON; UPDATE metadata_fetches SET fetched_at = -1",
            "PRAGMA ignore_check_constraints = ON; UPDATE catalog_pages SET result_count = 501",
            "UPDATE catalog_page_items SET position = 1",
        ];
        let mock = Mock::start(vec![ok(MOVIE); corruptions.len()])?;
        let dir = tempfile::tempdir()?;
        for (index, sql) in corruptions.iter().enumerate() {
            let path = dir.path().join(format!("catalog{index}"));
            let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
            let query = SearchQuery::new(MediaKind::Movie, "matrix")?;
            service.refresh(&query)?;
            service.connection.execute_batch(sql)?;
            assert!(service.read(&query).is_err(), "corruption accepted: {sql}");
            drop(service);
            assert!(CatalogService::open(&path)?.read(&query).is_err());
        }
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn search_query_stays_in_one_encoded_path_segment() -> TestResult {
        let mock = Mock::start(vec![ok(r#"{"metas":[]}"#); 2])?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let query = SearchQuery::new(MediaKind::Movie, "a/b?c#雪 %")?;
        let url = search_url(&mock.endpoint, &query)?;
        assert_eq!(url.query(), None);
        assert_eq!(url.fragment(), None);
        assert_eq!(url.path_segments().map(Iterator::count), Some(4));
        service.refresh(&query)?;
        service.refresh(&SearchQuery::new(MediaKind::Movie, "a&genre=Action")?)?;
        let requests = mock.finish()?;
        assert!(
            requests[1]
                .starts_with("GET /catalog/movie/top/search=a%26genre%3DAction.json HTTP/1.1\r\n")
        );
        assert!(requests[0].starts_with(
            "GET /catalog/movie/top/search=a%2Fb%3Fc%23%E9%9B%AA%20%25.json HTTP/1.1\r\n"
        ));
        Ok(())
    }

    #[test]
    fn fresh_database_has_only_normalized_tables_and_reopens() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("fresh");
        let service = CatalogService::open(&path)?;
        let tables: i64 = service.connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(tables, 15);
        assert_eq!(
            service
                .connection
                .pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            SCHEMA_VERSION
        );
        assert_eq!(
            service
                .connection
                .pragma_query_value(None, "foreign_keys", |row| row.get::<_, i64>(0))?,
            1
        );
        assert!(
            !service
                .connection
                .prepare("PRAGMA foreign_key_check")?
                .exists([])?
        );
        drop(service);
        CatalogService::open(&path)?;
        Ok(())
    }

    #[test]
    fn fresh_schema_failure_rolls_back_tables_and_version() -> TestResult {
        let mut connection = Connection::open_in_memory()?;
        // Force a DDL failure after titles/seasons/episodes have been created.
        connection.execute_batch(
            "CREATE TABLE catalog_pages(marker TEXT); INSERT INTO catalog_pages VALUES ('keep');",
        )?;
        assert_eq!(
            initialize_catalog(&mut connection),
            Err(CatalogError::Storage)
        );
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            0
        );
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(tables, 1);
        let marker: String =
            connection.query_row("SELECT marker FROM catalog_pages", [], |row| row.get(0))?;
        assert_eq!(marker, "keep");
        Ok(())
    }

    fn assert_retired(connection: &Connection) -> TestResult {
        let tables: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(tables, 15);
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            SCHEMA_VERSION
        );
        assert!(!connection.prepare("PRAGMA foreign_key_check")?.exists([])?);
        Ok(())
    }

    fn canonical_snapshot(
        connection: &Connection,
    ) -> Result<Vec<Vec<Vec<rusqlite::types::Value>>>, rusqlite::Error> {
        [
            "titles",
            "title_identifiers",
            "seasons",
            "season_identifiers",
            "episodes",
            "episode_identifiers",
            "ratings",
            "field_provenance",
            "metadata_fetches",
            "catalog_pages",
            "catalog_page_items",
            "indexer_snapshots",
        ]
        .iter()
        .map(|table| {
            let mut statement =
                connection.prepare(&format!("SELECT * FROM {table} ORDER BY rowid"))?;
            let columns = statement.column_count();
            statement
                .query_map([], |row| {
                    (0..columns).map(|column| row.get(column)).collect()
                })?
                .collect()
        })
        .collect()
    }

    fn populated_v5(path: &Path) -> Result<Connection, Box<dyn Error>> {
        let mut connection = legacy_database(path)?;
        let tx = connection.transaction()?;
        schema::migrate(&tx)?;
        let id = ImdbId::new("tt0903747")?;
        let details = TitleDetails {
            title: CatalogTitle::new(
                id,
                MediaKind::Series,
                "Breaking Bad",
                Some("Canonical overview".into()),
            )?,
            facts: Default::default(),
            poster_url: Some("https://example.test/poster.jpg".into()),
            background_url: None,
            release_info: Some("2008".into()),
            genres: vec![],
            episodes: vec![CatalogEpisode {
                season: 1,
                number: 1,
                name: Some("Pilot".into()),
                overview: Some("Canonical episode".into()),
                released: None,
                still_url: None,
                runtime_minutes: None,
            }],
        };
        normalized_pages::write_cinemeta_details(&tx, &details, 123, true, &Default::default())?;
        normalized_pages::write_page(
            &tx,
            "cinemeta",
            "preserved-page",
            1,
            None,
            &[normalized::imdb_observation(&details.title)],
            124,
            &Default::default(),
        )?;
        // Populate every normalized table, including identifiers and snapshots
        // not supplied by the reduced Cinemeta fixture above.
        let series: i64 = tx.query_row("SELECT id FROM titles", [], |row| row.get(0))?;
        let season: i64 = tx.query_row("SELECT id FROM seasons", [], |row| row.get(0))?;
        let episode: i64 = tx.query_row("SELECT id FROM episodes", [], |row| row.get(0))?;
        let title_fetch: i64 = tx.query_row(
            "SELECT id FROM metadata_fetches WHERE title_id=?1 LIMIT 1",
            [series],
            |row| row.get(0),
        )?;
        tx.execute("INSERT INTO ratings(title_id,rating_source,metric,supplied_by,value,vote_count,fetch_state_id,observed_at,updated_at) VALUES (?1,'imdb','score','cinemeta',8.5,123,?2,123,123)", params![series,title_fetch])?;
        tx.execute("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (?1,'tmdb',?2,'en-US',123,'data','detail')", params![season,crate::tmdb::cache_query_identity(&crate::tmdb::TmdbQuery::Season {series_id:1396,season:1})?])?;
        let season_fetch = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO season_identifiers VALUES ('tmdb','3572',?1,?2,123)",
            params![season, season_fetch],
        )?;
        tx.execute("INSERT INTO metadata_fetches(episode_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (?1,'tmdb','synthetic-episode-details','en-US',123,'data','detail')", [episode])?;
        let episode_fetch = tx.last_insert_rowid();
        tx.execute(
            "INSERT INTO episode_identifiers VALUES ('tmdb','62085',?1,?2,123)",
            params![episode, episode_fetch],
        )?;
        tx.execute("INSERT INTO indexer_snapshots(episode_id,indexer,query_key,cache_scope,payload_version,sources_json,has_omitted_sources,fetched_at,updated_at) VALUES (?1,'torrentio','series:tt0903747:1:1','public-default-v1',1,'{\"candidates\":[],\"omitted\":0}',0,123,123)", [episode])?;
        tx.pragma_update(None, "user_version", 5)?;
        tx.commit()?;
        assert!(
            canonical_snapshot(&connection)?
                .iter()
                .all(|rows| !rows.is_empty())
        );
        // Every legacy table contains data, including unsupported payloads and
        // stale details that must never overwrite the canonical series above.
        connection.execute_batch("INSERT INTO cinemeta_v3_searches VALUES ('movie','exact query',1,1);
            INSERT INTO cinemeta_v3_titles VALUES ('movie','tt1234567','Unique title','Unique overview');
            INSERT INTO cinemeta_v3_members VALUES ('movie','exact query',0,'tt1234567');
            INSERT INTO cinemeta_v3_browse VALUES ('movie',0,1,'unsupported browse');
            INSERT INTO cinemeta_v3_details VALUES ('series','tt0903747',999,'stale unsupported detail');
            INSERT INTO indexer_sources_v1 VALUES ('torrentio','movie:tt9999999','public-default-v1',2,'unsupported source');
            INSERT INTO tmdb_v1_cache VALUES ('unsupported','en-US',1,'unsupported TMDB');")?;
        Ok(connection)
    }

    #[test]
    fn version_five_disposes_legacy_without_replaying_or_changing_canonical_data() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("v5");
        let connection = populated_v5(&path)?;
        let before = canonical_snapshot(&connection)?;
        drop(connection);
        let mut service = CatalogService::open(&path)?;
        assert_retired(&service.connection)?;
        assert_eq!(canonical_snapshot(&service.connection)?, before);
        assert_eq!(
            service
                .read_details(MediaKind::Series, &ImdbId::new("tt0903747")?)?
                .ok_or("missing details")?
                .details
                .title
                .name(),
            "Breaking Bad"
        );
        drop(service);
        let service = CatalogService::open(&path)?;
        assert_retired(&service.connection)?;
        assert_eq!(canonical_snapshot(&service.connection)?, before);
        Ok(())
    }

    #[test]
    fn failed_last_drop_restores_all_legacy_tables_and_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("v5");
        let connection = populated_v5(&path)?;
        let before = canonical_snapshot(&connection)?;
        // A view with the final table's name makes DROP TABLE fail after the
        // preceding six drops. SQLite must roll all those DDL operations back.
        connection.execute_batch("ALTER TABLE tmdb_v1_cache RENAME TO retained_tmdb; CREATE VIEW tmdb_v1_cache AS SELECT * FROM retained_tmdb;")?;
        drop(connection);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::Storage)
        ));
        let connection = Connection::open(&path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            5
        );
        assert_eq!(canonical_snapshot(&connection)?, before);
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
                connection.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })?;
            assert_eq!(count, 1, "{table}");
        }
        assert!(!connection.prepare("PRAGMA foreign_key_check")?.exists([])?);
        // Repair only this synthetic fixture and verify the next open succeeds.
        connection.execute_batch(
            "DROP VIEW tmdb_v1_cache; ALTER TABLE retained_tmdb RENAME TO tmdb_v1_cache;",
        )?;
        drop(connection);
        let service = CatalogService::open(&path)?;
        assert_retired(&service.connection)?;
        Ok(())
    }

    #[test]
    fn version_four_failed_retirement_rolls_back_facts_alter_and_preserves_catalog() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("v4");
        let connection = populated_v5(&path)?;
        connection.execute_batch("ALTER TABLE metadata_fetches DROP COLUMN facts_json; PRAGMA user_version=4; ALTER TABLE tmdb_v1_cache RENAME TO retained_tmdb; CREATE VIEW tmdb_v1_cache AS SELECT * FROM retained_tmdb;")?;
        let before = canonical_snapshot(&connection)?;
        drop(connection);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::Storage)
        ));
        let connection = Connection::open(&path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            4
        );
        assert_eq!(canonical_snapshot(&connection)?, before);
        assert!(
            !connection
                .prepare(
                    "SELECT name FROM pragma_table_info('metadata_fetches') WHERE name='facts_json'"
                )?
                .exists([])?
        );
        let legacy_count: i64 =
            connection.query_row("SELECT count(*) FROM cinemeta_v3_members", [], |row| {
                row.get(0)
            })?;
        assert_eq!(legacy_count, 1);
        connection.execute_batch(
            "DROP VIEW tmdb_v1_cache; ALTER TABLE retained_tmdb RENAME TO tmdb_v1_cache;",
        )?;
        drop(connection);
        let service = CatalogService::open(&path)?;
        assert_retired(&service.connection)?;
        let mut after = canonical_snapshot(&service.connection)?;
        // ALTER appends exactly one nullable facts column; all v4 values survive.
        for fetch in &mut after[8] {
            assert_eq!(fetch.pop(), Some(rusqlite::types::Value::Null));
        }
        assert_eq!(after, before);
        Ok(())
    }

    #[test]
    fn normalized_only_version_five_upgrades_without_legacy_tables() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("v5");
        let service = CatalogService::open(&path)?;
        service.connection.execute_batch("DROP TABLE library_entries; DROP TABLE movie_watch_state; DROP TABLE episode_watch_state;")?;
        service.connection.pragma_update(None, "user_version", 5)?;
        drop(service);
        let service = CatalogService::open(&path)?;
        assert_retired(&service.connection)?;
        Ok(())
    }

    #[test]
    fn newer_or_unversioned_existing_database_is_never_reset() -> TestResult {
        let dir = tempfile::tempdir()?;
        for version in [0, SCHEMA_VERSION + 1] {
            let path = dir.path().join(format!("catalog{version}"));
            let connection = Connection::open(&path)?;
            connection.execute_batch(
                "CREATE TABLE marker (value TEXT); INSERT INTO marker VALUES ('keep')",
            )?;
            connection.pragma_update(None, "user_version", version)?;
            let expected = if version == 0 {
                CatalogError::InvalidCache
            } else {
                CatalogError::NewerDatabase
            };
            assert!(matches!(CatalogService::open(&path), Err(error) if error == expected));
            let value: String =
                connection.query_row("SELECT value FROM marker", [], |row| row.get(0))?;
            assert_eq!(value, "keep");
        }
        Ok(())
    }

    const DETAILS: &str = r#"{"meta":{"id":"tt0903747","type":"series","name":"Breaking Bad","description":"Full overview","poster":"https://images.metahub.space/poster/tt0903747","releaseInfo":"2008–2013","genres":["Drama"],"videos":[{"season":1,"episode":2,"name":"Second","overview":"Episode two","released":"2008-01-28T00:00:00Z"},{"season":1,"episode":1,"name":"Pilot"}]}}"#;

    #[test]
    fn migration_discards_unrepresentable_version_one_search_cache() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        connection.execute_batch("INSERT INTO cinemeta_v3_searches VALUES ('movie','ambiguous',17,1); INSERT INTO cinemeta_v3_titles VALUES ('movie','tt1234567','Legacy',NULL); INSERT INTO cinemeta_v3_members VALUES ('movie','ambiguous',0,'tt1234567'); DROP TABLE cinemeta_v3_browse; DROP TABLE cinemeta_v3_details; DROP TABLE indexer_sources_v1; DROP TABLE tmdb_v1_cache; PRAGMA user_version=1;")?;
        drop(connection);
        let mut service = CatalogService::open(&path)?;
        assert_eq!(
            service.read(&SearchQuery::new(MediaKind::Movie, "ambiguous")?)?,
            None
        );
        assert_retired(&service.connection)?;
        Ok(())
    }

    #[test]
    fn migration_failure_rolls_back_all_new_tables_and_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        connection.execute_batch("INSERT INTO cinemeta_v3_browse VALUES ('movie',0,123,'not json'); INSERT INTO cinemeta_v3_titles VALUES ('movie','tt1234567','Keep',NULL);")?;
        drop(connection);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::InvalidCache)
        ));
        let connection = Connection::open(&path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            3
        );
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='titles'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        let name: String =
            connection.query_row("SELECT name FROM cinemeta_v3_titles", [], |r| r.get(0))?;
        assert_eq!(name, "Keep");
        Ok(())
    }

    #[test]
    fn full_details_and_ordered_episodes_survive_summary_refresh_and_reload() -> TestResult {
        let summary = r#"{"metas":[{"id":"tt0903747","type":"series","name":"Short title"}]}"#;
        let mock = Mock::start(vec![ok(DETAILS), ok(summary), ok(summary)])?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let id = ImdbId::new("tt0903747")?;
        let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
        assert_eq!(service.read_details(MediaKind::Series, &id)?, None);
        let full = service.refresh_details(MediaKind::Series, &id)?;
        assert_eq!(full.details.episodes[0].number, 1);
        assert_eq!(
            full.details.episodes[1].overview.as_deref(),
            Some("Episode two")
        );
        assert_eq!(full.details.title.overview(), Some("Full overview"));
        service.refresh(&SearchQuery::new(MediaKind::Series, "breaking")?)?;
        service.refresh_browse(MediaKind::Series, 0)?;
        assert_eq!(
            service.read_details(MediaKind::Series, &id)?,
            Some(full.clone())
        );
        drop(service);
        assert_eq!(
            CatalogService::open(&path)?.read_details(MediaKind::Series, &id)?,
            Some(full)
        );
        let requests = mock.finish()?;
        assert!(requests[0].starts_with("GET /meta/series/tt0903747.json "));
        assert!(requests[2].starts_with("GET /catalog/series/top/skip=0.json "));
        Ok(())
    }

    #[test]
    fn invalid_details_and_failed_sql_refresh_preserve_saved_snapshot() -> TestResult {
        let invalid = [
            "{}".to_owned(),
            DETAILS.replace("tt0903747", "tt0903748"),
            DETAILS.replace("\"series\"", "\"movie\""),
            DETAILS.replace("\"episode\":2", "\"episode\":1"),
            DETAILS.replace("\"episode\":2", "\"episode\":0"),
            DETAILS.replace("https://images.metahub.space", "file:///private"),
        ];
        let mut responses = vec![ok(DETAILS)];
        responses.extend(invalid.iter().map(|body| ok(body)));
        responses.push(ok(DETAILS));
        let mock = Mock::start(responses)?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let id = ImdbId::new("tt0903747")?;
        service.refresh_details(MediaKind::Series, &id)?;
        service
            .connection
            .execute("UPDATE metadata_fetches SET fetched_at = 123", [])?;
        let saved = service.read_details(MediaKind::Series, &id)?;
        for _ in &invalid {
            assert!(service.refresh_details(MediaKind::Series, &id).is_err());
            assert_eq!(service.read_details(MediaKind::Series, &id)?, saved);
        }
        service.connection.execute_batch(
            "CREATE TRIGGER fail_details BEFORE UPDATE ON metadata_fetches
             BEGIN SELECT RAISE(ABORT, 'test'); END;",
        )?;
        assert_eq!(
            service.refresh_details(MediaKind::Series, &id),
            Err(CatalogError::Storage)
        );
        assert_eq!(service.read_details(MediaKind::Series, &id)?, saved);
        // Remove the fault trigger before testing a corrupt saved payload.
        service.connection.execute_batch(
            "DROP TRIGGER fail_details; PRAGMA ignore_check_constraints=ON; UPDATE titles SET name = '';",
        )?;
        assert_eq!(
            service.read_details(MediaKind::Series, &id),
            Err(CatalogError::InvalidCache)
        );
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn browse_pages_and_search_are_distinct_and_empty_is_saved() -> TestResult {
        let mock = Mock::start(vec![
            ok(MOVIE),
            ok(r#"{"metas":[]}"#),
            ok(r#"{"metas":[]}"#),
        ])?;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let mut service = CatalogService::open_at(&path, mock.endpoint.clone())?;
        let first = service.refresh_browse(MediaKind::Movie, 0)?;
        let empty = service.refresh_browse(MediaKind::Movie, 100)?;
        service.refresh(&SearchQuery::new(MediaKind::Movie, "skip=0")?)?;
        assert!(empty.titles.is_empty());
        assert_eq!(service.read_browse(MediaKind::Movie, 0)?, Some(first));
        assert_eq!(
            service.read_browse(MediaKind::Movie, 100)?,
            Some(empty.clone())
        );
        assert_eq!(service.read_browse(MediaKind::Series, 0)?, None);
        assert_eq!(service.read_browse(MediaKind::Movie, 200)?, None);
        drop(service);
        assert_eq!(
            CatalogService::open(&path)?.read_browse(MediaKind::Movie, 100)?,
            Some(empty)
        );
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn failed_browse_refresh_rolls_back_shared_summaries_and_timestamp() -> TestResult {
        let mock = Mock::start(vec![
            ok(MOVIE),
            (503, "offline".into()),
            ok(&MOVIE.replace("The Matrix", "Changed")),
        ])?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        service.refresh_browse(MediaKind::Movie, 0)?;
        service.connection.execute_batch(
            "UPDATE metadata_fetches SET fetched_at = 123;
             CREATE TRIGGER fail_browse BEFORE UPDATE ON catalog_pages BEGIN SELECT RAISE(ABORT, 'test'); END;",
        )?;
        let saved = service.read_browse(MediaKind::Movie, 0)?;
        assert_eq!(
            service.refresh_browse(MediaKind::Movie, 0),
            Err(CatalogError::HttpStatus)
        );
        assert_eq!(service.read_browse(MediaKind::Movie, 0)?, saved);
        assert_eq!(
            service.refresh_browse(MediaKind::Movie, 0),
            Err(CatalogError::Storage)
        );
        assert_eq!(service.read_browse(MediaKind::Movie, 0)?, saved);
        let name: String = service
            .connection
            .query_row("SELECT name FROM titles", [], |row| row.get(0))?;
        assert_eq!(name, "The Matrix");
        service.connection.execute_batch(
            "DROP TRIGGER fail_browse; UPDATE catalog_pages SET result_count=100;",
        )?;
        assert_eq!(
            service.read_browse(MediaKind::Movie, 0),
            Err(CatalogError::InvalidCache)
        );
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn source_cache_sanitizes_labels_and_separates_indexers_targets_and_scope() -> TestResult {
        use kinema_core::sources::{InfoHash, TorrentSource};
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let mut service = CatalogService::open(&path)?;
        seed_movie(&mut service)?;
        let target = PlaybackTarget::Movie(ImdbId::new("tt0133093")?);
        let results = crate::sources::SourceResults {
            omitted: 2,
            candidates: vec![SourceCandidate {
                torrent: TorrentSource {
                    hash: InfoHash::new("0123456789012345678901234567890123456789")?,
                    file_index: Some(1),
                    filename: Some("film.mkv https://private.test/?token=secret".into()),
                },
                name: "Film 1080p magnet:?xt=private https://example.test/token?secret=x".into(),
                quality: Some("1080p".into()),
                size: Some(100),
                seeders: Some(4),
            }],
        };
        let saved = service.save_sources(IndexerKind::Torrentio, &target, results)?;
        assert_eq!(saved.omitted, 2);
        let payload: String = service.connection.query_row(
            "SELECT sources_json FROM indexer_snapshots",
            [],
            |row| row.get(0),
        )?;
        for forbidden in ["private.test", "secret", "magnet:", "https://"] {
            assert!(!payload.contains(forbidden));
        }
        assert_eq!(
            service.read_sources(IndexerKind::Torrentio, &target)?,
            Some(saved.clone())
        );
        assert_eq!(service.read_sources(IndexerKind::Peerflix, &target)?, None);
        let other = PlaybackTarget::Episode {
            series: ImdbId::new("tt0133093")?,
            season: 1,
            episode: 1,
        };
        assert_eq!(service.read_sources(IndexerKind::Torrentio, &other)?, None);
        drop(service);
        let mut service = CatalogService::open(&path)?;
        assert_eq!(
            service.read_sources(IndexerKind::Torrentio, &target)?,
            Some(saved)
        );
        service.connection.execute(
            "UPDATE indexer_snapshots SET cache_scope = 'different-config'",
            [],
        )?;
        assert_eq!(service.read_sources(IndexerKind::Torrentio, &target)?, None);
        Ok(())
    }

    #[test]
    fn source_cache_empty_failure_and_corruption_remain_distinct() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        seed_movie(&mut service)?;
        let target = PlaybackTarget::Movie(ImdbId::new("tt0133093")?);
        assert_eq!(service.read_sources(IndexerKind::Torrentio, &target)?, None);
        let empty = || crate::sources::SourceResults {
            candidates: Vec::new(),
            omitted: 0,
        };
        let saved = service.save_sources(IndexerKind::Torrentio, &target, empty())?;
        assert!(saved.candidates.is_empty());
        service.connection.execute_batch("CREATE TRIGGER fail_sources BEFORE UPDATE ON indexer_snapshots BEGIN SELECT RAISE(ABORT, 'test'); END;")?;
        assert_eq!(
            service.save_sources(IndexerKind::Torrentio, &target, empty()),
            Err(CatalogError::Storage)
        );
        assert_eq!(
            service.read_sources(IndexerKind::Torrentio, &target)?,
            Some(saved)
        );
        service.connection.execute_batch("DROP TRIGGER fail_sources; UPDATE indexer_snapshots SET sources_json = '{\"candidates\":[],\"omitted\":0,\"url\":\"https://private\"}';")?;
        assert_eq!(
            service.read_sources(IndexerKind::Torrentio, &target),
            Err(CatalogError::InvalidCache)
        );
        Ok(())
    }

    fn legacy_database(path: &Path) -> Result<Connection, Box<dyn Error>> {
        let mut connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", true)?;
        connection.execute_batch(
            "CREATE TABLE cinemeta_v3_searches (
                    kind TEXT NOT NULL CHECK(kind IN ('movie', 'series')),
                    query TEXT NOT NULL,
                    fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
                    result_count INTEGER NOT NULL CHECK(result_count BETWEEN 0 AND 500),
                    PRIMARY KEY(kind, query)
                );
                CREATE TABLE cinemeta_v3_titles (
                    kind TEXT NOT NULL CHECK(kind IN ('movie', 'series')),
                    imdb_id TEXT NOT NULL,
                    name TEXT NOT NULL,
                    overview TEXT,
                    PRIMARY KEY(kind, imdb_id)
                );
                CREATE TABLE cinemeta_v3_members (
                    kind TEXT NOT NULL,
                    query TEXT NOT NULL,
                    position INTEGER NOT NULL CHECK(position BETWEEN 0 AND 499),
                    imdb_id TEXT NOT NULL,
                    PRIMARY KEY(kind, query, position),
                    UNIQUE(kind, query, imdb_id),
                    FOREIGN KEY(kind, query) REFERENCES cinemeta_v3_searches(kind, query)
                        ON DELETE CASCADE,
                    FOREIGN KEY(kind, imdb_id) REFERENCES cinemeta_v3_titles(kind, imdb_id)
                );
                PRAGMA user_version = 1;",
        )?;
        migrate_metadata(&mut connection)?;
        metadata::migrate_tmdb(&mut connection)?;
        Ok(connection)
    }

    #[test]
    fn valid_v3_backfill_retires_legacy_without_inventing_optional_rich_data() -> TestResult {
        use crate::tmdb::{TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbTitle};
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        let page_query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        let detail_query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 11,
        };
        let title = TmdbTitle {
            id: 11,
            kind: MediaKind::Movie,
            name: "Reduced TMDB".into(),
            overview: None,
            release_info: Some("2025".into()),
            poster_url: None,
            display: Default::default(),
            imdb: None,
        };
        let detail = TmdbData::Details(TmdbDetails {
            facts: Default::default(),
            backdrop_url: None,
            title: title.clone(),
            imdb: Some(ImdbId::new("tt4000001")?),
            genres: vec![],
            seasons: vec![],
        });
        let page = TmdbData::Page(TmdbPage {
            titles: vec![title],
            page: 1,
            total_pages: 1,
        });
        // Insert page before detail: migration must establish explicit mappings first.
        for (query, data, time) in [(&page_query, &page, 12), (&detail_query, &detail, 11)] {
            connection.execute(
                "INSERT INTO tmdb_v1_cache VALUES (?1,'en-US',?2,?3)",
                params![
                    crate::tmdb::cache_query_identity(query)?,
                    time,
                    crate::tmdb::encode(data)?
                ],
            )?;
        }
        let cinemeta = r#"{"id":"tt4000001","type":"movie","name":"Reduced Cinemeta","description":null,"poster":null,"background":null,"releaseInfo":"2025","videos":[]}"#;
        connection.execute(
            "INSERT INTO cinemeta_v3_details VALUES ('movie','tt4000001',10,?1)",
            params![cinemeta],
        )?;
        connection.execute("INSERT INTO indexer_sources_v1 VALUES ('torrentio','movie:tt4000001','public-default-v1',13,?1)",params![r#"{"candidates":[],"omitted":0}"#])?;
        drop(connection);
        let mut service = CatalogService::open(&path)?;
        let count: i64 = service
            .connection
            .query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
        assert_eq!(count, 1);
        assert!(service.read_tmdb(&page_query)?.is_some());
        assert_eq!(service.read_tmdb(&detail_query)?, None);
        assert_eq!(
            service.read_details(MediaKind::Movie, &ImdbId::new("tt4000001")?)?,
            None
        );
        assert_retired(&service.connection)?;
        for table in ["season_identifiers", "episode_identifiers", "ratings"] {
            let count: i64 = service.connection.query_row(
                &format!("SELECT count(*) FROM {table}"),
                [],
                |r| r.get(0),
            )?;
            assert_eq!(count, 0);
        }
        let genres: Option<String> =
            service
                .connection
                .query_row("SELECT genres_json FROM titles", [], |r| r.get(0))?;
        assert_eq!(genres, None);
        assert!(
            service
                .read_sources(
                    IndexerKind::Torrentio,
                    &PlaybackTarget::Movie(ImdbId::new("tt4000001")?)
                )?
                .is_some()
        );
        Ok(())
    }

    #[test]
    fn malformed_legacy_source_rolls_back_v4_and_preserves_old_snapshot() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        connection.execute(
            "INSERT INTO cinemeta_v3_browse VALUES ('movie',0,1,?1)",
            params![MOVIE],
        )?;
        connection.execute("INSERT INTO indexer_sources_v1 VALUES ('torrentio','movie:tt0133093','public-default-v1',2,?1)",params![r#"{"candidates":[],"omitted":0,"url":"https://unsafe.invalid"}"#])?;
        drop(connection);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::InvalidCache)
        ));
        let connection = Connection::open(&path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            3
        );
        let count: i64 = connection.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='titles'",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(count, 0);
        let payload: String =
            connection.query_row("SELECT payload FROM cinemeta_v3_browse", [], |r| r.get(0))?;
        assert_eq!(payload, MOVIE);
        Ok(())
    }

    fn seed_movie(service: &mut CatalogService) -> TestResult {
        let title = CatalogTitle::new(
            ImdbId::new("tt0133093")?,
            MediaKind::Movie,
            "Synthetic movie",
            None,
        )?;
        let tx = service.connection.transaction()?;
        normalized_pages::write_page(
            &tx,
            "cinemeta",
            "synthetic-seed-v1",
            1,
            None,
            &[normalized::imdb_observation(&title)],
            1,
            &Default::default(),
        )?;
        tx.commit()?;
        Ok(())
    }

    fn tmdb_page(kind: MediaKind, page: u32) -> crate::tmdb::TmdbData {
        crate::tmdb::TmdbData::Page(crate::tmdb::TmdbPage {
            titles: vec![crate::tmdb::TmdbTitle {
                id: 603,
                kind,
                name: "Fixture title".into(),
                overview: Some("Overview".into()),
                release_info: Some("1999".into()),
                poster_url: None,
                display: Default::default(),
                imdb: None,
            }],
            page,
            total_pages: 10,
        })
    }

    #[test]
    fn tmdb_cache_keys_bind_provider_locale_kind_text_and_page() -> TestResult {
        use crate::tmdb::TmdbQuery;
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let mut service = CatalogService::open(&path)?;
        let queries = [
            (
                TmdbQuery::Browse {
                    kind: MediaKind::Movie,
                    page: 1,
                },
                MediaKind::Movie,
                1,
            ),
            (
                TmdbQuery::Browse {
                    kind: MediaKind::Series,
                    page: 1,
                },
                MediaKind::Series,
                1,
            ),
            (
                TmdbQuery::Search {
                    query: SearchQuery::new(MediaKind::Movie, " Matrix ")?,
                    page: 1,
                },
                MediaKind::Movie,
                1,
            ),
            (
                TmdbQuery::Search {
                    query: SearchQuery::new(MediaKind::Movie, "matrix")?,
                    page: 1,
                },
                MediaKind::Movie,
                1,
            ),
            (
                TmdbQuery::Search {
                    query: SearchQuery::new(MediaKind::Movie, "Matrix")?,
                    page: 2,
                },
                MediaKind::Movie,
                2,
            ),
            (
                TmdbQuery::Search {
                    query: SearchQuery::new(MediaKind::Series, "Matrix")?,
                    page: 1,
                },
                MediaKind::Series,
                1,
            ),
        ];
        let mut saved = Vec::new();
        for (query, kind, page) in &queries {
            assert_eq!(service.read_tmdb(query)?, None);
            saved.push(service.save_tmdb(query, tmdb_page(*kind, *page))?);
        }
        let rows: i64 = service.connection.query_row(
            "SELECT count(*) FROM catalog_pages WHERE provider = 'tmdb'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(rows, queries.len() as i64);
        for ((query, _, _), snapshot) in queries.iter().zip(&saved) {
            assert_eq!(service.read_tmdb(query)?, Some(snapshot.clone()));
            let key = crate::tmdb::cache_query_identity(query)?;
            let matches: i64 = service.connection.query_row(
                "SELECT count(*) FROM catalog_pages WHERE query_key = ?1",
                [key],
                |row| row.get(0),
            )?;
            assert_eq!(matches, 1);
        }
        drop(service);
        let mut reopened = CatalogService::open(&path)?;
        for ((query, _, _), snapshot) in queries.iter().zip(saved) {
            assert_eq!(reopened.read_tmdb(query)?, Some(snapshot));
        }
        Ok(())
    }

    #[test]
    fn tmdb_details_and_seasons_are_bound_to_exact_numeric_identity() -> TestResult {
        use crate::tmdb::{TmdbData, TmdbDetails, TmdbQuery, TmdbSeason, TmdbTitle};
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let details_query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 603,
        };
        let details = TmdbData::Details(TmdbDetails {
            facts: Default::default(),
            backdrop_url: None,
            title: TmdbTitle {
                id: 603,
                kind: MediaKind::Movie,
                name: "Fixture".into(),
                overview: None,
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            },
            imdb: Some(ImdbId::new("tt0133093")?),
            genres: vec!["Action".into()],
            seasons: Vec::new(),
        });
        let saved_details = service.save_tmdb(&details_query, details.clone())?;
        assert_eq!(service.read_tmdb(&details_query)?, Some(saved_details));
        assert!(
            service
                .save_tmdb(
                    &TmdbQuery::Details {
                        kind: MediaKind::Movie,
                        id: 604
                    },
                    details.clone()
                )
                .is_err()
        );
        assert!(
            service
                .save_tmdb(
                    &TmdbQuery::Details {
                        kind: MediaKind::Series,
                        id: 603
                    },
                    details
                )
                .is_err()
        );
        let season_query = TmdbQuery::Season {
            series_id: 1399,
            season: 1,
        };
        let season = TmdbData::Season(TmdbSeason {
            poster_url: None,
            series_id: 1399,
            season: 1,
            episodes: vec![CatalogEpisode {
                still_url: None,
                runtime_minutes: None,
                season: 1,
                number: 1,
                name: Some("Pilot".into()),
                overview: None,
                released: None,
            }],
        });
        let saved_season = service.save_tmdb(&season_query, season.clone())?;
        assert_eq!(service.read_tmdb(&season_query)?, Some(saved_season));
        assert!(
            service
                .save_tmdb(
                    &TmdbQuery::Season {
                        series_id: 1400,
                        season: 1
                    },
                    season.clone()
                )
                .is_err()
        );
        assert!(
            service
                .save_tmdb(
                    &TmdbQuery::Season {
                        series_id: 1399,
                        season: 2
                    },
                    season
                )
                .is_err()
        );
        Ok(())
    }

    #[test]
    fn tmdb_saved_empty_is_readable_without_credentials_and_credential_errors_are_distinct()
    -> TestResult {
        use crate::{
            credentials::CredentialError,
            tmdb::{TmdbData, TmdbPage, TmdbQuery},
        };
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        let saved = service.save_tmdb(
            &query,
            TmdbData::Page(TmdbPage {
                titles: Vec::new(),
                page: 1,
                total_pages: 0,
            }),
        )?;
        assert_eq!(service.read_tmdb(&query)?, Some(saved.clone()));
        for (credential_error, expected) in [
            (
                CredentialError::Missing,
                CatalogError::TmdbMissingCredential,
            ),
            (
                CredentialError::Unavailable,
                CatalogError::TmdbCredentialUnavailable,
            ),
            (
                CredentialError::AccessDenied,
                CatalogError::TmdbCredentialDenied,
            ),
        ] {
            assert_eq!(
                service.refresh_tmdb_with_credential(&query, || Err(credential_error)),
                Err(expected)
            );
            assert_eq!(service.read_tmdb(&query)?, Some(saved.clone()));
        }
        Ok(())
    }

    #[test]
    fn tmdb_mismatch_corruption_and_sql_failure_preserve_saved_snapshot() -> TestResult {
        use crate::tmdb::TmdbQuery;
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        service.save_tmdb(&query, tmdb_page(MediaKind::Movie, 1))?;
        service.connection.execute_batch(
            "UPDATE metadata_fetches SET fetched_at = 123;
             CREATE TRIGGER fail_tmdb BEFORE UPDATE ON metadata_fetches BEGIN SELECT RAISE(ABORT, 'test'); END;",
        )?;
        let saved = service.read_tmdb(&query)?;
        assert!(
            service
                .save_tmdb(&query, tmdb_page(MediaKind::Movie, 2))
                .is_err()
        );
        assert_eq!(service.read_tmdb(&query)?, saved);
        assert_eq!(
            service.save_tmdb(&query, tmdb_page(MediaKind::Movie, 1)),
            Err(CatalogError::Storage)
        );
        assert_eq!(service.read_tmdb(&query)?, saved);
        service.connection.execute_batch(
            "DROP TRIGGER fail_tmdb; UPDATE catalog_pages SET result_count = 100;",
        )?;
        assert_eq!(service.read_tmdb(&query), Err(CatalogError::InvalidCache));
        Ok(())
    }

    #[test]
    fn schema_three_upgrade_preserves_all_version_two_cache_resources() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        connection.execute(
            "INSERT INTO cinemeta_v3_browse VALUES ('movie',0,12,?1)",
            params![MOVIE],
        )?;
        let response: serde_json::Value = serde_json::from_str(DETAILS)?;
        connection.execute(
            "INSERT INTO cinemeta_v3_details VALUES ('series','tt0903747',13,?1)",
            params![response["meta"].to_string()],
        )?;
        connection.execute_batch("DROP TABLE tmdb_v1_cache; PRAGMA user_version=2;")?;
        drop(connection);
        let mut service = CatalogService::open(&path)?;
        assert_eq!(
            service
                .read_browse(MediaKind::Movie, 0)?
                .ok_or("missing browse")?
                .titles[0]
                .name(),
            "The Matrix"
        );
        // Reduced legacy optional fields do not prove today's detail DTO checked them.
        assert_eq!(
            service.read_details(MediaKind::Series, &ImdbId::new("tt0903747")?)?,
            None
        );
        assert_retired(&service.connection)?;
        Ok(())
    }

    #[test]
    fn failed_schema_three_upgrade_does_not_advance_version_or_touch_existing_cache() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let connection = legacy_database(&path)?;
        // A collision at migration time must be an error, never a reset.
        connection.execute_batch("PRAGMA user_version = 2; INSERT INTO cinemeta_v3_searches(kind, query, fetched_at, result_count) VALUES ('movie', 'keep', 123, 0);")?;
        drop(connection);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::Storage)
        ));
        let connection = Connection::open(&path)?;
        assert_eq!(
            connection.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            2
        );
        let timestamp: i64 = connection.query_row(
            "SELECT fetched_at FROM cinemeta_v3_searches WHERE query = 'keep'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(timestamp, 123);
        Ok(())
    }

    #[test]
    fn canceled_http_reply_is_not_committed_to_browse_cache() -> TestResult {
        let flag = Arc::new(AtomicBool::new(false));
        let mock = Mock::start_canceling(
            vec![
                ok(MOVIE),
                ok(&MOVIE.replace("The Matrix", "Canceled title")),
            ],
            Some((1, Arc::clone(&flag))),
        )?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let saved = service.refresh_browse(MediaKind::Movie, 0)?;
        service.cancellation = Some(flag);
        assert_eq!(
            service.refresh_browse(MediaKind::Movie, 0),
            Err(CatalogError::Canceled)
        );
        assert_eq!(service.read_browse(MediaKind::Movie, 0)?, Some(saved));
        let name: String = service
            .connection
            .query_row("SELECT name FROM titles", [], |row| row.get(0))?;
        assert_eq!(name, "The Matrix");
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn canceled_original_search_reply_never_commits() -> TestResult {
        let flag = Arc::new(AtomicBool::new(false));
        let mock = Mock::start_canceling(
            vec![ok(MOVIE), ok(&MOVIE.replace("The Matrix", "Stale"))],
            Some((1, Arc::clone(&flag))),
        )?;
        let dir = tempfile::tempdir()?;
        let mut service =
            CatalogService::open_at(&dir.path().join("catalog"), mock.endpoint.clone())?;
        let query = SearchQuery::new(MediaKind::Movie, "synthetic")?;
        let saved = service.refresh(&query)?;
        service.cancellation = Some(flag);
        assert_eq!(service.refresh(&query), Err(CatalogError::Canceled));
        assert_eq!(service.read(&query)?, Some(saved));
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn canceled_tmdb_and_source_work_neither_reads_credentials_nor_updates_cache() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let query = crate::tmdb::TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        let saved = service.save_tmdb(&query, tmdb_page(MediaKind::Movie, 1))?;
        service.cancellation = Some(Arc::new(AtomicBool::new(true)));
        let credential_called = std::cell::Cell::new(false);
        assert_eq!(
            service.refresh_tmdb_with_credential(&query, || {
                credential_called.set(true);
                Err(crate::credentials::CredentialError::Missing)
            }),
            Err(CatalogError::Canceled)
        );
        assert!(!credential_called.get());
        assert_eq!(
            service.save_tmdb(&query, tmdb_page(MediaKind::Movie, 1)),
            Err(CatalogError::Canceled)
        );
        assert_eq!(service.read_tmdb(&query)?, Some(saved));
        let target = PlaybackTarget::Movie(ImdbId::new("tt0133093")?);
        assert_eq!(
            service.save_sources(
                IndexerKind::Torrentio,
                &target,
                crate::sources::SourceResults {
                    candidates: Vec::new(),
                    omitted: 0
                }
            ),
            Err(CatalogError::Canceled)
        );
        assert_eq!(service.read_sources(IndexerKind::Torrentio, &target)?, None);
        Ok(())
    }

    /// Opt-in integration evidence only, not an offline provider-support claim.
    /// Uses curated public titles and a temporary database, never AppPaths/data.
    #[test]
    #[ignore = "requires live public Cinemeta HTTPS access"]
    fn live_cinemeta_public_metadata_smoke() -> TestResult {
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("live-catalog.sqlite3"))?;
        let movie = ImdbId::new("tt0133093")?;
        let movie_without_videos = ImdbId::new("tt22084616")?;
        let series = ImdbId::new("tt0944947")?;
        let query = SearchQuery::new(MediaKind::Movie, "The Matrix")?;
        let operations = [
            (
                "browse",
                service
                    .refresh_browse(MediaKind::Movie, 0)
                    .map(|snapshot| snapshot.titles.len()),
            ),
            (
                "search",
                service
                    .refresh(&query)
                    .map(|snapshot| snapshot.titles.len()),
            ),
            (
                "movie details",
                service
                    .refresh_details(MediaKind::Movie, &movie)
                    .map(|snapshot| usize::from(snapshot.details.title.id() == &movie)),
            ),
            (
                "movie details without videos",
                service
                    .refresh_details(MediaKind::Movie, &movie_without_videos)
                    .map(|snapshot| {
                        usize::from(
                            snapshot.details.title.id() == &movie_without_videos
                                && snapshot.details.episodes.is_empty(),
                        )
                    }),
            ),
            (
                "series details",
                service
                    .refresh_details(MediaKind::Series, &series)
                    .map(|snapshot| snapshot.details.episodes.len()),
            ),
        ];
        let mut failures = Vec::new();
        for (operation, result) in operations {
            match result {
                Ok(count) if count > 0 => eprintln!("live {operation}: success ({count} entries)"),
                Ok(_) => failures.push(format!("live {operation}: unexpectedly empty")),
                Err(error) => failures.push(format!("live {operation}: {error}")),
            }
        }
        if !failures.is_empty() {
            return Err(failures.join("; ").into());
        }
        assert!(service.read_browse(MediaKind::Movie, 0)?.is_some());
        assert!(service.read(&query)?.is_some());
        assert!(service.read_details(MediaKind::Movie, &movie)?.is_some());
        let regression = service
            .read_details(MediaKind::Movie, &movie_without_videos)?
            .ok_or("missing saved regression movie")?;
        assert_eq!(regression.details.title.id(), &movie_without_videos);
        assert!(regression.details.episodes.is_empty());
        assert!(service.read_details(MediaKind::Series, &series)?.is_some());
        Ok(())
    }

    #[test]
    fn network_failure_is_not_a_saved_empty_result() -> TestResult {
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let endpoint = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
        drop(listener);
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open_at(&dir.path().join("catalog"), endpoint)?;
        let query = SearchQuery::new(MediaKind::Movie, "matrix")?;
        assert_eq!(service.refresh(&query), Err(CatalogError::Network));
        assert_eq!(service.read(&query)?, None);
        Ok(())
    }
}

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
const SCHEMA_VERSION: i64 = 4;
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
            Self::HttpStatus => "Cinemeta returned an unsuccessful response.",
            Self::InvalidResponse => "Cinemeta returned an invalid catalog response.",
            Self::InvalidMetadata => "The provider returned invalid catalog metadata.",
            Self::MissingIdentityMapping => {
                "Source lookup requires a verified catalog target and IMDb mapping."
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
    fn from(_: rusqlite::Error) -> Self {
        Self::Storage
    }
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
    client: Client,
    endpoint: Url,
    indexers: Option<crate::sources::IndexerClient>,
    pub(super) cancellation: Option<Arc<AtomicBool>>,
}

impl CatalogService {
    pub(super) fn ensure_active(&self) -> Result<(), CatalogError> {
        check_cancellation(self.cancellation.as_deref())
    }

    /// Resolve paths and perform all filesystem I/O on the owning worker.
    /// AppPaths owns development/native routing; no migration or fallback.
    pub(crate) fn open_default() -> Result<Self, CatalogError> {
        let paths = crate::paths::AppPaths::resolve().map_err(|_| CatalogError::DataDirectory)?;
        std::fs::create_dir_all(&paths.data).map_err(|_| CatalogError::DataDirectory)?;
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
            .map_err(|_| CatalogError::Network)?;
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
        if version == 0 {
            let objects: i64 = connection.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name NOT LIKE 'sqlite_%'",
                [],
                |row| row.get(0),
            )?;
            if objects != 0 {
                return Err(CatalogError::InvalidCache);
            }
            let tx = connection.transaction()?;
            // Schema v1: the sole provider/key namespace is Cinemeta v3. Query
            // keys are (kind, trimmed text), case-sensitive, with no TTL.
            tx.execute_batch(
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
            tx.commit()?;
        }
        if version < 2 {
            migrate_metadata(&mut connection)?;
        }
        if version < 3 {
            metadata::migrate_tmdb(&mut connection)?;
        }
        if version < 4 {
            let tx = connection.transaction()?;
            schema::migrate(&tx)?;
            normalized_backfill::backfill(&tx)?;
            tx.pragma_update(None, "user_version", 4)?;
            tx.commit()?;
        }
        Ok(Self {
            connection,
            client,
            endpoint,
            indexers: None,
            cancellation: None,
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
        let rich = normalized_rich::cinemeta_summaries(&raw)?;
        let payload: SearchResponse =
            serde_json::from_value(raw).map_err(|_| CatalogError::InvalidResponse)?;
        if payload.metas.len() > MAX_RESULTS {
            return Err(CatalogError::InvalidResponse);
        }
        let mut titles = Vec::with_capacity(payload.metas.len());
        let mut seen = HashSet::new();
        for meta in payload.metas {
            if meta.kind != kind_text(query.kind()) {
                return Err(CatalogError::InvalidResponse);
            }
            let title = validate_title(meta.id, query.kind(), meta.name, meta.description)?;
            if !seen.insert(title.id().clone()) {
                return Err(CatalogError::InvalidResponse);
            }
            titles.push(title);
        }
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
        )?;
        normalized_rich::write_page_rich(
            &tx,
            "cinemeta",
            &normalized_pages::cinemeta_search_key(query),
            1,
            query.kind(),
            &rich,
            timestamp,
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

#[cfg(test)]
mod tests {
    use std::{
        io::{self, Read, Write},
        net::TcpListener,
        thread::{self, JoinHandle},
    };

    use super::*;

    type TestResult = Result<(), Box<dyn Error>>;
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
    fn migration_preserves_version_one_search_cache() -> TestResult {
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
        let name: String =
            service
                .connection
                .query_row("SELECT name FROM cinemeta_v3_titles", [], |row| row.get(0))?;
        assert_eq!(name, "Legacy");
        assert_eq!(
            service
                .connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            4
        );
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
        let service = CatalogService::open(path)?;
        service.connection.execute_batch("PRAGMA foreign_keys=OFF; DROP TABLE indexer_snapshots; DROP TABLE catalog_page_items; DROP TABLE field_provenance; DROP TABLE ratings; DROP TABLE episode_identifiers; DROP TABLE season_identifiers; DROP TABLE title_identifiers; DROP TABLE metadata_fetches; DROP TABLE catalog_pages; DROP TABLE episodes; DROP TABLE seasons; DROP TABLE titles; PRAGMA user_version=3; PRAGMA foreign_keys=ON;")?;
        Ok(service.connection)
    }

    #[test]
    fn valid_v3_backfill_keeps_legacy_payloads_and_does_not_invent_optional_rich_data() -> TestResult
    {
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
        };
        let detail = TmdbData::Details(TmdbDetails {
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
        let before: Vec<String> = {
            let mut stmt =
                connection.prepare("SELECT payload FROM tmdb_v1_cache ORDER BY resource")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
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
        let details: String =
            service
                .connection
                .query_row("SELECT payload FROM cinemeta_v3_details", [], |r| r.get(0))?;
        assert_eq!(details, cinemeta);
        let after: Vec<String> = {
            let mut stmt = service
                .connection
                .prepare("SELECT payload FROM tmdb_v1_cache ORDER BY resource")?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<Result<_, _>>()?
        };
        assert_eq!(before, after);
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
            title: TmdbTitle {
                id: 603,
                kind: MediaKind::Movie,
                name: "Fixture".into(),
                overview: None,
                release_info: None,
                poster_url: None,
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
            series_id: 1399,
            season: 1,
            episodes: vec![CatalogEpisode {
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
        let payload: String =
            service
                .connection
                .query_row("SELECT payload FROM cinemeta_v3_browse", [], |r| r.get(0))?;
        assert_eq!(payload, MOVIE);
        assert_eq!(
            service
                .connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            4
        );
        Ok(())
    }

    #[test]
    fn failed_schema_three_upgrade_does_not_advance_version_or_touch_existing_cache() -> TestResult
    {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        let service = CatalogService::open(&path)?;
        // A collision at migration time must be an error, never a reset.
        service.connection.execute_batch("PRAGMA user_version = 2; INSERT INTO cinemeta_v3_searches(kind, query, fetched_at, result_count) VALUES ('movie', 'keep', 123, 0);")?;
        drop(service);
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

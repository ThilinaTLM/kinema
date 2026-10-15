//! Blocking Cinemeta/SQLite adapter. Construct, use, and drop on one background
//! worker, never during rendering. The caller owns cancellation/queueing and
//! rejects stale generations; an in-flight request is bounded by HTTP timeouts.

use std::{
    collections::HashSet,
    error::Error,
    fmt,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use directories::ProjectDirs;
use kinema_core::{
    MediaKind,
    catalog::{CatalogTitle, ImdbId, SearchQuery},
};
use reqwest::{Url, blocking::Client, redirect::Policy};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;

const ENDPOINT: &str = "https://v3-cinemeta.strem.io/";
const SCHEMA_VERSION: i64 = 1;
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
    ResponseTooLarge,
    Clock,
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
            Self::ResponseTooLarge => "Cinemeta returned a catalog response that is too large.",
            Self::Clock => "The system clock cannot timestamp catalog results.",
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

pub(crate) struct CatalogService {
    connection: Connection,
    client: Client,
    endpoint: Url,
}

impl CatalogService {
    /// `debug_assertions` builds use ./data relative to the process working
    /// directory (run `cargo run` from the repository root for repo-local data).
    /// Release builds use ProjectDirs("", "", "kinema").data_local_dir().
    /// Neither mode falls back to the other or migrates existing data.
    /// Directory resolution and all filesystem I/O run on the owning worker.
    pub(crate) fn open_default() -> Result<Self, CatalogError> {
        let directory = catalog_directory(
            cfg!(debug_assertions),
            || std::env::current_dir().map_err(|_| CatalogError::DataDirectory),
            || {
                ProjectDirs::from("", "", "kinema")
                    .map(|dirs| dirs.data_local_dir().to_path_buf())
                    .ok_or(CatalogError::DataDirectory)
            },
        )?;
        std::fs::create_dir_all(&directory).map_err(|_| CatalogError::DataDirectory)?;
        Self::open(&directory.join("catalog.sqlite3"))
    }

    /// The parent directory must exist. Opening errors never reset/delete a DB.
    pub(crate) fn open(path: &Path) -> Result<Self, CatalogError> {
        let endpoint = Url::parse(ENDPOINT).map_err(|_| CatalogError::Network)?;
        Self::open_at(path, endpoint)
    }

    // Private endpoint injection is only used by local HTTP tests.
    fn open_at(path: &Path, endpoint: Url) -> Result<Self, CatalogError> {
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            .redirect(Policy::none())
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
        if version != 0 && version != SCHEMA_VERSION {
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
        Ok(Self {
            connection,
            client,
            endpoint,
        })
    }

    /// A saved empty result is Some, a cache miss is None, failures are Err.
    pub(crate) fn read(
        &mut self,
        query: &SearchQuery,
    ) -> Result<Option<CachedSearch>, CatalogError> {
        // One read transaction prevents mixing header/members across commits by
        // another process. Missing/corrupt members must not appear as emptiness.
        let tx = self.connection.transaction()?;
        let header: Option<(i64, i64)> = tx
            .query_row(
                "SELECT fetched_at, result_count FROM cinemeta_v3_searches
                 WHERE kind = ?1 AND query = ?2",
                params![kind_text(query.kind()), query.text()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let Some((timestamp, count)) = header else {
            return Ok(None);
        };
        let fetched_at = u64::try_from(timestamp).map_err(|_| CatalogError::InvalidCache)?;
        let count = usize::try_from(count).map_err(|_| CatalogError::InvalidCache)?;
        if count > MAX_RESULTS {
            return Err(CatalogError::InvalidCache);
        }
        let mut titles = Vec::with_capacity(count);
        let mut seen = HashSet::new();
        {
            let mut statement = tx.prepare(
                "SELECT m.position, t.imdb_id, t.name, t.overview
                 FROM cinemeta_v3_members m LEFT JOIN cinemeta_v3_titles t
                   ON m.kind = t.kind AND m.imdb_id = t.imdb_id
                 WHERE m.kind = ?1 AND m.query = ?2 ORDER BY m.position LIMIT 501",
            )?;
            let mut rows = statement.query(params![kind_text(query.kind()), query.text()])?;
            while let Some(row) = rows.next()? {
                let position: i64 = row.get(0)?;
                if position != titles.len() as i64 || titles.len() >= count {
                    return Err(CatalogError::InvalidCache);
                }
                let id: String = row.get(1).map_err(|_| CatalogError::InvalidCache)?;
                let name: String = row.get(2).map_err(|_| CatalogError::InvalidCache)?;
                let overview: Option<String> = row.get(3)?;
                let title = validate_title(id, query.kind(), name, overview)
                    .map_err(|_| CatalogError::InvalidCache)?;
                if !seen.insert(title.id().clone()) {
                    return Err(CatalogError::InvalidCache);
                }
                titles.push(title);
            }
        }
        if titles.len() != count {
            return Err(CatalogError::InvalidCache);
        }
        tx.commit()?;
        Ok(Some(CachedSearch { titles, fetched_at }))
    }

    /// Always fetch remote data, including when a saved snapshot exists. Every
    /// failure leaves the saved snapshot and its timestamp unchanged.
    pub(crate) fn refresh(&mut self, query: &SearchQuery) -> Result<CachedSearch, CatalogError> {
        let url = search_url(&self.endpoint, query)?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .map_err(|_| CatalogError::Network)?;
        if !response.status().is_success() {
            return Err(CatalogError::HttpStatus);
        }
        if response
            .content_length()
            .is_some_and(|n| n > MAX_RESPONSE_BYTES)
        {
            return Err(CatalogError::ResponseTooLarge);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| CatalogError::Network)?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(CatalogError::ResponseTooLarge);
        }
        let payload: SearchResponse =
            serde_json::from_slice(&bytes).map_err(|_| CatalogError::InvalidResponse)?;
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
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO cinemeta_v3_searches(kind, query, fetched_at, result_count)
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(kind, query) DO UPDATE SET
             fetched_at = excluded.fetched_at, result_count = excluded.result_count",
            params![
                kind_text(query.kind()),
                query.text(),
                timestamp,
                titles.len() as i64
            ],
        )?;
        tx.execute(
            "DELETE FROM cinemeta_v3_members WHERE kind = ?1 AND query = ?2",
            params![kind_text(query.kind()), query.text()],
        )?;
        for (position, title) in titles.iter().enumerate() {
            tx.execute(
                "INSERT INTO cinemeta_v3_titles(kind, imdb_id, name, overview)
                 VALUES (?1, ?2, ?3, ?4) ON CONFLICT(kind, imdb_id) DO UPDATE SET
                 name = excluded.name, overview = excluded.overview",
                params![
                    kind_text(title.kind()),
                    title.id().as_str(),
                    title.name(),
                    title.overview()
                ],
            )?;
            tx.execute(
                "INSERT INTO cinemeta_v3_members(kind, query, position, imdb_id)
                 VALUES (?1, ?2, ?3, ?4)",
                params![
                    kind_text(query.kind()),
                    query.text(),
                    position as i64,
                    title.id().as_str()
                ],
            )?;
        }
        tx.commit()?;
        Ok(CachedSearch { titles, fetched_at })
    }
}

// Lazy resolvers keep development independent of native/home-directory lookup
// and release independent of the process working directory. Errors propagate;
// there is intentionally no fallback location.
fn catalog_directory(
    development: bool,
    current_directory: impl FnOnce() -> Result<PathBuf, CatalogError>,
    native_directory: impl FnOnce() -> Result<PathBuf, CatalogError>,
) -> Result<PathBuf, CatalogError> {
    if development {
        current_directory().map(|directory| directory.join("data"))
    } else {
        native_directory()
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
#[derive(Deserialize)]
struct SearchResponse {
    metas: Vec<SearchMeta>,
}

#[derive(Deserialize)]
struct SearchMeta {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
    description: Option<String>,
}

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
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let endpoint = Url::parse(&format!("http://{}/", listener.local_addr()?))?;
            let thread = thread::spawn(move || {
                let mut requests = Vec::new();
                for (status, body) in responses {
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
    fn development_uses_working_directory_without_resolving_native_data() -> TestResult {
        let temp = tempfile::tempdir()?;
        let working_directory = temp.path().join("working-directory");
        let native_called = std::cell::Cell::new(false);
        let directory = catalog_directory(
            true,
            || Ok(working_directory.clone()),
            || {
                native_called.set(true);
                Err(CatalogError::DataDirectory)
            },
        )?;
        assert_eq!(directory, working_directory.join("data"));
        assert_eq!(
            directory.join("catalog.sqlite3"),
            working_directory.join("data/catalog.sqlite3")
        );
        assert!(!native_called.get());
        assert!(!directory.exists());
        Ok(())
    }

    #[test]
    fn release_uses_native_data_without_resolving_working_directory() -> TestResult {
        let temp = tempfile::tempdir()?;
        let native_directory = temp.path().join("native-data").join("kinema");
        let current_called = std::cell::Cell::new(false);
        let directory = catalog_directory(
            false,
            || {
                current_called.set(true);
                Err(CatalogError::DataDirectory)
            },
            || Ok(native_directory.clone()),
        )?;
        assert_eq!(directory, native_directory);
        assert!(!current_called.get());
        assert!(!directory.exists());
        Ok(())
    }

    #[test]
    fn directory_resolution_failures_never_fall_back() {
        for development in [true, false] {
            let current_called = std::cell::Cell::new(false);
            let native_called = std::cell::Cell::new(false);
            let result = catalog_directory(
                development,
                || {
                    current_called.set(true);
                    Err(CatalogError::DataDirectory)
                },
                || {
                    native_called.set(true);
                    Err(CatalogError::DataDirectory)
                },
            );
            assert_eq!(result, Err(CatalogError::DataDirectory));
            assert_eq!(current_called.get(), development);
            assert_eq!(native_called.get(), !development);
        }
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
            .execute("UPDATE cinemeta_v3_searches SET fetched_at = 123", [])?;
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
            "UPDATE cinemeta_v3_searches SET fetched_at = 123;
             CREATE TRIGGER fail_second_member BEFORE INSERT ON cinemeta_v3_members
             WHEN NEW.position = 1 BEGIN SELECT RAISE(ABORT, 'forced test failure'); END;",
        )?;
        let saved = service.read(&query)?;
        assert_eq!(service.refresh(&query), Err(CatalogError::Storage));
        assert_eq!(service.read(&query)?, saved);
        let title_count: i64 =
            service
                .connection
                .query_row("SELECT count(*) FROM cinemeta_v3_titles", [], |row| {
                    row.get(0)
                })?;
        assert_eq!(title_count, 1);
        drop(service);
        assert_eq!(CatalogService::open(&path)?.read(&query)?, saved);
        mock.finish()?;
        Ok(())
    }

    #[test]
    fn corrupt_saved_data_is_an_error_not_empty() -> TestResult {
        let corruptions = [
            "UPDATE cinemeta_v3_titles SET name = ' '",
            "UPDATE cinemeta_v3_titles SET name = zeroblob(501)",
            "UPDATE cinemeta_v3_titles SET overview = replace(hex(zeroblob(20001)), '0', 'x')",
            "PRAGMA foreign_keys = OFF; UPDATE cinemeta_v3_titles SET imdb_id = 'bad'",
            "DELETE FROM cinemeta_v3_members",
            "PRAGMA ignore_check_constraints = ON; UPDATE cinemeta_v3_searches SET fetched_at = -1",
            "PRAGMA ignore_check_constraints = ON; UPDATE cinemeta_v3_searches SET result_count = 501",
            "UPDATE cinemeta_v3_members SET position = 1",
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

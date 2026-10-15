//! Version-two metadata persistence. No provider payload is stored wholesale;
//! only bounded, validated summaries/details and sanitized source identities.

use super::*;

pub(super) fn migrate_metadata(connection: &mut Connection) -> Result<(), CatalogError> {
    let tx = connection.transaction()?;
    tx.execute_batch(
        "CREATE TABLE cinemeta_v3_browse (
            kind TEXT NOT NULL CHECK(kind IN ('movie', 'series')),
            offset INTEGER NOT NULL CHECK(offset >= 0),
            fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
            payload TEXT NOT NULL,
            PRIMARY KEY(kind, offset)
        );
        CREATE TABLE cinemeta_v3_details (
            kind TEXT NOT NULL CHECK(kind IN ('movie', 'series')),
            imdb_id TEXT NOT NULL,
            fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
            payload TEXT NOT NULL,
            PRIMARY KEY(kind, imdb_id)
        );
        CREATE TABLE indexer_sources_v1 (
            indexer TEXT NOT NULL,
            target TEXT NOT NULL,
            scope TEXT NOT NULL,
            fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
            payload TEXT NOT NULL,
            PRIMARY KEY(indexer, target, scope)
        );
        PRAGMA user_version = 2;",
    )?;
    tx.commit()?;
    Ok(())
}

impl CatalogService {
    pub(crate) fn read_browse(
        &mut self,
        kind: MediaKind,
        offset: u32,
    ) -> Result<Option<CachedSearch>, CatalogError> {
        let row: Option<(i64, String)> = self.connection.query_row(
            "SELECT fetched_at, payload FROM cinemeta_v3_browse WHERE kind = ?1 AND offset = ?2",
            params![kind_text(kind), offset],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        row.map(|(timestamp, payload)| {
            let document: SearchResponse = decode_saved(&payload)?;
            Ok(CachedSearch {
                titles: validate_search(document, kind).map_err(|_| CatalogError::InvalidCache)?,
                fetched_at: saved_timestamp(timestamp)?,
            })
        })
        .transpose()
    }

    pub(crate) fn refresh_browse(
        &mut self,
        kind: MediaKind,
        offset: u32,
    ) -> Result<CachedSearch, CatalogError> {
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| CatalogError::Network)?
            .pop_if_empty()
            .extend(["catalog", kind_text(kind), "top"])
            .push(&format!("skip={offset}.json"));
        let document = self.fetch_json(url)?;
        let titles = validate_search(document, kind)?;
        let payload = serde_json::to_string(&search_document(&titles))
            .map_err(|_| CatalogError::InvalidResponse)?;
        let (fetched_at, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        for title in &titles {
            write_summary(&tx, title)?;
        }
        tx.execute(
            "INSERT INTO cinemeta_v3_browse(kind, offset, fetched_at, payload)
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(kind, offset) DO UPDATE SET
             fetched_at = excluded.fetched_at, payload = excluded.payload",
            params![kind_text(kind), offset, timestamp, payload],
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        Ok(CachedSearch { titles, fetched_at })
    }

    pub(crate) fn read_details(
        &mut self,
        kind: MediaKind,
        id: &ImdbId,
    ) -> Result<Option<CachedDetails>, CatalogError> {
        check_id(id)?;
        let row: Option<(i64, String)> = self.connection.query_row(
            "SELECT fetched_at, payload FROM cinemeta_v3_details WHERE kind = ?1 AND imdb_id = ?2",
            params![kind_text(kind), id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        row.map(|(timestamp, payload)| {
            let document: DetailsDocument = decode_saved(&payload)?;
            Ok(CachedDetails {
                details: document
                    .validate(kind, id)
                    .map_err(|_| CatalogError::InvalidCache)?,
                fetched_at: saved_timestamp(timestamp)?,
            })
        })
        .transpose()
    }

    pub(crate) fn refresh_details(
        &mut self,
        kind: MediaKind,
        id: &ImdbId,
    ) -> Result<CachedDetails, CatalogError> {
        check_id(id)?;
        let mut url = self.endpoint.clone();
        url.path_segments_mut()
            .map_err(|_| CatalogError::Network)?
            .pop_if_empty()
            .extend(["meta", kind_text(kind)])
            .push(&format!("{}.json", id.as_str()));
        let response: DetailsResponse = self.fetch_json(url)?;
        let details = response.meta.validate(kind, id)?;
        let payload = serde_json::to_string(&DetailsDocument::from_details(&details))
            .map_err(|_| CatalogError::InvalidResponse)?;
        let (fetched_at, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        write_summary(&tx, &details.title)?;
        tx.execute(
            "INSERT INTO cinemeta_v3_details(kind, imdb_id, fetched_at, payload)
             VALUES (?1, ?2, ?3, ?4) ON CONFLICT(kind, imdb_id) DO UPDATE SET
             fetched_at = excluded.fetched_at, payload = excluded.payload",
            params![kind_text(kind), id.as_str(), timestamp, payload],
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        Ok(CachedDetails {
            details,
            fetched_at,
        })
    }

    pub(super) fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        url: Url,
    ) -> Result<T, CatalogError> {
        self.ensure_active()?;
        let response = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT, "application/json")
            .send()
            .map_err(|error| {
                #[cfg(test)]
                transport_diagnostics(&error);
                drop(error);
                CatalogError::Network
            })?;
        self.ensure_active()?;
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
        self.ensure_active()?;
        if bytes.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(CatalogError::ResponseTooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|_| CatalogError::InvalidResponse)
    }
}

/// Test diagnostics must never display errors/sources: they may contain URLs.
#[cfg(test)]
fn transport_diagnostics(error: &reqwest::Error) {
    let mut tls = false;
    let mut dns = false;
    let mut io_kind = None;
    let mut cause: Option<&(dyn Error + 'static)> = Some(error);
    while let Some(source) = cause {
        let text = source.to_string().to_ascii_lowercase();
        tls |= text.contains("tls") || text.contains("certificate");
        dns |= text.contains("dns") || text.contains("resolv");
        if let Some(error) = source.downcast_ref::<std::io::Error>() {
            io_kind = Some(error.kind());
        }
        cause = source.source();
    }
    let catalog_host = error
        .url()
        .is_some_and(|url| url.host_str() == Some("cinemeta-catalogs.strem.io"));
    let primary_host = error
        .url()
        .is_some_and(|url| url.host_str() == Some("v3-cinemeta.strem.io"));
    eprintln!(
        "metadata HTTP failure categories: connect={} timeout={} request={} body={} tls_or_certificate={tls} dns_or_resolver={dns} io_kind={io_kind:?} primary_host={primary_host} catalog_host={catalog_host}",
        error.is_connect(),
        error.is_timeout(),
        error.is_request(),
        error.is_body()
    );
}

fn decode_saved<T: serde::de::DeserializeOwned>(payload: &str) -> Result<T, CatalogError> {
    if payload.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(CatalogError::InvalidCache);
    }
    serde_json::from_str(payload).map_err(|_| CatalogError::InvalidCache)
}

fn timestamp_now() -> Result<(u64, i64), CatalogError> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| CatalogError::Clock)?
        .as_secs();
    Ok((
        seconds,
        i64::try_from(seconds).map_err(|_| CatalogError::Clock)?,
    ))
}

fn saved_timestamp(value: i64) -> Result<u64, CatalogError> {
    u64::try_from(value).map_err(|_| CatalogError::InvalidCache)
}

fn write_summary(tx: &rusqlite::Transaction<'_>, title: &CatalogTitle) -> Result<(), CatalogError> {
    // Full details are stored independently: a later short search/browse result
    // never erases the full overview, artwork, genres, or episode metadata.
    tx.execute(
        "INSERT INTO cinemeta_v3_titles(kind, imdb_id, name, overview)
         VALUES (?1, ?2, ?3, ?4) ON CONFLICT(kind, imdb_id) DO UPDATE SET
         name = excluded.name, overview = COALESCE(excluded.overview, cinemeta_v3_titles.overview)",
        params![
            kind_text(title.kind()),
            title.id().as_str(),
            title.name(),
            title.overview()
        ],
    )?;
    Ok(())
}

fn validate_search(
    document: SearchResponse,
    kind: MediaKind,
) -> Result<Vec<CatalogTitle>, CatalogError> {
    if document.metas.len() > MAX_RESULTS {
        return Err(CatalogError::InvalidResponse);
    }
    let mut seen = HashSet::new();
    document
        .metas
        .into_iter()
        .map(|meta| {
            if meta.kind != kind_text(kind) {
                return Err(CatalogError::InvalidResponse);
            }
            let title = validate_title(meta.id, kind, meta.name, meta.description)?;
            if !seen.insert(title.id().clone()) {
                return Err(CatalogError::InvalidResponse);
            }
            Ok(title)
        })
        .collect()
}

fn search_document(titles: &[CatalogTitle]) -> SearchResponse {
    SearchResponse {
        metas: titles
            .iter()
            .map(|title| SearchMeta {
                id: title.id().as_str().to_owned(),
                kind: kind_text(title.kind()).to_owned(),
                name: title.name().to_owned(),
                description: title.overview().map(str::to_owned),
            })
            .collect(),
    }
}

#[derive(Deserialize)]
struct DetailsResponse {
    meta: DetailsDocument,
}

#[derive(Deserialize, Serialize)]
struct DetailsDocument {
    id: String,
    #[serde(rename = "type")]
    kind: String,
    name: String,
    description: Option<String>,
    poster: Option<String>,
    background: Option<String>,
    #[serde(rename = "releaseInfo")]
    release_info: Option<String>,
    #[serde(default)]
    genres: Vec<String>,
    videos: Vec<EpisodeDocument>,
}

#[derive(Deserialize, Serialize)]
struct EpisodeDocument {
    season: u32,
    episode: u32,
    name: Option<String>,
    overview: Option<String>,
    released: Option<String>,
}

impl DetailsDocument {
    fn validate(self, kind: MediaKind, id: &ImdbId) -> Result<TitleDetails, CatalogError> {
        if self.id != id.as_str()
            || self.kind != kind_text(kind)
            || self.genres.len() > 64
            || self.videos.len() > MAX_EPISODES
            || (kind == MediaKind::Movie && !self.videos.is_empty())
        {
            return Err(CatalogError::InvalidResponse);
        }
        let title = validate_title(self.id, kind, self.name, self.description)?;
        let poster_url = validate_image(self.poster)?;
        let background_url = validate_image(self.background)?;
        check_text(self.release_info.as_deref(), 200)?;
        for genre in &self.genres {
            check_text(Some(genre), 100)?;
            if genre.trim().is_empty() {
                return Err(CatalogError::InvalidResponse);
            }
        }
        let mut seen = HashSet::new();
        let mut episodes = Vec::with_capacity(self.videos.len());
        for video in self.videos {
            if video.episode == 0 || !seen.insert((video.season, video.episode)) {
                return Err(CatalogError::InvalidResponse);
            }
            check_text(video.name.as_deref(), MAX_NAME_CHARS)?;
            check_text(video.overview.as_deref(), MAX_OVERVIEW_CHARS)?;
            check_text(video.released.as_deref(), 100)?;
            episodes.push(CatalogEpisode {
                season: video.season,
                number: video.episode,
                name: video.name,
                overview: video.overview,
                released: video.released,
            });
        }
        episodes.sort_by_key(|episode| (episode.season, episode.number));
        Ok(TitleDetails {
            title,
            poster_url,
            background_url,
            release_info: self.release_info,
            genres: self.genres,
            episodes,
        })
    }

    fn from_details(details: &TitleDetails) -> Self {
        Self {
            id: details.title.id().as_str().to_owned(),
            kind: kind_text(details.title.kind()).into(),
            name: details.title.name().into(),
            description: details.title.overview().map(str::to_owned),
            poster: details.poster_url.clone(),
            background: details.background_url.clone(),
            release_info: details.release_info.clone(),
            genres: details.genres.clone(),
            videos: details
                .episodes
                .iter()
                .map(|episode| EpisodeDocument {
                    season: episode.season,
                    episode: episode.number,
                    name: episode.name.clone(),
                    overview: episode.overview.clone(),
                    released: episode.released.clone(),
                })
                .collect(),
        }
    }
}

fn check_text(value: Option<&str>, max: usize) -> Result<(), CatalogError> {
    if value.is_some_and(|text| text.chars().count() > max || text.contains('\0')) {
        return Err(CatalogError::InvalidResponse);
    }
    Ok(())
}

fn validate_image(value: Option<String>) -> Result<Option<String>, CatalogError> {
    if let Some(text) = &value {
        if text.len() > MAX_URL_BYTES || text.chars().any(char::is_control) {
            return Err(CatalogError::InvalidResponse);
        }
        let url = Url::parse(text).map_err(|_| CatalogError::InvalidResponse)?;
        if !matches!(url.scheme(), "http" | "https")
            || url.host_str().is_none()
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err(CatalogError::InvalidResponse);
        }
    }
    Ok(value)
}

// No configurable options or credentials are sent to these public endpoints.
// Bump this scope before changing the fixed indexer request configuration.
const SOURCE_SCOPE: &str = "public-default-v1";
const MAX_SOURCES: usize = 1000;

impl CatalogService {
    pub(crate) fn read_sources(
        &mut self,
        indexer: IndexerKind,
        target: &PlaybackTarget,
    ) -> Result<Option<CachedSources>, CatalogError> {
        let key = target_key(target)?;
        let row: Option<(i64, String)> = self
            .connection
            .query_row(
                "SELECT fetched_at, payload FROM indexer_sources_v1
             WHERE indexer = ?1 AND target = ?2 AND scope = ?3",
                params![indexer_key(indexer), key, SOURCE_SCOPE],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        row.map(|(timestamp, payload)| {
            let document: SourcesDocument = decode_saved(&payload)?;
            let (candidates, omitted) = document
                .validate()
                .map_err(|_| CatalogError::InvalidCache)?;
            Ok(CachedSources {
                candidates,
                omitted,
                fetched_at: saved_timestamp(timestamp)?,
            })
        })
        .transpose()
    }

    pub(crate) fn refresh_sources(
        &mut self,
        indexer: IndexerKind,
        target: &PlaybackTarget,
    ) -> Result<CachedSources, CatalogError> {
        // Reject invalid targets/canceled work before attempting a remote call.
        self.ensure_active()?;
        target_key(target)?;
        if self.indexers.is_none() {
            self.indexers =
                Some(crate::sources::IndexerClient::new().map_err(|_| CatalogError::Indexer)?);
        }
        let client = self.indexers.as_ref().ok_or(CatalogError::Indexer)?;
        let never_cancel = AtomicBool::new(false);
        let flag = self.cancellation.as_deref().unwrap_or(&never_cancel);
        let results = client.fetch(indexer, target, flag).map_err(|_| {
            if flag.load(Ordering::Acquire) {
                CatalogError::Canceled
            } else {
                CatalogError::Indexer
            }
        })?;
        self.ensure_active()?;
        self.save_sources(indexer, target, results)
    }

    pub(super) fn save_sources(
        &mut self,
        indexer: IndexerKind,
        target: &PlaybackTarget,
        results: crate::sources::SourceResults,
    ) -> Result<CachedSources, CatalogError> {
        let key = target_key(target)?;
        let document = SourcesDocument::from_results(results);
        let (candidates, omitted) = document.validate().map_err(|_| CatalogError::Indexer)?;
        let payload =
            serde_json::to_string(&document).map_err(|_| CatalogError::InvalidResponse)?;
        if payload.len() as u64 > MAX_RESPONSE_BYTES {
            return Err(CatalogError::ResponseTooLarge);
        }
        let (fetched_at, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO indexer_sources_v1(indexer, target, scope, fetched_at, payload)
             VALUES (?1, ?2, ?3, ?4, ?5) ON CONFLICT(indexer, target, scope) DO UPDATE SET
             fetched_at = excluded.fetched_at, payload = excluded.payload",
            params![indexer_key(indexer), key, SOURCE_SCOPE, timestamp, payload],
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        Ok(CachedSources {
            candidates,
            omitted,
            fetched_at,
        })
    }
}

fn indexer_key(indexer: IndexerKind) -> &'static str {
    match indexer {
        IndexerKind::Torrentio => "torrentio",
        IndexerKind::Peerflix => "peerflix",
    }
}

fn check_id(id: &ImdbId) -> Result<(), CatalogError> {
    if id.as_str().len() > MAX_ID_CHARS {
        return Err(CatalogError::InvalidResponse);
    }
    Ok(())
}

fn target_key(target: &PlaybackTarget) -> Result<String, CatalogError> {
    check_id(target.imdb_id()).map_err(|_| CatalogError::Indexer)?;
    match target {
        PlaybackTarget::Movie(id) => Ok(format!("movie:{}", id.as_str())),
        PlaybackTarget::Episode {
            series,
            season,
            episode,
        } if *episode > 0 => Ok(format!("series:{}:{season}:{episode}", series.as_str())),
        _ => Err(CatalogError::Indexer),
    }
}

// Whitelist the persistence shape: never store upstream URLs, headers, magnets,
// raw JSON, or tokens. Indexer DTOs themselves do not cross this boundary.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SourcesDocument {
    candidates: Vec<CandidateDocument>,
    omitted: usize,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CandidateDocument {
    hash: String,
    file_index: Option<u32>,
    filename: Option<String>,
    name: String,
    quality: Option<String>,
    size: Option<u64>,
    seeders: Option<u32>,
}

fn safe_label(value: &str, limit: usize) -> String {
    value
        .split_whitespace()
        .map(|word| {
            if word.contains("://")
                || Url::parse(word).is_ok()
                || word.contains('?')
                || word.contains('=')
            {
                "[redacted]"
            } else {
                word
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .filter(|character| !character.is_control())
        .take(limit)
        .collect()
}

impl SourcesDocument {
    fn from_results(results: crate::sources::SourceResults) -> Self {
        Self {
            omitted: results.omitted,
            candidates: results
                .candidates
                .into_iter()
                .map(|candidate| CandidateDocument {
                    hash: candidate.torrent.hash.as_str().to_owned(),
                    file_index: candidate.torrent.file_index,
                    filename: candidate
                        .torrent
                        .filename
                        .map(|text| safe_label(&text, 512)),
                    name: safe_label(&candidate.name, 512),
                    quality: candidate.quality.map(|text| safe_label(&text, 64)),
                    size: candidate.size,
                    seeders: candidate.seeders,
                })
                .collect(),
        }
    }

    fn validate(&self) -> Result<(Vec<SourceCandidate>, usize), CatalogError> {
        use kinema_core::sources::{InfoHash, TorrentSource};
        if self.candidates.len() > MAX_SOURCES || self.omitted > 1_000_000 {
            return Err(CatalogError::InvalidResponse);
        }
        let mut seen = HashSet::new();
        let candidates = self
            .candidates
            .iter()
            .map(|row| {
                let hash =
                    InfoHash::new(row.hash.clone()).map_err(|_| CatalogError::InvalidResponse)?;
                if hash.as_str() != row.hash
                    || !seen.insert((hash.clone(), row.file_index))
                    || row.name.is_empty()
                    || safe_label(&row.name, 512) != row.name
                    || row
                        .filename
                        .as_ref()
                        .is_some_and(|text| safe_label(text, 512) != *text)
                    || row
                        .quality
                        .as_ref()
                        .is_some_and(|text| safe_label(text, 64) != *text)
                {
                    return Err(CatalogError::InvalidResponse);
                }
                Ok(SourceCandidate {
                    torrent: TorrentSource {
                        hash,
                        file_index: row.file_index,
                        filename: row.filename.clone(),
                    },
                    name: row.name.clone(),
                    quality: row.quality.clone(),
                    size: row.size,
                    seeders: row.seeders,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok((candidates, self.omitted))
    }
}

pub(super) fn migrate_tmdb(connection: &mut Connection) -> Result<(), CatalogError> {
    let tx = connection.transaction()?;
    tx.execute_batch(
        "CREATE TABLE tmdb_v1_cache (
            resource TEXT NOT NULL,
            locale TEXT NOT NULL CHECK(locale = 'en-US'),
            fetched_at INTEGER NOT NULL CHECK(fetched_at >= 0),
            payload TEXT NOT NULL,
            PRIMARY KEY(resource, locale)
        );
        PRAGMA user_version = 3;",
    )?;
    tx.commit()?;
    Ok(())
}

impl CatalogService {
    /// Saved public metadata is readable without touching secure storage.
    pub(crate) fn read_tmdb(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
    ) -> Result<Option<CachedTmdb>, CatalogError> {
        let resource = crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
        let row: Option<(i64, String)> = self.connection.query_row(
            "SELECT fetched_at, payload FROM tmdb_v1_cache WHERE resource = ?1 AND locale = 'en-US'",
            params![resource], |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        row.map(|(timestamp, payload)| {
            Ok(CachedTmdb {
                data: crate::tmdb::decode(&payload, query)
                    .map_err(|_| CatalogError::InvalidCache)?,
                fetched_at: saved_timestamp(timestamp)?,
            })
        })
        .transpose()
    }

    pub(crate) fn refresh_tmdb(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
    ) -> Result<CachedTmdb, CatalogError> {
        self.refresh_tmdb_with_credential(query, || {
            crate::credentials::MetadataCredentialStore::new().get()
        })
    }

    // Credential lookup is injected only by tests. There is no environment/file
    // fallback, and the credential never becomes part of a cache key or payload.
    pub(super) fn refresh_tmdb_with_credential(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
        credential: impl FnOnce() -> Result<
            crate::credentials::SecretString,
            crate::credentials::CredentialError,
        >,
    ) -> Result<CachedTmdb, CatalogError> {
        self.ensure_active()?;
        crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
        let secret = credential().map_err(|error| match error {
            crate::credentials::CredentialError::Missing => CatalogError::TmdbMissingCredential,
            crate::credentials::CredentialError::Unavailable => {
                CatalogError::TmdbCredentialUnavailable
            }
            crate::credentials::CredentialError::AccessDenied => CatalogError::TmdbCredentialDenied,
        })?;
        let client = crate::tmdb::TmdbClient::new(secret).map_err(CatalogError::Tmdb)?;
        self.ensure_active()?;
        let data = client.fetch(query).map_err(CatalogError::Tmdb)?;
        self.ensure_active()?;
        self.save_tmdb(query, data)
    }

    pub(super) fn save_tmdb(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
        data: crate::tmdb::TmdbData,
    ) -> Result<CachedTmdb, CatalogError> {
        let resource = crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
        let payload = crate::tmdb::encode(&data).map_err(CatalogError::Tmdb)?;
        // Bind even fresh responses to the exact request before committing.
        let data = crate::tmdb::decode(&payload, query).map_err(CatalogError::Tmdb)?;
        let (fetched_at, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        tx.execute(
            "INSERT INTO tmdb_v1_cache(resource, locale, fetched_at, payload)
             VALUES (?1, 'en-US', ?2, ?3) ON CONFLICT(resource, locale) DO UPDATE SET
             fetched_at = excluded.fetched_at, payload = excluded.payload",
            params![resource, timestamp, payload],
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        Ok(CachedTmdb { data, fetched_at })
    }
}

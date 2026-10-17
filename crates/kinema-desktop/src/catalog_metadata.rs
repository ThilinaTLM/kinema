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
        normalized_pages::read_cinemeta_page(
            self,
            kind,
            &normalized_pages::cinemeta_browse_key(kind, offset),
        )
    }

    pub(crate) fn refresh_browse(
        &mut self,
        kind: MediaKind,
        offset: u32,
    ) -> Result<CachedSearch, CatalogError> {
        let result = self.refresh_browse_inner(kind, offset);
        if let Err(error) = result {
            normalized_fetches::browse(self, kind, offset, error);
        }
        result
    }
    fn refresh_browse_inner(
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
        let raw: serde_json::Value = self.fetch_json(url)?;
        let rich = normalized_rich::cinemeta_summaries(&raw)?;
        let document: SearchResponse =
            serde_json::from_value(raw).map_err(|_| CatalogError::InvalidResponse)?;
        let titles = validate_search(document, kind)?;
        let (_, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        normalized_pages::write_page(
            &tx,
            "cinemeta",
            &normalized_pages::cinemeta_browse_key(kind, offset),
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
            &normalized_pages::cinemeta_browse_key(kind, offset),
            1,
            kind,
            &rich,
            timestamp,
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        self.read_browse(kind, offset)?
            .ok_or(CatalogError::InvalidCache)
    }

    pub(crate) fn read_details(
        &mut self,
        kind: MediaKind,
        id: &ImdbId,
    ) -> Result<Option<CachedDetails>, CatalogError> {
        check_id(id)?;
        normalized_pages::read_cinemeta_details(self, kind, id)
    }

    pub(crate) fn refresh_details(
        &mut self,
        kind: MediaKind,
        id: &ImdbId,
    ) -> Result<CachedDetails, CatalogError> {
        let result = self.refresh_details_inner(kind, id);
        if let Err(error) = result {
            normalized_fetches::details(self, kind, id, error);
        }
        result
    }
    fn refresh_details_inner(
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
        let raw: serde_json::Value = self.fetch_json(url)?;
        let rich =
            normalized_rich::cinemeta(raw.get("meta").ok_or(CatalogError::InvalidResponse)?)?;
        let response: DetailsResponse =
            serde_json::from_value(raw).map_err(|_| CatalogError::InvalidResponse)?;
        let details = response.meta.validate(kind, id)?;
        let (_, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        normalized_pages::write_cinemeta_details(&tx, &details, timestamp, true)?;
        let title = normalized::title_id(&tx, kind, "imdb", id.as_str())?
            .ok_or(CatalogError::InvalidResponse)?;
        let fetch_id: i64 = tx.query_row("SELECT id FROM metadata_fetches WHERE title_id=?1 AND provider='cinemeta' AND resource_key=?2",params![title,normalized_pages::cinemeta_detail_key(kind,id)],|row|row.get(0))?;
        normalized_rich::write(&tx, title, &rich, "cinemeta", fetch_id, timestamp)?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        self.read_details(kind, id)?
            .ok_or(CatalogError::InvalidCache)
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

pub(super) fn timestamp_now() -> Result<(u64, i64), CatalogError> {
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

pub(super) fn backfill_browse(
    tx: &rusqlite::Transaction<'_>,
    kind: MediaKind,
    offset: u32,
    time: i64,
    payload: &str,
) -> Result<(), CatalogError> {
    if time < 0 {
        return Err(CatalogError::InvalidCache);
    }
    let document: SearchResponse = decode_saved(payload)?;
    let titles = validate_search(document, kind).map_err(|_| CatalogError::InvalidCache)?;
    normalized_pages::write_page(
        tx,
        "cinemeta",
        &normalized_pages::cinemeta_browse_key(kind, offset),
        1,
        None,
        &titles
            .iter()
            .map(normalized::imdb_observation)
            .collect::<Vec<_>>(),
        time,
    )
}
pub(super) fn backfill_details(
    tx: &rusqlite::Transaction<'_>,
    kind: MediaKind,
    external: &str,
    time: i64,
    payload: &str,
) -> Result<(), CatalogError> {
    if time < 0 {
        return Err(CatalogError::InvalidCache);
    }
    let id = ImdbId::new(external).map_err(|_| CatalogError::InvalidCache)?;
    let document: DetailsDocument = decode_saved(payload)?;
    let details = document
        .validate(kind, &id)
        .map_err(|_| CatalogError::InvalidCache)?;
    normalized_pages::write_cinemeta_details(tx, &details, time, false)
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
    #[serde(default, deserialize_with = "deserialize_videos")]
    videos: Option<Vec<EpisodeDocument>>,
}

#[derive(Deserialize, Serialize)]
struct EpisodeDocument {
    thumbnail: Option<String>,
    season: u32,
    episode: u32,
    name: Option<String>,
    overview: Option<String>,
    released: Option<String>,
}

// Missing and null are deliberately different: only an omitted movie field is
// allowed. Deserialize a present value as a vector so null remains invalid.
fn deserialize_videos<'de, D>(deserializer: D) -> Result<Option<Vec<EpisodeDocument>>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::deserialize(deserializer).map(Some)
}

impl DetailsDocument {
    fn validate(self, kind: MediaKind, id: &ImdbId) -> Result<TitleDetails, CatalogError> {
        let videos = match self.videos {
            Some(videos) => videos,
            None if kind == MediaKind::Movie => Vec::new(),
            None => return Err(CatalogError::InvalidResponse),
        };
        if self.id != id.as_str()
            || self.kind != kind_text(kind)
            || self.genres.len() > 64
            || videos.len() > MAX_EPISODES
            || (kind == MediaKind::Movie && !videos.is_empty())
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
        let mut episodes = Vec::with_capacity(videos.len());
        for video in videos {
            if video.episode == 0 || !seen.insert((video.season, video.episode)) {
                return Err(CatalogError::InvalidResponse);
            }
            check_text(video.name.as_deref(), MAX_NAME_CHARS)?;
            check_text(video.overview.as_deref(), MAX_OVERVIEW_CHARS)?;
            check_text(video.released.as_deref(), 100)?;
            episodes.push(CatalogEpisode {
                still_url: validate_image(video.thumbnail)?,
                runtime_minutes: None,
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

    #[cfg(test)]
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
            videos: Some(
                details
                    .episodes
                    .iter()
                    .map(|episode| EpisodeDocument {
                        thumbnail: episode.still_url.clone(),
                        season: episode.season,
                        episode: episode.number,
                        name: episode.name.clone(),
                        overview: episode.overview.clone(),
                        released: episode.released.clone(),
                    })
                    .collect(),
            ),
        }
    }
}

fn check_text(value: Option<&str>, max: usize) -> Result<(), CatalogError> {
    if value.is_some_and(|text| text.chars().count() > max || text.contains('\0')) {
        return Err(CatalogError::InvalidResponse);
    }
    Ok(())
}

pub(super) fn validate_image(value: Option<String>) -> Result<Option<String>, CatalogError> {
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
        let Some((movie, episode)) = normalized_source_target(&self.connection, target)? else {
            return Ok(None);
        };
        let row: Option<(i64,String)> = self.connection.query_row(
            "SELECT fetched_at,sources_json FROM indexer_snapshots WHERE movie_id IS ?1 AND episode_id IS ?2 AND indexer=?3 AND query_key=?4 AND cache_scope=?5 AND payload_version=1",
            params![movie,episode,indexer_key(indexer),key,SOURCE_SCOPE],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
        row.map(|(time, payload)| {
            let document: SourcesDocument = decode_saved(&payload)?;
            let (candidates, omitted) = document
                .validate()
                .map_err(|_| CatalogError::InvalidCache)?;
            Ok(CachedSources {
                candidates,
                omitted,
                fetched_at: saved_timestamp(time)?,
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
        normalized_source_target(&self.connection, target)?
            .ok_or(CatalogError::MissingIdentityMapping)?;
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
        let (movie, episode) =
            normalized_source_target(&tx, target)?.ok_or(CatalogError::MissingIdentityMapping)?;
        let previous: Option<(i64,String,bool)> = tx.query_row("SELECT id,sources_json,has_omitted_sources FROM indexer_snapshots WHERE movie_id IS ?1 AND episode_id IS ?2 AND indexer=?3 AND query_key=?4 AND cache_scope=?5",params![movie,episode,indexer_key(indexer),key,SOURCE_SCOPE],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?))).optional()?;
        if let Some((id, old, old_omitted)) = previous {
            tx.execute("UPDATE indexer_snapshots SET sources_json=?2,has_omitted_sources=?3,fetched_at=?4,updated_at=CASE WHEN ?5 THEN ?4 ELSE updated_at END WHERE id=?1",params![id,payload,omitted>0,timestamp,old != payload || old_omitted != (omitted>0)])?;
        } else {
            tx.execute("INSERT INTO indexer_snapshots(movie_id,movie_kind,episode_id,indexer,query_key,cache_scope,payload_version,sources_json,has_omitted_sources,fetched_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8,?9,?9)",params![movie,movie.map(|_|"movie"),episode,indexer_key(indexer),key,SOURCE_SCOPE,payload,omitted>0,timestamp])?;
        }
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        Ok(CachedSources {
            candidates,
            omitted,
            fetched_at,
        })
    }
}

type SourceTargetIds = (Option<i64>, Option<i64>);

fn normalized_source_target(
    connection: &Connection,
    target: &PlaybackTarget,
) -> Result<Option<SourceTargetIds>, CatalogError> {
    match target {
        PlaybackTarget::Movie(id) => Ok(connection.query_row("SELECT title_id FROM title_identifiers WHERE namespace='imdb' AND media_kind='movie' AND external_id=?1",params![id.as_str()],|row|row.get(0)).optional()?.map(|id|(Some(id),None))),
        PlaybackTarget::Episode {series,season,episode} => Ok(connection.query_row("SELECT e.id FROM title_identifiers i JOIN seasons s ON s.series_id=i.title_id JOIN episodes e ON e.season_id=s.id WHERE i.namespace='imdb' AND i.media_kind='series' AND i.external_id=?1 AND s.season_number=?2 AND e.episode_number=?3",params![series.as_str(),season,episode],|row|row.get(0)).optional()?.map(|id|(None,Some(id)))),
    }
}

pub(super) fn backfill_sources(
    tx: &rusqlite::Transaction<'_>,
    indexer: &str,
    key: &str,
    scope: &str,
    time: i64,
    payload: &str,
) -> Result<(), CatalogError> {
    let document: SourcesDocument = decode_saved(payload)?;
    let (_, omitted) = document
        .validate()
        .map_err(|_| CatalogError::InvalidCache)?;
    if time < 0 {
        return Err(CatalogError::InvalidCache);
    }
    // Unknown endpoint/scope contracts and missing metadata mappings remain
    // retained legacy evidence; never create a guessed catalog parent.
    if !matches!(indexer, "torrentio" | "peerflix") || scope != SOURCE_SCOPE {
        return Ok(());
    }
    let parts: Vec<_> = key.split(':').collect();
    let target = match parts.as_slice() {
        ["movie", id] => {
            PlaybackTarget::Movie(ImdbId::new(*id).map_err(|_| CatalogError::InvalidCache)?)
        }
        ["series", id, season, episode] => PlaybackTarget::Episode {
            series: ImdbId::new(*id).map_err(|_| CatalogError::InvalidCache)?,
            season: season.parse().map_err(|_| CatalogError::InvalidCache)?,
            episode: episode.parse().map_err(|_| CatalogError::InvalidCache)?,
        },
        _ => return Err(CatalogError::InvalidCache),
    };
    if target_key(&target).map_err(|_| CatalogError::InvalidCache)? != key {
        return Err(CatalogError::InvalidCache);
    }
    let Some((movie, episode)) = normalized_source_target(tx, &target)? else {
        return Ok(());
    };
    tx.execute("INSERT INTO indexer_snapshots(movie_id,movie_kind,episode_id,indexer,query_key,cache_scope,payload_version,sources_json,has_omitted_sources,fetched_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,1,?7,?8,?9,?9)",params![movie,movie.map(|_|"movie"),episode,indexer,key,scope,payload,omitted>0,time])?;
    Ok(())
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
        normalized_tmdb::read(self, query)
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
        let result = self.refresh_tmdb_credential_inner(query, credential);
        if let Err(error) = result {
            normalized_fetches::tmdb(self, query, error);
        }
        result
    }
    fn refresh_tmdb_credential_inner(
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
        let (data, rich) = client.fetch_normalized(query).map_err(CatalogError::Tmdb)?;
        self.ensure_active()?;
        self.save_tmdb_rich(query, data, &rich)
    }

    #[cfg(test)]
    pub(super) fn save_tmdb(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
        data: crate::tmdb::TmdbData,
    ) -> Result<CachedTmdb, CatalogError> {
        self.save_tmdb_rich(query, data, &[])
    }

    fn save_tmdb_rich(
        &mut self,
        query: &crate::tmdb::TmdbQuery,
        data: crate::tmdb::TmdbData,
        rich: &[normalized_rich::RichRecord],
    ) -> Result<CachedTmdb, CatalogError> {
        crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
        let payload = crate::tmdb::encode(&data).map_err(CatalogError::Tmdb)?;
        // Bind even fresh responses to the exact request before committing.
        let data = crate::tmdb::decode(&payload, query).map_err(CatalogError::Tmdb)?;
        let (_, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        normalized_tmdb::write(&tx, query, &data, timestamp, true)?;
        normalized_tmdb::write_rich(&tx, query, rich, timestamp)?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        self.read_tmdb(query)?.ok_or(CatalogError::InvalidCache)
    }
}

/// Offline production-screen fixtures use the same response decoder/validation
/// as the transport path, without constructing SQL/network services.
#[cfg(feature = "ui-probe")]
pub(super) fn decode_fixture_details(
    kind: MediaKind,
    id: &ImdbId,
    payload: &str,
) -> Result<TitleDetails, CatalogError> {
    let response: DetailsResponse =
        serde_json::from_str(payload).map_err(|_| CatalogError::InvalidResponse)?;
    response.meta.validate(kind, id)
}

#[cfg(test)]
mod cinemeta_details_tests {
    use super::*;

    const MOVIE: &str =
        include_str!("../tests/fixtures/cinemeta_movie_tt22084616_without_videos.json");

    #[test]
    fn movie_without_videos_has_no_episodes_and_round_trips() -> Result<(), Box<dyn Error>> {
        let response: DetailsResponse = serde_json::from_str(MOVIE)?;
        let id = ImdbId::new("tt22084616")?;
        let details = response.meta.validate(MediaKind::Movie, &id)?;
        assert_eq!(details.title.name(), "Spider-Man: Brand New Day");
        assert!(details.episodes.is_empty());
        let saved = serde_json::to_string(&DetailsDocument::from_details(&details))?;
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&saved)?["videos"],
            serde_json::json!([])
        );
        let restored: DetailsDocument = serde_json::from_str(&saved)?;
        assert_eq!(restored.validate(MediaKind::Movie, &id)?, details);
        Ok(())
    }

    #[test]
    fn omitted_videos_is_movie_only_and_does_not_relax_other_fields() -> Result<(), Box<dyn Error>>
    {
        let id = ImdbId::new("tt22084616")?;
        let original: serde_json::Value = serde_json::from_str(MOVIE)?;
        let mut series = original.clone();
        series["meta"]["type"] = serde_json::json!("series");
        let response: DetailsResponse = serde_json::from_value(series.clone())?;
        assert!(response.meta.validate(MediaKind::Series, &id).is_err());
        series["meta"]["videos"] = serde_json::json!([]);
        let response: DetailsResponse = serde_json::from_value(series)?;
        assert!(response.meta.validate(MediaKind::Series, &id).is_ok());

        for kind in ["movie", "series"] {
            for videos in [
                serde_json::json!(null),
                serde_json::json!({}),
                serde_json::json!("bad"),
                serde_json::json!([{"season": 1}]),
            ] {
                let mut malformed = original.clone();
                malformed["meta"]["type"] = serde_json::json!(kind);
                malformed["meta"]["videos"] = videos;
                assert!(serde_json::from_value::<DetailsResponse>(malformed).is_err());
            }
        }
        for (field, value) in [
            ("name", serde_json::json!(42)),
            ("genres", serde_json::json!(null)),
        ] {
            let mut malformed = original.clone();
            malformed["meta"][field] = value;
            assert!(serde_json::from_value::<DetailsResponse>(malformed).is_err());
        }
        let mut nonempty_movie = original.clone();
        nonempty_movie["meta"]["videos"] = serde_json::json!([{"season": 1, "episode": 1}]);
        let response: DetailsResponse = serde_json::from_value(nonempty_movie)?;
        assert!(response.meta.validate(MediaKind::Movie, &id).is_err());
        let mut malformed = original;
        malformed["meta"]["poster"] = serde_json::json!("file:///private/poster");
        let response: DetailsResponse = serde_json::from_value(malformed)?;
        assert!(response.meta.validate(MediaKind::Movie, &id).is_err());
        Ok(())
    }
}

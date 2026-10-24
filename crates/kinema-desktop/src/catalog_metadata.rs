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
        let (titles, rich) = parse_search(raw, kind)?;
        self.save_browse(kind, offset, titles, rich)
    }

    fn save_browse(
        &mut self,
        kind: MediaKind,
        offset: u32,
        titles: Vec<CatalogTitle>,
        rich: Vec<normalized_rich::RichRecord>,
    ) -> Result<CachedSearch, CatalogError> {
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
            &self.provider_priority,
        )?;
        normalized_rich::write_page_rich(
            &tx,
            "cinemeta",
            &normalized_pages::cinemeta_browse_key(kind, offset),
            1,
            kind,
            &rich,
            timestamp,
            &self.provider_priority,
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
        let started = std::time::Instant::now();
        let result = self.refresh_details_inner(kind, id);
        tracing::debug!(
            provider = "cinemeta",
            request_kind = "details",
            elapsed_ms = started.elapsed().as_millis() as u64,
            outcome = if result.is_ok() { "success" } else { "failure" },
            "metadata request completed"
        );
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
        let (details, rich) = parse_details(raw, kind, id)?;
        self.save_details(kind, id, details, rich)
    }

    fn save_details(
        &mut self,
        kind: MediaKind,
        id: &ImdbId,
        details: TitleDetails,
        rich: Vec<normalized_rich::RichRecord>,
    ) -> Result<CachedDetails, CatalogError> {
        let (_, timestamp) = timestamp_now()?;
        let cancellation = self.cancellation.clone();
        check_cancellation(cancellation.as_deref())?;
        let tx = self.connection.transaction()?;
        normalized_pages::write_cinemeta_details(
            &tx,
            &details,
            timestamp,
            true,
            &self.provider_priority,
        )?;
        let title = normalized::title_id(&tx, kind, "imdb", id.as_str())?
            .ok_or(CatalogError::InvalidResponse)?;
        let fetch_id: i64 = tx.query_row("SELECT id FROM metadata_fetches WHERE title_id=?1 AND provider='cinemeta' AND resource_key=?2",params![title,normalized_pages::cinemeta_detail_key(kind,id)],|row|row.get(0))?;
        normalized_rich::write(
            &tx,
            title,
            &rich,
            "cinemeta",
            fetch_id,
            timestamp,
            &self.provider_priority,
        )?;
        check_cancellation(cancellation.as_deref())?;
        tx.commit()?;
        self.read_details(kind, id)?
            .ok_or(CatalogError::InvalidCache)
    }

    pub(super) fn fetch_json<T: serde::de::DeserializeOwned>(
        &self,
        url: Url,
    ) -> Result<T, CatalogError> {
        let never_cancel = AtomicBool::new(false);
        fetch_json(
            &self.client,
            url,
            self.cancellation.as_deref().unwrap_or(&never_cancel),
        )
    }
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
        &Default::default(),
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
    normalized_pages::write_cinemeta_details(tx, &details, time, false, &Default::default())
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
    #[serde(default)]
    facts: crate::tmdb::FactsPayload,
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
            facts: self
                .facts
                .normalize()
                .map_err(|_| CatalogError::InvalidResponse)?,
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
            facts: crate::tmdb::FactsPayload::from(&details.facts),
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
            self.indexers = Some(Arc::new(
                crate::sources::IndexerClient::new().map_err(|_| CatalogError::Indexer)?,
            ));
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
    // Unknown endpoint/scope contracts and missing metadata mappings cannot be
    // imported: never create a guessed catalog parent. The v6 migration later
    // discards this obsolete cache under the explicit disposal policy.
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

    /// Acquisition completeness, not an age/TTL or nonempty-facts check. A NULL
    /// payload identifies details cached before facts were fetched; a validated
    /// non-NULL payload (even all unknown values) records a successful attempt.
    /// Call only after a successful metadata cache hit: misses and corrupt cache
    /// payloads keep their existing worker semantics. The worker must use this
    /// result to show the saved copy with `refreshing=true` and then fetch.
    pub(crate) fn metadata_needs_refresh(
        &self,
        action: &crate::catalog_worker::MetadataAction,
    ) -> Result<bool, CatalogError> {
        use crate::{catalog_worker::MetadataAction, tmdb::TmdbQuery};
        let (kind, namespace, external, provider, resource) = match action {
            MetadataAction::Details { kind, id } => (
                *kind,
                "imdb",
                id.as_str().to_owned(),
                "cinemeta",
                normalized_pages::cinemeta_detail_key(*kind, id),
            ),
            MetadataAction::Tmdb(query @ TmdbQuery::Details { kind, id }) => (
                *kind,
                "tmdb",
                id.to_string(),
                "tmdb",
                crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?,
            ),
            _ => return Ok(false),
        };
        self.connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM metadata_fetches f JOIN title_identifiers i ON i.title_id=f.title_id WHERE i.media_kind=?1 AND i.namespace=?2 AND i.external_id=?3 AND f.provider=?4 AND f.resource_key=?5 AND f.locale='en-US' AND f.completeness='detail' AND f.facts_json IS NULL)",
            params![kind_text(kind), namespace, external, provider, resource],
            |row| row.get(0),
        ).map_err(|_| CatalogError::Storage)
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
        let client = self.tmdb_client(credential)?;
        self.ensure_active()?;
        let result = client.fetch_normalized(query);
        if matches!(result, Err(crate::tmdb::TmdbError::RejectedCredential)) {
            tracing::debug!(
                provider = "tmdb",
                reason = "credential_rejection",
                "TMDB credential cache invalidated"
            );
            self.tmdb = None;
        }
        let (data, rich) = result.map_err(CatalogError::Tmdb)?;
        self.ensure_active()?;
        self.save_tmdb_rich(query, data, &rich)
    }

    pub(super) fn tmdb_client(
        &mut self,
        credential: impl FnOnce() -> Result<
            crate::credentials::SecretString,
            crate::credentials::CredentialError,
        >,
    ) -> Result<Arc<crate::tmdb::TmdbClient>, CatalogError> {
        let generation = crate::credentials::metadata_generation();
        if let Some((saved, client)) = &self.tmdb {
            let reason = if *saved != generation {
                Some("generation_change")
            } else if client.credential_rejected() {
                Some("credential_rejection")
            } else {
                None
            };
            if let Some(reason) = reason {
                tracing::debug!(
                    provider = "tmdb",
                    reason,
                    "TMDB credential cache invalidated"
                );
                self.tmdb = None;
            }
        }
        if let Some((_, client)) = &self.tmdb {
            tracing::debug!(
                provider = "tmdb",
                cache = "hit",
                "TMDB credential client reused"
            );
            return Ok(Arc::clone(client));
        }
        tracing::debug!(
            provider = "tmdb",
            reason = "cache_miss",
            "TMDB credential reloading from secure storage"
        );
        let secret = credential().map_err(|error| {
            tracing::debug!(provider = "tmdb", category = ?error, "TMDB credential reload failed");
            match error {
                crate::credentials::CredentialError::Missing => CatalogError::TmdbMissingCredential,
                crate::credentials::CredentialError::Unavailable => {
                    CatalogError::TmdbCredentialUnavailable
                }
                crate::credentials::CredentialError::AccessDenied => {
                    CatalogError::TmdbCredentialDenied
                }
            }
        })?;
        let transport = match &self.tmdb_transport {
            Some(client) => client.clone(),
            None => {
                let client = crate::tmdb::TmdbClient::transport().map_err(CatalogError::Tmdb)?;
                self.tmdb_transport = Some(client.clone());
                client
            }
        };
        let client = Arc::new(
            crate::tmdb::TmdbClient::with_transport(secret, transport)
                .map_err(CatalogError::Tmdb)?,
        );
        self.tmdb = Some((generation, Arc::clone(&client)));
        Ok(client)
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
        normalized_tmdb::write(&tx, query, &data, timestamp, true, &self.provider_priority)?;
        normalized_tmdb::write_rich(&tx, query, rich, timestamp, &self.provider_priority)?;
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
    let raw = serde_json::from_str(payload).map_err(|_| CatalogError::InvalidResponse)?;
    parse_details(raw, kind, id).map(|(details, _)| details)
}

fn parse_details(
    raw: serde_json::Value,
    kind: MediaKind,
    id: &ImdbId,
) -> Result<(TitleDetails, Vec<normalized_rich::RichRecord>), CatalogError> {
    let rich = normalized_rich::cinemeta(raw.get("meta").ok_or(CatalogError::InvalidResponse)?)?;
    let facts = cinemeta_facts(raw.get("meta").ok_or(CatalogError::InvalidResponse)?, kind)?;
    let response: DetailsResponse =
        serde_json::from_value(raw).map_err(|_| CatalogError::InvalidResponse)?;
    let mut details = response.meta.validate(kind, id)?;
    details.facts = facts;
    Ok((details, rich))
}

fn fetch_json<T: serde::de::DeserializeOwned>(
    client: &Client,
    url: Url,
    cancel: &AtomicBool,
) -> Result<T, CatalogError> {
    let started = std::time::Instant::now();
    check_cancellation(Some(cancel))?;
    let response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .map_err(|error| {
            tracing::warn!(
                provider = "cinemeta",
                kind = crate::logging::http_error_kind(&error),
                duration_ms = started.elapsed().as_millis() as u64,
                "Cinemeta HTTP transport failed"
            );
            CatalogError::Network
        })?;
    check_cancellation(Some(cancel))?;
    let status = response.status().as_u16();
    tracing::debug!(
        provider = "cinemeta",
        status,
        duration_ms = started.elapsed().as_millis() as u64,
        "Cinemeta HTTP headers received"
    );
    if response.status().as_u16() == 404 {
        return Err(CatalogError::NotFound);
    }
    if !response.status().is_success() {
        tracing::warn!(
            provider = "cinemeta",
            status,
            "Cinemeta HTTP request failed"
        );
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
        .map_err(|error| { tracing::warn!(provider = "cinemeta", io_kind = ?error.kind(), "Cinemeta body read failed"); CatalogError::Network })?;
    check_cancellation(Some(cancel))?;
    if bytes.len() as u64 > MAX_RESPONSE_BYTES {
        return Err(CatalogError::ResponseTooLarge);
    }
    tracing::debug!(
        provider = "cinemeta",
        bytes = bytes.len(),
        duration_ms = started.elapsed().as_millis() as u64,
        "Cinemeta HTTP body received"
    );
    serde_json::from_slice(&bytes).map_err(|_| {
        tracing::warn!(
            provider = "cinemeta",
            category = "json_decode",
            "Cinemeta response decode failed"
        );
        CatalogError::InvalidResponse
    })
}

// Only these validated values cross back from the fetch pool. SQLite never does.
struct EnrichmentData {
    values: Vec<crate::metadata_providers::ProviderTitleDetails>,
    failed: Vec<kinema_core::provider::ProviderId>,
    inactive: Vec<kinema_core::provider::ProviderId>,
}
enum RemoteData {
    Enrichment(EnrichmentData),
    Page(Vec<CatalogTitle>, Vec<normalized_rich::RichRecord>),
    Details(TitleDetails, Vec<normalized_rich::RichRecord>),
    Sources(crate::sources::SourceResults),
    Tmdb(crate::tmdb::TmdbData, Vec<normalized_rich::RichRecord>),
}

impl CatalogService {
    pub(crate) fn prepare_remote(
        &mut self,
        work: crate::catalog_worker::Work,
        cancel: Arc<AtomicBool>,
    ) -> Result<crate::catalog_worker::Fetch<Self>, CatalogError> {
        let result = self.prepare_remote_inner(work.clone(), cancel);
        if let Err(error) = result {
            let _span = work.diagnostic_span().entered();
            if error != CatalogError::Canceled {
                tracing::warn!(category = ?error, "Catalog fetch preparation failed");
            }
            self.record_remote_failure(&work, error);
        }
        result
    }

    fn prepare_remote_inner(
        &mut self,
        work: crate::catalog_worker::Work,
        cancel: Arc<AtomicBool>,
    ) -> Result<crate::catalog_worker::Fetch<Self>, CatalogError> {
        use crate::catalog_worker::{
            MetadataAction as A, MetadataSnapshot as S, Refreshed as R, Work,
        };
        let network: Box<dyn FnOnce() -> Result<RemoteData, CatalogError> + Send> = match &work {
            Work::Search(request) => {
                let url = search_url(&self.endpoint, &request.query)?;
                let kind = request.query.kind();
                let client = self.client.clone();
                Box::new(move || {
                    let raw = fetch_json(&client, url, &cancel)?;
                    let (titles, rich) = parse_search(raw, kind)?;
                    Ok(RemoteData::Page(titles, rich))
                })
            }
            Work::Metadata(request) => match &request.action {
                A::Enrich {
                    kind,
                    imdb,
                    providers,
                    priority,
                    mode,
                } => {
                    use crate::metadata_providers::{MetadataSource, ProviderError};
                    use kinema_core::provider::{ProviderId, ProviderPriority};
                    self.set_provider_priority(ProviderPriority::new(priority.iter().copied()));
                    if imdb.len() > 20 {
                        return Err(CatalogError::InvalidMetadata);
                    }
                    let mut ids = Vec::new();
                    for id in imdb.iter().take(20) {
                        check_id(id)?;
                        if ids.contains(id) {
                            continue;
                        }
                        if *mode == crate::catalog_worker::EnrichMode::CardFields
                            && self.card_fields_complete(*kind, id)?
                        {
                            continue;
                        }
                        ids.push(id.clone());
                    }
                    let mut sources: Vec<Box<dyn MetadataSource>> = Vec::new();
                    let mut failed = Vec::new();
                    let mut inactive = Vec::new();
                    for provider in providers.iter().copied().filter(|_| !ids.is_empty()) {
                        if sources.iter().any(|s| s.id() == provider)
                            || failed.contains(&provider)
                            || inactive.contains(&provider)
                        {
                            continue;
                        }
                        match provider {
                            ProviderId::Cinemeta => sources.push(Box::new(
                                crate::metadata_providers::cinemeta::Cinemeta {
                                    client: self.client.clone(),
                                    endpoint: self.endpoint.clone(),
                                },
                            )),
                            ProviderId::Tmdb => match self.tmdb_client(|| {
                                crate::credentials::MetadataCredentialStore::new().get()
                            }) {
                                Ok(client) => {
                                    sources.push(Box::new(crate::metadata_providers::tmdb::Tmdb {
                                        client: Some(client),
                                    }))
                                }
                                Err(CatalogError::TmdbMissingCredential) => inactive.push(provider),
                                Err(_) => failed.push(provider),
                            },
                        }
                    }
                    let (kind, mode) = (*kind, *mode);
                    Box::new(move || {
                        let mut values = Vec::new();
                        for id in ids {
                            for source in &sources {
                                check_cancellation(Some(&cancel))?;
                                let result = match mode {
                                    crate::catalog_worker::EnrichMode::Details => {
                                        source.details_by_imdb(kind, &id, &cancel)
                                    }
                                    crate::catalog_worker::EnrichMode::CardFields => {
                                        source.summary_by_imdb(kind, &id, &cancel)
                                    }
                                };
                                match result {
                                    Ok(Some(value)) => values.push(value),
                                    Ok(None) => (),
                                    Err(ProviderError::Inactive) => {
                                        if !inactive.contains(&source.id()) {
                                            inactive.push(source.id());
                                        }
                                    }
                                    Err(ProviderError::Canceled) => {
                                        return Err(CatalogError::Canceled);
                                    }
                                    Err(ProviderError::Failed(category)) => {
                                        tracing::debug!(
                                            provider = source.id().as_str(),
                                            ?category,
                                            "Optional metadata enrichment failed"
                                        );
                                        if !failed.contains(&source.id()) {
                                            failed.push(source.id());
                                        }
                                    }
                                }
                            }
                        }
                        Ok(RemoteData::Enrichment(EnrichmentData {
                            values,
                            failed,
                            inactive,
                        }))
                    })
                }
                A::Browse { kind, offset } => {
                    let kind = *kind;
                    let mut url = self.endpoint.clone();
                    url.path_segments_mut()
                        .map_err(|_| CatalogError::Network)?
                        .pop_if_empty()
                        .extend(["catalog", kind_text(kind), "top"])
                        .push(&format!("skip={offset}.json"));
                    let client = self.client.clone();
                    Box::new(move || {
                        let raw = fetch_json(&client, url, &cancel)?;
                        let (titles, rich) = parse_search(raw, kind)?;
                        Ok(RemoteData::Page(titles, rich))
                    })
                }
                A::Details { kind, id } => {
                    check_id(id)?;
                    let (kind, id) = (*kind, id.clone());
                    let mut url = self.endpoint.clone();
                    url.path_segments_mut()
                        .map_err(|_| CatalogError::Network)?
                        .pop_if_empty()
                        .extend(["meta", kind_text(kind)])
                        .push(&format!("{}.json", id.as_str()));
                    let client = self.client.clone();
                    Box::new(move || {
                        let started = std::time::Instant::now();
                        let result = (|| {
                            let raw = fetch_json(&client, url, &cancel)?;
                            let (details, rich) = parse_details(raw, kind, &id)?;
                            Ok(RemoteData::Details(details, rich))
                        })();
                        tracing::debug!(provider="cinemeta", request_kind="details", elapsed_ms=started.elapsed().as_millis() as u64, outcome=?result.as_ref().map(|_| "success").map_err(|e| *e), "metadata request completed");
                        result
                    })
                }
                A::Sources { indexer, target } => {
                    target_key(target)?;
                    normalized_source_target(&self.connection, target)?
                        .ok_or(CatalogError::MissingIdentityMapping)?;
                    if self.indexers.is_none() {
                        self.indexers = Some(Arc::new(
                            crate::sources::IndexerClient::new()
                                .map_err(|_| CatalogError::Indexer)?,
                        ));
                    }
                    let client = Arc::clone(self.indexers.as_ref().ok_or(CatalogError::Indexer)?);
                    let (indexer, target) = (*indexer, target.clone());
                    Box::new(move || {
                        client
                            .fetch(indexer, &target, &cancel)
                            .map(RemoteData::Sources)
                            .map_err(|_| {
                                if cancel.load(Ordering::Acquire) {
                                    CatalogError::Canceled
                                } else {
                                    CatalogError::Indexer
                                }
                            })
                    })
                }
                A::Tmdb(query) => {
                    crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
                    let client = self
                        .tmdb_client(|| crate::credentials::MetadataCredentialStore::new().get())?;
                    let query = query.clone();
                    Box::new(move || {
                        check_cancellation(Some(&cancel))?;
                        let (data, rich) = client
                            .fetch_normalized(&query)
                            .map_err(CatalogError::Tmdb)?;
                        Ok(RemoteData::Tmdb(data, rich))
                    })
                }
            },
        };
        Ok(Box::new(move || {
            let _span = work.diagnostic_span().entered();
            let result = network();
            if let Err(category) = &result {
                if *category == CatalogError::Canceled {
                    tracing::debug!(?category, "Catalog network work canceled");
                } else {
                    tracing::warn!(?category, "Catalog network or validation failed");
                }
            }
            Box::new(move |service: &mut Self| {
                let _span = work.diagnostic_span().entered();
                let result = match (result, &work) {
                    (Ok(RemoteData::Enrichment(data)), Work::Metadata(request)) => {
                        if let A::Enrich {
                            kind,
                            imdb,
                            priority,
                            ..
                        } = &request.action
                        {
                            service.set_provider_priority(
                                kinema_core::provider::ProviderPriority::new(
                                    priority.iter().copied(),
                                ),
                            );
                            service
                                .save_enrichment(*kind, imdb, data)
                                .map(|s| R::metadata(S::Enrichment(s)))
                        } else {
                            Err(CatalogError::InvalidResponse)
                        }
                    }
                    (Ok(RemoteData::Page(titles, rich)), Work::Search(request)) => service
                        .save_search(&request.query, titles, rich)
                        .map(|s| R::Search(crate::catalog_worker::snapshot(s))),
                    (Ok(RemoteData::Page(titles, rich)), Work::Metadata(request)) => {
                        match &request.action {
                            A::Browse { kind, offset } => service
                                .save_browse(*kind, *offset, titles, rich)
                                .map(|s| R::metadata(S::Browse(s))),
                            _ => Err(CatalogError::InvalidResponse),
                        }
                    }
                    (Ok(RemoteData::Details(details, rich)), Work::Metadata(request)) => {
                        match &request.action {
                            A::Details { kind, id } => service
                                .save_details(*kind, id, details, rich)
                                .map(|s| R::metadata(S::Details(s))),
                            _ => Err(CatalogError::InvalidResponse),
                        }
                    }
                    (Ok(RemoteData::Sources(results)), Work::Metadata(request)) => {
                        match &request.action {
                            A::Sources { indexer, target } => service
                                .save_sources(*indexer, target, results)
                                .map(|s| R::metadata(S::Sources(s))),
                            _ => Err(CatalogError::InvalidResponse),
                        }
                    }
                    (Ok(RemoteData::Tmdb(data, rich)), Work::Metadata(request)) => {
                        match &request.action {
                            A::Tmdb(query) => service
                                .save_tmdb_rich(query, data, &rich)
                                .map(|s| R::metadata(S::Tmdb(s))),
                            _ => Err(CatalogError::InvalidResponse),
                        }
                    }
                    (Err(error), _) => Err(error),
                    _ => Err(CatalogError::InvalidResponse),
                };
                if let Err(error) = result {
                    service.record_remote_failure(&work, error);
                }
                result.map_err(super::diagnostic_error)
            })
        }))
    }

    fn record_remote_failure(&mut self, work: &crate::catalog_worker::Work, error: CatalogError) {
        use crate::catalog_worker::{MetadataAction as A, Work};
        if self.ensure_active().is_err() {
            return;
        }
        if error == CatalogError::Tmdb(crate::tmdb::TmdbError::RejectedCredential) {
            tracing::debug!(
                provider = "tmdb",
                reason = "credential_rejection",
                "TMDB credential cache invalidated"
            );
            self.tmdb = None;
        }
        match work {
            Work::Search(request) => normalized_fetches::search(self, &request.query, error),
            Work::Metadata(request) => match &request.action {
                A::Browse { kind, offset } => {
                    normalized_fetches::browse(self, *kind, *offset, error)
                }
                A::Details { kind, id } => normalized_fetches::details(self, *kind, id, error),
                A::Tmdb(query) => normalized_fetches::tmdb(self, query, error),
                A::Sources { .. } | A::Enrich { .. } => (),
            },
        }
    }
}

#[cfg(test)]
mod cinemeta_details_tests {
    use super::*;

    #[test]
    fn tmdb_client_caches_secret_and_reloads_on_store_changes_and_401() -> Result<(), Box<dyn Error>>
    {
        let _guard = crate::credentials::METADATA_TEST_LOCK
            .lock()
            .map_err(|_| "metadata test lock")?;
        use crate::credentials::{CredentialError, SecretString};
        use std::{
            io::{Read, Write},
            net::TcpListener,
        };
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let store = crate::credentials::test_metadata_store();
        assert!(matches!(
            service.tmdb_client(|| store.get()),
            Err(CatalogError::TmdbMissingCredential)
        ));
        store.set(SecretString::new("first-token".into()))?;
        let first = service.tmdb_client(|| store.get())?;
        let cached = service.tmdb_client(|| Err(CredentialError::AccessDenied))?;
        assert!(
            Arc::ptr_eq(&first, &cached),
            "cached client must avoid secure storage"
        );
        store.set(SecretString::new("second-token".into()))?;
        let second = service.tmdb_client(|| store.get())?;
        assert!(!Arc::ptr_eq(&first, &second));
        store.delete()?;
        assert!(matches!(
            service.tmdb_client(|| store.get()),
            Err(CatalogError::TmdbMissingCredential)
        ));
        assert!(matches!(
            service.tmdb_client(|| Err(CredentialError::AccessDenied)),
            Err(CatalogError::TmdbCredentialDenied)
        ));
        assert!(matches!(
            service.tmdb_client(|| Err(CredentialError::Unavailable)),
            Err(CatalogError::TmdbCredentialUnavailable)
        ));
        // Inject only a loopback transport; no OS credential store or real provider.
        let listener = TcpListener::bind("127.0.0.1:0")?;
        let client = Arc::new(crate::tmdb::TmdbClient::at(
            SecretString::new("rejected-token".into()),
            Url::parse(&format!("http://{}/3/", listener.local_addr()?))?,
        )?);
        service.tmdb = Some((
            crate::credentials::metadata_generation(),
            Arc::clone(&client),
        ));
        let server = std::thread::spawn(move || -> std::io::Result<()> {
            let (mut stream, _) = listener.accept()?;
            let mut request = [0; 4096];
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let count = stream.read(&mut request)?;
            assert!(count > 0);
            stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}",
            )
        });
        assert!(matches!(
            client.check_account(),
            Err(crate::tmdb::TmdbError::RejectedCredential)
        ));
        server.join().map_err(|_| "server panic")??;
        assert!(client.credential_rejected());
        let next = service.tmdb_client(|| Ok(SecretString::new("fresh-token".into())))?;
        assert!(!Arc::ptr_eq(&client, &next));
        assert!(Arc::ptr_eq(
            &next,
            &service.tmdb_client(|| Err(CredentialError::AccessDenied))?
        ));
        Ok(())
    }

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

fn cinemeta_facts(
    raw: &serde_json::Value,
    kind: MediaKind,
) -> Result<kinema_core::catalog::TitleFacts, CatalogError> {
    let mut payload = serde_json::Map::new();
    if let Some(value) = raw
        .get("imdbRating")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        let value = value
            .as_f64()
            .or_else(|| value.as_str()?.parse().ok())
            .ok_or(CatalogError::InvalidResponse)?;
        payload.insert("imdb_rating".into(), serde_json::json!(value));
    }
    if let Some(runtime) = raw
        .get("runtime")
        .filter(|v| !v.is_null() && v.as_str() != Some(""))
    {
        let minutes = runtime
            .as_u64()
            .or_else(|| {
                runtime
                    .as_str()?
                    .trim()
                    .strip_suffix("min")
                    .unwrap_or(runtime.as_str()?)
                    .trim()
                    .parse()
                    .ok()
            })
            .ok_or(CatalogError::InvalidResponse)?;
        payload.insert("runtime_minutes".into(), minutes.into());
    }
    for (source, target) in [
        ("director", "directors"),
        ("writer", "writers"),
        ("country", "countries"),
    ] {
        if let Some(v) = raw.get(source).filter(|v| !v.is_null()) {
            let values = if let Some(s) = v.as_str() {
                s.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| serde_json::Value::from(s.to_owned()))
                    .collect()
            } else {
                v.as_array().ok_or(CatalogError::InvalidResponse)?.clone()
            };
            payload.insert(target.into(), serde_json::Value::Array(values));
        }
    }
    if let Some(cast) = raw.get("cast").filter(|v| !v.is_null()) {
        let cast = cast
            .as_array()
            .ok_or(CatalogError::InvalidResponse)?
            .iter()
            .take(10)
            .map(|v| {
                v.as_str()
                    .map(|s| (s, Option::<String>::None))
                    .ok_or(CatalogError::InvalidResponse)
            })
            .collect::<Result<Vec<_>, _>>()?;
        payload.insert("cast".into(), serde_json::json!(cast));
    }
    if let Some(info) = raw
        .get("releaseInfo")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        payload.insert("release_info".into(), info.into());
        if kinema_core::catalog_normalized::valid_date(info) {
            payload.insert(
                if kind == MediaKind::Movie {
                    "release_date"
                } else {
                    "first_air_date"
                }
                .into(),
                info.into(),
            );
        }
    }
    if let Some(released) = raw
        .get("released")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        let date = released
            .get(..10)
            .filter(|s| kinema_core::catalog_normalized::valid_date(s))
            .ok_or(CatalogError::InvalidResponse)?;
        payload.insert(
            if kind == MediaKind::Movie {
                "release_date"
            } else {
                "first_air_date"
            }
            .into(),
            date.into(),
        );
    }
    serde_json::from_value::<crate::tmdb::FactsPayload>(payload.into())
        .map_err(|_| CatalogError::InvalidResponse)?
        .normalize()
        .map_err(|_| CatalogError::InvalidResponse)
}

#[cfg(test)]
mod facts_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn cinemeta_facts_mapping_and_normalized_cache_round_trip() -> Result<(), Box<dyn Error>> {
        let raw = json!({"meta": {"id": "tt1234567", "type": "movie", "name": "Movie", "imdbRating": "8.1", "runtime": "123 min", "director": ["Director"], "writer": ["Writer"], "cast": ["Actor"], "country": "US, Canada", "releaseInfo": "2020-01-02"}});
        let id = ImdbId::new("tt1234567")?;
        let (details, _) = parse_details(raw, MediaKind::Movie, &id)?;
        assert_eq!(details.facts.runtime_minutes, Some(123));
        assert_eq!(
            details
                .facts
                .imdb_rating
                .map(kinema_core::catalog::ImdbRating::value),
            Some(8.1)
        );
        assert_eq!(details.facts.directors, ["Director"]);
        assert_eq!(details.facts.countries, ["US", "Canada"]);
        assert_eq!(details.facts.cast[0].character, None);
        let mut service = CatalogService::open(std::path::Path::new(":memory:"))?;
        let tx = service.connection.transaction()?;
        normalized_pages::write_cinemeta_details(&tx, &details, 1, true, &Default::default())?;
        tx.commit()?;
        let saved = service
            .read_details(MediaKind::Movie, &id)?
            .ok_or("missing cache")?;
        assert_eq!(saved.details.facts, details.facts);
        service
            .connection
            .execute("UPDATE metadata_fetches SET facts_json=NULL", [])?;
        let old = service
            .read_details(MediaKind::Movie, &id)?
            .ok_or("missing old cache")?;
        assert_eq!(old.details.facts, Default::default());
        service.connection.execute(
            "UPDATE metadata_fetches SET facts_json=?1",
            params![r#"{"imdb_rating":11}"#],
        )?;
        assert_eq!(
            service.read_details(MediaKind::Movie, &id),
            Err(CatalogError::InvalidCache)
        );
        Ok(())
    }
}

#[cfg(test)]
mod facts_migration_tests {
    use super::*;
    #[test]
    fn v4_upgrade_preserves_detail_rows_with_empty_facts() -> Result<(), Box<dyn Error>> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog.sqlite3");
        let mut service = CatalogService::open(&path)?;
        let id = ImdbId::new("tt1234567")?;
        let (details, _) = parse_details(
            serde_json::json!({"meta": {"id": "tt1234567", "type": "movie", "name": "Preserved"}}),
            MediaKind::Movie,
            &id,
        )?;
        let tx = service.connection.transaction()?;
        normalized_pages::write_cinemeta_details(&tx, &details, 123, true, &Default::default())?;
        tx.commit()?;
        service.connection.execute_batch(
            "DROP TABLE library_entries;
             DROP TABLE movie_watch_state;
             DROP TABLE episode_watch_state;
             ALTER TABLE metadata_fetches DROP COLUMN facts_json;
             PRAGMA user_version=4;",
        )?;
        drop(service);
        let mut upgraded = CatalogService::open(&path)?;
        let saved = upgraded
            .read_details(MediaKind::Movie, &id)?
            .ok_or("missing details")?;
        assert!(upgraded.metadata_needs_refresh(
            &crate::catalog_worker::MetadataAction::Details {
                kind: MediaKind::Movie,
                id: id.clone()
            }
        )?);
        assert_eq!(saved.details.title.name(), "Preserved");
        assert_eq!(saved.fetched_at, 123);
        assert_eq!(saved.details.facts, Default::default());
        assert_eq!(
            upgraded
                .connection
                .pragma_query_value(None, "user_version", |r| r.get::<_, i64>(0))?,
            SCHEMA_VERSION
        );
        Ok(())
    }
}

pub(crate) fn provider_cinemeta(
    client: &Client,
    endpoint: &Url,
    kind: MediaKind,
    imdb: &ImdbId,
    cancel: &AtomicBool,
) -> Result<Option<crate::metadata_providers::ProviderTitleDetails>, CatalogError> {
    use crate::metadata_providers::{Payload, ProviderTitleDetails};
    use kinema_core::catalog_normalized::{FieldValue, Quality};
    let mut url = endpoint.clone();
    url.path_segments_mut()
        .map_err(|_| CatalogError::Network)?
        .pop_if_empty()
        .extend(["meta", kind_text(kind)])
        .push(&format!("{}.json", imdb.as_str()));
    let raw: serde_json::Value = match fetch_json(client, url, cancel) {
        Ok(raw) => raw,
        Err(CatalogError::NotFound) => return Ok(None),
        Err(error) => return Err(error),
    };
    if raw.get("meta").is_none_or(serde_json::Value::is_null) {
        return Ok(None);
    }
    let (details, rich) = parse_details(raw, kind, imdb)?;
    let mut observation = normalized::imdb_observation(&details.title);
    for (field, value) in [
        ("poster_url", &details.poster_url),
        ("backdrop_url", &details.background_url),
    ] {
        if let Some(value) = value {
            observation
                .fields
                .push((field.into(), FieldValue::Text(value.clone())));
        }
    }
    if !details.genres.is_empty() {
        observation.fields.push((
            "genres_json".into(),
            FieldValue::Labels(details.genres.clone()),
        ));
    }
    for record in rich
        .iter()
        .filter(|r| r.season.is_none() && r.episode.is_none())
    {
        observation.fields.extend(record.fields.clone());
        observation.ratings.extend(record.ratings.clone());
        observation.identifiers.extend(record.identifiers.clone());
    }
    Ok(Some(ProviderTitleDetails {
        observation,
        quality: Quality::Detail,
        payload: Payload::Cinemeta(details),
        rich,
    }))
}

impl CatalogService {
    /// Reconstruct details even when only TMDB has acquired the title. Optional
    /// legacy detail facts are gap-filled in priority order; normalized fields
    /// are authoritative for facts represented by the canonical schema.
    fn enriched_details(
        &mut self,
        record: &kinema_core::catalog_normalized::CanonicalTitle,
        title: CatalogTitle,
    ) -> Result<CachedDetails, CatalogError> {
        use kinema_core::catalog::{ImdbRating, TitleFacts};
        let tx = self.connection.transaction()?;
        let mut facts = TitleFacts::default();
        for provider in self.provider_priority.order() {
            let resource: Option<String> = tx.query_row("SELECT resource_key FROM metadata_fetches WHERE title_id=?1 AND provider=?2 AND facts_json IS NOT NULL ORDER BY fetched_at DESC LIMIT 1", params![record.id, provider.as_str()], |r| r.get(0)).optional()?;
            if let Some(resource) = resource {
                let incoming =
                    normalized_pages::read_facts(&tx, record.id, provider.as_str(), &resource)?;
                macro_rules! optional { ($($field:ident),*) => { $(if facts.$field.is_none() { facts.$field = incoming.$field; })* }; }
                macro_rules! lists { ($($field:ident),*) => { $(if facts.$field.is_empty() { facts.$field = incoming.$field; })* }; }
                optional!(
                    tmdb_rating,
                    tmdb_vote_count,
                    imdb_rating,
                    runtime_minutes,
                    status,
                    release_date,
                    first_air_date,
                    last_air_date,
                    certification,
                    original_language,
                    tagline,
                    release_info
                );
                lists!(
                    directors, creators, cast, studios, networks, writers, countries
                );
            }
        }
        facts.runtime_minutes = record
            .integer("runtime_minutes")
            .and_then(|v| u32::try_from(v).ok())
            .or(facts.runtime_minutes);
        for (field, slot) in [
            ("status", &mut facts.status),
            ("release_date", &mut facts.release_date),
            ("first_air_date", &mut facts.first_air_date),
            ("last_air_date", &mut facts.last_air_date),
            ("original_language", &mut facts.original_language),
            ("tagline", &mut facts.tagline),
        ] {
            if let Some(value) = record.text(field) {
                *slot = Some(value.into());
            }
        }
        for source in ["imdb", "tmdb"] {
            let rating = record
                .ratings
                .iter()
                .filter(|r| r.observation.source == source && r.observation.metric == "score")
                .min_by_key(|r| {
                    kinema_core::provider::ProviderId::parse(&r.observation.supplied_by)
                        .map_or(usize::MAX, |p| self.provider_priority.rank(p))
                });
            if let Some(rating) = rating {
                let score = rating.observation.value.and_then(ImdbRating::new);
                if source == "imdb" {
                    facts.imdb_rating = score;
                } else {
                    facts.tmdb_rating = score;
                    facts.tmdb_vote_count = rating.observation.vote_count;
                }
            }
        }
        let prefix = if record.kind == MediaKind::Movie {
            "release"
        } else {
            "first_air"
        };
        let release_info = record
            .text(&format!("{prefix}_date"))
            .map(str::to_owned)
            .or_else(|| {
                record
                    .integer(&format!("{prefix}_year"))
                    .map(|v| v.to_string())
            });
        Ok(CachedDetails {
            details: TitleDetails {
                facts,
                poster_url: record.text("poster_url").map(str::to_owned),
                background_url: record.text("backdrop_url").map(str::to_owned),
                release_info,
                genres: title.genres().to_vec(),
                title,
                episodes: normalized::read_episodes(&tx, record.id, None)?,
            },
            fetched_at: u64::try_from(record.updated_at).map_err(|_| CatalogError::InvalidCache)?,
        })
    }

    fn card_fields_complete(
        &mut self,
        kind: MediaKind,
        imdb: &ImdbId,
    ) -> Result<bool, CatalogError> {
        let tx = self.connection.transaction()?;
        let Some(id) = normalized::title_id(&tx, kind, "imdb", imdb.as_str())? else {
            return Ok(false);
        };
        let record = Self::title_record(&tx, id)?;
        Ok(complete_card(
            &Self::display_title(&record)?,
            record.ratings.iter().any(|r| r.observation.value.is_some()),
        ))
    }

    fn save_enrichment(
        &mut self,
        kind: MediaKind,
        imdb: &[ImdbId],
        mut data: EnrichmentData,
    ) -> Result<crate::catalog_worker::EnrichmentSnapshot, CatalogError> {
        use crate::metadata_providers::Payload;
        use kinema_core::provider::ProviderId;
        for value in data.values {
            self.ensure_active()?;
            // Observations are normalized and validated before crossing workers;
            // the established imports also retain child episodes and rich facts.
            if !value.observation.identifiers.iter().all(|id| id.valid()) {
                continue;
            }
            debug_assert_eq!(
                value.quality,
                kinema_core::catalog_normalized::Quality::Detail
            );
            let (provider, result) = match value.payload {
                Payload::Cinemeta(details) => {
                    let id = details.title.id().clone();
                    (
                        ProviderId::Cinemeta,
                        self.save_details(kind, &id, details, value.rich)
                            .map(|_| ()),
                    )
                }
                Payload::Tmdb(data) => {
                    let crate::tmdb::TmdbData::Details(details) = &data else {
                        continue;
                    };
                    let query = crate::tmdb::TmdbQuery::Details {
                        kind,
                        id: details.title.id,
                    };
                    (
                        ProviderId::Tmdb,
                        self.save_tmdb_rich(&query, data, &value.rich).map(|_| ()),
                    )
                }
            };
            if let Err(category) = result {
                if category == CatalogError::Canceled {
                    return Err(category);
                }
                tracing::debug!(
                    provider = provider.as_str(),
                    ?category,
                    "Optional metadata import failed"
                );
                if !data.failed.contains(&provider) {
                    data.failed.push(provider);
                }
            }
        }
        let mut titles = Vec::new();
        let mut seen = HashSet::new();
        for external in imdb.iter().take(20) {
            if !seen.insert(external) {
                continue;
            }
            self.ensure_active()?;
            let record = {
                let tx = self.connection.transaction()?;
                let Some(id) = normalized::title_id(&tx, kind, "imdb", external.as_str())? else {
                    continue;
                };
                Self::title_record(&tx, id)?
            };
            let mut contributors = Vec::new();
            for supplier in record
                .provenance
                .iter()
                .filter(|p| record.fields.contains_key(&p.field_name))
                .map(|p| p.supplied_by.as_str())
                .chain(
                    record
                        .ratings
                        .iter()
                        .map(|r| r.observation.supplied_by.as_str()),
                )
            {
                if let Some(provider) = ProviderId::parse(supplier)
                    && !contributors.contains(&provider)
                {
                    contributors.push(provider);
                }
            }
            contributors.sort_by_key(|p| self.provider_priority.rank(*p));
            let title = Self::display_title(&record)?;
            let details = self.enriched_details(&record, title.clone())?;
            titles.push(crate::catalog_worker::EnrichedTitle {
                title,
                fields: record.fields,
                details,
                contributors,
            });
        }
        Ok(crate::catalog_worker::EnrichmentSnapshot {
            titles,
            failed: data.failed,
            inactive: data.inactive,
        })
    }
}

fn complete_card(title: &CatalogTitle, has_rating: bool) -> bool {
    title.poster_url().is_some()
        && title.year().is_some()
        && has_rating
        && title.runtime_minutes().is_some()
        && !title.genres().is_empty()
}

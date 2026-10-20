//! Blocking TMDB v3 adapter; call only on an owned background worker.
//! No Cinemeta fallback or name-based IMDb matching. Sources require `details.imdb`.
//! Snapshots contain normalized public metadata only. The cache owner must bind them
//! to `cache_query_identity`: a page alone cannot encode its search/browse identity.

use std::{collections::HashSet, fmt, io::Read, time::Duration};

use kinema_core::{
    MediaKind,
    catalog::{CatalogDisplay, CatalogEpisode, ImdbId, SearchQuery},
};
use reqwest::{
    Url,
    blocking::Client,
    header::{AUTHORIZATION, HeaderValue},
    redirect::Policy,
};
use serde::{Deserialize, Serialize};

use crate::credentials::SecretString;

pub const CACHE_QUERY_IDENTITY: &str = "tmdb-v3:normalized-v1:language=en-US";
const LANGUAGE: &str = "en-US";
const ENDPOINT: &str = "https://api.themoviedb.org/3/";
const IMAGE_PREFIX: &str = "https://image.tmdb.org/t/p/w500";
const MAX_BYTES: u64 = 2 * 1024 * 1024;
const MAX_RESULTS: usize = 20;
const MAX_EPISODES: usize = 1000;
const MAX_SEASONS: usize = 1000;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum TmdbQuery {
    Browse { kind: MediaKind, page: u32 },
    Search { query: SearchQuery, page: u32 },
    Details { kind: MediaKind, id: u64 },
    Similar { id: u64, page: u32 },
    Season { series_id: u64, season: u32 },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbTitle {
    pub id: u64,
    pub kind: MediaKind,
    pub name: String,
    pub overview: Option<String>,
    pub release_info: Option<String>,
    pub poster_url: Option<String>,
    /// Retained display facts from canonical reads, not automatic enrichment.
    pub display: std::sync::Arc<CatalogDisplay>,
    /// Explicit retained identifier mapping, independent from score availability.
    pub imdb: Option<ImdbId>,
}
impl TmdbTitle {
    pub fn year(&self) -> Option<u16> {
        self.display
            .year
            .or_else(|| self.release_info.as_deref()?.get(..4)?.parse().ok())
    }
    pub fn poster_url(&self) -> Option<&str> {
        self.poster_url.as_deref()
    }
    pub fn imdb_rating(&self) -> Option<f32> {
        self.display
            .imdb_rating
            .map(kinema_core::catalog::ImdbRating::value)
    }
    pub fn runtime_minutes(&self) -> Option<u16> {
        self.display.runtime_minutes
    }
    pub fn genres(&self) -> &[String] {
        &self.display.genres
    }
    pub fn season_count(&self) -> Option<u32> {
        self.display.season_count
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbPage {
    pub titles: Vec<TmdbTitle>,
    pub page: u32,
    pub total_pages: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbSeasonSummary {
    pub number: u32,
    pub name: Option<String>,
    pub episode_count: u32,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbDetails {
    pub backdrop_url: Option<String>,
    pub title: TmdbTitle,
    pub imdb: Option<ImdbId>,
    pub genres: Vec<String>,
    pub seasons: Vec<TmdbSeasonSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbSeason {
    pub poster_url: Option<String>,
    pub series_id: u64,
    pub season: u32,
    pub episodes: Vec<CatalogEpisode>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TmdbData {
    Page(TmdbPage),
    Details(TmdbDetails),
    Season(TmdbSeason),
}

/// Deliberately excludes URLs, response bodies, tokens and underlying errors.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TmdbError {
    InvalidCredential,
    InvalidQuery,
    RejectedCredential,
    Network,
    RateLimited,
    HttpStatus,
    Malformed,
    TooLarge,
}
impl fmt::Display for TmdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCredential => "TMDB credential is invalid.",
            Self::InvalidQuery => "TMDB query is invalid.",
            Self::RejectedCredential => "TMDB rejected the credential.",
            Self::Network => "TMDB could not be reached or timed out.",
            Self::RateLimited => "TMDB rate limit reached; retry later.",
            Self::HttpStatus => "TMDB returned an unsuccessful response.",
            Self::Malformed => "TMDB metadata is invalid.",
            Self::TooLarge => "TMDB metadata exceeds safety limits.",
        })
    }
}
impl std::error::Error for TmdbError {}

pub struct TmdbClient {
    rejected: std::sync::atomic::AtomicBool,
    client: Client,
    secret: SecretString,
    endpoint: Url,
}
impl fmt::Debug for TmdbClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("TmdbClient([REDACTED])")
    }
}
impl TmdbClient {
    pub fn new(secret: SecretString) -> Result<Self, TmdbError> {
        Self::with_transport(secret, Self::transport()?)
    }
    pub(crate) fn transport() -> Result<Client, TmdbError> {
        Self::client_builder()
            .build()
            .map_err(|_| TmdbError::Network)
    }
    pub(crate) fn with_transport(secret: SecretString, client: Client) -> Result<Self, TmdbError> {
        Self::at_with_transport(
            secret,
            Url::parse(ENDPOINT).map_err(|_| TmdbError::Network)?,
            client,
        )
    }
    #[cfg(test)]
    pub(crate) fn at(secret: SecretString, endpoint: Url) -> Result<Self, TmdbError> {
        Self::at_with_transport(secret, endpoint, Self::transport()?)
    }
    fn at_with_transport(
        secret: SecretString,
        endpoint: Url,
        client: Client,
    ) -> Result<Self, TmdbError> {
        if secret.expose().is_empty()
            || secret.expose().len() > 4096
            || !secret.expose().bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(TmdbError::InvalidCredential);
        }
        Ok(Self {
            rejected: std::sync::atomic::AtomicBool::new(false),
            client,
            secret,
            endpoint,
        })
    }
    pub(crate) fn credential_rejected(&self) -> bool {
        self.rejected.load(std::sync::atomic::Ordering::Acquire)
    }
    fn client_builder() -> reqwest::blocking::ClientBuilder {
        Client::builder()
            // System DNS can spend one 5-second retry before returning valid
            // addresses. Allow that retry plus TCP/TLS, as for Cinemeta; keep
            // the total deadline bounded and never retry authenticated requests.
            .connect_timeout(Duration::from_secs(10))
            .timeout(Duration::from_secs(15))
            // Never forward bearer credentials to redirects, including another host.
            .redirect(Policy::none())
            .user_agent("Kinema-Next/0.1 TMDB")
    }
    fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> Result<T, TmdbError> {
        let mut url = self
            .endpoint
            .join(path)
            .map_err(|_| TmdbError::InvalidQuery)?;
        url.query_pairs_mut()
            .append_pair("language", LANGUAGE)
            .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
        let mut authorization = HeaderValue::from_str(&format!("Bearer {}", self.secret.expose()))
            .map_err(|_| TmdbError::InvalidCredential)?;
        authorization.set_sensitive(true);
        let response = self
            .client
            .get(url)
            .header(AUTHORIZATION, authorization)
            .send()
            .map_err(|_| TmdbError::Network)?;
        match response.status().as_u16() {
            401 | 403 => {
                self.rejected
                    .store(true, std::sync::atomic::Ordering::Release);
                return Err(TmdbError::RejectedCredential);
            }
            429 => return Err(TmdbError::RateLimited),
            200..=299 => (),
            _ => return Err(TmdbError::HttpStatus),
        }
        if response.content_length().is_some_and(|n| n > MAX_BYTES) {
            return Err(TmdbError::TooLarge);
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| TmdbError::Network)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(TmdbError::TooLarge);
        }
        serde_json::from_slice(&bytes).map_err(|_| TmdbError::Malformed)
    }
    /// Official v3 validation endpoint; no account profile is requested or returned.
    pub fn check_account(&self) -> Result<(), TmdbError> {
        #[derive(Deserialize)]
        struct Authentication {
            success: bool,
        }
        let result: Authentication = self.get("authentication", &[])?;
        if result.success {
            Ok(())
        } else {
            Err(TmdbError::RejectedCredential)
        }
    }
    #[cfg(test)]
    pub fn fetch(&self, query: &TmdbQuery) -> Result<TmdbData, TmdbError> {
        self.fetch_normalized(query).map(|(data, _)| data)
    }
    pub(crate) fn fetch_normalized(
        &self,
        query: &TmdbQuery,
    ) -> Result<(TmdbData, Vec<crate::catalog::normalized_rich::RichRecord>), TmdbError> {
        validate_query(query)?;
        let rich;
        let data = match query {
            TmdbQuery::Browse { kind, page } => {
                let raw: serde_json::Value = self.get(
                    &format!("{}/popular", kind_path(*kind)),
                    &[("page", page.to_string())],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let dto: PageDto = serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                TmdbData::Page(dto.normalize(*kind)?)
            }
            TmdbQuery::Search { query, page } => {
                let raw: serde_json::Value = self.get(
                    &format!("search/{}", kind_path(query.kind())),
                    &[
                        ("query", query.text().to_owned()),
                        ("page", page.to_string()),
                        ("include_adult", "false".into()),
                    ],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(
                    &raw,
                    &TmdbQuery::Search {
                        query: query.clone(),
                        page: *page,
                    },
                )
                .map_err(|_| TmdbError::Malformed)?;
                let dto: PageDto = serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                TmdbData::Page(dto.normalize(query.kind())?)
            }
            TmdbQuery::Similar { id, page } => {
                let raw: serde_json::Value = self.get(
                    &format!("movie/{id}/similar"),
                    &[("page", page.to_string())],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let dto: PageDto = serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                TmdbData::Page(dto.normalize(MediaKind::Movie)?)
            }
            TmdbQuery::Details { kind, id } => {
                let raw: serde_json::Value = self.get(
                    &format!("{}/{id}", kind_path(*kind)),
                    &[("append_to_response", "external_ids".into())],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let dto: DetailDto =
                    serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                TmdbData::Details(dto.normalize(*kind)?)
            }
            TmdbQuery::Season { series_id, season } => {
                let raw: serde_json::Value =
                    self.get(&format!("tv/{series_id}/season/{season}"), &[])?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let dto: SeasonDto =
                    serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                if dto
                    .episodes
                    .iter()
                    .any(|e| e.still_path.as_ref().is_some_and(|p| !valid_image_path(p)))
                {
                    return Err(TmdbError::Malformed);
                }
                TmdbData::Season(TmdbSeason {
                    poster_url: image_url(dto.poster_path)?,
                    series_id: *series_id,
                    season: dto.season_number,
                    episodes: dto
                        .episodes
                        .into_iter()
                        .map(EpisodeDto::normalize)
                        .collect::<Result<Vec<_>, _>>()?,
                })
            }
        };
        validate_data(&data, Some(query))?;
        Ok((data, rich))
    }
}
fn kind_path(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Series => "tv",
    }
}
fn validate_query(query: &TmdbQuery) -> Result<(), TmdbError> {
    let valid = match query {
        TmdbQuery::Browse { page, .. } | TmdbQuery::Search { page, .. } => (1..=500).contains(page),
        TmdbQuery::Details { id, .. } => *id > 0,
        TmdbQuery::Similar { id, page } => *id > 0 && (1..=500).contains(page),
        TmdbQuery::Season { series_id, season } => *series_id > 0 && *season <= 1000,
    };
    if valid {
        Ok(())
    } else {
        Err(TmdbError::InvalidQuery)
    }
}
/// Use as the SQLite query key; preserves search case, kind, page, and fixed locale.
pub fn cache_query_identity(query: &TmdbQuery) -> Result<String, TmdbError> {
    validate_query(query)?;
    let key = match query {
        TmdbQuery::Browse { kind, page } => format!("browse:{}:{page}", kind_path(*kind)),
        TmdbQuery::Search { query, page } => {
            format!("search:{}:{page}:{}", kind_path(query.kind()), query.text())
        }
        TmdbQuery::Details { kind, id } => format!("details:{}:{id}", kind_path(*kind)),
        TmdbQuery::Similar { id, page } => format!("similar:movie:{id}:{page}"),
        TmdbQuery::Season { series_id, season } => format!("season:{series_id}:{season}"),
    };
    Ok(format!("{CACHE_QUERY_IDENTITY}:{key}"))
}

#[derive(Deserialize)]
struct TitleDto {
    id: u64,
    title: Option<String>,
    name: Option<String>,
    overview: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    poster_path: Option<String>,
}
impl TitleDto {
    fn normalize(self, kind: MediaKind) -> Result<TmdbTitle, TmdbError> {
        let (name, release_info) = match kind {
            MediaKind::Movie => (self.title, self.release_date),
            MediaKind::Series => (self.name, self.first_air_date),
        };
        let poster_url = self
            .poster_path
            .map(|path| {
                if valid_image_path(&path) {
                    Ok(format!("{IMAGE_PREFIX}{path}"))
                } else {
                    Err(TmdbError::Malformed)
                }
            })
            .transpose()?;
        Ok(TmdbTitle {
            id: self.id,
            kind,
            name: name.ok_or(TmdbError::Malformed)?.trim().to_owned(),
            overview: nonempty(self.overview),
            release_info: nonempty(release_info),
            poster_url,
            display: Default::default(),
            imdb: None,
        })
    }
}
fn nonempty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.trim().is_empty())
}
#[derive(Deserialize)]
struct PageDto {
    results: Vec<TitleDto>,
    page: u32,
    total_pages: u32,
}
impl PageDto {
    fn normalize(self, kind: MediaKind) -> Result<TmdbPage, TmdbError> {
        if self.results.len() > MAX_RESULTS {
            return Err(TmdbError::TooLarge);
        }
        Ok(TmdbPage {
            titles: self
                .results
                .into_iter()
                .map(|t| t.normalize(kind))
                .collect::<Result<_, _>>()?,
            page: self.page,
            total_pages: self.total_pages,
        })
    }
}
#[derive(Deserialize)]
struct ExternalIds {
    imdb_id: Option<String>,
}
#[derive(Deserialize)]
struct GenreDto {
    name: String,
}
#[derive(Deserialize)]
struct DetailDto {
    #[serde(flatten)]
    title: TitleDto,
    imdb_id: Option<String>,
    external_ids: Option<ExternalIds>,
    backdrop_path: Option<String>,
    genres: Vec<GenreDto>,
    #[serde(default)]
    seasons: Vec<SummaryDto>,
}
#[derive(Deserialize, Serialize)]
struct SummaryDto {
    season_number: u32,
    name: Option<String>,
    episode_count: u32,
}
impl DetailDto {
    fn normalize(self, kind: MediaKind) -> Result<TmdbDetails, TmdbError> {
        let direct = nonempty(self.imdb_id);
        let external = self.external_ids.and_then(|e| nonempty(e.imdb_id));
        if direct
            .as_ref()
            .zip(external.as_ref())
            .is_some_and(|(a, b)| a != b)
        {
            return Err(TmdbError::Malformed);
        }
        let imdb = direct
            .or(external)
            .map(|s| {
                if s.len() > 64 {
                    return Err(TmdbError::Malformed);
                }
                ImdbId::new(s).map_err(|_| TmdbError::Malformed)
            })
            .transpose()?;
        Ok(TmdbDetails {
            backdrop_url: image_url(self.backdrop_path)?,
            title: self.title.normalize(kind)?,
            imdb,
            genres: self.genres.into_iter().map(|g| g.name).collect(),
            seasons: self
                .seasons
                .into_iter()
                .map(|s| TmdbSeasonSummary {
                    number: s.season_number,
                    name: nonempty(s.name),
                    episode_count: s.episode_count,
                })
                .collect(),
        })
    }
}
#[derive(Deserialize)]
struct SeasonDto {
    poster_path: Option<String>,
    season_number: u32,
    episodes: Vec<EpisodeDto>,
}
#[derive(Deserialize, Serialize)]
struct EpisodeDto {
    still_path: Option<String>,
    runtime: Option<u32>,
    season_number: u32,
    episode_number: u32,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
}
impl EpisodeDto {
    fn normalize(self) -> Result<CatalogEpisode, TmdbError> {
        Ok(CatalogEpisode {
            still_url: image_url(self.still_path)?,
            runtime_minutes: self.runtime,
            season: self.season_number,
            number: self.episode_number,
            name: nonempty(self.name),
            overview: nonempty(self.overview),
            released: nonempty(self.air_date),
        })
    }
}
fn image_url(path: Option<String>) -> Result<Option<String>, TmdbError> {
    path.map(|p| {
        if valid_image_path(&p) {
            Ok(format!("{IMAGE_PREFIX}{p}"))
        } else {
            Err(TmdbError::Malformed)
        }
    })
    .transpose()
}
fn valid_optional_image(url: &Option<String>) -> bool {
    url.as_ref()
        .is_none_or(|s| s.strip_prefix(IMAGE_PREFIX).is_some_and(valid_image_path))
}
fn valid_image_path(path: &str) -> bool {
    path.len() <= 256
        && path.starts_with('/')
        && path.len() > 1
        && path[1..]
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        && !path.contains("..")
        && (path.ends_with(".jpg") || path.ends_with(".png"))
}
fn text(s: &str, max: usize, required: bool) -> bool {
    s.chars().count() <= max
        && (!required || !s.trim().is_empty())
        && !s
            .chars()
            .any(|c| c.is_control() && !matches!(c, '\n' | '\r' | '\t'))
}
fn optional_text(s: &Option<String>, max: usize) -> bool {
    s.as_ref().is_none_or(|s| text(s, max, true))
}
fn valid_title(t: &TmdbTitle) -> bool {
    t.id > 0
        && text(&t.name, 500, true)
        && optional_text(&t.overview, 20_000)
        && optional_text(&t.release_info, 64)
        && t.release_info.as_ref().is_none_or(|value| {
            kinema_core::catalog_normalized::valid_date(value)
                || (value.len() == 4
                    && value
                        .parse::<u32>()
                        .is_ok_and(|year| (1..=9999).contains(&year)))
        })
        && t.poster_url
            .as_ref()
            .is_none_or(|s| s.strip_prefix(IMAGE_PREFIX).is_some_and(valid_image_path))
}
fn validate_data(data: &TmdbData, query: Option<&TmdbQuery>) -> Result<(), TmdbError> {
    let valid = match data {
        TmdbData::Page(p) => {
            let mut ids = HashSet::new();
            p.page > 0
                && p.titles
                    .first()
                    .is_none_or(|first| p.titles.iter().all(|t| t.kind == first.kind))
                && p.page <= 500
                && p.total_pages <= 1_000_000
                && (p.titles.is_empty() || p.total_pages >= p.page)
                && p.titles.len() <= MAX_RESULTS
                && p.titles.iter().all(|t| valid_title(t) && ids.insert(t.id))
        }
        TmdbData::Details(d) => {
            let mut seasons = HashSet::new();
            valid_title(&d.title)
                && valid_optional_image(&d.backdrop_url)
                && d.imdb.as_ref().is_none_or(|i| i.as_str().len() <= 64)
                && d.genres.len() <= 100
                && d.genres.iter().all(|s| text(s, 500, true))
                && d.seasons.len() <= MAX_SEASONS
                && (d.title.kind != MediaKind::Movie || d.seasons.is_empty())
                && d.seasons.iter().all(|s| {
                    s.number <= 1000
                        && s.episode_count as usize <= MAX_EPISODES
                        && optional_text(&s.name, 500)
                        && seasons.insert(s.number)
                })
        }
        TmdbData::Season(s) => {
            let mut numbers = HashSet::new();
            valid_optional_image(&s.poster_url)
                && s.series_id > 0
                && s.season <= 1000
                && s.episodes.len() <= MAX_EPISODES
                && s.episodes.iter().all(|e| {
                    valid_optional_image(&e.still_url)
                        && e.runtime_minutes.is_none_or(|n| n <= 10_000)
                        && e.season == s.season
                        && e.number > 0
                        && e.number <= 1000
                        && numbers.insert(e.number)
                        && optional_text(&e.name, 500)
                        && optional_text(&e.overview, 20_000)
                        && optional_text(&e.released, 64)
                })
        }
    };
    if !valid {
        return Err(TmdbError::Malformed);
    }
    if let Some(query) = query {
        validate_query(query)?;
        let matches = match (data, query) {
            (TmdbData::Page(p), TmdbQuery::Similar { page, .. }) => {
                p.page == *page && p.titles.iter().all(|t| t.kind == MediaKind::Movie)
            }
            (TmdbData::Page(p), TmdbQuery::Browse { kind, page }) => {
                p.page == *page && p.titles.iter().all(|t| t.kind == *kind)
            }
            (TmdbData::Page(p), TmdbQuery::Search { query, page }) => {
                p.page == *page && p.titles.iter().all(|t| t.kind == query.kind())
            }
            (TmdbData::Details(d), TmdbQuery::Details { kind, id }) => {
                d.title.kind == *kind && d.title.id == *id
            }
            (TmdbData::Season(s), TmdbQuery::Season { series_id, season }) => {
                s.series_id == *series_id && s.season == *season
            }
            _ => false,
        };
        if !matches {
            return Err(TmdbError::Malformed);
        }
    }
    Ok(())
}

// Separate normalized snapshot DTOs: never serialize transport responses or secrets.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    identity: String,
    data: SnapshotData,
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "resource", deny_unknown_fields)]
enum SnapshotData {
    Page {
        titles: Vec<SnapshotTitle>,
        page: u32,
        total_pages: u32,
    },
    Details {
        #[serde(default)]
        backdrop_url: Option<String>,
        title: SnapshotTitle,
        imdb: Option<String>,
        genres: Vec<String>,
        seasons: Vec<SummaryDto>,
    },
    Season {
        #[serde(default)]
        poster_url: Option<String>,
        series_id: u64,
        season: u32,
        episodes: Vec<EpisodeDto>,
    },
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotTitle {
    id: u64,
    kind: String,
    name: String,
    overview: Option<String>,
    release_info: Option<String>,
    poster_url: Option<String>,
}
impl From<&TmdbTitle> for SnapshotTitle {
    fn from(t: &TmdbTitle) -> Self {
        Self {
            id: t.id,
            kind: kind_path(t.kind).into(),
            name: t.name.clone(),
            overview: t.overview.clone(),
            release_info: t.release_info.clone(),
            poster_url: t.poster_url.clone(),
        }
    }
}
impl SnapshotTitle {
    fn normalize(self) -> Result<TmdbTitle, TmdbError> {
        let kind = match self.kind.as_str() {
            "movie" => MediaKind::Movie,
            "tv" => MediaKind::Series,
            _ => return Err(TmdbError::Malformed),
        };
        Ok(TmdbTitle {
            id: self.id,
            kind,
            name: self.name,
            overview: self.overview,
            release_info: self.release_info,
            poster_url: self.poster_url,
            display: Default::default(),
            imdb: None,
        })
    }
}
pub fn encode(data: &TmdbData) -> Result<String, TmdbError> {
    validate_data(data, None)?;
    let data = match data {
        TmdbData::Page(p) => SnapshotData::Page {
            titles: p.titles.iter().map(Into::into).collect(),
            page: p.page,
            total_pages: p.total_pages,
        },
        TmdbData::Details(d) => SnapshotData::Details {
            backdrop_url: d.backdrop_url.clone(),
            title: (&d.title).into(),
            imdb: d.imdb.as_ref().map(|i| i.as_str().to_owned()),
            genres: d.genres.clone(),
            seasons: d
                .seasons
                .iter()
                .map(|s| SummaryDto {
                    season_number: s.number,
                    name: s.name.clone(),
                    episode_count: s.episode_count,
                })
                .collect(),
        },
        TmdbData::Season(s) => SnapshotData::Season {
            poster_url: s.poster_url.clone(),
            series_id: s.series_id,
            season: s.season,
            episodes: s
                .episodes
                .iter()
                .map(|e| EpisodeDto {
                    still_path: e
                        .still_url
                        .as_ref()
                        .and_then(|u| u.strip_prefix(IMAGE_PREFIX))
                        .map(str::to_owned),
                    runtime: e.runtime_minutes,
                    season_number: e.season,
                    episode_number: e.number,
                    name: e.name.clone(),
                    overview: e.overview.clone(),
                    air_date: e.released.clone(),
                })
                .collect(),
        },
    };
    let encoded = serde_json::to_string(&Snapshot {
        identity: CACHE_QUERY_IDENTITY.into(),
        data,
    })
    .map_err(|_| TmdbError::Malformed)?;
    if encoded.len() as u64 > MAX_BYTES {
        return Err(TmdbError::TooLarge);
    }
    Ok(encoded)
}
pub fn decode(encoded: &str, query: &TmdbQuery) -> Result<TmdbData, TmdbError> {
    validate_query(query)?;
    if encoded.len() as u64 > MAX_BYTES {
        return Err(TmdbError::TooLarge);
    }
    let snapshot: Snapshot = serde_json::from_str(encoded).map_err(|_| TmdbError::Malformed)?;
    if snapshot.identity != CACHE_QUERY_IDENTITY {
        return Err(TmdbError::Malformed);
    }
    let data = match snapshot.data {
        SnapshotData::Page {
            titles,
            page,
            total_pages,
        } => TmdbData::Page(TmdbPage {
            titles: titles
                .into_iter()
                .map(SnapshotTitle::normalize)
                .collect::<Result<_, _>>()?,
            page,
            total_pages,
        }),
        SnapshotData::Details {
            backdrop_url,
            title,
            imdb,
            genres,
            seasons,
        } => TmdbData::Details(TmdbDetails {
            backdrop_url,
            title: title.normalize()?,
            imdb: imdb
                .map(|s| ImdbId::new(s).map_err(|_| TmdbError::Malformed))
                .transpose()?,
            genres,
            seasons: seasons
                .into_iter()
                .map(|s| TmdbSeasonSummary {
                    number: s.season_number,
                    name: s.name,
                    episode_count: s.episode_count,
                })
                .collect(),
        }),
        SnapshotData::Season {
            poster_url,
            series_id,
            season,
            episodes,
        } => TmdbData::Season(TmdbSeason {
            poster_url,
            series_id,
            season,
            episodes: episodes
                .into_iter()
                .map(EpisodeDto::normalize)
                .collect::<Result<Vec<_>, _>>()?,
        }),
    };
    validate_data(&data, Some(query))?;
    Ok(data)
}

/// Decode real-shaped offline responses through production DTO normalization.
/// No client, token, filesystem, or transport is constructed.
#[cfg(feature = "ui-probe")]
pub(crate) fn decode_fixture_response(
    query: &TmdbQuery,
    payload: &str,
) -> Result<TmdbData, TmdbError> {
    validate_query(query)?;
    let raw: serde_json::Value = serde_json::from_str(payload).map_err(|_| TmdbError::Malformed)?;
    let data = match query {
        TmdbQuery::Details { kind, .. } => TmdbData::Details(
            serde_json::from_value::<DetailDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(*kind)?,
        ),
        TmdbQuery::Season { series_id, .. } => {
            let dto: SeasonDto = serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
            TmdbData::Season(TmdbSeason {
                series_id: *series_id,
                season: dto.season_number,
                poster_url: image_url(dto.poster_path)?,
                episodes: dto
                    .episodes
                    .into_iter()
                    .map(EpisodeDto::normalize)
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        TmdbQuery::Browse { kind, .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(*kind)?,
        ),
        TmdbQuery::Search { query, .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(query.kind())?,
        ),
        TmdbQuery::Similar { .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(MediaKind::Movie)?,
        ),
    };
    validate_data(&data, Some(query))?;
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    const TOKEN: &str = "private-test-bearer-token";
    fn ok<T, E: fmt::Debug>(r: Result<T, E>) -> T {
        r.unwrap_or_else(|e| panic!("unexpected error: {e:?}"))
    }
    fn local<T>(status: u16, body: &str, run: impl FnOnce(&TmdbClient) -> T) -> (T, String) {
        let listener = ok(TcpListener::bind("127.0.0.1:0"));
        let endpoint = ok(Url::parse(&format!(
            "http://{}/3/",
            ok(listener.local_addr())
        )));
        let body = body.to_owned();
        let server = thread::spawn(move || {
            let (mut stream, _) = ok(listener.accept());
            ok(stream.set_read_timeout(Some(Duration::from_secs(3))));
            let mut request = Vec::new();
            loop {
                let mut bytes = [0; 1024];
                let n = ok(stream.read(&mut bytes));
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
                assert!(request.len() < 16384);
            }
            // Oversized responses can be rejected before consuming the body.
            let _ = write!(
                stream,
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            ok(String::from_utf8(request))
        });
        let client = ok(TmdbClient::at(SecretString::new(TOKEN.into()), endpoint));
        assert!(!format!("{client:?}").contains(TOKEN));
        let result = run(&client);
        (result, ok(server.join()))
    }
    fn roundtrip(data: &TmdbData, query: &TmdbQuery) {
        let encoded = ok(encode(data));
        assert!(!encoded.contains(TOKEN));
        assert!(!encoded.contains("authorization"));
        assert!(!format!("{data:?}").contains(TOKEN));
        assert_eq!(ok(decode(&encoded, query)), *data);
        assert!(decode(&encoded.replace("en-US", "fr-FR"), query).is_err());
    }
    #[test]
    fn similar_uses_movie_scoped_endpoint_and_distinct_cache_identity() {
        let query = TmdbQuery::Similar { id: 91, page: 2 };
        let (result, request) = local(
            200,
            r#"{"page":2,"total_pages":3,"results":[{"id":42,"title":"Related","overview":"Supplied by provider","poster_path":"/related.jpg"}]}"#,
            |c| c.fetch(&query),
        );
        let data = ok(result);
        assert!(request.contains("/3/movie/91/similar?"));
        assert!(request.contains("page=2"));
        roundtrip(&data, &query);
        assert_ne!(
            ok(cache_query_identity(&query)),
            ok(cache_query_identity(&TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 2
            }))
        );
        assert_ne!(
            ok(cache_query_identity(&query)),
            ok(cache_query_identity(&TmdbQuery::Similar {
                id: 92,
                page: 2
            }))
        );
    }
    #[test]
    fn backdrop_season_poster_and_episode_still_survive_transport_and_snapshot() {
        let query = TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 91,
        };
        let (result, _) = local(
            200,
            r#"{"id":91,"name":"Series","backdrop_path":"/backdrop.jpg","external_ids":{"imdb_id":"tt1234567"},"genres":[],"seasons":[{"season_number":2,"name":"Season 2","episode_count":1}]}"#,
            |c| c.fetch(&query),
        );
        let data = ok(result);
        roundtrip(&data, &query);
        let TmdbData::Details(details) = data else {
            panic!("details")
        };
        assert_eq!(
            details.backdrop_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/backdrop.jpg")
        );
        let query = TmdbQuery::Season {
            series_id: 91,
            season: 2,
        };
        let (result, _) = local(
            200,
            r#"{"season_number":2,"poster_path":"/season.jpg","episodes":[{"season_number":2,"episode_number":1,"name":"One","runtime":48,"still_path":"/still.jpg"}]}"#,
            |c| c.fetch(&query),
        );
        let data = ok(result);
        roundtrip(&data, &query);
        let TmdbData::Season(season) = data else {
            panic!("season")
        };
        assert!(season.poster_url.is_some());
        assert!(season.episodes[0].still_url.is_some());
        assert_eq!(season.episodes[0].runtime_minutes, Some(48));
        let (result, _) = local(
            200,
            r#"{"season_number":2,"episodes":[{"season_number":2,"episode_number":1,"still_path":"/../secret.jpg"}]}"#,
            |c| c.fetch(&query),
        );
        assert!(matches!(result, Err(TmdbError::Malformed)));
    }
    #[test]
    fn connect_budget_allows_one_five_second_dns_retry() {
        struct SlowDns(std::net::SocketAddr);
        impl reqwest::dns::Resolve for SlowDns {
            fn resolve(&self, _name: reqwest::dns::Name) -> reqwest::dns::Resolving {
                let address = self.0;
                Box::pin(async move {
                    tokio::time::sleep(Duration::from_millis(5100)).await;
                    Ok(Box::new(std::iter::once(address)) as reqwest::dns::Addrs)
                })
            }
        }
        let listener = ok(TcpListener::bind("127.0.0.1:0"));
        let address = ok(listener.local_addr());
        ok(listener.set_nonblocking(true));
        let server = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(12);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "no connection after DNS"
                        );
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("local accept failed: {error}"),
                }
            };
            ok(stream.set_read_timeout(Some(Duration::from_secs(3))));
            let mut request = [0; 4096];
            assert!(ok(stream.read(&mut request)) > 0);
            ok(stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            ));
        });
        let client = ok(TmdbClient::client_builder()
            .no_proxy()
            .dns_resolver(std::sync::Arc::new(SlowDns(address)))
            .build());
        // A synthetic hostname forces DNS, without public networking or credentials.
        let response = client
            .get(format!("http://tmdb.test:{}/3/", address.port()))
            .send();
        ok(server.join());
        assert_eq!(ok(response).status(), reqwest::StatusCode::UNAUTHORIZED);
    }
    #[test]
    #[ignore = "requires live public TMDB HTTPS access; sends no credentials"]
    fn live_unauthenticated_transport_reaches_tmdb() {
        let client = ok(TmdbClient::client_builder().build());
        let started = std::time::Instant::now();
        // No TmdbClient/SecretString, Authorization header, or API key involved.
        let response = client
            .get("https://api.themoviedb.org/3/movie/popular?language=en-US")
            .send();
        match response {
            Ok(response) => {
                eprintln!(
                    "TMDB public probe: status={}, elapsed={:?}",
                    response.status(),
                    started.elapsed()
                );
                assert_eq!(response.status(), reqwest::StatusCode::UNAUTHORIZED);
            }
            Err(error) => panic!(
                "TMDB public probe: transport failure; timeout={}, connect={}, elapsed={:?}",
                error.is_timeout(),
                error.is_connect(),
                started.elapsed()
            ),
        }
    }
    #[test]
    fn browse_and_search_are_encoded_with_explicit_locale() {
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 2,
        };
        let (data, request) = local(
            200,
            r#"{"page":2,"total_pages":4,"results":[{"id":11,"title":" Movie ","poster_path":"/abc.jpg","release_date":"2026-01-01"}]}"#,
            |c| ok(c.fetch(&query)),
        );
        assert!(request.starts_with("GET /3/movie/popular?language=en-US&page=2 "));
        assert!(request.contains(&format!("authorization: Bearer {TOKEN}")));
        assert!(!request.contains("api_key"));
        roundtrip(&data, &query);
        let TmdbData::Page(page) = &data else {
            panic!("expected page")
        };
        assert_eq!(page.titles[0].name, "Movie");
        assert_eq!(
            page.titles[0].poster_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/abc.jpg")
        );
        assert!(
            decode(
                &ok(encode(&data)),
                &TmdbQuery::Browse {
                    kind: MediaKind::Series,
                    page: 2
                }
            )
            .is_err()
        );
        assert!(
            decode(
                &ok(encode(&data)),
                &TmdbQuery::Browse {
                    kind: MediaKind::Movie,
                    page: 1
                }
            )
            .is_err()
        );
        let query = TmdbQuery::Search {
            query: ok(SearchQuery::new(MediaKind::Series, "a/b & 映?")),
            page: 1,
        };
        let (data, request) = local(200, r#"{"page":1,"total_pages":0,"results":[]}"#, |c| {
            ok(c.fetch(&query))
        });
        let path = ok(request.lines().next().ok_or("request"))
            .split_whitespace()
            .nth(1)
            .unwrap_or("");
        let url = ok(Url::parse(&format!("http://localhost{path}")));
        assert_eq!(url.path(), "/3/search/tv");
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "query" && v == "a/b & 映?")
        );
        assert!(
            url.query_pairs()
                .any(|(k, v)| k == "language" && v == LANGUAGE)
        );
        roundtrip(&data, &query);
    }
    #[test]
    fn details_allow_absent_imdb_and_preserve_series_mapping() {
        let movie = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 12,
        };
        let (data, request) = local(
            200,
            r#"{"id":12,"title":"Film","imdb_id":null,"external_ids":{"imdb_id":null},"genres":[]}"#,
            |c| ok(c.fetch(&movie)),
        );
        assert!(
            request.starts_with("GET /3/movie/12?language=en-US&append_to_response=external_ids ")
        );
        let TmdbData::Details(details) = &data else {
            panic!("expected details")
        };
        assert_eq!(details.imdb, None);
        roundtrip(&data, &movie);
        let series = TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 33,
        };
        let (data, request) = local(
            200,
            r#"{"id":33,"name":"Series","first_air_date":"2020-01-01","external_ids":{"imdb_id":"tt1234567"},"genres":[{"id":1,"name":"Drama"}],"seasons":[{"id":8,"season_number":0,"name":"Specials","episode_count":2}]}"#,
            |c| ok(c.fetch(&series)),
        );
        assert!(request.starts_with("GET /3/tv/33?"));
        let TmdbData::Details(details) = &data else {
            panic!("expected details")
        };
        assert_eq!(details.imdb.as_ref().map(ImdbId::as_str), Some("tt1234567"));
        assert_eq!(details.seasons[0].number, 0);
        roundtrip(&data, &series);
        assert!(decode(&ok(encode(&data)), &movie).is_err());
    }
    #[test]
    fn season_is_lazy_and_checks_episode_identity() {
        let query = TmdbQuery::Season {
            series_id: 33,
            season: 0,
        };
        let (data, request) = local(
            200,
            r#"{"season_number":0,"episodes":[{"season_number":0,"episode_number":1,"name":"Special","overview":"Plot","air_date":null}]}"#,
            |c| ok(c.fetch(&query)),
        );
        assert!(request.starts_with("GET /3/tv/33/season/0?language=en-US "));
        roundtrip(&data, &query);
        assert!(
            decode(
                &ok(encode(&data)),
                &TmdbQuery::Season {
                    series_id: 34,
                    season: 0
                }
            )
            .is_err()
        );
        let (result, _) = local(
            200,
            r#"{"season_number":0,"episodes":[{"season_number":1,"episode_number":1}]}"#,
            |c| c.fetch(&query),
        );
        assert_eq!(result, Err(TmdbError::Malformed));
        let (data, _) = local(200, r#"{"season_number":0,"episodes":[]}"#, |c| {
            ok(c.fetch(&query))
        });
        roundtrip(&data, &query);
    }
    #[test]
    fn error_categories_and_authentication_are_safe() {
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        for (status, error) in [
            (401, TmdbError::RejectedCredential),
            (403, TmdbError::RejectedCredential),
            (429, TmdbError::RateLimited),
            (500, TmdbError::HttpStatus),
            (302, TmdbError::HttpStatus),
            (200, TmdbError::Malformed),
        ] {
            let (result, _) = local(status, TOKEN, |c| c.fetch(&query));
            assert_eq!(result, Err(error));
            assert!(!format!("{error:?}: {error}").contains(TOKEN));
        }
        let (_, request) = local(200, r#"{"success":true,"status_code":1}"#, |c| {
            ok(c.check_account())
        });
        assert!(request.starts_with("GET /3/authentication?language=en-US "));
        let (result, _) = local(200, r#"{"success":false}"#, TmdbClient::check_account);
        assert_eq!(result, Err(TmdbError::RejectedCredential));
        let listener = ok(TcpListener::bind("127.0.0.1:0"));
        let endpoint = ok(Url::parse(&format!(
            "http://{}/3/",
            ok(listener.local_addr())
        )));
        drop(listener);
        let client = ok(TmdbClient::at(SecretString::new(TOKEN.into()), endpoint));
        assert_eq!(client.fetch(&query), Err(TmdbError::Network));
    }
    #[test]
    fn rejects_bad_ids_pages_posters_and_oversized_data() {
        for query in [
            TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 0,
            },
            TmdbQuery::Browse {
                kind: MediaKind::Series,
                page: 501,
            },
            TmdbQuery::Details {
                kind: MediaKind::Movie,
                id: 0,
            },
            TmdbQuery::Season {
                series_id: 0,
                season: 1,
            },
        ] {
            assert_eq!(cache_query_identity(&query), Err(TmdbError::InvalidQuery));
        }
        assert!(TmdbClient::new(SecretString::new("\r\nsecret".into())).is_err());
        let query = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        for path in [
            "https://evil.test/token.jpg",
            "//evil.jpg",
            "/a.jpg?token=secret",
            "/../a.jpg",
            "/a%2Fb.jpg",
        ] {
            let body = serde_json::json!({"page":1,"total_pages":1,"results":[{"id":1,"title":"Movie","poster_path":path}]}).to_string();
            let (result, _) = local(200, &body, |c| c.fetch(&query));
            assert_eq!(result, Err(TmdbError::Malformed));
        }
        let body = "x".repeat(MAX_BYTES as usize + 1);
        let (result, _) = local(200, &body, |c| c.fetch(&query));
        assert_eq!(result, Err(TmdbError::TooLarge));
        assert_eq!(decode(&body, &query), Err(TmdbError::TooLarge));
        for body in [
            r#"{"page":1,"total_pages":1,"results":[{"id":0,"title":"No"}]}"#,
            r#"{"page":2,"total_pages":2,"results":[]}"#,
            r#"{"page":1,"total_pages":1,"results":[{"id":1,"name":"Wrong kind"}]}"#,
        ] {
            let (result, _) = local(200, body, |c| c.fetch(&query));
            assert_eq!(result, Err(TmdbError::Malformed));
        }
    }
    #[test]
    fn normalized_snapshots_enforce_collection_and_text_limits() {
        let title = TmdbTitle {
            id: 1,
            kind: MediaKind::Movie,
            name: "Movie".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        };
        let mut page = TmdbPage {
            titles: vec![title.clone()],
            page: 1,
            total_pages: 1,
        };
        assert!(encode(&TmdbData::Page(page.clone())).is_ok());
        page.titles[0].name = "x".repeat(501);
        assert_eq!(
            encode(&TmdbData::Page(page.clone())),
            Err(TmdbError::Malformed)
        );
        page.titles = vec![title.clone(), title.clone()];
        assert_eq!(
            encode(&TmdbData::Page(page.clone())),
            Err(TmdbError::Malformed)
        );
        page.titles = (1..=21)
            .map(|id| TmdbTitle {
                id,
                ..title.clone()
            })
            .collect();
        assert!(encode(&TmdbData::Page(page)).is_err());
        let season = TmdbSeason {
            poster_url: None,
            series_id: 1,
            season: 1,
            episodes: (1..=1001)
                .map(|number| CatalogEpisode {
                    still_url: None,
                    runtime_minutes: None,
                    season: 1,
                    number,
                    name: None,
                    overview: None,
                    released: None,
                })
                .collect(),
        };
        assert!(encode(&TmdbData::Season(season)).is_err());
        let query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 1,
        };
        let (result, _) = local(
            200,
            r#"{"id":1,"title":"Movie","imdb_id":"not-an-imdb-id","genres":[]}"#,
            |c| c.fetch(&query),
        );
        assert_eq!(result, Err(TmdbError::Malformed));
        let (result, _) = local(
            200,
            r#"{"id":2,"title":"Wrong resource","genres":[]}"#,
            |c| c.fetch(&query),
        );
        assert_eq!(result, Err(TmdbError::Malformed));
    }
    #[test]
    fn cache_identity_binds_locale_kind_search_and_pagination() {
        let a = TmdbQuery::Search {
            query: ok(SearchQuery::new(MediaKind::Movie, "Name")),
            page: 1,
        };
        let b = TmdbQuery::Search {
            query: ok(SearchQuery::new(MediaKind::Movie, "name")),
            page: 1,
        };
        let c = TmdbQuery::Browse {
            kind: MediaKind::Movie,
            page: 1,
        };
        assert_ne!(ok(cache_query_identity(&a)), ok(cache_query_identity(&b)));
        assert_ne!(ok(cache_query_identity(&a)), ok(cache_query_identity(&c)));
        assert!(ok(cache_query_identity(&a)).contains("language=en-US"));
        let mixed = TmdbData::Page(TmdbPage {
            titles: vec![TmdbTitle {
                id: 1,
                kind: MediaKind::Movie,
                name: "Name".into(),
                overview: None,
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            }],
            page: 1,
            total_pages: 1,
        });
        let encoded = ok(encode(&mixed));
        assert!(
            decode(
                &encoded,
                &TmdbQuery::Details {
                    kind: MediaKind::Movie,
                    id: 1
                }
            )
            .is_err()
        );
        assert!(decode(&encoded.replace("\"id\":1", "\"id\":0"), &c).is_err());
        assert!(decode(&encoded.replace("normalized-v1", "normalized-v2"), &c).is_err());
    }
}

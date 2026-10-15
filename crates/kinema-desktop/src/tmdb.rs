//! Blocking TMDB v3 adapter; call only on an owned background worker.
//! No Cinemeta fallback or name-based IMDb matching. Sources require `details.imdb`.
//! Snapshots contain normalized public metadata only. The cache owner must bind them
//! to `cache_query_identity`: a page alone cannot encode its search/browse identity.

use std::{collections::HashSet, fmt, io::Read, time::Duration};

use kinema_core::{
    MediaKind,
    catalog::{CatalogEpisode, ImdbId, SearchQuery},
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
    pub title: TmdbTitle,
    pub imdb: Option<ImdbId>,
    pub genres: Vec<String>,
    pub seasons: Vec<TmdbSeasonSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbSeason {
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
        Self::at(
            secret,
            Url::parse(ENDPOINT).map_err(|_| TmdbError::Network)?,
        )
    }
    fn at(secret: SecretString, endpoint: Url) -> Result<Self, TmdbError> {
        if secret.expose().is_empty()
            || secret.expose().len() > 4096
            || !secret.expose().bytes().all(|b| b.is_ascii_graphic())
        {
            return Err(TmdbError::InvalidCredential);
        }
        let client = Client::builder()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(15))
            // Never forward bearer credentials to redirects, including another host.
            .redirect(Policy::none())
            .user_agent("Kinema-Next/0.1 TMDB")
            .build()
            .map_err(|_| TmdbError::Network)?;
        Ok(Self {
            client,
            secret,
            endpoint,
        })
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
            401 | 403 => return Err(TmdbError::RejectedCredential),
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
    pub fn fetch(&self, query: &TmdbQuery) -> Result<TmdbData, TmdbError> {
        validate_query(query)?;
        let data = match query {
            TmdbQuery::Browse { kind, page } => {
                let dto: PageDto = self.get(
                    &format!("{}/popular", kind_path(*kind)),
                    &[("page", page.to_string())],
                )?;
                TmdbData::Page(dto.normalize(*kind)?)
            }
            TmdbQuery::Search { query, page } => {
                let dto: PageDto = self.get(
                    &format!("search/{}", kind_path(query.kind())),
                    &[
                        ("query", query.text().to_owned()),
                        ("page", page.to_string()),
                        ("include_adult", "false".into()),
                    ],
                )?;
                TmdbData::Page(dto.normalize(query.kind())?)
            }
            TmdbQuery::Details { kind, id } => {
                let dto: DetailDto = self.get(
                    &format!("{}/{id}", kind_path(*kind)),
                    &[("append_to_response", "external_ids".into())],
                )?;
                TmdbData::Details(dto.normalize(*kind)?)
            }
            TmdbQuery::Season { series_id, season } => {
                let dto: SeasonDto = self.get(&format!("tv/{series_id}/season/{season}"), &[])?;
                TmdbData::Season(TmdbSeason {
                    series_id: *series_id,
                    season: dto.season_number,
                    episodes: dto
                        .episodes
                        .into_iter()
                        .map(EpisodeDto::normalize)
                        .collect(),
                })
            }
        };
        validate_data(&data, Some(query))?;
        Ok(data)
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
    season_number: u32,
    episodes: Vec<EpisodeDto>,
}
#[derive(Deserialize, Serialize)]
struct EpisodeDto {
    season_number: u32,
    episode_number: u32,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
}
impl EpisodeDto {
    fn normalize(self) -> CatalogEpisode {
        CatalogEpisode {
            season: self.season_number,
            number: self.episode_number,
            name: nonempty(self.name),
            overview: nonempty(self.overview),
            released: nonempty(self.air_date),
        }
    }
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
            s.series_id > 0
                && s.season <= 1000
                && s.episodes.len() <= MAX_EPISODES
                && s.episodes.iter().all(|e| {
                    e.season == s.season
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
        title: SnapshotTitle,
        imdb: Option<String>,
        genres: Vec<String>,
        seasons: Vec<SummaryDto>,
    },
    Season {
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
            series_id: s.series_id,
            season: s.season,
            episodes: s
                .episodes
                .iter()
                .map(|e| EpisodeDto {
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
            title,
            imdb,
            genres,
            seasons,
        } => TmdbData::Details(TmdbDetails {
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
            series_id,
            season,
            episodes,
        } => TmdbData::Season(TmdbSeason {
            series_id,
            season,
            episodes: episodes.into_iter().map(EpisodeDto::normalize).collect(),
        }),
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
            series_id: 1,
            season: 1,
            episodes: (1..=1001)
                .map(|number| CatalogEpisode {
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

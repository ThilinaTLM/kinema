//! Blocking TMDB v3 adapter; call only on an owned background worker.
//! No name-based IMDb matching. Sources require explicit external identifiers.
//! Snapshots contain normalized public metadata only. The cache owner must bind them
//! to `cache_query_identity`: a page alone cannot encode its search/browse identity.

use std::{collections::HashSet, fmt, io::Read, time::Duration};

use kinema_core::{
    MediaKind,
    catalog::{
        CastMember, CatalogDisplay, CatalogEpisode, ImdbId, ImdbRating, SearchQuery, TitleFacts,
    },
    discover::{DiscoverFilters, DiscoverSort},
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
    Browse {
        kind: MediaKind,
        page: u32,
    },
    Discover {
        kind: MediaKind,
        page: u32,
        filters: DiscoverFilters,
    },
    Search {
        query: SearchQuery,
        page: u32,
    },
    Details {
        kind: MediaKind,
        id: u64,
    },
    /// Related titles of the same `kind`: movies use `/similar`, series use
    /// `/recommendations`.
    Similar {
        kind: MediaKind,
        id: u64,
        page: u32,
    },
    Season {
        series_id: u64,
        season: u32,
    },
}
impl TmdbQuery {
    pub(crate) fn operation(&self) -> &'static str {
        match self {
            Self::Browse { .. } | Self::Discover { .. } => "browse",
            Self::Search { .. } => "search",
            Self::Details { .. } => "details",
            Self::Similar { .. } => "similar",
            Self::Season { .. } => "season",
        }
    }
    // No Debug of the query: search text can contain private text or URLs.
    pub(crate) fn record_diagnostics(&self, span: &tracing::Span) {
        match self {
            Self::Browse { kind, page } | Self::Discover { kind, page, .. } => {
                span.record("media_kind", tracing::field::debug(kind));
                span.record("page", page);
            }
            Self::Search { query, page } => {
                span.record("media_kind", tracing::field::debug(query.kind()));
                span.record("page", page);
            }
            Self::Details { kind, id } => {
                span.record("media_kind", tracing::field::debug(kind));
                span.record("tmdb_id", id);
            }
            Self::Similar { kind, id, page } => {
                span.record("media_kind", tracing::field::debug(kind));
                span.record("tmdb_id", id);
                span.record("page", page);
            }
            Self::Season { series_id, season } => {
                span.record("tmdb_id", series_id);
                span.record("season", season);
            }
        }
    }
}
pub(crate) fn data_count(data: &TmdbData) -> usize {
    match data {
        TmdbData::Page(p) => p.titles.len(),
        TmdbData::Details(_) => 1,
        TmdbData::Season(s) => s.episodes.len(),
    }
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
    /// Season poster, overview and air date straight from the series details,
    /// so season tiles and headers need no per-season request.
    pub poster_url: Option<String>,
    pub air_date: Option<String>,
    pub overview: Option<String>,
}
impl TmdbSeasonSummary {
    /// Four-digit year of the air date, when there is one.
    pub fn year(&self) -> Option<u16> {
        self.air_date.as_deref()?.get(..4)?.parse().ok()
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TmdbDetails {
    pub facts: TitleFacts,
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
#[expect(
    clippy::large_enum_variant,
    reason = "Single owned worker result; detail resources use the unboxed TmdbDetails interface"
)]
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
        Self::client_builder().build().map_err(|error| {
            tracing::warn!(
                provider = "tmdb",
                operation = "build_client",
                kind = crate::logging::http_error_kind(&error),
                "TMDB transport initialization failed"
            );
            TmdbError::Network
        })
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
            tracing::warn!(
                provider = "tmdb",
                category = "invalid_credential",
                "TMDB credential validation failed"
            );
            return Err(TmdbError::InvalidCredential);
        }
        Ok(Self {
            rejected: std::sync::atomic::AtomicBool::new(false),
            client,
            secret,
            endpoint,
        })
    }
    /// Official external-ID lookup; never falls back to name/year matching.
    pub(crate) fn find_by_imdb(
        &self,
        kind: MediaKind,
        imdb: &ImdbId,
    ) -> Result<Option<u64>, TmdbError> {
        let raw: serde_json::Value = self.get(
            &format!("find/{}", imdb.as_str()),
            &[("external_source", "imdb_id".into())],
        )?;
        parse_imdb_find(raw, kind)
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
        let _span = tracing::info_span!("tmdb_http", provider = "tmdb").entered();
        let started = std::time::Instant::now();
        let result = (|| {
            let mut url = self
                .endpoint
                .join(path)
                .map_err(|_| TmdbError::InvalidQuery)?;
            url.query_pairs_mut()
                .append_pair("language", LANGUAGE)
                .extend_pairs(params.iter().map(|(k, v)| (*k, v.as_str())));
            let mut authorization =
                HeaderValue::from_str(&format!("Bearer {}", self.secret.expose()))
                    .map_err(|_| TmdbError::InvalidCredential)?;
            authorization.set_sensitive(true);
            let response = self
                .client
                .get(url)
                .header(AUTHORIZATION, authorization)
                .send()
                .map_err(|error| {
                    tracing::warn!(
                        kind = crate::logging::http_error_kind(&error),
                        duration_ms = started.elapsed().as_millis() as u64,
                        "TMDB HTTP transport failed"
                    );
                    TmdbError::Network
                })?;
            let status = response.status().as_u16();
            tracing::debug!(
                status,
                duration_ms = started.elapsed().as_millis() as u64,
                "TMDB HTTP headers received"
            );
            if !response.status().is_success() {
                tracing::warn!(
                    status,
                    category = match status {
                        401 | 403 => "credential_rejected",
                        429 => "rate_limited",
                        _ => "http_status",
                    },
                    duration_ms = started.elapsed().as_millis() as u64,
                    "TMDB HTTP request failed"
                );
            }
            match status {
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
                .map_err(|error| {
                    tracing::warn!(io_kind = ?error.kind(), "TMDB HTTP body read failed");
                    TmdbError::Network
                })?;
            if bytes.len() as u64 > MAX_BYTES {
                return Err(TmdbError::TooLarge);
            }
            tracing::debug!(
                bytes = bytes.len(),
                duration_ms = started.elapsed().as_millis() as u64,
                "TMDB HTTP body received"
            );
            serde_json::from_slice(&bytes).map_err(|_| TmdbError::Malformed)
        })();
        if let Err(category) = &result {
            tracing::warn!(
                ?category,
                duration_ms = started.elapsed().as_millis() as u64,
                "TMDB HTTP operation failed"
            );
        }
        result
    }
    /// Official v3 validation endpoint; no account profile is requested or returned.
    pub fn check_account(&self) -> Result<(), TmdbError> {
        let _span = tracing::info_span!(
            "tmdb_account",
            provider = "tmdb",
            operation = "check_account"
        )
        .entered();
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
        let span = tracing::info_span!(
            "tmdb_request",
            provider = "tmdb",
            operation = query.operation(),
            media_kind = tracing::field::Empty,
            tmdb_id = tracing::field::Empty,
            page = tracing::field::Empty,
            season = tracing::field::Empty
        );
        query.record_diagnostics(&span);
        let _span = span.entered();
        let started = std::time::Instant::now();
        let result = self.fetch_normalized_inner(query);
        let request_kind = match query {
            TmdbQuery::Browse { .. } | TmdbQuery::Discover { .. } => "browse",
            TmdbQuery::Search { .. } => "search",
            TmdbQuery::Details { .. } => "details",
            TmdbQuery::Similar { .. } => "similar",
            TmdbQuery::Season { .. } => "season",
        };
        match &result {
            Ok((data, _)) => tracing::debug!(
                provider = "tmdb",
                request_kind,
                duration_ms = started.elapsed().as_millis() as u64,
                count = data_count(data),
                empty = data_count(data) == 0,
                "TMDB metadata request completed"
            ),
            Err(category) => tracing::warn!(
                ?category,
                duration_ms = started.elapsed().as_millis() as u64,
                "TMDB metadata validation or fetch failed"
            ),
        }
        result
    }
    fn fetch_normalized_inner(
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
            TmdbQuery::Discover {
                kind,
                page,
                filters,
            } => {
                let (path, params) = discover_request(*kind, *page, filters);
                let raw: serde_json::Value = self.get(&path, &params)?;
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
            TmdbQuery::Similar { kind, id, page } => {
                let raw: serde_json::Value = self.get(
                    &format!("{}/{id}/{}", kind_path(*kind), similar_path(*kind)),
                    &[("page", page.to_string())],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let dto: PageDto = serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                TmdbData::Page(dto.normalize(*kind)?)
            }
            TmdbQuery::Details { kind, id } => {
                let raw: serde_json::Value = self.get(
                    &format!("{}/{id}", kind_path(*kind)),
                    &[(
                        "append_to_response",
                        match kind {
                            MediaKind::Movie => "external_ids,credits,release_dates",
                            MediaKind::Series => "external_ids,aggregate_credits,content_ratings",
                        }
                        .into(),
                    )],
                )?;
                rich = crate::catalog::normalized_rich::tmdb(&raw, query)
                    .map_err(|_| TmdbError::Malformed)?;
                let facts = tmdb_facts(&raw, *kind)?;
                let dto: DetailDto =
                    serde_json::from_value(raw).map_err(|_| TmdbError::Malformed)?;
                {
                    let mut details = dto.normalize(*kind)?;
                    details.facts = facts;
                    TmdbData::Details(details)
                }
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
// A minimum of 100 votes avoids tiny-sample leaders and rating matches.
const DISCOVER_VOTE_FLOOR: &str = "100";
/// Today's UTC date as `YYYY-MM-DD` (civil-from-days; no calendar dependency).
fn utc_today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86_400) as i64;
    civil_date(days)
}
fn civil_date(days_since_epoch: i64) -> String {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}
fn discover_request(
    kind: MediaKind,
    page: u32,
    filters: &DiscoverFilters,
) -> (String, Vec<(&'static str, String)>) {
    discover_request_on(kind, page, filters, &utc_today())
}
fn discover_request_on(
    kind: MediaKind,
    page: u32,
    filters: &DiscoverFilters,
    today: &str,
) -> (String, Vec<(&'static str, String)>) {
    if filters.is_default() {
        return (
            format!("{}/popular", kind_path(kind)),
            vec![("page", page.to_string())],
        );
    }
    let sort = match (filters.sort, kind) {
        (DiscoverSort::Popular, _) => "popularity.desc",
        (DiscoverSort::TopRated, _) => "vote_average.desc",
        (DiscoverSort::Newest, MediaKind::Movie) => "primary_release_date.desc",
        (DiscoverSort::Newest, MediaKind::Series) => "first_air_date.desc",
        (DiscoverSort::Title, MediaKind::Movie) => "title.asc",
        (DiscoverSort::Title, MediaKind::Series) => "name.asc",
    };
    let mut params = vec![
        ("sort_by", sort.into()),
        ("include_adult", "false".into()),
        ("page", page.to_string()),
    ];
    if !filters.genres.is_empty() {
        params.push((
            "with_genres",
            filters
                .genres
                .iter()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(","),
        ));
    }
    let (gte, lte) = match kind {
        MediaKind::Movie => ("primary_release_date.gte", "primary_release_date.lte"),
        MediaKind::Series => ("first_air_date.gte", "first_air_date.lte"),
    };
    if let Some(years) = filters.years {
        params.push((gte, format!("{:04}-01-01", years.from)));
    }
    // Newest means already released: without a ceiling TMDB lists announced
    // titles with far-future dates first. ISO dates compare lexically.
    let ceiling = filters.years.map(|years| format!("{:04}-12-31", years.to));
    let ceiling = match (filters.sort == DiscoverSort::Newest, ceiling) {
        (true, Some(end)) => Some(end.min(today.to_owned())),
        (true, None) => Some(today.to_owned()),
        (false, end) => end,
    };
    if let Some(ceiling) = ceiling {
        params.push((lte, ceiling));
    }
    if let Some(rating) = filters.min_rating {
        params.push(("vote_average.gte", rating.to_string()));
    }
    if filters.sort == DiscoverSort::TopRated || filters.min_rating.is_some() {
        params.push(("vote_count.gte", DISCOVER_VOTE_FLOOR.into()));
    }
    (format!("discover/{}", kind_path(kind)), params)
}
fn validate_query(query: &TmdbQuery) -> Result<(), TmdbError> {
    let valid = match query {
        TmdbQuery::Browse { page, .. } | TmdbQuery::Search { page, .. } => (1..=500).contains(page),
        TmdbQuery::Discover {
            kind,
            page,
            filters,
        } => (1..=500).contains(page) && filters.is_valid_for(*kind),
        TmdbQuery::Details { id, .. } => *id > 0,
        TmdbQuery::Similar { id, page, .. } => *id > 0 && (1..=500).contains(page),
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
        TmdbQuery::Discover {
            kind,
            page,
            filters,
        } => {
            if filters.is_default() {
                format!("browse:{}:{page}", kind_path(*kind))
            } else {
                let (path, params) = discover_request(*kind, *page, filters);
                let mut identity =
                    Url::parse("https://cache.invalid/").map_err(|_| TmdbError::InvalidQuery)?;
                identity.set_path(&path);
                identity.query_pairs_mut().extend_pairs(params);
                format!(
                    "discover-v1:{}?{}",
                    identity.path(),
                    identity.query().unwrap_or_default()
                )
            }
        }
        TmdbQuery::Search { query, page } => {
            format!("search:{}:{page}:{}", kind_path(query.kind()), query.text())
        }
        // The normalized writer formerly omitted season summary artwork. A new
        // series-detail resource identity refreshes those incomplete rows once,
        // even when they were marked complete. Explicit nulls stay complete.
        TmdbQuery::Details {
            kind: MediaKind::Series,
            id,
        } => format!("details:tv:{id}:season-art-v1"),
        TmdbQuery::Details { kind, id } => format!("details:{}:{id}", kind_path(*kind)),
        TmdbQuery::Similar { kind, id, page } => {
            format!("{}:{}:{id}:{page}", similar_path(*kind), kind_path(*kind))
        }
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
    #[serde(default)]
    poster_path: Option<String>,
    #[serde(default)]
    air_date: Option<String>,
    #[serde(default)]
    overview: Option<String>,
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
            facts: TitleFacts::default(),
            backdrop_url: image_url(self.backdrop_path)?,
            title: self.title.normalize(kind)?,
            imdb,
            genres: self.genres.into_iter().map(|g| g.name).collect(),
            seasons: self
                .seasons
                .into_iter()
                .map(|s| {
                    Ok(TmdbSeasonSummary {
                        number: s.season_number,
                        name: nonempty(s.name),
                        episode_count: s.episode_count,
                        poster_url: image_url(s.poster_path)?,
                        air_date: nonempty(s.air_date),
                        overview: nonempty(s.overview),
                    })
                })
                .collect::<Result<_, TmdbError>>()?,
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
/// Endpoint segment (and cache-identity word) of related titles per kind.
fn similar_path(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "similar",
        MediaKind::Series => "recommendations",
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
            if !d.facts.is_valid() {
                return Err(TmdbError::Malformed);
            }
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
                        && valid_optional_image(&s.poster_url)
                        && s.air_date
                            .as_deref()
                            .is_none_or(kinema_core::catalog_normalized::valid_date)
                        && optional_text(&s.overview, 20_000)
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
            (TmdbData::Page(p), TmdbQuery::Similar { kind, page, .. }) => {
                p.page == *page && p.titles.iter().all(|t| t.kind == *kind)
            }
            (
                TmdbData::Page(p),
                TmdbQuery::Browse { kind, page } | TmdbQuery::Discover { kind, page, .. },
            ) => p.page == *page && p.titles.iter().all(|t| t.kind == *kind),
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
        facts: Box<FactsPayload>,
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
            facts: Box::new(FactsPayload::from(&d.facts)),
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
                    poster_path: s
                        .poster_url
                        .as_deref()
                        .and_then(|u| u.strip_prefix(IMAGE_PREFIX))
                        .map(str::to_owned),
                    air_date: s.air_date.clone(),
                    overview: s.overview.clone(),
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
    // Old details snapshots predate season artwork. Absent is incomplete, while
    // explicit null is a valid provider response and must not refresh forever.
    let value: serde_json::Value =
        serde_json::from_str(encoded).map_err(|_| TmdbError::Malformed)?;
    if value["data"]["resource"] == "Details"
        && value["data"]["seasons"].as_array().is_some_and(|seasons| {
            seasons.iter().any(|season| {
                ["poster_path", "air_date", "overview"]
                    .iter()
                    .any(|field| season.get(*field).is_none())
            })
        })
    {
        return Err(TmdbError::Malformed);
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
            facts,
            backdrop_url,
            title,
            imdb,
            genres,
            seasons,
        } => TmdbData::Details(TmdbDetails {
            facts: facts.normalize()?,
            backdrop_url,
            title: title.normalize()?,
            imdb: imdb
                .map(|s| ImdbId::new(s).map_err(|_| TmdbError::Malformed))
                .transpose()?,
            genres,
            seasons: seasons
                .into_iter()
                .map(|s| {
                    Ok(TmdbSeasonSummary {
                        number: s.season_number,
                        name: s.name,
                        episode_count: s.episode_count,
                        poster_url: image_url(s.poster_path)?,
                        air_date: s.air_date,
                        overview: s.overview,
                    })
                })
                .collect::<Result<_, TmdbError>>()?,
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
        TmdbQuery::Details { kind, .. } => {
            let facts = tmdb_facts(&raw, *kind)?;
            let mut details = serde_json::from_value::<DetailDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(*kind)?;
            details.facts = facts;
            TmdbData::Details(details)
        }
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
        TmdbQuery::Browse { kind, .. } | TmdbQuery::Discover { kind, .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(*kind)?,
        ),
        TmdbQuery::Search { query, .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(query.kind())?,
        ),
        TmdbQuery::Similar { kind, .. } => TmdbData::Page(
            serde_json::from_value::<PageDto>(raw)
                .map_err(|_| TmdbError::Malformed)?
                .normalize(*kind)?,
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
    fn discover_requests_encode_all_facets_for_each_kind() {
        use kinema_core::discover::YearRange;
        for (kind, genre, date, sort) in [
            (MediaKind::Movie, 28, "primary_release_date", "title.asc"),
            (MediaKind::Series, 10759, "first_air_date", "name.asc"),
        ] {
            let query = TmdbQuery::Discover {
                kind,
                page: 2,
                filters: DiscoverFilters {
                    sort: DiscoverSort::Title,
                    genres: [18, genre].into(),
                    years: Some(YearRange {
                        from: 1990,
                        to: 1999,
                    }),
                    min_rating: Some(7),
                },
            };
            let (result, request) = local(200, r#"{"page":2,"total_pages":2,"results":[]}"#, |c| {
                c.fetch(&query)
            });
            let data = ok(result);
            roundtrip(&data, &query);
            let target = request
                .split_whitespace()
                .nth(1)
                .unwrap_or_else(|| panic!("request target"));
            let url = ok(Url::parse(&format!("http://local{target}")));
            assert_eq!(url.path(), format!("/3/discover/{}", kind_path(kind)));
            let params: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
            assert_eq!(params.get("sort_by").map(String::as_str), Some(sort));
            assert_eq!(params.get("with_genres"), Some(&format!("18,{genre}")));
            assert_eq!(
                params.get(&format!("{date}.gte")).map(String::as_str),
                Some("1990-01-01")
            );
            assert_eq!(
                params.get(&format!("{date}.lte")).map(String::as_str),
                Some("1999-12-31")
            );
            for (key, value) in [
                ("page", "2"),
                ("include_adult", "false"),
                ("vote_average.gte", "7"),
                ("vote_count.gte", "100"),
            ] {
                assert_eq!(params.get(key).map(String::as_str), Some(value));
            }
        }
    }
    #[test]
    fn newest_is_capped_at_today_and_civil_dates_are_exact() {
        assert_eq!(civil_date(0), "1970-01-01");
        assert_eq!(civil_date(11_016), "2000-02-29");
        assert_eq!(civil_date(20_753), "2026-10-27");
        let params = |filters: &DiscoverFilters| {
            discover_request_on(MediaKind::Movie, 1, filters, "2026-10-27")
                .1
                .into_iter()
                .collect::<std::collections::HashMap<_, _>>()
        };
        let mut filters = DiscoverFilters {
            sort: DiscoverSort::Newest,
            ..Default::default()
        };
        assert_eq!(
            params(&filters)
                .get("primary_release_date.lte")
                .map(String::as_str),
            Some("2026-10-27")
        );
        filters.years = Some(kinema_core::discover::YearRange {
            from: 2020,
            to: 2029,
        });
        assert_eq!(
            params(&filters)
                .get("primary_release_date.lte")
                .map(String::as_str),
            Some("2026-10-27")
        );
        filters.years = Some(kinema_core::discover::YearRange {
            from: 1990,
            to: 1999,
        });
        assert_eq!(
            params(&filters)
                .get("primary_release_date.lte")
                .map(String::as_str),
            Some("1999-12-31")
        );
        // Other sorts keep the decade ceiling and never add one by themselves.
        filters.sort = DiscoverSort::Title;
        filters.years = None;
        assert!(!params(&filters).contains_key("primary_release_date.lte"));
    }
    #[test]
    fn discover_sort_requests_and_default_popular_fallback() {
        for kind in [MediaKind::Movie, MediaKind::Series] {
            for sort in [
                DiscoverSort::Popular,
                DiscoverSort::TopRated,
                DiscoverSort::Newest,
            ] {
                let filters = DiscoverFilters {
                    sort,
                    ..Default::default()
                };
                let query = TmdbQuery::Discover {
                    kind,
                    page: 1,
                    filters,
                };
                let (result, request) =
                    local(200, r#"{"page":1,"total_pages":1,"results":[]}"#, |c| {
                        c.fetch(&query)
                    });
                ok(result);
                if sort == DiscoverSort::Popular {
                    assert!(request.starts_with(&format!("GET /3/{}/popular?", kind_path(kind))));
                    assert!(!request.contains("sort_by="));
                    assert_eq!(
                        ok(cache_query_identity(&query)),
                        ok(cache_query_identity(&TmdbQuery::Browse { kind, page: 1 }))
                    );
                } else {
                    let expected = if sort == DiscoverSort::TopRated {
                        "vote_average.desc"
                    } else if kind == MediaKind::Movie {
                        "primary_release_date.desc"
                    } else {
                        "first_air_date.desc"
                    };
                    assert!(request.contains(&format!("sort_by={expected}")));
                    assert_eq!(
                        request.contains("vote_count.gte=100"),
                        sort == DiscoverSort::TopRated
                    );
                }
            }
        }
    }
    #[test]
    fn discover_cache_keys_separate_every_facet_and_validate_decode() {
        use kinema_core::discover::YearRange;
        let base = DiscoverFilters::default();
        let mut identities = HashSet::new();
        for filters in [
            base.clone(),
            DiscoverFilters {
                sort: DiscoverSort::TopRated,
                ..base.clone()
            },
            DiscoverFilters {
                genres: [18].into(),
                ..base.clone()
            },
            DiscoverFilters {
                genres: [18, 35].into(),
                ..base.clone()
            },
            DiscoverFilters {
                years: Some(YearRange {
                    from: 1990,
                    to: 1999,
                }),
                ..base.clone()
            },
            DiscoverFilters {
                min_rating: Some(7),
                ..base.clone()
            },
            DiscoverFilters {
                min_rating: Some(8),
                ..base.clone()
            },
        ] {
            for kind in [MediaKind::Movie, MediaKind::Series] {
                for page in [1, 2] {
                    assert!(
                        identities.insert(ok(cache_query_identity(&TmdbQuery::Discover {
                            kind,
                            page,
                            filters: filters.clone()
                        })))
                    );
                }
            }
        }
        for filters in [
            DiscoverFilters {
                min_rating: Some(11),
                ..base.clone()
            },
            DiscoverFilters {
                genres: [10759].into(),
                ..base
            },
        ] {
            let query = TmdbQuery::Discover {
                kind: MediaKind::Movie,
                page: 1,
                filters,
            };
            assert_eq!(cache_query_identity(&query), Err(TmdbError::InvalidQuery));
            assert_eq!(decode("{}", &query), Err(TmdbError::InvalidQuery));
        }
    }
    #[test]
    fn tmdb_diagnostics_redact_bearer_search_and_error_payloads() {
        if !crate::providers::test_logging::isolated(
            module_path!(),
            "tmdb_diagnostics_redact_bearer_search_and_error_payloads",
        ) {
            return;
        }

        use crate::providers::test_logging::{capture, excludes};
        let query = TmdbQuery::Search {
            query: ok(SearchQuery::new(
                MediaKind::Movie,
                "https://private.invalid/path?token=search-secret",
            )),
            page: 1,
        };
        for status in [401, 429] {
            let ((result, output), request) = local(
                status,
                r#"{"error":"payload-secret","url":"https://cdn.invalid/?signature=signed-secret"}"#,
                |client| capture(|| client.fetch(&query)),
            );
            assert!(matches!(
                result,
                Err(TmdbError::RejectedCredential | TmdbError::RateLimited)
            ));
            assert!(request.contains(TOKEN));
            assert!(output.contains(&status.to_string()));
            excludes(
                &output,
                &[
                    TOKEN,
                    "search-secret",
                    "payload-secret",
                    "signed-secret",
                    "token=",
                    "signature=",
                ],
            );
        }
    }

    #[test]
    fn similar_uses_movie_scoped_endpoint_and_distinct_cache_identity() {
        let query = TmdbQuery::Similar {
            kind: MediaKind::Movie,
            id: 91,
            page: 2,
        };
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
                kind: MediaKind::Movie,
                id: 92,
                page: 2
            }))
        );
    }
    #[test]
    fn series_recommendations_use_tv_endpoint_series_pages_and_own_cache_identity() {
        let query = TmdbQuery::Similar {
            kind: MediaKind::Series,
            id: 91,
            page: 1,
        };
        let (result, request) = local(
            200,
            r#"{"page":1,"total_pages":2,"results":[{"id":42,"name":"Related show","first_air_date":"2019-05-01","poster_path":"/related.jpg"}]}"#,
            |c| c.fetch(&query),
        );
        let data = ok(result);
        assert!(request.contains("/3/tv/91/recommendations?"));
        roundtrip(&data, &query);
        let TmdbData::Page(page) = &data else {
            panic!("page")
        };
        assert!(page.titles.iter().all(|t| t.kind == MediaKind::Series));
        // A movie page can never satisfy a series query, nor the reverse.
        assert!(
            validate_data(
                &data,
                Some(&TmdbQuery::Similar {
                    kind: MediaKind::Movie,
                    id: 91,
                    page: 1
                })
            )
            .is_err()
        );
        assert_ne!(
            ok(cache_query_identity(&query)),
            ok(cache_query_identity(&TmdbQuery::Similar {
                kind: MediaKind::Movie,
                id: 91,
                page: 1
            }))
        );
    }
    #[test]
    fn season_summaries_carry_poster_year_and_overview_through_snapshots() {
        let query = TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 91,
        };
        let (result, _) = local(
            200,
            r#"{"id":91,"name":"Series","external_ids":{"imdb_id":"tt1234567"},"genres":[],"seasons":[{"season_number":1,"name":"Season 1","episode_count":8,"poster_path":"/s1.jpg","air_date":"2013-09-24","overview":"The first season."},{"season_number":2,"name":"Season 2","episode_count":3,"poster_path":null,"air_date":"","overview":""}]}"#,
            |c| c.fetch(&query),
        );
        let data = ok(result);
        roundtrip(&data, &query);
        let mut snapshot: serde_json::Value = ok(serde_json::from_str(&ok(encode(&data))));
        // Explicit nulls remain complete. Absent fields force one reacquisition.
        assert!(decode(&snapshot.to_string(), &query).is_ok());
        snapshot["data"]["seasons"][0]
            .as_object_mut()
            .unwrap_or_else(|| panic!("snapshot object"))
            .remove("poster_path");
        assert!(decode(&snapshot.to_string(), &query).is_err());
        let TmdbData::Details(details) = data else {
            panic!("details")
        };
        let first = &details.seasons[0];
        assert_eq!(
            first.poster_url.as_deref(),
            Some("https://image.tmdb.org/t/p/w500/s1.jpg")
        );
        assert_eq!(first.year(), Some(2013));
        assert_eq!(first.overview.as_deref(), Some("The first season."));
        let second = &details.seasons[1];
        assert_eq!((second.poster_url.as_deref(), second.year()), (None, None));
        assert_eq!(second.overview, None);
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
            // Accepted sockets may inherit nonblocking mode on macOS.
            ok(stream.set_nonblocking(false));
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
            r#"{"id":12,"title":"Film","imdb_id":null,"external_ids":{"imdb_id":null},"genres":[],"credits":{"crew":[{"name":"Director","job":"Director"}],"cast":[{"name":"Actor","character":"Hero"}]},"release_dates":{"results":[{"iso_3166_1":"US","release_dates":[{"certification":"PG"}]}]}}"#,
            |c| ok(c.fetch(&movie)),
        );
        assert!(
            request.starts_with("GET /3/movie/12?language=en-US&append_to_response=external_ids%2Ccredits%2Crelease_dates ")
        );
        let TmdbData::Details(details) = &data else {
            panic!("expected details")
        };
        assert_eq!(details.imdb, None);
        assert_eq!(details.facts.directors, ["Director"]);
        assert_eq!(details.facts.cast[0].character.as_deref(), Some("Hero"));
        assert_eq!(details.facts.certification.as_deref(), Some("PG"));
        roundtrip(&data, &movie);
        let series = TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 33,
        };
        let (data, request) = local(
            200,
            r#"{"id":33,"name":"Series","first_air_date":"2020-01-01","external_ids":{"imdb_id":"tt1234567"},"aggregate_credits":{"cast":[{"name":"Actor","roles":[{"character":"Lead"}]}]},"content_ratings":{"results":[{"iso_3166_1":"US","rating":"TV-14"}]},"genres":[{"id":1,"name":"Drama"}],"seasons":[{"id":8,"season_number":0,"name":"Specials","episode_count":2}]}"#,
            |c| ok(c.fetch(&series)),
        );
        assert!(request.starts_with("GET /3/tv/33?language=en-US&append_to_response=external_ids%2Caggregate_credits%2Ccontent_ratings "));
        let TmdbData::Details(series_details) = &data else {
            panic!("expected details")
        };
        assert_eq!(
            series_details.facts.cast[0].character.as_deref(),
            Some("Lead")
        );
        assert_eq!(series_details.facts.certification.as_deref(), Some("TV-14"));
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

/// Bounded normalized facts payload, not a provider response.
#[derive(Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct FactsPayload {
    tmdb_rating: Option<f64>,
    tmdb_vote_count: Option<u64>,
    imdb_rating: Option<f64>,
    runtime_minutes: Option<u32>,
    status: Option<String>,
    release_date: Option<String>,
    first_air_date: Option<String>,
    last_air_date: Option<String>,
    certification: Option<String>,
    original_language: Option<String>,
    tagline: Option<String>,
    release_info: Option<String>,
    directors: Vec<String>,
    creators: Vec<String>,
    studios: Vec<String>,
    networks: Vec<String>,
    writers: Vec<String>,
    countries: Vec<String>,
    cast: Vec<(String, Option<String>)>,
    /// Portrait URL per `cast` entry (same order); absent in older snapshots.
    cast_profiles: Vec<Option<String>>,
}
impl From<&TitleFacts> for FactsPayload {
    fn from(f: &TitleFacts) -> Self {
        Self {
            tmdb_rating: f.tmdb_rating.map(|v| f64::from(v.value())),
            tmdb_vote_count: f.tmdb_vote_count,
            imdb_rating: f.imdb_rating.map(|v| f64::from(v.value())),
            runtime_minutes: f.runtime_minutes,
            status: f.status.clone(),
            release_date: f.release_date.clone(),
            first_air_date: f.first_air_date.clone(),
            last_air_date: f.last_air_date.clone(),
            certification: f.certification.clone(),
            original_language: f.original_language.clone(),
            tagline: f.tagline.clone(),
            release_info: f.release_info.clone(),
            directors: f.directors.clone(),
            creators: f.creators.clone(),
            studios: f.studios.clone(),
            networks: f.networks.clone(),
            writers: f.writers.clone(),
            countries: f.countries.clone(),
            cast: f
                .cast
                .iter()
                .map(|c| (c.name.clone(), c.character.clone()))
                .collect(),
            cast_profiles: f.cast.iter().map(|c| c.profile_url.clone()).collect(),
        }
    }
}
impl FactsPayload {
    pub(crate) fn normalize(self) -> Result<TitleFacts, TmdbError> {
        let tmdb_rating = self
            .tmdb_rating
            .map(|v| ImdbRating::new(v).ok_or(TmdbError::Malformed))
            .transpose()?;
        let imdb_rating = self
            .imdb_rating
            .map(|v| ImdbRating::new(v).ok_or(TmdbError::Malformed))
            .transpose()?;
        let facts = TitleFacts {
            tmdb_rating,
            tmdb_vote_count: self.tmdb_vote_count,
            imdb_rating,
            runtime_minutes: self.runtime_minutes,
            status: self.status,
            release_date: self.release_date,
            first_air_date: self.first_air_date,
            last_air_date: self.last_air_date,
            certification: self.certification,
            original_language: self.original_language,
            tagline: self.tagline,
            release_info: self.release_info,
            directors: self.directors,
            creators: self.creators,
            studios: self.studios,
            networks: self.networks,
            writers: self.writers,
            countries: self.countries,
            cast: {
                let mut profiles = self.cast_profiles.into_iter();
                self.cast
                    .into_iter()
                    .map(|(name, character)| CastMember {
                        name,
                        character,
                        profile_url: profiles.next().flatten(),
                    })
                    .collect()
            },
        };
        if !facts.is_valid() {
            return Err(TmdbError::Malformed);
        }
        Ok(facts)
    }
}

fn tmdb_facts(raw: &serde_json::Value, kind: MediaKind) -> Result<TitleFacts, TmdbError> {
    let mut payload = serde_json::Map::new();
    for (source, target) in [
        ("vote_average", "tmdb_rating"),
        ("vote_count", "tmdb_vote_count"),
        ("runtime", "runtime_minutes"),
        ("status", "status"),
        ("release_date", "release_date"),
        ("first_air_date", "first_air_date"),
        ("last_air_date", "last_air_date"),
        ("original_language", "original_language"),
        ("tagline", "tagline"),
    ] {
        if let Some(v) = raw.get(source).filter(|v| {
            !v.is_null()
                && v.as_str() != Some("")
                && !(source == "runtime" && v.as_u64() == Some(0))
        }) {
            payload.insert(target.into(), v.clone());
        }
    }
    if kind == MediaKind::Series {
        payload.remove("runtime_minutes");
        if let Some(v) = raw
            .get("episode_run_time")
            .and_then(|v| v.as_array())
            .and_then(|v| v.iter().find(|n| n.as_u64().is_some_and(|n| n > 0)))
        {
            payload.insert("runtime_minutes".into(), v.clone());
        }
    }
    let names = |key: &str| -> Vec<String> {
        raw.get(key)
            .and_then(|v| v.as_array())
            .into_iter()
            .flatten()
            .filter_map(|v| v.get("name").and_then(|v| v.as_str()).map(str::to_owned))
            .collect()
    };
    payload.insert(
        "studios".into(),
        serde_json::json!(names("production_companies")),
    );
    payload.insert("networks".into(), serde_json::json!(names("networks")));
    payload.insert("creators".into(), serde_json::json!(names("created_by")));
    let credits = raw.get(if kind == MediaKind::Movie {
        "credits"
    } else {
        "aggregate_credits"
    });
    let directors: Vec<&str> = credits
        .and_then(|v| v.get("crew"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .filter(|v| v.get("job").and_then(|v| v.as_str()) == Some("Director"))
        .filter_map(|v| v.get("name").and_then(|v| v.as_str()))
        .collect();
    payload.insert("directors".into(), serde_json::json!(directors));
    let cast: Vec<_> = credits
        .and_then(|v| v.get("cast"))
        .and_then(|v| v.as_array())
        .into_iter()
        .flatten()
        .take(kinema_core::catalog::MAX_CAST)
        .filter_map(|v| {
            let name = v.get("name")?.as_str()?;
            let character = v
                .get("character")
                .and_then(|v| v.as_str())
                .or_else(|| {
                    v.get("roles")?
                        .as_array()?
                        .first()?
                        .get("character")?
                        .as_str()
                })
                .filter(|s| !s.is_empty());
            let profile = v
                .get("profile_path")
                .and_then(|v| v.as_str())
                .filter(|p| valid_image_path(p))
                .map(|p| format!("{IMAGE_PREFIX}{p}"));
            Some(((name, character), profile))
        })
        .collect();
    let (cast, profiles): (Vec<_>, Vec<_>) = cast.into_iter().unzip();
    payload.insert("cast".into(), serde_json::json!(cast));
    payload.insert("cast_profiles".into(), serde_json::json!(profiles));
    let ratings = raw
        .get(if kind == MediaKind::Movie {
            "release_dates"
        } else {
            "content_ratings"
        })
        .and_then(|v| v.get("results"))
        .and_then(|v| v.as_array());
    let region = ratings
        .into_iter()
        .flatten()
        .find(|v| v.get("iso_3166_1").and_then(|v| v.as_str()) == Some("US"));
    let certification = region.and_then(|v| {
        if kind == MediaKind::Movie {
            v.get("release_dates")?
                .as_array()?
                .iter()
                .filter_map(|v| v.get("certification")?.as_str())
                .find(|s| !s.is_empty())
        } else {
            v.get("rating")?.as_str().filter(|s| !s.is_empty())
        }
    });
    if let Some(v) = certification {
        payload.insert("certification".into(), v.into());
    }
    serde_json::from_value::<FactsPayload>(payload.into())
        .map_err(|_| TmdbError::Malformed)?
        .normalize()
}

#[cfg(test)]
mod facts_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn cast_profiles_survive_snapshots_and_old_snapshots_have_none() -> Result<(), TmdbError> {
        let credits = json!({"cast": [
            {"name": "A", "character": "x", "profile_path": "/a.jpg"},
            {"name": "B", "profile_path": null},
            {"name": "C", "profile_path": "no-slash"},
        ]});
        let facts = tmdb_facts(&json!({"credits": credits}), MediaKind::Movie)?;
        let urls: Vec<_> = facts
            .cast
            .iter()
            .map(|c| c.profile_url.as_deref())
            .collect();
        assert_eq!(
            urls,
            [Some("https://image.tmdb.org/t/p/w500/a.jpg"), None, None]
        );
        let payload =
            serde_json::to_value(FactsPayload::from(&facts)).map_err(|_| TmdbError::Malformed)?;
        let again = serde_json::from_value::<FactsPayload>(payload.clone())
            .map_err(|_| TmdbError::Malformed)?
            .normalize()?;
        assert_eq!(again, facts);
        let mut old = payload;
        old.as_object_mut().map(|o| o.remove("cast_profiles"));
        let old = serde_json::from_value::<FactsPayload>(old)
            .map_err(|_| TmdbError::Malformed)?
            .normalize()?;
        assert_eq!(old.cast.len(), 3);
        assert!(old.cast.iter().all(|c| c.profile_url.is_none()));
        Ok(())
    }

    #[test]
    fn movie_and_series_facts_decode_with_credits_and_us_certification() -> Result<(), TmdbError> {
        let movie = tmdb_facts(
            &json!({"vote_average": 8.4, "vote_count": 42, "runtime": 120, "status": "Released", "release_date": "2024-01-01", "original_language": "en", "tagline": "A story", "credits": {"crew": [{"name": "Director", "job": "Director"}], "cast": [{"name": "Actor", "character": "Hero"}]}, "production_companies": [{"name": "Studio"}], "release_dates": {"results": [{"iso_3166_1": "GB", "release_dates": [{"certification": "15"}]}, {"iso_3166_1": "US", "release_dates": [{"certification": ""}, {"certification": "PG-13"}]}]}}),
            MediaKind::Movie,
        )?;
        assert_eq!(movie.tmdb_rating.map(ImdbRating::value), Some(8.4));
        assert_eq!(movie.imdb_rating, None);
        assert_eq!(movie.directors, ["Director"]);
        assert_eq!(movie.cast[0].character.as_deref(), Some("Hero"));
        assert_eq!(movie.certification.as_deref(), Some("PG-13"));
        assert_eq!(movie.runtime_minutes, Some(120));
        let series = tmdb_facts(
            &json!({"episode_run_time": [0, 45], "created_by": [{"name": "Creator"}], "networks": [{"name": "Network"}], "aggregate_credits": {"cast": [{"name": "Actor", "roles": [{"character": "Lead"}]}]}, "content_ratings": {"results": [{"iso_3166_1": "US", "rating": "TV-14"}]}}),
            MediaKind::Series,
        )?;
        assert_eq!(series.runtime_minutes, Some(45));
        assert_eq!(series.creators, ["Creator"]);
        assert_eq!(series.cast[0].character.as_deref(), Some("Lead"));
        assert_eq!(series.certification.as_deref(), Some("TV-14"));
        Ok(())
    }
    #[test]
    fn facts_payload_rejects_invalid_scores_and_bounds() -> Result<(), serde_json::Error> {
        for score in [-1.0, 10.1] {
            assert!(
                serde_json::from_value::<FactsPayload>(json!({"tmdb_rating": score}))?
                    .normalize()
                    .is_err()
            );
        }
        assert!(
            serde_json::from_value::<FactsPayload>(json!({"status": "x".repeat(201)}))?
                .normalize()
                .is_err()
        );
        Ok(())
    }
}

#[cfg(test)]
mod facts_snapshot_tests {
    use super::*;
    #[test]
    fn snapshots_preserve_facts_and_old_missing_facts_load_empty() -> Result<(), TmdbError> {
        let mut details = serde_json::from_value::<DetailDto>(
            serde_json::json!({"id": 12, "title": "Movie", "genres": []}),
        )
        .map_err(|_| TmdbError::Malformed)?
        .normalize(MediaKind::Movie)?;
        details.facts = tmdb_facts(
            &serde_json::json!({"vote_average": 0.0, "vote_count": 0, "status": "Released"}),
            MediaKind::Movie,
        )?;
        let query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 12,
        };
        let data = TmdbData::Details(details);
        let encoded = encode(&data)?;
        assert_eq!(decode(&encoded, &query)?, data);
        let mut old: serde_json::Value =
            serde_json::from_str(&encoded).map_err(|_| TmdbError::Malformed)?;
        old["data"]
            .as_object_mut()
            .ok_or(TmdbError::Malformed)?
            .remove("facts");
        let TmdbData::Details(old) = decode(&old.to_string(), &query)? else {
            return Err(TmdbError::Malformed);
        };
        assert_eq!(old.facts, TitleFacts::default());
        Ok(())
    }
}

fn parse_imdb_find(raw: serde_json::Value, kind: MediaKind) -> Result<Option<u64>, TmdbError> {
    let key = match kind {
        MediaKind::Movie => "movie_results",
        MediaKind::Series => "tv_results",
    };
    let results = raw
        .get(key)
        .and_then(serde_json::Value::as_array)
        .ok_or(TmdbError::Malformed)?;
    // Ambiguous results are not evidence for joining identities.
    if results.len() > 1 {
        return Err(TmdbError::Malformed);
    }
    results
        .first()
        .map(|r| {
            r.get("id")
                .and_then(serde_json::Value::as_u64)
                .filter(|id| *id > 0)
                .ok_or(TmdbError::Malformed)
        })
        .transpose()
}
#[cfg(test)]
mod imdb_find_tests {
    use super::*;
    #[test]
    fn explicit_kind_and_not_found() {
        let raw = serde_json::json!({"movie_results":[{"id":42}],"tv_results":[]});
        assert_eq!(parse_imdb_find(raw.clone(), MediaKind::Movie), Ok(Some(42)));
        assert_eq!(parse_imdb_find(raw, MediaKind::Series), Ok(None));
        assert_eq!(
            parse_imdb_find(serde_json::json!({}), MediaKind::Movie),
            Err(TmdbError::Malformed)
        );
    }
}

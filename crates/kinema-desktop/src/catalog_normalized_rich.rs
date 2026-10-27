//! Bounded rich provider DTOs. Only supplied, documented fields are imported.
use super::normalized::*;
use super::*;
use kinema_core::catalog_normalized::{ExternalIdentifier, FieldValue, Quality, RatingObservation};
use rusqlite::Transaction;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RichRecord {
    pub title: Option<ExternalIdentifier>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    pub identifiers: Vec<ExternalIdentifier>,
    pub fields: Vec<(String, FieldValue)>,
    pub ratings: Vec<RatingObservation>,
}
impl RichRecord {
    fn new(season: Option<u32>, episode: Option<u32>) -> Self {
        Self {
            title: None,
            season,
            episode,
            identifiers: vec![],
            fields: vec![],
            ratings: vec![],
        }
    }
    fn text(&mut self, name: &str, value: Option<String>) -> Result<(), CatalogError> {
        if let Some(value) = value.filter(|s| !s.trim().is_empty()) {
            if value.len() > 80_000
                || value.contains('\0')
                || (name.ends_with("_date") && !kinema_core::catalog_normalized::valid_date(&value))
            {
                return Err(CatalogError::InvalidMetadata);
            }
            self.fields.push((name.into(), FieldValue::Text(value)));
        }
        Ok(())
    }
    fn number(&mut self, name: &str, value: Option<u32>) -> Result<(), CatalogError> {
        if let Some(value) = value {
            if name == "runtime_minutes" && value == 0 {
                return Ok(());
            }
            self.fields
                .push((name.into(), FieldValue::Integer(i64::from(value))));
        }
        Ok(())
    }
    fn rating(
        &mut self,
        source: &str,
        provider: &str,
        value: Option<f64>,
        votes: Option<u64>,
    ) -> Result<(), CatalogError> {
        if value.is_some() || votes.is_some() {
            let rating = RatingObservation {
                source: source.into(),
                metric: "score".into(),
                supplied_by: provider.into(),
                value,
                vote_count: votes,
                source_updated_at: None,
            };
            if !rating.valid() {
                return Err(CatalogError::InvalidMetadata);
            }
            self.ratings.push(rating);
        }
        Ok(())
    }
}

#[derive(Deserialize, Default)]
struct TmdbRichDto {
    id: Option<u64>,
    original_title: Option<String>,
    original_name: Option<String>,
    tagline: Option<String>,
    original_language: Option<String>,
    status: Option<String>,
    runtime: Option<u32>,
    backdrop_path: Option<String>,
    poster_path: Option<String>,
    still_path: Option<String>,
    last_air_date: Option<String>,
    number_of_seasons: Option<u32>,
    number_of_episodes: Option<u32>,
    vote_average: Option<f64>,
    vote_count: Option<u64>,
    season_number: Option<u32>,
    episode_number: Option<u32>,
    name: Option<String>,
    overview: Option<String>,
    air_date: Option<String>,
    episode_count: Option<u32>,
    production_code: Option<String>,
}
fn image(path: Option<String>) -> Result<Option<String>, CatalogError> {
    path.map(|path| {
        if path.len() > 256
            || !path.starts_with('/')
            || path.len() < 2
            || path.contains("..")
            || !path[1..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
            || !(path.ends_with(".jpg") || path.ends_with(".png"))
        {
            return Err(CatalogError::InvalidMetadata);
        }
        Ok(format!("https://image.tmdb.org/t/p/w500{path}"))
    })
    .transpose()
}
fn parse_dto(value: &serde_json::Value) -> Result<TmdbRichDto, CatalogError> {
    serde_json::from_value(value.clone()).map_err(|_| CatalogError::InvalidMetadata)
}
fn child(dto: TmdbRichDto, season: u32, episode: Option<u32>) -> Result<RichRecord, CatalogError> {
    let mut record = RichRecord::new(Some(season), episode);
    if let Some(id) = dto.id {
        if id == 0 {
            return Err(CatalogError::InvalidMetadata);
        }
        record.identifiers.push(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: id.to_string(),
        });
    }
    record.text("name", dto.name)?;
    record.text("overview", dto.overview)?;
    record.text("air_date", dto.air_date.filter(|s| !s.is_empty()))?;
    if episode.is_some() {
        record.text("still_url", image(dto.still_path)?)?;
        record.number("runtime_minutes", dto.runtime)?;
        record.text("production_code", dto.production_code)?;
    } else {
        record.text("poster_url", image(dto.poster_path)?)?;
        record.number("episode_count", dto.episode_count)?;
    }
    record.rating("tmdb", "tmdb", dto.vote_average, dto.vote_count)?;
    Ok(record)
}
pub(crate) fn tmdb(
    value: &serde_json::Value,
    query: &crate::tmdb::TmdbQuery,
) -> Result<Vec<RichRecord>, CatalogError> {
    use crate::tmdb::TmdbQuery;
    let mut records = Vec::new();
    match query {
        TmdbQuery::Details { kind, .. } => {
            let dto = parse_dto(value)?;
            let mut record = RichRecord::new(None, None);
            record.text(
                "original_name",
                if *kind == MediaKind::Movie {
                    dto.original_title
                } else {
                    dto.original_name
                },
            )?;
            record.text("tagline", dto.tagline)?;
            record.text("original_language", dto.original_language)?;
            record.text(
                "status",
                dto.status.filter(|s| {
                    matches!(
                        s.as_str(),
                        "Rumored"
                            | "Planned"
                            | "In Production"
                            | "Post Production"
                            | "Released"
                            | "Canceled"
                            | "Returning Series"
                            | "Ended"
                            | "Pilot"
                    )
                }),
            )?;
            record.text("backdrop_url", image(dto.backdrop_path)?)?;
            if *kind == MediaKind::Movie {
                record.number("runtime_minutes", dto.runtime)?;
            } else {
                record.text("last_air_date", dto.last_air_date.filter(|s| !s.is_empty()))?;
                record.number("season_count", dto.number_of_seasons)?;
                record.number("episode_count", dto.number_of_episodes)?;
            }
            record.rating("tmdb", "tmdb", dto.vote_average, dto.vote_count)?;
            records.push(record);
            if let Some(seasons) = value.get("seasons").and_then(|v| v.as_array()) {
                for season in seasons {
                    let dto = parse_dto(season)?;
                    let number = dto.season_number.ok_or(CatalogError::InvalidMetadata)?;
                    records.push(child(dto, number, None)?);
                }
            }
        }
        TmdbQuery::Season { season, .. } => {
            records.push(child(parse_dto(value)?, *season, None)?);
            let episodes = value
                .get("episodes")
                .and_then(|v| v.as_array())
                .ok_or(CatalogError::InvalidMetadata)?;
            for episode in episodes {
                let dto = parse_dto(episode)?;
                let number = dto
                    .episode_number
                    .filter(|n| *n > 0)
                    .ok_or(CatalogError::InvalidMetadata)?;
                if dto.season_number != Some(*season) {
                    return Err(CatalogError::InvalidMetadata);
                }
                records.push(child(dto, *season, Some(number))?);
            }
        }
        TmdbQuery::Browse { kind, .. }
        | TmdbQuery::Discover { kind, .. }
        | TmdbQuery::Similar { kind, .. } => {
            records = tmdb_summaries(value, *kind)?;
        }
        TmdbQuery::Search { query, .. } => {
            records = tmdb_summaries(value, query.kind())?;
        }
    }
    Ok(records)
}

fn tmdb_summaries(
    value: &serde_json::Value,
    kind: MediaKind,
) -> Result<Vec<RichRecord>, CatalogError> {
    let rows = value
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or(CatalogError::InvalidMetadata)?;
    rows.iter()
        .map(|value| {
            let dto = parse_dto(value)?;
            let id = dto
                .id
                .filter(|id| *id > 0)
                .ok_or(CatalogError::InvalidMetadata)?;
            let mut record = RichRecord::new(None, None);
            record.title = Some(ExternalIdentifier {
                namespace: "tmdb".into(),
                external_id: id.to_string(),
            });
            record.text(
                "original_name",
                if kind == MediaKind::Movie {
                    dto.original_title
                } else {
                    dto.original_name
                },
            )?;
            record.text("original_language", dto.original_language)?;
            record.text("backdrop_url", image(dto.backdrop_path)?)?;
            record.rating("tmdb", "tmdb", dto.vote_average, dto.vote_count)?;
            Ok(record)
        })
        .collect()
}

#[derive(Deserialize)]
struct CinemetaRichDto {
    #[serde(rename = "type")]
    kind: Option<String>,
    runtime: Option<String>,
    released: Option<String>,
    #[serde(rename = "releaseInfo")]
    release_info: Option<String>,
    logo: Option<String>,
    genres: Option<Vec<String>>,
    #[serde(rename = "imdbRating")]
    imdb_rating: Option<String>,
    #[serde(default)]
    videos: Vec<CinemetaEpisodeRich>,
}
#[derive(Deserialize)]
struct CinemetaEpisodeRich {
    season: u32,
    episode: u32,
    id: Option<String>,
    thumbnail: Option<String>,
}
pub(super) fn cinemeta(value: &serde_json::Value) -> Result<Vec<RichRecord>, CatalogError> {
    let dto: CinemetaRichDto =
        serde_json::from_value(value.clone()).map_err(|_| CatalogError::InvalidMetadata)?;
    let mut record = RichRecord::new(None, None);
    let movie = dto.kind.as_deref() == Some("movie");
    if movie
        && let Some(runtime) = dto
            .runtime
            .as_deref()
            .and_then(|text| {
                text.strip_suffix(" min")
                    .or_else(|| text.bytes().all(|b| b.is_ascii_digit()).then_some(text))
            })
            .and_then(|text| text.parse::<u32>().ok())
    {
        record.number("runtime_minutes", Some(runtime))?;
    }
    if let Some(released) = dto.released.filter(|text| !text.trim().is_empty()) {
        let date = released
            .get(..10)
            .filter(|date| kinema_core::catalog_normalized::valid_date(date))
            .ok_or(CatalogError::InvalidResponse)?;
        let year = date[..4]
            .parse::<u32>()
            .map_err(|_| CatalogError::InvalidResponse)?;
        if dto
            .release_info
            .as_deref()
            .and_then(super::normalized_pages::cinemeta_years)
            .is_some_and(|(first, _)| first != i64::from(year))
        {
            return Err(CatalogError::InvalidResponse);
        }
        record.text(
            if movie {
                "release_date"
            } else {
                "first_air_date"
            },
            Some(date.into()),
        )?;
        record.number(
            if movie {
                "release_year"
            } else {
                "first_air_year"
            },
            Some(year),
        )?;
    }
    record.text("logo_url", super::metadata::validate_image(dto.logo)?)?;
    if let Some(genres) = dto.genres {
        record
            .fields
            .push(("genres_json".into(), FieldValue::Labels(genres)));
    }
    let rating = dto
        .imdb_rating
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.parse::<f64>().map_err(|_| CatalogError::InvalidMetadata))
        .transpose()?;
    record.rating("imdb", "cinemeta", rating, None)?;
    let mut records = vec![record];
    for episode in dto.videos {
        if episode.episode == 0 {
            return Err(CatalogError::InvalidMetadata);
        }
        let mut record = RichRecord::new(Some(episode.season), Some(episode.episode));
        // Cinemeta coordinate strings such as tt1234567:1:2 are not IMDb IDs.
        if let Some(id) = episode.id.filter(|s| ImdbId::new(s.clone()).is_ok()) {
            record.identifiers.push(ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: id,
            });
        }
        record.text(
            "still_url",
            super::metadata::validate_image(episode.thumbnail)?,
        )?;
        records.push(record);
    }
    Ok(records)
}
pub(super) fn write(
    tx: &Transaction<'_>,
    title: i64,
    records: &[RichRecord],
    provider: &str,
    fetch_id: i64,
    time: i64,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    for record in records {
        let target = match (record.season, record.episode) {
            (None, None) => Target::Title(title),
            (Some(number), None) => Target::Season(season(tx, title, number, time)?),
            (Some(number), Some(ep)) => {
                Target::Episode(episode(tx, season(tx, title, number, time)?, ep, time)?)
            }
            _ => return Err(CatalogError::InvalidMetadata),
        };
        for ident in &record.identifiers {
            identifier(tx, target, None, ident, fetch_id, time)?;
        }
        super::normalized_pairs::fields(
            tx,
            target,
            &record.fields,
            provider,
            Quality::Detail,
            fetch_id,
            time,
            priority,
        )?;
        for observation in &record.ratings {
            rating(tx, target, observation, provider, fetch_id, time)?;
        }
    }
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit transaction, resource identity, observation and merge policy context"
)]
pub(super) fn write_page_rich(
    tx: &Transaction<'_>,
    provider: &str,
    key: &str,
    page: u32,
    kind: MediaKind,
    records: &[RichRecord],
    time: i64,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    if records.is_empty() {
        return Ok(());
    }
    let fetch_id:i64=tx.query_row("SELECT f.id FROM metadata_fetches f JOIN catalog_pages p ON p.id=f.page_id WHERE p.provider=?1 AND p.query_key=?2 AND p.page_number=?3 AND f.provider=?1 AND f.resource_key=?2 AND f.locale='en-US'",params![provider,key,page],|r|r.get(0))?;
    for record in records {
        let external = record.title.as_ref().ok_or(CatalogError::InvalidMetadata)?;
        let id = title_id(tx, kind, &external.namespace, &external.external_id)?
            .ok_or(CatalogError::InvalidMetadata)?;
        // Page observations must remain summary quality, even rich optional fields.
        super::normalized_pairs::fields(
            tx,
            Target::Title(id),
            &record.fields,
            provider,
            Quality::Summary,
            fetch_id,
            time,
            priority,
        )?;
        for observation in &record.ratings {
            rating(tx, Target::Title(id), observation, provider, fetch_id, time)?;
        }
    }
    Ok(())
}

#[derive(Deserialize)]
struct CinemetaSummaryRich {
    id: String,
    poster: Option<String>,
    background: Option<String>,
    logo: Option<String>,
    genres: Option<Vec<String>>,
    #[serde(rename = "imdbRating")]
    imdb_rating: Option<String>,
}
pub(super) fn cinemeta_summaries(
    value: &serde_json::Value,
) -> Result<Vec<RichRecord>, CatalogError> {
    let metas = value
        .get("metas")
        .and_then(|v| v.as_array())
        .ok_or(CatalogError::InvalidMetadata)?;
    metas
        .iter()
        .map(|value| {
            let dto: CinemetaSummaryRich =
                serde_json::from_value(value.clone()).map_err(|_| CatalogError::InvalidMetadata)?;
            let mut record = RichRecord::new(None, None);
            record.title = Some(ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: dto.id,
            });
            for (name, value) in [
                ("poster_url", dto.poster),
                ("backdrop_url", dto.background),
                ("logo_url", dto.logo),
            ] {
                record.text(name, super::metadata::validate_image(value)?)?;
            }
            if let Some(genres) = dto.genres {
                record
                    .fields
                    .push(("genres_json".into(), FieldValue::Labels(genres)));
            }
            let rating = dto
                .imdb_rating
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.parse::<f64>().map_err(|_| CatalogError::InvalidMetadata))
                .transpose()?;
            record.rating("imdb", "cinemeta", rating, None)?;
            Ok(record)
        })
        .collect()
}

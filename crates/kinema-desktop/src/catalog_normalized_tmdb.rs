//! TMDB canonical imports and legacy UI projections.
use super::*;
use super::{normalized::*, normalized_pages::*};
use crate::tmdb::{
    TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbSeason, TmdbSeasonSummary, TmdbTitle,
};
use kinema_core::catalog_normalized::{ExternalIdentifier, FieldValue, Quality, TitleObservation};
use rusqlite::Transaction;

pub(crate) fn observation(title: &TmdbTitle) -> TitleObservation {
    let mut fields = vec![("name".into(), FieldValue::Text(title.name.clone()))];
    for (name, value) in [
        ("overview", title.overview.as_ref()),
        ("poster_url", title.poster_url.as_ref()),
    ] {
        if let Some(value) = value {
            fields.push((name.into(), FieldValue::Text(value.clone())));
        }
    }
    if let Some(release) = &title.release_info {
        let prefix = if title.kind == MediaKind::Movie {
            "release"
        } else {
            "first_air"
        };
        if kinema_core::catalog_normalized::valid_date(release) {
            fields.push((format!("{prefix}_date"), FieldValue::Text(release.clone())));
        }
        if let Some(year) = release
            .get(..4)
            .and_then(|s| s.parse::<i64>().ok())
            .filter(|y| (1..=9999).contains(y))
        {
            fields.push((format!("{prefix}_year"), FieldValue::Integer(year)));
        }
    }
    TitleObservation {
        kind: title.kind,
        identifiers: vec![ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: title.id.to_string(),
        }],
        fields,
        ratings: vec![],
    }
}

pub(super) fn write(
    tx: &Transaction<'_>,
    query: &TmdbQuery,
    data: &TmdbData,
    time: i64,
    complete: bool,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    let resource = crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
    let completeness = if complete {
        Quality::Detail
    } else {
        Quality::Summary
    };
    match data {
        TmdbData::Page(page) => write_page(
            tx,
            "tmdb",
            &resource,
            page.page,
            Some(page.total_pages),
            &page.titles.iter().map(observation).collect::<Vec<_>>(),
            time,
            priority,
        )?,
        TmdbData::Details(details) => {
            let mut item = observation(&details.title);
            if let Some(url) = &details.backdrop_url {
                item.fields
                    .push(("backdrop_url".into(), FieldValue::Text(url.clone())));
            }
            if let Some(imdb) = &details.imdb {
                item.identifiers.push(ExternalIdentifier {
                    namespace: "imdb".into(),
                    external_id: imdb.as_str().into(),
                });
            }
            if complete || !details.genres.is_empty() {
                item.fields.push((
                    "genres_json".into(),
                    FieldValue::Labels(details.genres.clone()),
                ));
            }
            let id = resolve_title(tx, &item, time, priority)?;
            let fetch_id = fetch(
                tx,
                Target::Title(id),
                "tmdb",
                &resource,
                completeness,
                false,
                time,
            )?;
            write_facts(tx, fetch_id, &details.facts)?;
            // Reduced legacy DTOs do not satisfy today's rich detail request,
            // but the fields they actually retain still came from a detail resource.
            import_title(
                tx,
                id,
                &item,
                "tmdb",
                Quality::Detail,
                fetch_id,
                time,
                priority,
            )?;
            for summary in &details.seasons {
                let id = season(tx, id, summary.number, time)?;
                for (key, value) in [
                    ("poster_url", summary.poster_url.as_ref()),
                    ("air_date", summary.air_date.as_ref()),
                    ("overview", summary.overview.as_ref()),
                ] {
                    if let Some(value) = value {
                        field(
                            tx,
                            Target::Season(id),
                            (key, &FieldValue::Text(value.clone())),
                            "tmdb",
                            Quality::Summary,
                            fetch_id,
                            time,
                            priority,
                        )?;
                    }
                }
                if let Some(name) = &summary.name {
                    field(
                        tx,
                        Target::Season(id),
                        ("name", &FieldValue::Text(name.clone())),
                        "tmdb",
                        Quality::Summary,
                        fetch_id,
                        time,
                        priority,
                    )?;
                }
                field(
                    tx,
                    Target::Season(id),
                    (
                        "episode_count",
                        &FieldValue::Integer(i64::from(summary.episode_count)),
                    ),
                    "tmdb",
                    Quality::Summary,
                    fetch_id,
                    time,
                    priority,
                )?;
            }
        }
        TmdbData::Season(data) => {
            let item = TitleObservation {
                kind: MediaKind::Series,
                identifiers: vec![ExternalIdentifier {
                    namespace: "tmdb".into(),
                    external_id: data.series_id.to_string(),
                }],
                fields: vec![],
                ratings: vec![],
            };
            let series = resolve_title(tx, &item, time, priority)?;
            let id = season(tx, series, data.season, time)?;
            let fetch_id = fetch(
                tx,
                Target::Season(id),
                "tmdb",
                &resource,
                completeness,
                data.episodes.is_empty(),
                time,
            )?;
            if let Some(url) = &data.poster_url {
                field(
                    tx,
                    Target::Season(id),
                    ("poster_url", &FieldValue::Text(url.clone())),
                    "tmdb",
                    Quality::Detail,
                    fetch_id,
                    time,
                    priority,
                )?;
            }
            // A season resource supplies the parent identity and child metadata;
            // it does not assert title detail success.
            import_title(
                tx,
                series,
                &item,
                "tmdb",
                Quality::Summary,
                fetch_id,
                time,
                priority,
            )?;
            import_episodes(tx, series, &data.episodes, "tmdb", fetch_id, time, priority)?;
        }
    }
    Ok(())
}
fn title(tx: &Transaction<'_>, id: i64, kind: MediaKind) -> Result<TmdbTitle, CatalogError> {
    let record = CatalogService::title_record(tx, id)?;
    if record.kind != kind {
        return Err(CatalogError::InvalidCache);
    }
    let external = record
        .identifiers
        .iter()
        .find(|id| id.namespace == "tmdb")
        .ok_or(CatalogError::InvalidCache)?;
    let prefix = if kind == MediaKind::Movie {
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
                .map(|y| y.to_string())
        });
    Ok(TmdbTitle {
        id: external
            .external_id
            .parse()
            .map_err(|_| CatalogError::InvalidCache)?,
        kind,
        name: record
            .text("name")
            .ok_or(CatalogError::InvalidCache)?
            .into(),
        overview: record.text("overview").map(str::to_owned),
        release_info,
        poster_url: record.text("poster_url").map(str::to_owned),
        display: Arc::new(CatalogService::display_facts(&record)),
        imdb: record
            .identifiers
            .iter()
            .find(|id| id.namespace == "imdb")
            .map(|id| ImdbId::new(id.external_id.clone()).map_err(|_| CatalogError::InvalidCache))
            .transpose()?,
    })
}
pub(super) fn read(
    service: &mut CatalogService,
    query: &TmdbQuery,
) -> Result<Option<CachedTmdb>, CatalogError> {
    let resource = crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
    let tx = service.connection.transaction()?;
    let (data, fetched_at) = match query {
        TmdbQuery::Similar { kind, page, .. } => {
            let Some(saved) = read_page(&tx, "tmdb", &resource, *page)? else {
                return Ok(None);
            };
            (
                TmdbData::Page(TmdbPage {
                    titles: saved
                        .ids
                        .into_iter()
                        .map(|id| title(&tx, id, *kind))
                        .collect::<Result<_, _>>()?,
                    page: *page,
                    total_pages: saved.total.ok_or(CatalogError::InvalidCache)?,
                }),
                saved.fetched_at,
            )
        }
        TmdbQuery::Browse { kind, page } | TmdbQuery::Discover { kind, page, .. } => {
            let Some(saved) = read_page(&tx, "tmdb", &resource, *page)? else {
                return Ok(None);
            };
            (
                TmdbData::Page(TmdbPage {
                    titles: saved
                        .ids
                        .into_iter()
                        .map(|id| title(&tx, id, *kind))
                        .collect::<Result<_, _>>()?,
                    page: *page,
                    total_pages: saved.total.ok_or(CatalogError::InvalidCache)?,
                }),
                saved.fetched_at,
            )
        }
        TmdbQuery::Search { query, page } => {
            let Some(saved) = read_page(&tx, "tmdb", &resource, *page)? else {
                return Ok(None);
            };
            (
                TmdbData::Page(TmdbPage {
                    titles: saved
                        .ids
                        .into_iter()
                        .map(|id| title(&tx, id, query.kind()))
                        .collect::<Result<_, _>>()?,
                    page: *page,
                    total_pages: saved.total.ok_or(CatalogError::InvalidCache)?,
                }),
                saved.fetched_at,
            )
        }
        TmdbQuery::Details { kind, id: external } => {
            let Some(id) = title_id(&tx, *kind, "tmdb", &external.to_string())? else {
                return Ok(None);
            };
            let Some(time) = detail_time(&tx, Target::Title(id), "tmdb", &resource)? else {
                return Ok(None);
            };
            let title = title(&tx, id, *kind)?;
            let imdb: Option<String> = tx.query_row("SELECT external_id FROM title_identifiers WHERE title_id=?1 AND namespace='imdb'",params![id],|row|row.get(0)).optional()?;
            let imdb = imdb
                .map(|s| ImdbId::new(s).map_err(|_| CatalogError::InvalidCache))
                .transpose()?;
            let genres: Option<String> = tx.query_row(
                "SELECT genres_json FROM titles WHERE id=?1",
                params![id],
                |row| row.get(0),
            )?;
            let genres = genres
                .map(|s| serde_json::from_str(&s).map_err(|_| CatalogError::InvalidCache))
                .transpose()?
                .unwrap_or_default();
            let mut seasons = Vec::new();
            for record in CatalogService::season_records(&tx, id)? {
                if let Some(count) = record.integer("episode_count") {
                    seasons.push(TmdbSeasonSummary {
                        number: record.number,
                        name: record.text("name").map(str::to_owned),
                        episode_count: u32::try_from(count)
                            .map_err(|_| CatalogError::InvalidCache)?,
                        poster_url: record.text("poster_url").map(str::to_owned),
                        air_date: record.text("air_date").map(str::to_owned),
                        overview: record.text("overview").map(str::to_owned),
                    });
                }
            }
            let backdrop_url = CatalogService::title_record(&tx, id)?
                .text("backdrop_url")
                .map(str::to_owned);
            (
                TmdbData::Details(TmdbDetails {
                    facts: read_facts(&tx, id, "tmdb", &resource)?,
                    backdrop_url,
                    title,
                    imdb,
                    genres,
                    seasons,
                }),
                time,
            )
        }
        TmdbQuery::Season {
            series_id,
            season: number,
        } => {
            let Some(series) = title_id(&tx, MediaKind::Series, "tmdb", &series_id.to_string())?
            else {
                return Ok(None);
            };
            let id: Option<i64> = tx
                .query_row(
                    "SELECT id FROM seasons WHERE series_id=?1 AND season_number=?2",
                    params![series, number],
                    |row| row.get(0),
                )
                .optional()?;
            let Some(id) = id else {
                return Ok(None);
            };
            let Some(time) = detail_time(&tx, Target::Season(id), "tmdb", &resource)? else {
                return Ok(None);
            };
            (
                TmdbData::Season(TmdbSeason {
                    poster_url: CatalogService::season_records(&tx, series)?
                        .into_iter()
                        .find(|r| r.number == *number)
                        .and_then(|r| r.text("poster_url").map(str::to_owned)),
                    series_id: *series_id,
                    season: *number,
                    episodes: read_episodes(&tx, series, Some(*number))?,
                }),
                time,
            )
        }
    };
    Ok(Some(CachedTmdb { data, fetched_at }))
}

pub(super) fn write_rich(
    tx: &Transaction<'_>,
    query: &TmdbQuery,
    rich: &[super::normalized_rich::RichRecord],
    time: i64,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    if rich.is_empty() {
        return Ok(());
    }
    match query {
        TmdbQuery::Similar { kind, page, .. } => {
            return super::normalized_rich::write_page_rich(
                tx,
                "tmdb",
                &crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?,
                *page,
                *kind,
                rich,
                time,
                priority,
            );
        }
        TmdbQuery::Browse { kind, page } | TmdbQuery::Discover { kind, page, .. } => {
            return super::normalized_rich::write_page_rich(
                tx,
                "tmdb",
                &crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?,
                *page,
                *kind,
                rich,
                time,
                priority,
            );
        }
        TmdbQuery::Search {
            query: search,
            page,
        } => {
            return super::normalized_rich::write_page_rich(
                tx,
                "tmdb",
                &crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?,
                *page,
                search.kind(),
                rich,
                time,
                priority,
            );
        }
        _ => {}
    }
    let (kind, external, target) = match query {
        TmdbQuery::Details { kind, id } => (*kind, *id, None),
        TmdbQuery::Season { series_id, season } => (MediaKind::Series, *series_id, Some(*season)),
        _ => return Err(CatalogError::InvalidMetadata),
    };
    let id =
        title_id(tx, kind, "tmdb", &external.to_string())?.ok_or(CatalogError::InvalidMetadata)?;
    let resource = crate::tmdb::cache_query_identity(query).map_err(CatalogError::Tmdb)?;
    let (column, owner) = if let Some(number) = target {
        ("season_id", season(tx, id, number, time)?)
    } else {
        ("title_id", id)
    };
    let fetch_id:i64=tx.query_row(&format!("SELECT id FROM metadata_fetches WHERE {column}=?1 AND provider='tmdb' AND resource_key=?2"),params![owner,resource],|row|row.get(0))?;
    super::normalized_rich::write(tx, id, rich, "tmdb", fetch_id, time, priority)
}

#[cfg(test)]
mod display_projection_tests {
    use super::*;
    #[test]
    fn all_season_artwork_survives_sql_and_old_complete_rows_refresh_once()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let mut service = CatalogService::open(&directory.path().join("catalog"))?;
        let query = TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 42,
        };
        let data = TmdbData::Details(TmdbDetails {
            facts: Default::default(),
            backdrop_url: None,
            title: TmdbTitle {
                id: 42,
                kind: MediaKind::Series,
                name: "Series".into(),
                overview: None,
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            },
            imdb: None,
            genres: vec![],
            seasons: (1..=3)
                .map(|number| TmdbSeasonSummary {
                    number,
                    name: Some(format!("Season {number}")),
                    episode_count: 8,
                    poster_url: (number != 3)
                        .then(|| format!("https://image.tmdb.org/t/p/w500/season{number}.jpg")),
                    air_date: Some("2024-03-12".into()),
                    overview: Some("Season overview".into()),
                })
                .collect(),
        });
        service.save_tmdb(&query, data.clone())?;
        let saved = service.read_tmdb(&query)?.ok_or("missing details")?;
        let TmdbData::Details(details) = saved.data else {
            return Err("wrong resource".into());
        };
        assert_eq!(details.seasons.len(), 3);
        assert!(details.seasons[0].poster_url.is_some());
        assert!(details.seasons[1].poster_url.is_some());
        assert!(details.seasons[2].poster_url.is_none());
        assert_eq!(details.seasons[1].air_date.as_deref(), Some("2024-03-12"));
        assert_eq!(
            details.seasons[1].overview.as_deref(),
            Some("Season overview")
        );
        let current = crate::tmdb::cache_query_identity(&query)?;
        let old = current
            .strip_suffix(":season-art-v1")
            .ok_or("identity suffix")?;
        service.connection.execute(
            "UPDATE metadata_fetches SET resource_key=?1 WHERE resource_key=?2",
            params![old, current],
        )?;
        assert!(service.read_tmdb(&query)?.is_none());
        service.save_tmdb(&query, data)?;
        assert!(service.read_tmdb(&query)?.is_some());
        assert!(service.read_tmdb(&query)?.is_some()); // null poster is complete, no loop
        Ok(())
    }

    #[test]
    fn actual_tmdb_read_exposes_retained_facts_and_only_genuine_imdb_scores()
    -> Result<(), Box<dyn std::error::Error>> {
        let directory = tempfile::tempdir()?;
        let mut service = CatalogService::open(&directory.path().join("catalog"))?;
        let query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 42,
        };
        let imdb = ImdbId::new("tt1234567")?;
        let data = TmdbData::Details(TmdbDetails {
            facts: kinema_core::catalog::TitleFacts {
                tagline: Some("Retained facts".into()),
                ..Default::default()
            },
            backdrop_url: None,
            title: TmdbTitle {
                id: 42,
                kind: MediaKind::Movie,
                name: "Synthetic".into(),
                overview: None,
                release_info: Some("2024-03-12".into()),
                poster_url: Some("https://image.tmdb.org/t/p/w500/poster.png".into()),
                display: Default::default(),
                imdb: None,
            },
            imdb: Some(imdb.clone()),
            genres: vec!["Drama".into()],
            seasons: vec![],
        });
        service.save_tmdb(&query, data)?;
        let tx = service.connection.transaction()?;
        let rich = normalized_rich::tmdb(
            &serde_json::json!({"runtime":93,"vote_average":9.8,"vote_count":12}),
            &query,
        )?;
        write_rich(&tx, &query, &rich, 20, &Default::default())?;
        tx.commit()?;
        let saved = service.read_tmdb(&query)?.ok_or("missing TMDB cache")?;
        let TmdbData::Details(details) = saved.data else {
            return Err("wrong resource".into());
        };
        assert_eq!(details.facts.tagline.as_deref(), Some("Retained facts"));
        assert_eq!(details.title.year(), Some(2024));
        assert_eq!(
            details.title.poster_url(),
            Some("https://image.tmdb.org/t/p/w500/poster.png")
        );
        assert_eq!(details.title.runtime_minutes(), Some(93));
        assert_eq!(details.title.genres(), &["Drama"]);
        assert_eq!(details.title.season_count(), None);
        assert_eq!(details.title.imdb.as_ref(), Some(&imdb));
        assert_eq!(details.title.imdb_rating(), None); // TMDB 9.8 is never IMDb.
        let tx = service.connection.transaction()?;
        normalized_pages::write_cinemeta_details(
            &tx,
            &TitleDetails {
                facts: Default::default(),
                title: CatalogTitle::new(imdb, MediaKind::Movie, "Synthetic", None)?,
                poster_url: None,
                background_url: None,
                release_info: None,
                genres: vec![],
                episodes: vec![],
            },
            30,
            true,
            &Default::default(),
        )?;
        let id = title_id(&tx, MediaKind::Movie, "tmdb", "42")?.ok_or("missing canonical title")?;
        let fetch_id: i64 = tx.query_row(
            "SELECT id FROM metadata_fetches WHERE title_id=?1 AND provider='cinemeta'",
            params![id],
            |row| row.get(0),
        )?;
        let rich =
            normalized_rich::cinemeta(&serde_json::json!({"type":"movie", "imdbRating":"0.0"}))?;
        normalized_rich::write(
            &tx,
            id,
            &rich,
            "cinemeta",
            fetch_id,
            30,
            &Default::default(),
        )?;
        tx.commit()?;
        let saved = service.read_tmdb(&query)?.ok_or("missing TMDB cache")?;
        let TmdbData::Details(details) = saved.data else {
            return Err("wrong resource".into());
        };
        assert_eq!(details.title.imdb_rating(), Some(0.0));
        // A retained score does not manufacture an external identifier.
        service.connection.execute(
            "DELETE FROM title_identifiers WHERE namespace='imdb' AND title_id=?1",
            params![id],
        )?;
        let tx = service.connection.transaction()?;
        let title = title(&tx, id, MediaKind::Movie)?;
        assert_eq!(title.imdb, None);
        assert_eq!(title.imdb_rating(), Some(0.0));
        Ok(())
    }
}

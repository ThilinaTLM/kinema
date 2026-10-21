//! Synthetic canonical read/write acceptance tests (temporary SQLite only).
use super::*;
use super::{normalized::*, normalized_pages::*};
use crate::tmdb::{TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbTitle};
use kinema_core::catalog_normalized::{FieldValue, Quality, RatingObservation};
type TestResult = Result<(), Box<dyn Error>>;
fn movie(id: &str, name: &str) -> Result<CatalogTitle, Box<dyn Error>> {
    Ok(CatalogTitle::new(
        ImdbId::new(id)?,
        MediaKind::Movie,
        name,
        None,
    )?)
}
fn tmdb_details(id: u64, imdb: Option<&str>) -> Result<TmdbData, Box<dyn Error>> {
    Ok(TmdbData::Details(TmdbDetails {
        facts: Default::default(),
        backdrop_url: None,
        title: TmdbTitle {
            id,
            kind: MediaKind::Movie,
            name: "Synthetic title".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        },
        imdb: imdb.map(ImdbId::new).transpose()?,
        genres: vec![],
        seasons: vec![],
    }))
}

#[test]
fn canonical_titles_and_episodes_are_primary_not_legacy_json() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let title = CatalogTitle::new(
        ImdbId::new("tt1000001")?,
        MediaKind::Series,
        "Synthetic series",
        None,
    )?;
    let details = TitleDetails {
        facts: Default::default(),
        title,
        poster_url: None,
        background_url: None,
        release_info: None,
        genres: vec![],
        episodes: vec![CatalogEpisode {
            still_url: None,
            runtime_minutes: None,
            season: 0,
            number: 1,
            name: Some("Original episode".into()),
            overview: None,
            released: None,
        }],
    };
    let tx = service.connection.transaction()?;
    write_cinemeta_details(&tx, &details, 10, true, &Default::default())?;
    tx.commit()?;
    service.connection.execute("INSERT INTO cinemeta_v3_details VALUES ('series','tt1000001',1,'unchanged legacy evidence')",[])?;
    service.connection.execute_batch(
        "UPDATE titles SET name='Canonical series'; UPDATE episodes SET name='Canonical episode';",
    )?;
    let saved = service
        .read_details(MediaKind::Series, details.title.id())?
        .ok_or("missing canonical details")?;
    assert_eq!(saved.details.title.name(), "Canonical series");
    assert_eq!(
        saved.details.episodes[0].name.as_deref(),
        Some("Canonical episode")
    );
    let payload: String =
        service
            .connection
            .query_row("SELECT payload FROM cinemeta_v3_details", [], |r| r.get(0))?;
    assert_eq!(payload, "unchanged legacy evidence");
    Ok(())
}

#[test]
fn explicit_imdb_mapping_shares_title_and_tmdb_numeric_ids_remain_kind_scoped() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    write_page(
        &tx,
        "cinemeta",
        "seed-v1",
        1,
        None,
        &[imdb_observation(&movie("tt1000002", "Cinemeta")?)],
        1,
        &Default::default(),
    )?;
    let query = TmdbQuery::Details {
        kind: MediaKind::Movie,
        id: 42,
    };
    normalized_tmdb::write(
        &tx,
        &query,
        &tmdb_details(42, Some("tt1000002"))?,
        2,
        true,
        &Default::default(),
    )?;
    assert_eq!(
        title_id(&tx, MediaKind::Movie, "imdb", "tt1000002")?,
        title_id(&tx, MediaKind::Movie, "tmdb", "42")?
    );
    let series = TmdbData::Page(TmdbPage {
        titles: vec![TmdbTitle {
            id: 42,
            kind: MediaKind::Series,
            name: "Different series".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        }],
        page: 1,
        total_pages: 1,
    });
    normalized_tmdb::write(
        &tx,
        &TmdbQuery::Browse {
            kind: MediaKind::Series,
            page: 1,
        },
        &series,
        3,
        true,
        &Default::default(),
    )?;
    assert_ne!(
        title_id(&tx, MediaKind::Movie, "tmdb", "42")?,
        title_id(&tx, MediaKind::Series, "tmdb", "42")?
    );
    tx.commit()?;
    Ok(())
}

#[test]
fn separately_cached_provider_identities_join_only_after_explicit_mapping() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    {
        let tx = service.connection.transaction()?;
        write_page(
            &tx,
            "cinemeta",
            "seed-v1",
            1,
            None,
            &[imdb_observation(&movie("tt1000003", "Cinemeta")?)],
            1,
            &Default::default(),
        )?;
        normalized_tmdb::write(
            &tx,
            &TmdbQuery::Details {
                kind: MediaKind::Movie,
                id: 43,
            },
            &tmdb_details(43, None)?,
            2,
            true,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    let query = TmdbQuery::Details {
        kind: MediaKind::Movie,
        id: 43,
    };
    let before = service.read_tmdb(&query)?;
    let saved = service.save_tmdb(&query, tmdb_details(43, Some("tt1000003"))?)?;
    assert_ne!(Some(saved.clone()), before);
    assert_eq!(service.read_tmdb(&query)?, Some(saved));
    let count: i64 = service
        .connection
        .query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 1);
    let message = CatalogError::IdentityConflict.to_string();
    assert!(!message.contains("Cinemeta"));
    assert!(!message.contains("tt1000003"));
    assert!(
        !CatalogError::InvalidMetadata
            .to_string()
            .contains("Cinemeta")
    );
    Ok(())
}

#[test]
fn identical_refresh_advances_fetch_not_material_time_and_summary_preserves_detail_provenance()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let mut item = imdb_observation(&movie("tt1000004", "Rich")?);
    item.fields
        .push(("overview".into(), FieldValue::Text("Full overview".into())));
    {
        let tx = service.connection.transaction()?;
        let id = resolve_title(&tx, &item, 10, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "cinemeta",
            "detail-v1",
            Quality::Detail,
            false,
            10,
        )?;
        import_title(
            &tx,
            id,
            &item,
            "cinemeta",
            Quality::Detail,
            f,
            10,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    {
        let tx = service.connection.transaction()?;
        let id = resolve_title(&tx, &item, 20, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "cinemeta",
            "detail-v1",
            Quality::Detail,
            false,
            20,
        )?;
        import_title(
            &tx,
            id,
            &item,
            "cinemeta",
            Quality::Detail,
            f,
            20,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    let (updated, fetched): (i64, i64) = service.connection.query_row(
        "SELECT t.updated_at,f.fetched_at FROM titles t JOIN metadata_fetches f ON f.title_id=t.id",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!((updated, fetched), (10, 20));
    {
        let tx = service.connection.transaction()?;
        write_page(
            &tx,
            "cinemeta",
            "summary-v1",
            1,
            None,
            &[imdb_observation(&movie("tt1000004", "Short")?)],
            30,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    let (name,overview,provider,quality,observed):(String,String,String,String,i64)=service.connection.query_row("SELECT t.name,t.overview,p.supplied_by,p.quality,p.observed_at FROM titles t JOIN field_provenance p ON p.title_id=t.id AND p.field_name='overview'",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?)))?;
    assert_eq!(
        (
            name.as_str(),
            overview.as_str(),
            provider.as_str(),
            quality.as_str(),
            observed
        ),
        ("Rich", "Full overview", "cinemeta", "detail", 20)
    );
    Ok(())
}

#[test]
fn ratings_are_supplier_scoped_atomic_observations_and_reject_cross_entity_imdb() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    let item = imdb_observation(&CatalogTitle::new(
        ImdbId::new("tt1000005")?,
        MediaKind::Series,
        "Series",
        None,
    )?);
    let id = resolve_title(&tx, &item, 1, &Default::default())?;
    let f = fetch(
        &tx,
        Target::Title(id),
        "cinemeta",
        "series-detail-v1",
        Quality::Detail,
        false,
        1,
    )?;
    import_title(
        &tx,
        id,
        &item,
        "cinemeta",
        Quality::Detail,
        f,
        1,
        &Default::default(),
    )?;
    let mut observation = RatingObservation {
        source: "imdb".into(),
        metric: "score".into(),
        supplied_by: "cinemeta".into(),
        value: Some(8.0),
        vote_count: Some(100),
        source_updated_at: None,
    };
    rating(&tx, Target::Title(id), &observation, "cinemeta", f, 1)?;
    observation.value = Some(9.0);
    observation.vote_count = None;
    rating(&tx, Target::Title(id), &observation, "cinemeta", f, 2)?;
    let votes: Option<i64> = tx.query_row("SELECT vote_count FROM ratings", [], |r| r.get(0))?;
    assert_eq!(votes, None);
    let episode = episode(&tx, season(&tx, id, 1, 1)?, 1, 1)?;
    assert_eq!(
        identifier(
            &tx,
            Target::Episode(episode),
            None,
            &item.identifiers[0],
            f,
            3
        ),
        Err(CatalogError::IdentityConflict)
    );
    observation.value = Some(100.0);
    assert_eq!(
        rating(&tx, Target::Title(id), &observation, "cinemeta", f, 3),
        Err(CatalogError::InvalidMetadata)
    );
    Ok(())
}

#[test]
fn grouped_dates_change_year_atomically_and_inconsistent_partial_updates_rollback() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    for (time, date, year) in [(1, "2025-03-12", 2025), (2, "2026-04-13", 2026)] {
        let tx = service.connection.transaction()?;
        let mut item = imdb_observation(&movie("tt1000006", "Dated movie")?);
        item.fields.extend([
            ("release_date".into(), FieldValue::Text(date.into())),
            ("release_year".into(), FieldValue::Integer(year)),
        ]);
        let id = resolve_title(&tx, &item, time, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "cinemeta",
            "date-v1",
            Quality::Detail,
            false,
            time,
        )?;
        import_title(
            &tx,
            id,
            &item,
            "cinemeta",
            Quality::Detail,
            f,
            time,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    {
        let tx = service.connection.transaction()?;
        let mut item = imdb_observation(&movie("tt1000006", "Dated movie")?);
        item.fields
            .push(("release_year".into(), FieldValue::Integer(2027)));
        let id = resolve_title(&tx, &item, 3, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "cinemeta",
            "date-v1",
            Quality::Detail,
            false,
            3,
        )?;
        assert_eq!(
            import_title(
                &tx,
                id,
                &item,
                "cinemeta",
                Quality::Detail,
                f,
                3,
                &Default::default()
            ),
            Err(CatalogError::InvalidMetadata)
        );
    }
    let (date,year,time):(String,i64,i64)=service.connection.query_row("SELECT t.release_date,t.release_year,f.fetched_at FROM titles t JOIN metadata_fetches f ON f.title_id=t.id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    assert_eq!((date.as_str(), year, time), ("2026-04-13", 2026, 2));
    Ok(())
}

#[test]
fn page_pruning_is_a_miss_with_retained_identity_and_fetch_stubs() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    write_page(
        &tx,
        "cinemeta",
        "retained-v1",
        1,
        None,
        &[imdb_observation(&movie("tt1000007", "Retained")?)],
        1,
        &Default::default(),
    )?;
    tx.commit()?;
    service.connection.execute_batch(
        "DELETE FROM catalog_page_items; UPDATE catalog_pages SET membership_available=0;",
    )?;
    let tx = service.connection.transaction()?;
    assert!(read_page(&tx, "cinemeta", "retained-v1", 1)?.is_none());
    assert!(title_id(&tx, MediaKind::Movie, "imdb", "tt1000007")?.is_some());
    assert!(tx.execute("DELETE FROM metadata_fetches", []).is_err());
    assert!(tx.execute("DELETE FROM catalog_pages", []).is_err());
    Ok(())
}

#[test]
fn rich_tmdb_dtos_supply_real_child_ids_artwork_and_distinct_rating_sources() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let query = TmdbQuery::Season {
        series_id: 81,
        season: 0,
    };
    let raw = serde_json::json!({"id":91,"season_number":0,"name":"Specials","poster_path":"/season.jpg","episodes":[{"id":101,"season_number":0,"episode_number":1,"name":"Special","runtime":44,"still_path":"/episode.jpg","vote_average":0,"vote_count":0}]});
    let rich = normalized_rich::tmdb(&raw, &query)?;
    let data = TmdbData::Season(crate::tmdb::TmdbSeason {
        poster_url: None,
        series_id: 81,
        season: 0,
        episodes: vec![CatalogEpisode {
            still_url: None,
            runtime_minutes: None,
            season: 0,
            number: 1,
            name: Some("Special".into()),
            overview: None,
            released: None,
        }],
    });
    let tx = service.connection.transaction()?;
    normalized_tmdb::write(&tx, &query, &data, 1, true, &Default::default())?;
    normalized_tmdb::write_rich(&tx, &query, &rich, 1, &Default::default())?;
    let (season,episode,still,source,value,votes):(String,String,String,String,f64,i64)=tx.query_row("SELECT si.external_id,ei.external_id,e.still_url,r.rating_source,r.value,r.vote_count FROM seasons s JOIN season_identifiers si ON si.season_id=s.id JOIN episodes e ON e.season_id=s.id JOIN episode_identifiers ei ON ei.episode_id=e.id JOIN ratings r ON r.episode_id=e.id",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?)))?;
    assert_eq!(
        (
            season.as_str(),
            episode.as_str(),
            source.as_str(),
            value,
            votes
        ),
        ("91", "101", "tmdb", 0.0, 0)
    );
    assert!(still.ends_with("/episode.jpg"));
    tx.commit()?;
    let saved = normalized_tmdb::read(&mut service, &query)?.ok_or("missing projected season")?;
    let TmdbData::Season(saved) = saved.data else {
        return Err("wrong projection".into());
    };
    assert!(
        saved
            .poster_url
            .as_deref()
            .is_some_and(|u| u.ends_with("/season.jpg"))
    );
    assert!(
        saved.episodes[0]
            .still_url
            .as_deref()
            .is_some_and(|u| u.ends_with("/episode.jpg"))
    );
    assert_eq!(saved.episodes[0].runtime_minutes, Some(44));
    Ok(())
}

#[test]
fn episode_art_and_season_art_are_projected_and_not_erased_by_reduced_updates() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let query = TmdbQuery::Details {
        kind: MediaKind::Series,
        id: 91,
    };
    let details = TmdbData::Details(TmdbDetails {
        facts: Default::default(),
        backdrop_url: Some("https://image.tmdb.org/t/p/w500/backdrop.jpg".into()),
        title: TmdbTitle {
            id: 91,
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
        seasons: vec![],
    });
    let season_query = TmdbQuery::Season {
        series_id: 91,
        season: 2,
    };
    let mut season = crate::tmdb::TmdbSeason {
        poster_url: Some("https://image.tmdb.org/t/p/w500/season.jpg".into()),
        series_id: 91,
        season: 2,
        episodes: vec![CatalogEpisode {
            season: 2,
            number: 1,
            name: Some("Episode".into()),
            overview: None,
            released: None,
            still_url: Some("https://image.tmdb.org/t/p/w500/still.jpg".into()),
            runtime_minutes: Some(48),
        }],
    };
    let tx = service.connection.transaction()?;
    super::normalized_tmdb::write(&tx, &query, &details, 1, true, &Default::default())?;
    super::normalized_tmdb::write(
        &tx,
        &season_query,
        &TmdbData::Season(season.clone()),
        1,
        true,
        &Default::default(),
    )?;
    tx.commit()?;
    season.poster_url = None;
    season.episodes[0].still_url = None;
    season.episodes[0].runtime_minutes = None;
    let tx = service.connection.transaction()?;
    super::normalized_tmdb::write(
        &tx,
        &season_query,
        &TmdbData::Season(season),
        2,
        true,
        &Default::default(),
    )?;
    tx.commit()?;
    let saved = super::normalized_tmdb::read(&mut service, &query)?.ok_or("details")?;
    let TmdbData::Details(saved) = saved.data else {
        return Err("details type".into());
    };
    assert!(saved.backdrop_url.is_some());
    let saved = super::normalized_tmdb::read(&mut service, &season_query)?.ok_or("season")?;
    let TmdbData::Season(saved) = saved.data else {
        return Err("season type".into());
    };
    assert!(saved.poster_url.is_some());
    assert!(saved.episodes[0].still_url.is_some());
    assert_eq!(saved.episodes[0].runtime_minutes, Some(48));
    Ok(())
}

#[test]
fn similar_pages_are_isolated_from_browse_and_other_titles() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    for (query, id) in [
        (TmdbQuery::Similar { id: 91, page: 1 }, 201),
        (TmdbQuery::Similar { id: 92, page: 1 }, 202),
        (
            TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 1,
            },
            203,
        ),
    ] {
        let data = TmdbData::Page(TmdbPage {
            page: 1,
            total_pages: 2,
            titles: vec![TmdbTitle {
                id,
                kind: MediaKind::Movie,
                name: format!("Film {id}"),
                overview: None,
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            }],
        });
        let tx = service.connection.transaction()?;
        super::normalized_tmdb::write(&tx, &query, &data, 1, true, &Default::default())?;
        tx.commit()?;
    }
    for (query, id) in [
        (TmdbQuery::Similar { id: 91, page: 1 }, 201),
        (TmdbQuery::Similar { id: 92, page: 1 }, 202),
        (
            TmdbQuery::Browse {
                kind: MediaKind::Movie,
                page: 1,
            },
            203,
        ),
    ] {
        let saved = super::normalized_tmdb::read(&mut service, &query)?.ok_or("page")?;
        let TmdbData::Page(page) = saved.data else {
            return Err("page type".into());
        };
        assert_eq!(page.titles[0].id, id);
    }
    Ok(())
}

#[test]
fn cinemeta_thumbnail_survives_reduced_episode_updates() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let title = CatalogTitle::new(ImdbId::new("tt1234567")?, MediaKind::Series, "Series", None)?;
    let mut details = TitleDetails {
        facts: Default::default(),
        title,
        poster_url: None,
        background_url: Some("https://images.example.test/show.jpg".into()),
        release_info: None,
        genres: vec![],
        episodes: vec![CatalogEpisode {
            season: 1,
            number: 1,
            name: Some("Pilot".into()),
            overview: None,
            released: None,
            still_url: Some("https://images.example.test/still.jpg".into()),
            runtime_minutes: None,
        }],
    };
    let tx = service.connection.transaction()?;
    write_cinemeta_details(&tx, &details, 1, true, &Default::default())?;
    tx.commit()?;
    details.episodes[0].still_url = None;
    let tx = service.connection.transaction()?;
    write_cinemeta_details(&tx, &details, 2, true, &Default::default())?;
    tx.commit()?;
    let saved = service
        .read_details(MediaKind::Series, details.title.id())?
        .ok_or("details")?;
    assert_eq!(
        saved.details.episodes[0].still_url.as_deref(),
        Some("https://images.example.test/still.jpg")
    );
    Ok(())
}

#[test]
fn ranked_providers_merge_fields_and_keep_supplier_scoped_ratings() -> TestResult {
    use kinema_core::provider::{ProviderId, ProviderPriority};
    for primary in [ProviderId::Cinemeta, ProviderId::Tmdb] {
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        service.set_provider_priority(ProviderPriority::new([primary]));
        let priority = service.provider_priority.clone();
        let tx = service.connection.transaction()?;
        let mut item = imdb_observation(&movie("tt1000091", "Cinemeta title")?);
        item.fields.push((
            "overview".into(),
            FieldValue::Text("Cinemeta overview".into()),
        ));
        item.ratings.push(RatingObservation {
            source: "imdb".into(),
            metric: "score".into(),
            supplied_by: "cinemeta".into(),
            value: Some(7.0),
            vote_count: Some(100),
            source_updated_at: None,
        });
        let id = resolve_title(&tx, &item, 100, &priority)?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "cinemeta",
            "detail-v1",
            Quality::Detail,
            false,
            100,
        )?;
        import_title(
            &tx,
            id,
            &item,
            "cinemeta",
            Quality::Detail,
            f,
            100,
            &priority,
        )?;
        item.fields = vec![
            ("overview".into(), FieldValue::Text("TMDB overview".into())),
            ("runtime_minutes".into(), FieldValue::Integer(95)),
            (
                "genres_json".into(),
                FieldValue::Labels(vec!["Drama".into()]),
            ),
        ];
        item.ratings[0].supplied_by = "tmdb".into();
        item.ratings[0].value = Some(8.0);
        // Deliberately older and lower quality: supplier priority wins first.
        let f = fetch(
            &tx,
            Target::Title(id),
            "tmdb",
            "summary-v1",
            Quality::Summary,
            false,
            1,
        )?;
        import_title(&tx, id, &item, "tmdb", Quality::Summary, f, 1, &priority)?;
        let record = CatalogService::title_record(&tx, id)?;
        assert_eq!(
            record.text("overview"),
            Some(if primary == ProviderId::Tmdb {
                "TMDB overview"
            } else {
                "Cinemeta overview"
            })
        );
        assert_eq!(record.integer("runtime_minutes"), Some(95));
        assert_eq!(
            record.fields.get("genres_json"),
            Some(&FieldValue::Labels(vec!["Drama".into()]))
        );
        let overview = record
            .provenance
            .iter()
            .find(|field| field.field_name == "overview")
            .ok_or("overview provenance missing")?;
        assert_eq!(overview.supplied_by, primary.as_str());
        assert_eq!(record.ratings.len(), 2);
        for (supplier, value) in [("cinemeta", 7.0), ("tmdb", 8.0)] {
            assert!(
                record
                    .ratings
                    .iter()
                    .any(|rating| rating.observation.supplied_by == supplier
                        && rating.observation.value == Some(value))
            );
        }
        tx.commit()?;
    }
    Ok(())
}

#[test]
fn unknown_field_suppliers_rank_below_known_providers() {
    use kinema_core::provider::ProviderPriority;
    let priority = ProviderPriority::default();
    assert!(accepts_supplier_field(
        Some(("unknown".into(), "detail".into(), 100)),
        "tmdb",
        Quality::Summary,
        1,
        &priority
    ));
    assert!(!accepts_supplier_field(
        Some(("tmdb".into(), "summary".into(), 1)),
        "unknown",
        Quality::Detail,
        100,
        &priority
    ));
}

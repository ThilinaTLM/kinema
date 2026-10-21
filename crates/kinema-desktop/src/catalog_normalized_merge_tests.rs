//! Approved explicit-identity reconciliation, including full reference graphs.
use super::*;
use super::{normalized::*, normalized_pages::*};
use crate::tmdb::{TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbSeason, TmdbTitle};
use kinema_core::catalog_normalized::{ExternalIdentifier, Quality, RatingObservation};
type TestResult = Result<(), Box<dyn Error>>;
fn series(id: &str) -> Result<CatalogTitle, Box<dyn Error>> {
    Ok(CatalogTitle::new(
        ImdbId::new(id)?,
        MediaKind::Series,
        "Cinemeta rich series",
        Some("Detailed overview".into()),
    )?)
}

#[test]
fn cinemeta_then_tmdb_browse_then_details_unifies_canonical_title_and_both_pages() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let imdb = ImdbId::new("tt3000001")?;
    {
        let tx = service.connection.transaction()?;
        write_page(
            &tx,
            "cinemeta",
            "cinemeta-page-v1",
            1,
            None,
            &[imdb_observation(&CatalogTitle::new(
                imdb.clone(),
                MediaKind::Movie,
                "Cinemeta",
                None,
            )?)],
            1,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    let page_query = TmdbQuery::Browse {
        kind: MediaKind::Movie,
        page: 1,
    };
    let title = TmdbTitle {
        id: 71,
        kind: MediaKind::Movie,
        name: "TMDB".into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: None,
    };
    service.save_tmdb(
        &page_query,
        TmdbData::Page(TmdbPage {
            titles: vec![title.clone()],
            page: 1,
            total_pages: 1,
        }),
    )?;
    let count: i64 = service
        .connection
        .query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 2);
    service.save_tmdb(
        &TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 71,
        },
        TmdbData::Details(TmdbDetails {
            facts: Default::default(),
            backdrop_url: None,
            title,
            imdb: Some(imdb),
            genres: vec![],
            seasons: vec![],
        }),
    )?;
    let tx = service.connection.transaction()?;
    assert_eq!(
        title_id(&tx, MediaKind::Movie, "imdb", "tt3000001")?,
        title_id(&tx, MediaKind::Movie, "tmdb", "71")?
    );
    let count: i64 = tx.query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 1);
    let old = read_page(&tx, "cinemeta", "cinemeta-page-v1", 1)?.ok_or("missing page")?;
    let new = read_page(
        &tx,
        "tmdb",
        &crate::tmdb::cache_query_identity(&page_query)?,
        1,
    )?
    .ok_or("missing page")?;
    assert_eq!(old.ids, new.ids);
    Ok(())
}

#[test]
fn compatible_graph_reconciliation_preserves_children_provenance_ratings_fetches_and_sources()
-> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let query = TmdbQuery::Season {
        series_id: 81,
        season: 0,
    };
    {
        let tx = service.connection.transaction()?;
        normalized_tmdb::write(
            &tx,
            &query,
            &TmdbData::Season(TmdbSeason {
                poster_url: None,
                series_id: 81,
                season: 0,
                episodes: vec![CatalogEpisode {
                    still_url: None,
                    runtime_minutes: None,
                    season: 0,
                    number: 1,
                    name: Some("TMDB episode".into()),
                    overview: None,
                    released: None,
                }],
            }),
            1,
            true,
            &Default::default(),
        )?;
        let raw = serde_json::json!({"id":91,"season_number":0,"poster_path":"/season.jpg","episodes":[{"id":101,"season_number":0,"episode_number":1,"name":"TMDB episode","still_path":"/still.jpg","vote_average":8,"vote_count":0}]});
        normalized_tmdb::write_rich(
            &tx,
            &query,
            &normalized_rich::tmdb(&raw, &query)?,
            1,
            &Default::default(),
        )?;
        let details = TitleDetails {
            facts: Default::default(),
            title: series("tt3000002")?,
            poster_url: Some("https://images.example.test/poster.jpg".into()),
            background_url: None,
            release_info: None,
            genres: vec!["Drama".into()],
            episodes: vec![
                CatalogEpisode {
                    still_url: None,
                    runtime_minutes: None,
                    season: 0,
                    number: 1,
                    name: Some("IMDb episode".into()),
                    overview: Some("Rich child overview".into()),
                    released: None,
                },
                CatalogEpisode {
                    still_url: None,
                    runtime_minutes: None,
                    season: 1,
                    number: 1,
                    name: None,
                    overview: None,
                    released: None,
                },
            ],
        };
        write_cinemeta_details(&tx, &details, 2, true, &Default::default())?;
        let parent =
            title_id(&tx, MediaKind::Series, "imdb", "tt3000002")?.ok_or("missing IMDb")?;
        let season_id = season(&tx, parent, 0, 2)?;
        let episode_id = episode(&tx, season_id, 1, 2)?;
        let fetch: i64 = tx.query_row(
            "SELECT id FROM metadata_fetches WHERE title_id=?1",
            params![parent],
            |r| r.get(0),
        )?;
        identifier(
            &tx,
            Target::Episode(episode_id),
            None,
            &ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: "tt3000099".into(),
            },
            fetch,
            2,
        )?;
        rating(
            &tx,
            Target::Episode(episode_id),
            &RatingObservation {
                source: "imdb".into(),
                metric: "score".into(),
                supplied_by: "cinemeta".into(),
                value: Some(9.0),
                vote_count: None,
                source_updated_at: None,
            },
            "cinemeta",
            fetch,
            2,
        )?;
        tx.commit()?;
    }
    service.save_sources(
        IndexerKind::Torrentio,
        &PlaybackTarget::Episode {
            series: ImdbId::new("tt3000002")?,
            season: 0,
            episode: 1,
        },
        crate::sources::SourceResults {
            candidates: vec![],
            omitted: 0,
        },
    )?;
    let title = TmdbTitle {
        id: 81,
        kind: MediaKind::Series,
        name: "TMDB series".into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: None,
    };
    service.save_tmdb(
        &TmdbQuery::Details {
            kind: MediaKind::Series,
            id: 81,
        },
        TmdbData::Details(TmdbDetails {
            facts: Default::default(),
            backdrop_url: None,
            title,
            imdb: Some(ImdbId::new("tt3000002")?),
            genres: vec![],
            seasons: vec![],
        }),
    )?;
    let tx = service.connection.transaction()?;
    for (table, count) in [
        ("titles", 1),
        ("seasons", 2),
        ("episodes", 2),
        ("episode_identifiers", 2),
        ("season_identifiers", 1),
        ("ratings", 2),
        ("indexer_snapshots", 1),
    ] {
        let actual: i64 =
            tx.query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        assert_eq!(actual, count, "{table}");
    }
    let (name,overview,still):(String,String,String)=tx.query_row("SELECT name,overview,still_url FROM episodes WHERE episode_number=1 AND season_id=(SELECT id FROM seasons WHERE season_number=0)",[],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?)))?;
    assert_eq!(
        (name.as_str(), overview.as_str()),
        ("IMDb episode", "Rich child overview")
    );
    assert!(still.ends_with("/still.jpg"));
    let dangling: i64 = tx.query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
        r.get(0)
    })?;
    assert_eq!(dangling, 0);
    let supplied:i64=tx.query_row("SELECT count(*) FROM field_provenance p JOIN metadata_fetches f ON f.id=p.fetch_state_id WHERE p.episode_id IS NOT NULL",[],|r|r.get(0))?;
    assert!(supplied >= 3);
    let target:i64=tx.query_row("SELECT s.series_id FROM indexer_snapshots x JOIN episodes e ON e.id=x.episode_id JOIN seasons s ON s.id=e.season_id",[],|r|r.get(0))?;
    assert_eq!(
        Some(target),
        title_id(&tx, MediaKind::Series, "tmdb", "81")?
    );
    Ok(())
}

#[test]
fn contradictory_external_ids_abort_reconciliation_without_mutating_either_graph() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    {
        let tx = service.connection.transaction()?;
        let mut first = imdb_observation(&series("tt3000003")?);
        first.identifiers.push(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "100".into(),
        });
        let id = resolve_title(&tx, &first, 1, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "tmdb",
            "first-v1",
            Quality::Detail,
            false,
            1,
        )?;
        import_title(
            &tx,
            id,
            &first,
            "tmdb",
            Quality::Detail,
            f,
            1,
            &Default::default(),
        )?;
        let second = normalized_tmdb::observation(&TmdbTitle {
            id: 101,
            kind: MediaKind::Series,
            name: "Second".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        });
        let id = resolve_title(&tx, &second, 2, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "tmdb",
            "second-v1",
            Quality::Detail,
            false,
            2,
        )?;
        import_title(
            &tx,
            id,
            &second,
            "tmdb",
            Quality::Detail,
            f,
            2,
            &Default::default(),
        )?;
        tx.commit()?;
    }
    {
        let tx = service.connection.transaction()?;
        let mut incoming = imdb_observation(&series("tt3000003")?);
        incoming.identifiers.push(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "101".into(),
        });
        assert_eq!(
            resolve_title(&tx, &incoming, 3, &Default::default()),
            Err(CatalogError::IdentityConflict)
        );
    }
    let count: i64 = service
        .connection
        .query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 2);
    let ids: Vec<String> = {
        let mut stmt = service.connection.prepare(
            "SELECT external_id FROM title_identifiers WHERE namespace='tmdb' ORDER BY external_id",
        )?;
        stmt.query_map([], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    assert_eq!(ids, ["100", "101"]);
    Ok(())
}

#[test]
fn contradictory_same_coordinate_child_identifiers_rollback_before_graph_changes() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    {
        let tx = service.connection.transaction()?;
        for (imdb, tmdb, child) in [(Some("tt3000004"), None, 501), (None, Some(400), 502)] {
            let item = if let Some(imdb) = imdb {
                imdb_observation(&series(imdb)?)
            } else {
                normalized_tmdb::observation(&TmdbTitle {
                    id: tmdb.ok_or("missing tmdb")?,
                    kind: MediaKind::Series,
                    name: "Series".into(),
                    overview: None,
                    release_info: None,
                    poster_url: None,
                    display: Default::default(),
                    imdb: None,
                })
            };
            let id = resolve_title(&tx, &item, 1, &Default::default())?;
            let f = fetch(
                &tx,
                Target::Title(id),
                "tmdb",
                &format!("series-{id}-v1"),
                Quality::Detail,
                false,
                1,
            )?;
            import_title(
                &tx,
                id,
                &item,
                "tmdb",
                Quality::Detail,
                f,
                1,
                &Default::default(),
            )?;
            let season = season(&tx, id, 1, 1)?;
            identifier(
                &tx,
                Target::Season(season),
                None,
                &ExternalIdentifier {
                    namespace: "tmdb".into(),
                    external_id: child.to_string(),
                },
                f,
                1,
            )?;
        }
        tx.commit()?;
    }
    {
        let tx = service.connection.transaction()?;
        let mut incoming = imdb_observation(&series("tt3000004")?);
        incoming.identifiers.push(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "400".into(),
        });
        assert_eq!(
            resolve_title(&tx, &incoming, 2, &Default::default()),
            Err(CatalogError::IdentityConflict)
        );
    }
    let count: i64 = service
        .connection
        .query_row("SELECT count(*) FROM seasons", [], |r| r.get(0))?;
    assert_eq!(count, 2);
    Ok(())
}

#[test]
fn duplicate_fetch_resources_and_duplicate_page_members_reconcile_without_orphans() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    let first = imdb_observation(&CatalogTitle::new(
        ImdbId::new("tt3000005")?,
        MediaKind::Movie,
        "First",
        None,
    )?);
    let second = normalized_tmdb::observation(&TmdbTitle {
        id: 500,
        kind: MediaKind::Movie,
        name: "Second".into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: None,
    });
    write_page(
        &tx,
        "tmdb",
        "mixed-fixture-v1",
        1,
        None,
        &[first.clone(), second.clone()],
        1,
        &Default::default(),
    )?;
    for (item, time) in [(&first, 2), (&second, 3)] {
        let id = resolve_title(&tx, item, time, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "tmdb",
            "same-fixture-resource-v1",
            Quality::Detail,
            false,
            time,
        )?;
        import_title(
            &tx,
            id,
            item,
            "tmdb",
            Quality::Detail,
            f,
            time,
            &Default::default(),
        )?;
    }
    let mut incoming = first;
    incoming.identifiers.extend(second.identifiers);
    let id = resolve_title(&tx, &incoming, 4, &Default::default())?;
    let page = read_page(&tx, "tmdb", "mixed-fixture-v1", 1)?.ok_or("missing page")?;
    assert_eq!(page.ids, [id]);
    let (count, time): (i64, i64) = tx.query_row(
        "SELECT count(*),max(fetched_at) FROM metadata_fetches WHERE title_id=?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!((count, time), (1, 3));
    let record = CatalogService::title_record(&tx, id)?;
    assert_eq!(record.text("name"), Some("Second"));
    assert_eq!(record.provenance[0].observed_at, 3);
    Ok(())
}

#[test]
fn tied_fetch_merge_keeps_acquired_facts_over_unacquired_payload() -> TestResult {
    use kinema_core::catalog::{ImdbRating, TitleFacts};
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    let first = imdb_observation(&CatalogTitle::new(
        ImdbId::new("tt3000006")?,
        MediaKind::Movie,
        "First",
        None,
    )?);
    let second = normalized_tmdb::observation(&TmdbTitle {
        id: 501,
        kind: MediaKind::Movie,
        name: "Second".into(),
        overview: None,
        release_info: None,
        poster_url: None,
        display: Default::default(),
        imdb: None,
    });
    let facts = TitleFacts {
        tmdb_rating: ImdbRating::new(8.1),
        ..Default::default()
    };
    for (item, acquired) in [(&first, false), (&second, true)] {
        let id = resolve_title(&tx, item, 2, &Default::default())?;
        let f = fetch(
            &tx,
            Target::Title(id),
            "tmdb",
            "same-fixture-resource-v1",
            Quality::Detail,
            false,
            2,
        )?;
        import_title(
            &tx,
            id,
            item,
            "tmdb",
            Quality::Detail,
            f,
            2,
            &Default::default(),
        )?;
        if acquired {
            write_facts(&tx, f, &facts)?;
        }
    }
    let mut incoming = first;
    incoming.identifiers.extend(second.identifiers);
    let id = resolve_title(&tx, &incoming, 3, &Default::default())?;
    assert_eq!(
        read_facts(&tx, id, "tmdb", "same-fixture-resource-v1")?,
        facts
    );
    Ok(())
}

#[test]
fn reconciliation_uses_supplier_priority_before_quality_and_time() -> TestResult {
    use kinema_core::catalog_normalized::{FieldValue, TitleObservation};
    use kinema_core::provider::{ProviderId, ProviderPriority};
    for primary in [ProviderId::Cinemeta, ProviderId::Tmdb] {
        for reverse in [false, true] {
            let dir = tempfile::tempdir()?;
            let mut service = CatalogService::open(&dir.path().join("catalog"))?;
            service.set_provider_priority(ProviderPriority::new([primary]));
            let priority = service.provider_priority.clone();
            let tx = service.connection.transaction()?;
            let first = TitleObservation {
                kind: MediaKind::Movie,
                identifiers: vec![ExternalIdentifier {
                    namespace: "imdb".into(),
                    external_id: "tt3000099".into(),
                }],
                fields: vec![(
                    "overview".into(),
                    FieldValue::Text("Cinemeta overview".into()),
                )],
                ratings: vec![],
            };
            let second = TitleObservation {
                identifiers: vec![ExternalIdentifier {
                    namespace: "tmdb".into(),
                    external_id: "999".into(),
                }],
                fields: vec![("overview".into(), FieldValue::Text("TMDB overview".into()))],
                ..first.clone()
            };
            let mut observations = vec![
                (&first, "cinemeta", Quality::Summary, 1),
                (&second, "tmdb", Quality::Detail, 100),
            ];
            if reverse {
                observations.reverse();
            }
            for (item, supplier, quality, time) in observations {
                let id = resolve_title(&tx, item, time, &priority)?;
                let f = fetch(
                    &tx,
                    Target::Title(id),
                    supplier,
                    "detail-v1",
                    quality,
                    false,
                    time,
                )?;
                import_title(&tx, id, item, supplier, quality, f, time, &priority)?;
            }
            let mut mapping = first.clone();
            mapping.identifiers.extend(second.identifiers);
            let id = resolve_title(&tx, &mapping, 101, &priority)?;
            let record = CatalogService::title_record(&tx, id)?;
            assert_eq!(
                record.text("overview"),
                Some(if primary == ProviderId::Cinemeta {
                    "Cinemeta overview"
                } else {
                    "TMDB overview"
                })
            );
            assert_eq!(record.provenance[0].supplied_by, primary.as_str());
        }
    }
    Ok(())
}

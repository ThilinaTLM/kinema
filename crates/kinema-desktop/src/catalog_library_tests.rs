use super::*;
type TestResult = Result<(), Box<dyn std::error::Error>>;
fn required<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("required fixture value is missing"),
    }
}
fn id(n: i64) -> TitleRef {
    TitleRef::Canonical(required(TitleId::new(n)))
}
fn seed(service: &CatalogService) -> Result<(), CatalogError> {
    service.connection.execute_batch("INSERT INTO titles(id,kind,locale,updated_at) VALUES (1,'movie','en-US',1),(2,'series','en-US',1),(3,'movie','en-US',1);
    INSERT INTO seasons(id,series_id,series_kind,season_number,episode_count,locale,updated_at) VALUES (10,2,'series',1,2,'en-US',1);
    INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (100,10,1,'en-US',1),(101,10,2,'en-US',1);
    INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10000,2,'cinemeta','identity-anchor','en-US',1,'data','summary');
    INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES('imdb','series','tt1234567',2,10000,1);")?;
    Ok(())
}
#[test]
fn durable_membership_watch_and_offline_detail() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog");
    let mut service = CatalogService::open(&path)?;
    seed(&service)?;
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(1),
        watched: true,
    })?;
    service.library_operation(LibraryOperation::SetMembership {
        title: id(1),
        saved: true,
        restore_added_at: Some(20),
    })?;
    service.library_operation(LibraryOperation::SetMembership {
        title: id(1),
        saved: true,
        restore_added_at: Some(99),
    })?;
    let LibraryResult::Membership {
        previous_added_at, ..
    } = service.library_operation(LibraryOperation::SetMembership {
        title: id(1),
        saved: false,
        restore_added_at: None,
    })?
    else {
        panic!("membership")
    };
    assert_eq!(previous_added_at, Some(20));
    drop(service);
    let mut service = CatalogService::open(&path)?;
    let LibraryResult::Detail(detail) =
        service.library_operation(LibraryOperation::Detail(id(1)))?
    else {
        panic!("detail")
    };
    assert_eq!(detail.personal.added_at, None);
    assert!(required(detail.personal.movie_watch).watched);
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(1),
        watched: false,
    })?;
    let state = service.connection.query_row(
        "SELECT watched,updated_at FROM movie_watch_state WHERE title_id=1",
        [],
        |r| Ok((r.get::<_, bool>(0)?, r.get::<_, i64>(1)?)),
    )?;
    assert!(!state.0);
    assert!(state.1 > required(detail.personal.movie_watch).updated_at);
    assert!(
        service
            .library_operation(LibraryOperation::SetMovieWatched {
                title: id(2),
                watched: true
            })
            .is_err()
    );
    Ok(())
}
#[test]
fn listing_filters_before_paging_literal_search_and_stable_sort() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    for n in [1, 2, 3] {
        service.library_operation(LibraryOperation::SetMembership {
            title: id(n),
            saved: true,
            restore_added_at: Some(n),
        })?;
    }
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(1),
        watched: true,
    })?;
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            status: WatchFilter::Watched,
            limit: 1,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 1);
    assert_eq!(page.entries[0].metadata.id, 1);
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            search: "%".into(),
            limit: 1,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 0);
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            limit: 1,
            offset: 1,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.entries[0].metadata.id, 2);
    assert_eq!(page.total, 3);
    Ok(())
}
#[test]
fn complete_season_evidence_and_frozen_bulk_are_atomic() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    assert!(
        service
            .library_operation(LibraryOperation::SeasonSnapshot {
                title: id(2),
                season: 1
            })
            .is_err()
    );
    service.connection.execute_batch("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (10,'tmdb','season','en-US',1,'data','detail');")?;
    let LibraryResult::SeasonSnapshot(snapshot) =
        service.library_operation(LibraryOperation::SeasonSnapshot {
            title: id(2),
            season: 1,
        })?
    else {
        panic!("snapshot")
    };
    assert_eq!(snapshot.episodes.len(), 2);
    service.connection.execute_batch("INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (102,10,3,'en-US',1); UPDATE seasons SET episode_count=3 WHERE id=10;")?;
    service.library_operation(LibraryOperation::SetSeasonWatched {
        snapshot: snapshot.clone(),
        watched: true,
    })?;
    let count: i64 = service.connection.query_row(
        "SELECT count(*) FROM episode_watch_state WHERE watched=1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 2);
    let mut invalid = snapshot;
    invalid.episodes.push(required(EpisodeId::new(999)));
    assert!(
        service
            .library_operation(LibraryOperation::SetSeasonWatched {
                snapshot: invalid,
                watched: false
            })
            .is_err()
    );
    let count: i64 = service.connection.query_row(
        "SELECT count(*) FROM episode_watch_state WHERE watched=1",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 2);
    Ok(())
}
#[test]
fn merge_preserves_membership_and_conservative_watch_rows() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO library_entries VALUES (1,20),(3,10); INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES (1,1,50),(3,0,50);
    INSERT INTO titles(id,kind,locale,updated_at) VALUES (4,'series','en-US',1);
    INSERT INTO seasons(id,series_id,series_kind,season_number,locale,updated_at) VALUES (11,4,'series',1,'en-US',1);
    INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (103,11,1,'en-US',1);
    INSERT INTO episode_watch_state(episode_id,watched,updated_at) VALUES (100,1,60),(103,0,70);")?;
    service.connection.execute_batch("UPDATE movie_watch_state SET revision=7,active_run=7 WHERE title_id=3;
    INSERT INTO movie_playback_progress(title_id,position_ms,duration_ms,updated_at,revision) VALUES(1,500,1000,20,0),(3,100,1000,30,7);
    INSERT INTO episode_playback_progress(episode_id,position_ms,duration_ms,updated_at,revision) VALUES(103,800,NULL,40,0);")?;
    let priority = service.provider_priority.clone();
    let tx = service.connection.transaction()?;
    super::super::normalized_merge::reconcile(&tx, 1, 3, 100, &priority)?;
    super::super::normalized_merge::reconcile(&tx, 2, 4, 100, &priority)?;
    tx.commit()?;
    let state = personal(&service.connection.transaction()?, 1)?;
    assert_eq!(state.added_at, Some(10));
    assert!(!required(state.movie_watch).watched);
    let movie = required(state.playback_progress);
    assert_eq!(movie.active_run, 0);
    assert!(movie.epoch > 7);
    assert_eq!(required(movie.checkpoint).observation.position_ms(), 100);
    assert_eq!(required(movie.checkpoint).revision, movie.epoch);
    let episode = episode_personal(&service.connection.transaction()?, 100)?;
    assert_eq!(
        required(episode.playback_progress.checkpoint)
            .observation
            .position_ms(),
        800
    );
    let state: (bool, i64) = service.connection.query_row(
        "SELECT watched,updated_at FROM episode_watch_state WHERE episode_id=100",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(state, (false, 70));
    let violations: i64 =
        service
            .connection
            .query_row("SELECT count(*) FROM pragma_foreign_key_check", [], |r| {
                r.get(0)
            })?;
    assert_eq!(violations, 0);
    Ok(())
}
#[test]
fn v6_migration_preserves_catalog_and_restricts_deletion() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog");
    let mut service = CatalogService::open(&path)?;
    seed(&service)?;
    service.connection.execute_batch("UPDATE titles SET overview='Preserved v6 metadata' WHERE id=1;
    INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(1,1,'tmdb','title','en-US',42,'data','detail');
    INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES('tmdb','movie','42',1,1,42);
    INSERT INTO field_provenance(title_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(1,'overview','tmdb','detail',1,42);")?;
    let before = {
        let tx = service.connection.transaction()?;
        CatalogService::title_record(&tx, 1)?
    };
    service.connection.execute_batch("DROP TABLE movie_playback_progress; DROP TABLE episode_playback_progress; DROP TABLE library_entries; DROP TABLE movie_watch_state; DROP TABLE episode_watch_state; PRAGMA user_version=6;")?;
    drop(service);
    let mut service = CatalogService::open(&path)?;
    let after = {
        let tx = service.connection.transaction()?;
        CatalogService::title_record(&tx, 1)?
    };
    assert_eq!(before, after);
    let indexes:i64 = service.connection.query_row("SELECT count(*) FROM sqlite_schema WHERE type='index' AND name IN ('library_entries_added','movie_watch_state_updated','episode_watch_state_updated')", [], |r|r.get(0))?;
    assert_eq!(indexes, 3);
    let count: i64 = service
        .connection
        .query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 3);
    service
        .connection
        .execute("INSERT INTO library_entries VALUES (1,0)", [])?;
    assert!(
        service
            .connection
            .execute("DELETE FROM titles WHERE id=1", [])
            .is_err()
    );
    assert!(
        service
            .connection
            .execute(
                "INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES(2,1,0)",
                []
            )
            .is_err()
    );
    assert!(
        service
            .connection
            .execute(
                "INSERT INTO episode_watch_state(episode_id,watched,updated_at) VALUES(100,2,0)",
                []
            )
            .is_err()
    );
    Ok(())
}

#[test]
fn completeness_is_unknown_without_acquisition_or_with_contradictions() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    fn summary(service: &mut CatalogService) -> Result<ShowSummary, CatalogError> {
        let tx = service.connection.transaction()?;
        Ok(required(personal(&tx, 2)?.show_summary))
    }
    assert!(!summary(&mut service)?.complete);
    service.connection.execute_batch("UPDATE titles SET season_count=1,episode_count=2 WHERE id=2;
    INSERT INTO metadata_fetches(title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (2,'tmdb','title','en-US',1,'data','detail');")?;
    assert!(!summary(&mut service)?.complete);
    service.connection.execute_batch("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (10,'tmdb','season','en-US',1,'data','detail');")?;
    assert_eq!(
        summary(&mut service)?.status(),
        Some(WatchFilter::Unwatched)
    );
    service.library_operation(LibraryOperation::SetEpisodeWatched {
        episode: EpisodeRef::Canonical(required(EpisodeId::new(100))),
        watched: true,
    })?;
    assert_eq!(
        summary(&mut service)?.status(),
        Some(WatchFilter::InProgress)
    );
    service.library_operation(LibraryOperation::SetEpisodeWatched {
        episode: EpisodeRef::Canonical(required(EpisodeId::new(101))),
        watched: true,
    })?;
    assert_eq!(summary(&mut service)?.status(), Some(WatchFilter::Watched));
    service.connection.execute_batch("UPDATE metadata_fetches SET last_error_code='network',last_attempt_at=2 WHERE season_id=10;")?;
    assert_eq!(summary(&mut service)?.status(), None);
    service.connection.execute_batch("UPDATE metadata_fetches SET last_error_code=NULL WHERE season_id=10; UPDATE titles SET episode_count=3 WHERE id=2;")?;
    assert!(!summary(&mut service)?.complete);
    // Specials count in the canonical episode set, not in advertised regular-season count.
    service.connection.execute_batch("INSERT INTO seasons(id,series_id,series_kind,season_number,episode_count,locale,updated_at) VALUES(12,2,'series',0,1,'en-US',2);
    INSERT INTO episodes(id,season_id,episode_number,air_date,locale,updated_at) VALUES(102,12,1,'2099-01-01','en-US',2);
    INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(12,'tmdb','specials','en-US',2,'data','detail');")?;
    let state = summary(&mut service)?;
    assert_eq!(state.total, 3);
    assert_eq!(state.watched, 2);
    assert_eq!(state.status(), Some(WatchFilter::InProgress));
    service
        .connection
        .execute("UPDATE titles SET episode_count=2 WHERE id=2", [])?;
    assert_eq!(
        summary(&mut service)?.status(),
        Some(WatchFilter::InProgress)
    );
    service
        .connection
        .execute_batch("UPDATE seasons SET episode_count=NULL WHERE id=12;")?;
    assert_eq!(summary(&mut service)?.status(), None);
    service.connection.execute_batch("DELETE FROM episode_watch_state; DELETE FROM episodes; UPDATE seasons SET episode_count=0; UPDATE titles SET episode_count=0 WHERE id=2;")?;
    assert_eq!(summary(&mut service)?.status(), None);
    Ok(())
}
#[test]
fn batched_provider_and_coordinate_resolution_preserves_order_and_rejects_guesses() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(1,2,'tmdb','title','en-US',1,'data','detail');
    INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES('tmdb','series','42',2,1,1);
    INSERT INTO metadata_fetches(id,episode_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(2,100,'tmdb','ep','en-US',1,'data','detail');
    INSERT INTO episode_identifiers(namespace,external_id,episode_id,fetch_state_id,observed_at) VALUES('tmdb','900',100,2,1);")?;
    let title = TitleRef::External {
        kind: MediaKind::Series,
        identifier: ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "42".into(),
        },
    };
    let LibraryResult::Titles(states) =
        service.library_operation(LibraryOperation::ReadTitles(vec![title.clone(); 500]))?
    else {
        panic!("titles")
    };
    assert_eq!(states.len(), 500);
    assert!(
        states
            .iter()
            .all(|p| p.as_ref().is_ok_and(|p| p.title_id.get() == 2))
    );
    let refs = [
        EpisodeRef::External(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "900".into(),
        }),
        EpisodeRef::Coordinates {
            title,
            season: 1,
            episode: 2,
        },
    ];
    let LibraryResult::Episodes(states) = service.library_operation(
        LibraryOperation::ReadEpisodes(refs.iter().cloned().cycle().take(600).collect()),
    )?
    else {
        panic!("episodes")
    };
    assert_eq!(states.len(), 600);
    assert_eq!(states[0].as_ref().map(|p| p.episode_id.get()), Ok(100));
    assert_eq!(states[1].as_ref().map(|p| p.episode_id.get()), Ok(101));
    assert_eq!(
        service.library_operation(LibraryOperation::ReadTitles(vec![id(999)])),
        Ok(LibraryResult::Titles(vec![Err(
            CatalogError::MissingIdentityMapping
        )]))
    );
    assert_eq!(
        service.library_operation(LibraryOperation::ReadEpisodes(vec![EpisodeRef::Canonical(
            required(EpisodeId::new(999))
        )])),
        Ok(LibraryResult::Episodes(vec![Err(
            CatalogError::MissingIdentityMapping
        )]))
    );
    Ok(())
}
#[test]
fn duplicate_snapshot_and_overflow_watch_do_not_change_data() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES (10,'tmdb','season','en-US',1,'data','detail');")?;
    let LibraryResult::SeasonSnapshot(mut snapshot) =
        service.library_operation(LibraryOperation::SeasonSnapshot {
            title: id(2),
            season: 1,
        })?
    else {
        panic!("snapshot")
    };
    snapshot.episodes[1] = snapshot.episodes[0];
    assert!(
        service
            .library_operation(LibraryOperation::SetSeasonWatched {
                snapshot,
                watched: true
            })
            .is_err()
    );
    service.connection.execute(
        "INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES(1,0,?1)",
        [i64::MAX],
    )?;
    assert_eq!(
        service.library_operation(LibraryOperation::SetEpisodeWatched {
            episode: EpisodeRef::Canonical(required(EpisodeId::new(100))),
            watched: true
        }),
        Err(CatalogError::Clock)
    );
    let count: i64 =
        service
            .connection
            .query_row("SELECT count(*) FROM episode_watch_state", [], |r| r.get(0))?;
    assert_eq!(count, 0);
    Ok(())
}

#[test]
fn failed_v7_migration_rolls_back_schema_and_version() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog");
    let service = CatalogService::open(&path)?;
    seed(&service)?;
    service.connection.execute_batch("DROP TABLE movie_playback_progress; DROP TABLE episode_playback_progress; DROP TABLE library_entries; DROP TABLE movie_watch_state; DROP TABLE episode_watch_state; CREATE VIEW movie_watch_state AS SELECT 1 title_id; PRAGMA user_version=6;")?;
    drop(service);
    assert!(matches!(
        CatalogService::open(&path),
        Err(CatalogError::Storage)
    ));
    let connection = Connection::open(&path)?;
    let version: i64 = connection.pragma_query_value(None, "user_version", |r| r.get(0))?;
    assert_eq!(version, 6);
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name='library_entries'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(count, 0);
    let count: i64 = connection.query_row("SELECT count(*) FROM titles", [], |r| r.get(0))?;
    assert_eq!(count, 3);
    Ok(())
}
#[test]
fn conflicting_graph_merge_rolls_back_personal_and_catalog_together() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO library_entries VALUES(1,10),(3,20);
    INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES(1,0,30),(3,1,40);
    INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale) VALUES(1,1,'tmdb','one','en-US'),(2,3,'tmdb','two','en-US');
    INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES('tmdb','movie','1',1,1,0),('tmdb','movie','3',3,2,0);")?;
    let priority = service.provider_priority.clone();
    let tx = service.connection.transaction()?;
    assert_eq!(
        super::super::normalized_merge::reconcile(&tx, 1, 3, 100, &priority),
        Err(CatalogError::IdentityConflict)
    );
    drop(tx);
    let count: i64 =
        service
            .connection
            .query_row("SELECT count(*) FROM library_entries", [], |r| r.get(0))?;
    assert_eq!(count, 2);
    let state: (bool, i64) = service.connection.query_row(
        "SELECT watched,updated_at FROM movie_watch_state WHERE title_id=3",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(state, (true, 40));
    Ok(())
}

#[test]
fn unicode_case_insensitive_search_is_literal_and_local_detail_keeps_saved_metadata() -> TestResult
{
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("UPDATE titles SET name='ÉTÉ 100%_Film',overview='Saved without credentials' WHERE id=1;
    INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(1,1,'tmdb','title','en-US',1,'data','detail');
    INSERT INTO field_provenance(title_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(1,'name','tmdb','detail',1,1),(1,'overview','tmdb','detail',1,1);")?;
    service.library_operation(LibraryOperation::SetMembership {
        title: id(1),
        saved: true,
        restore_added_at: Some(1),
    })?;
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            search: "été 100%_".into(),
            limit: 20,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 1);
    assert_eq!(
        page.entries[0].metadata.fields.get("name"),
        Some(&kinema_core::catalog_normalized::FieldValue::Text(
            "ÉTÉ 100%_Film".into()
        ))
    );
    let LibraryResult::Detail(detail) =
        service.library_operation(LibraryOperation::Detail(id(1)))?
    else {
        panic!("detail")
    };
    assert!(detail.metadata.fields.contains_key("overview"));
    Ok(())
}

#[test]
fn merge_absence_does_not_erase_explicit_unwatched_and_newest_wins() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    service.connection.execute_batch("INSERT INTO titles(id,kind,locale,updated_at) VALUES(1,'movie','en-US',0),(2,'movie','en-US',0),(3,'movie','en-US',0),(4,'movie','en-US',0);
    INSERT INTO library_entries VALUES(2,7);
    INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES(2,0,0),(3,0,10),(4,1,11);")?;
    let priority = service.provider_priority.clone();
    let tx = service.connection.transaction()?;
    super::super::normalized_merge::reconcile(&tx, 1, 2, 20, &priority)?;
    super::super::normalized_merge::reconcile(&tx, 3, 4, 20, &priority)?;
    tx.commit()?;
    let tx = service.connection.transaction()?;
    let first = personal(&tx, 1)?;
    assert_eq!(first.added_at, Some(7));
    assert_eq!(
        first.movie_watch,
        Some(WatchState {
            watched: false,
            updated_at: 0
        })
    );
    assert_eq!(
        personal(&tx, 3)?.movie_watch,
        Some(WatchState {
            watched: true,
            updated_at: 11
        })
    );
    Ok(())
}

#[test]
fn stale_bulk_snapshot_rejects_merged_graph_row_id_reuse() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10,'tmdb','season','en-US',1,'data','detail');")?;
    let LibraryResult::SeasonSnapshot(snapshot) =
        service.library_operation(LibraryOperation::SeasonSnapshot {
            title: id(2),
            season: 1,
        })?
    else {
        panic!("snapshot")
    };
    assert_eq!(snapshot.episode_numbers, vec![1, 2]);
    assert!(!snapshot.title_identifiers.is_empty());
    // The lower-ID verified graph survives; all frozen canonical rows are deleted.
    service.connection.execute_batch("INSERT INTO field_provenance(season_id,field_name,supplied_by,quality,fetch_state_id,observed_at) SELECT 10,'episode_count','tmdb','detail',id,1 FROM metadata_fetches WHERE season_id=10 AND resource_key='season';
    DELETE FROM titles WHERE id=1;
    INSERT INTO titles(id,kind,locale,updated_at) VALUES(1,'series','en-US',1);
    INSERT INTO seasons(id,series_id,series_kind,season_number,episode_count,locale,updated_at) VALUES(11,1,'series',1,2,'en-US',1);
    INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES(90,11,1,'en-US',1),(91,11,2,'en-US',1);")?;
    let priority = service.provider_priority.clone();
    let tx = service.connection.transaction()?;
    super::super::normalized_merge::reconcile(&tx, 1, 2, 2, &priority)?;
    tx.commit()?;
    // SQLite row IDs are reusable: unrelated entities now occupy every old ID.
    service.connection.execute_batch("INSERT INTO titles(id,kind,locale,updated_at) VALUES(2,'series','en-US',3);
    INSERT INTO seasons(id,series_id,series_kind,season_number,episode_count,locale,updated_at) VALUES(10,2,'series',1,2,'en-US',3);
    INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES(100,10,1,'en-US',3),(101,10,2,'en-US',3);
    INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10,'tmdb','unrelated-season','en-US',3,'data','detail');")?;
    assert_eq!(
        service.library_operation(LibraryOperation::SetSeasonWatched {
            snapshot,
            watched: true
        }),
        Err(CatalogError::MissingIdentityMapping)
    );
    let rows: i64 =
        service
            .connection
            .query_row("SELECT count(*) FROM episode_watch_state", [], |r| r.get(0))?;
    assert_eq!(rows, 0);
    Ok(())
}

#[test]
fn stale_bulk_snapshot_rejects_episode_id_reuse_at_different_coordinate() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch("INSERT INTO metadata_fetches(season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10,'tmdb','season','en-US',1,'data','detail');")?;
    let LibraryResult::SeasonSnapshot(snapshot) =
        service.library_operation(LibraryOperation::SeasonSnapshot {
            title: id(2),
            season: 1,
        })?
    else {
        panic!("snapshot")
    };
    service.connection.execute_batch("DELETE FROM episodes WHERE id=101;
    INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES(101,10,99,'en-US',2);")?;
    assert_eq!(
        service.library_operation(LibraryOperation::SetSeasonWatched {
            snapshot: snapshot.clone(),
            watched: true
        }),
        Err(CatalogError::MissingIdentityMapping)
    );
    let rows: i64 =
        service
            .connection
            .query_row("SELECT count(*) FROM episode_watch_state", [], |r| r.get(0))?;
    assert_eq!(rows, 0);
    service.connection.execute_batch("UPDATE episodes SET episode_number=2 WHERE id=101; UPDATE seasons SET season_number=2 WHERE id=10;")?;
    assert_eq!(
        service.library_operation(LibraryOperation::SetSeasonWatched {
            snapshot,
            watched: true
        }),
        Err(CatalogError::MissingIdentityMapping)
    );
    service
        .connection
        .execute("UPDATE seasons SET season_number=1 WHERE id=10", [])?;
    // A canonical-only title cannot provide an identity-safe bulk confirmation.
    service
        .connection
        .execute("DELETE FROM title_identifiers WHERE title_id=2", [])?;
    assert_eq!(
        service.library_operation(LibraryOperation::SeasonSnapshot {
            title: id(2),
            season: 1
        }),
        Err(CatalogError::MissingIdentityMapping)
    );
    Ok(())
}

#[test]
fn batch_missing_identities_are_positional_without_poisoning_mapped_neighbors() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    let external = TitleRef::External {
        kind: MediaKind::Series,
        identifier: ExternalIdentifier {
            namespace: "imdb".into(),
            external_id: "tt1234567".into(),
        },
    };
    let invalid = ExternalIdentifier {
        namespace: "unknown".into(),
        external_id: "bad".into(),
    };
    let titles = [
        external.clone(),
        TitleRef::External {
            kind: MediaKind::Series,
            identifier: ExternalIdentifier {
                namespace: "tmdb".into(),
                external_id: "999999".into(),
            },
        },
        TitleRef::External {
            kind: MediaKind::Series,
            identifier: invalid.clone(),
        },
        id(1),
        external,
    ];
    let LibraryResult::Titles(states) = service.library_operation(LibraryOperation::ReadTitles(
        titles.iter().cloned().cycle().take(500).collect(),
    ))?
    else {
        panic!("titles")
    };
    assert_eq!(states.len(), 500);
    for chunk in states.chunks(5) {
        assert_eq!(chunk[0].as_ref().map(|p| p.title_id.get()), Ok(2));
        assert_eq!(chunk[1], Err(CatalogError::MissingIdentityMapping));
        assert_eq!(chunk[2], Err(CatalogError::MissingIdentityMapping));
        assert_eq!(chunk[3].as_ref().map(|p| p.title_id.get()), Ok(1));
        assert_eq!(chunk[0], chunk[4]);
    }
    let episode = EpisodeRef::Canonical(required(EpisodeId::new(100)));
    let refs = [
        episode.clone(),
        EpisodeRef::External(ExternalIdentifier {
            namespace: "tmdb".into(),
            external_id: "999999".into(),
        }),
        EpisodeRef::External(invalid),
        EpisodeRef::Coordinates {
            title: id(2),
            season: 1,
            episode: 2,
        },
        episode,
    ];
    let LibraryResult::Episodes(states) = service.library_operation(
        LibraryOperation::ReadEpisodes(refs.iter().cloned().cycle().take(600).collect()),
    )?
    else {
        panic!("episodes")
    };
    assert_eq!(states.len(), 600);
    for chunk in states.chunks(5) {
        assert_eq!(chunk[0].as_ref().map(|p| p.episode_id.get()), Ok(100));
        assert_eq!(chunk[1], Err(CatalogError::MissingIdentityMapping));
        assert_eq!(chunk[2], Err(CatalogError::MissingIdentityMapping));
        assert_eq!(chunk[3].as_ref().map(|p| p.episode_id.get()), Ok(101));
        assert_eq!(chunk[0], chunk[4]);
    }
    Ok(())
}

#[test]
fn batch_storage_and_corrupt_data_failures_remain_outer_errors() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    seed(&service)?;
    service.connection.execute_batch(
        "PRAGMA ignore_check_constraints=ON; UPDATE titles SET kind='invalid' WHERE id=1;",
    )?;
    assert_eq!(
        service.library_operation(LibraryOperation::ReadTitles(vec![id(999), id(1)])),
        Err(CatalogError::InvalidCache)
    );
    service.connection.execute_batch(
        "UPDATE titles SET kind='movie' WHERE id=1; DROP TABLE movie_watch_state;",
    )?;
    assert_eq!(
        service.library_operation(LibraryOperation::ReadTitles(vec![id(999), id(1)])),
        Err(CatalogError::Storage)
    );
    service
        .connection
        .execute_batch("DROP TABLE episode_watch_state;")?;
    assert_eq!(
        service.library_operation(LibraryOperation::ReadEpisodes(vec![
            EpisodeRef::Canonical(required(EpisodeId::new(999))),
            EpisodeRef::Canonical(required(EpisodeId::new(100)))
        ])),
        Err(CatalogError::Storage)
    );
    Ok(())
}

#[test]
fn real_worker_local_roundtrips_commit_and_survive_reopen_without_remote_work() -> TestResult {
    use crate::catalog_worker::{
        CatalogWorker,
        local::{LocalEvent, LocalRequest, LocalSubmit},
    };
    use std::time::{Duration, Instant};
    fn receive(worker: &CatalogWorker) -> Result<LocalEvent, Box<dyn std::error::Error>> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(event) = worker.poll_local() {
                return Ok(event);
            }
            if Instant::now() >= deadline {
                return Err("timed out waiting for local catalog acknowledgement".into());
            }
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn request(worker: &CatalogWorker, id: u64, operation: LibraryOperation) {
        assert_eq!(
            worker.submit_local(LocalRequest {
                request_id: id,
                generation: 1,
                operation
            }),
            LocalSubmit::Queued
        );
    }
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("worker-catalog.sqlite3");
    let service = CatalogService::open(&path)?;
    seed(&service)?;
    service.connection.execute_batch("UPDATE titles SET name='Offline movie' WHERE id=1;
    UPDATE titles SET name='Offline show' WHERE id=2;
    UPDATE episodes SET name='Saved pilot' WHERE id=100;
    INSERT INTO metadata_fetches(id,title_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10010,1,'cinemeta','movie','en-US',1,'data','detail');
    INSERT INTO metadata_fetches(id,season_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10011,10,'tmdb','season','en-US',1,'data','detail');
    INSERT INTO metadata_fetches(id,episode_id,provider,resource_key,locale,fetched_at,success_kind,completeness) VALUES(10012,100,'tmdb','pilot','en-US',1,'data','detail');
    INSERT INTO field_provenance(title_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(1,'name','cinemeta','detail',10010,1),(2,'name','cinemeta','summary',10000,1);
    INSERT INTO field_provenance(season_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(10,'episode_count','tmdb','detail',10011,1);
    INSERT INTO field_provenance(episode_id,field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES(100,'name','tmdb','detail',10012,1);")?;
    drop(service);
    let worker_path = path.clone();
    let worker = CatalogWorker::spawn(move || {
        CatalogService::open(&worker_path).map_err(super::super::diagnostic_error)
    })?;
    // Only the production local FIFO is used. No search/metadata/network jobs are submitted.
    request(
        &worker,
        1,
        LibraryOperation::SetMembership {
            title: id(1),
            saved: true,
            restore_added_at: Some(10),
        },
    );
    request(
        &worker,
        2,
        LibraryOperation::SetMembership {
            title: id(2),
            saved: true,
            restore_added_at: Some(20),
        },
    );
    request(
        &worker,
        3,
        LibraryOperation::SetMovieWatched {
            title: id(1),
            watched: true,
        },
    );
    request(
        &worker,
        4,
        LibraryOperation::SetEpisodeWatched {
            episode: EpisodeRef::Canonical(required(EpisodeId::new(100))),
            watched: true,
        },
    );
    for expected in 1..=4 {
        let event = receive(&worker)?;
        assert_eq!(event.request.request_id, expected);
        assert_eq!(event.revision, expected);
        assert!(event.result.is_ok());
    }
    request(
        &worker,
        5,
        LibraryOperation::ReadTitles(vec![id(1), id(999), id(2)]),
    );
    let event = receive(&worker)?;
    assert_eq!(event.request.request_id, 5);
    assert_eq!(event.revision, 4);
    let LibraryResult::Titles(titles) = event.result? else {
        panic!("title batch")
    };
    assert!(
        titles[0]
            .as_ref()
            .is_ok_and(|p| p.added_at == Some(10) && p.movie_watch.is_some_and(|w| w.watched))
    );
    assert_eq!(titles[1], Err(CatalogError::MissingIdentityMapping));
    assert!(titles[2].as_ref().is_ok_and(|p| p.added_at == Some(20)));
    request(
        &worker,
        6,
        LibraryOperation::ReadEpisodes(vec![
            EpisodeRef::Canonical(required(EpisodeId::new(100))),
            EpisodeRef::Canonical(required(EpisodeId::new(999))),
        ]),
    );
    let event = receive(&worker)?;
    assert_eq!(event.request.request_id, 6);
    assert_eq!(event.revision, 4);
    let LibraryResult::Episodes(episodes) = event.result? else {
        panic!("episode batch")
    };
    assert!(
        episodes[0]
            .as_ref()
            .is_ok_and(|p| p.watch.is_some_and(|w| w.watched))
    );
    assert_eq!(episodes[1], Err(CatalogError::MissingIdentityMapping));
    request(
        &worker,
        7,
        LibraryOperation::List(LibraryQuery {
            limit: 10,
            ..Default::default()
        }),
    );
    let event = receive(&worker)?;
    assert_eq!(event.request.request_id, 7);
    assert_eq!(event.revision, 4);
    let LibraryResult::Page(page) = event.result? else {
        panic!("library page")
    };
    assert_eq!(page.total, 2);
    assert_eq!(page.entries[0].metadata.id, 2);
    assert_eq!(
        page.entries[0].metadata.fields.get("name"),
        Some(&kinema_core::catalog_normalized::FieldValue::Text(
            "Offline show".into()
        ))
    );
    request(&worker, 8, LibraryOperation::Detail(id(2)));
    let event = receive(&worker)?;
    assert_eq!(event.request.request_id, 8);
    assert_eq!(event.revision, 4);
    let LibraryResult::Detail(detail) = event.result? else {
        panic!("local detail")
    };
    assert_eq!(detail.episodes.len(), 2);
    assert!(detail.episodes[0].personal.watch.is_some_and(|w| w.watched));
    assert_eq!(
        detail.episodes[0].metadata.fields.get("name"),
        Some(&kinema_core::catalog_normalized::FieldValue::Text(
            "Saved pilot".into()
        ))
    );
    request(
        &worker,
        9,
        LibraryOperation::SetMembership {
            title: id(1),
            saved: false,
            restore_added_at: None,
        },
    );
    let event = receive(&worker)?;
    assert_eq!(event.request.request_id, 9);
    assert_eq!(event.revision, 5);
    let LibraryResult::Membership {
        personal,
        previous_added_at,
    } = event.result?
    else {
        panic!("membership removal")
    };
    assert_eq!(previous_added_at, Some(10));
    assert_eq!(personal.added_at, None);
    assert!(personal.movie_watch.is_some_and(|w| w.watched));
    // Wait for the real SQLite owner to exit before reopening/cleaning up its temp file.
    worker.stop_for_test();
    let deadline = Instant::now() + Duration::from_secs(5);
    while !worker.is_finished_for_test() {
        if Instant::now() >= deadline {
            return Err("timed out stopping local catalog owner".into());
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    drop(worker);
    let mut reopened = CatalogService::open(&path)?;
    let LibraryResult::Titles(titles) =
        reopened.library_operation(LibraryOperation::ReadTitles(vec![id(1), id(2)]))?
    else {
        panic!("durable titles")
    };
    assert!(
        titles[0]
            .as_ref()
            .is_ok_and(|p| p.added_at.is_none() && p.movie_watch.is_some_and(|w| w.watched))
    );
    assert!(titles[1].as_ref().is_ok_and(|p| p.added_at == Some(20)));
    let LibraryResult::Episodes(episodes) = reopened.library_operation(
        LibraryOperation::ReadEpisodes(vec![EpisodeRef::Canonical(required(EpisodeId::new(100)))]),
    )?
    else {
        panic!("durable episode")
    };
    assert!(
        episodes[0]
            .as_ref()
            .is_ok_and(|p| p.watch.is_some_and(|w| w.watched))
    );
    let LibraryResult::Page(page) =
        reopened.library_operation(LibraryOperation::List(LibraryQuery {
            limit: 10,
            ..Default::default()
        }))?
    else {
        panic!("durable page")
    };
    assert_eq!(page.total, 1);
    assert_eq!(page.entries[0].metadata.id, 2);
    Ok(())
}

#[test]
fn playback_epochs_manual_override_backward_and_clear() -> TestResult {
    use crate::catalog::playback_progress::{
        PlaybackProgressOperation as Op, PlaybackProgressResult as Out,
    };
    use kinema_core::watch_progress::{
        PlaybackObservation as Obs, PlaybackProgressTarget as Target,
    };
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("progress");
    let mut service = CatalogService::open(&path)?;
    seed(&service)?;
    let target = Target::Movie(required(TitleId::new(1)));
    let applied = |out| match out {
        Out::Applied(s) => s,
        Out::Stale(_) => panic!("unexpected stale"),
    };
    let begin = applied(service.playback_progress_operation(Op::BeginRun {
        target,
        start_over: false,
    })?);
    let save = |epoch, position| Op::Save {
        target,
        run: begin.active_run,
        epoch,
        observation: required(Obs::new(position, Some(1000))),
    };
    let at = applied(service.playback_progress_operation(save(begin.epoch, 900))?);
    assert!(required(at.watch).watched);
    let back = applied(service.playback_progress_operation(save(begin.epoch, 100))?);
    assert_eq!(required(back.checkpoint).observation.position_ms(), 100);
    assert_eq!(back.watch, at.watch); // No repeated watched writes.
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(1),
        watched: false,
    })?;
    let Out::Stale(manual) = service.playback_progress_operation(save(begin.epoch, 999))? else {
        panic!("stale expected")
    };
    assert!(manual.auto_mark_suppressed);
    assert_eq!(manual.active_run, begin.active_run);
    assert_eq!(required(manual.checkpoint).observation.position_ms(), 100);
    let suppressed = applied(service.playback_progress_operation(save(manual.epoch, 999))?);
    assert!(!required(suppressed.watch).watched);
    let cleared = applied(service.playback_progress_operation(Op::Clear(target))?);
    assert!(cleared.checkpoint.is_none());
    assert!(matches!(
        service.playback_progress_operation(save(manual.epoch, 999))?,
        Out::Stale(_)
    ));
    let resumed = applied(service.playback_progress_operation(save(cleared.epoch, 500))?);
    assert!(resumed.auto_mark_suppressed);
    drop(service);
    let mut service = CatalogService::open(&path)?;
    let next = applied(service.playback_progress_operation(Op::BeginRun {
        target,
        start_over: false,
    })?);
    assert!(!next.auto_mark_suppressed);
    assert!(matches!(
        service.playback_progress_operation(save(cleared.epoch, 999))?,
        Out::Stale(_)
    ));
    let done = applied(service.playback_progress_operation(Op::CompleteRun {
        target,
        run: next.active_run,
        epoch: next.epoch,
        observation: Some(required(Obs::new(900, Some(1000)))),
    })?);
    assert!(required(done.watch).watched);
    assert_eq!(done.active_run, 0);
    assert!(done.checkpoint.is_some());
    Ok(())
}

#[test]
fn progress_migration_preserves_flags_and_failure_is_atomic() -> TestResult {
    use crate::catalog::playback_progress::{
        PlaybackProgressOperation as Op, PlaybackProgressResult as Out,
    };
    use kinema_core::watch_progress::{
        PlaybackObservation as Obs, PlaybackProgressTarget as Target,
    };
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("v7");
    let mut db = Connection::open(&path)?;
    let tx = db.transaction()?;
    schema::migrate(&tx)?;
    schema::migrate_library(&tx)?;
    tx.execute_batch("INSERT INTO titles(id,kind,locale,updated_at) VALUES(1,'movie','en-US',1); INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES(1,1,17); INSERT INTO titles(id,kind,locale,updated_at) VALUES(2,'series','en-US',1); INSERT INTO seasons(id,series_id,series_kind,season_number,locale,updated_at) VALUES(10,2,'series',1,'en-US',1); INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES(100,10,1,'en-US',1),(101,10,2,'en-US',1); INSERT INTO episode_watch_state(episode_id,watched,updated_at) VALUES(100,1,13),(101,0,14); PRAGMA user_version=7;")?;
    tx.commit()?;
    drop(db);
    let mut service = CatalogService::open(&path)?;
    let target = Target::Movie(required(TitleId::new(1)));
    let Out::Applied(old) = service.playback_progress_operation(Op::Read(target))? else {
        panic!("read")
    };
    assert_eq!(
        old.watch,
        Some(WatchState {
            watched: true,
            updated_at: 17
        })
    );
    assert!(old.checkpoint.is_none());
    for (id, watched, time) in [(100, true, 13), (101, false, 14)] {
        let Out::Applied(episode) = service
            .playback_progress_operation(Op::Read(Target::Episode(required(EpisodeId::new(id)))))?
        else {
            panic!("read")
        };
        assert_eq!(
            episode.watch,
            Some(WatchState {
                watched,
                updated_at: time
            })
        );
        assert_eq!(episode.epoch, 0);
        assert!(episode.checkpoint.is_none());
    }
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(1),
        watched: false,
    })?;
    let Out::Applied(run) = service.playback_progress_operation(Op::BeginRun {
        target,
        start_over: false,
    })?
    else {
        panic!("begin")
    };
    service.connection.execute_batch("CREATE TRIGGER fail_watch BEFORE UPDATE OF watched ON movie_watch_state BEGIN SELECT RAISE(ABORT,'fixture'); END;")?;
    assert_eq!(
        service.playback_progress_operation(Op::Save {
            target,
            run: run.active_run,
            epoch: run.epoch,
            observation: required(Obs::new(900, Some(1000)))
        }),
        Err(CatalogError::Storage)
    );
    let Out::Applied(after) = service.playback_progress_operation(Op::Read(target))? else {
        panic!("read")
    };
    assert!(after.checkpoint.is_none());
    assert!(!required(after.watch).watched);
    Ok(())
}

#[test]
fn episode_progress_cross_process_latest_run_and_failed_reset() -> TestResult {
    use crate::catalog::playback_progress::{
        PlaybackProgressOperation as Op, PlaybackProgressResult as Out,
    };
    use kinema_core::watch_progress::{
        PlaybackObservation as Obs, PlaybackProgressTarget as Target,
    };
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("episodes");
    let mut first = CatalogService::open(&path)?;
    seed(&first)?;
    let target = Target::Episode(required(EpisodeId::new(100)));
    let applied = |out| match out {
        Out::Applied(s) => s,
        Out::Stale(_) => panic!("stale"),
    };
    let a = applied(first.playback_progress_operation(Op::BeginRun {
        target,
        start_over: false,
    })?);
    let unknown = applied(first.playback_progress_operation(Op::Save {
        target,
        run: a.active_run,
        epoch: a.epoch,
        observation: required(Obs::new(999999, None)),
    })?);
    assert!(!required(unknown.watch).watched);
    let mut second = CatalogService::open(&path)?;
    let b = applied(second.playback_progress_operation(Op::BeginRun {
        target,
        start_over: false,
    })?);
    assert!(b.epoch > a.epoch);
    assert!(matches!(
        first.playback_progress_operation(Op::Save {
            target,
            run: a.active_run,
            epoch: a.epoch,
            observation: required(Obs::new(900, Some(1000)))
        }),
        Ok(Out::Stale(_))
    ));
    second.connection.execute_batch("CREATE TRIGGER fail_reset BEFORE DELETE ON episode_playback_progress BEGIN SELECT RAISE(ABORT,'fixture'); END;")?;
    assert_eq!(
        second.playback_progress_operation(Op::BeginRun {
            target,
            start_over: true
        }),
        Err(CatalogError::Storage)
    );
    let preserved = applied(first.playback_progress_operation(Op::Read(target))?);
    assert_eq!(preserved, b);
    second
        .connection
        .execute_batch("DROP TRIGGER fail_reset;")?;
    second.library_operation(LibraryOperation::SetEpisodeWatched {
        episode: EpisodeRef::Canonical(required(EpisodeId::new(100))),
        watched: true,
    })?;
    let manual = applied(first.playback_progress_operation(Op::Read(target))?);
    assert_eq!(
        manual.checkpoint.map(|c| c.observation),
        b.checkpoint.map(|c| c.observation)
    );
    assert!(required(manual.watch).watched);
    let clear = applied(first.playback_progress_operation(Op::Clear(target))?);
    assert!(required(clear.watch).watched);
    assert!(clear.checkpoint.is_none());
    Ok(())
}

#[test]
fn in_progress_counts_checkpoints_before_pagination_and_excludes_watched_replays() -> TestResult {
    use crate::catalog::playback_progress::{
        PlaybackProgressOperation as Op, PlaybackProgressResult as Out,
    };
    use kinema_core::watch_progress::{
        PlaybackObservation as Obs, PlaybackProgressTarget as Target,
    };
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("aggregates"))?;
    seed(&service)?;
    for title in [1, 2, 3] {
        service.library_operation(LibraryOperation::SetMembership {
            title: id(title),
            saved: true,
            restore_added_at: Some(title),
        })?;
    }
    let mut movie_run = None;
    for target in [
        Target::Movie(required(TitleId::new(1))),
        Target::Movie(required(TitleId::new(3))),
        Target::Episode(required(EpisodeId::new(100))),
    ] {
        let Out::Applied(run) = service.playback_progress_operation(Op::BeginRun {
            target,
            start_over: false,
        })?
        else {
            panic!("begin")
        };
        service.playback_progress_operation(Op::Save {
            target,
            run: run.active_run,
            epoch: run.epoch,
            observation: required(Obs::new(500, None)),
        })?;
        if target == Target::Movie(required(TitleId::new(1))) {
            movie_run = Some(run);
        }
    }
    service.library_operation(LibraryOperation::SetMovieWatched {
        title: id(3),
        watched: true,
    })?;
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            status: WatchFilter::InProgress,
            limit: 1,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 2);
    assert_eq!(page.entries.len(), 1);
    assert!(page.entries[0].personal.has_resume);
    let LibraryResult::Page(movies) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            kind: LibraryKind::Movies,
            status: WatchFilter::InProgress,
            limit: 1,
            ..Default::default()
        }))?
    else {
        panic!("movies page")
    };
    assert_eq!(movies.total, 1);
    assert_eq!(movies.entries.len(), 1);
    assert_eq!(
        movies.entries[0].personal.title_id,
        required(TitleId::new(1))
    );

    service.library_operation(LibraryOperation::SetEpisodeWatched {
        episode: EpisodeRef::Canonical(required(EpisodeId::new(100))),
        watched: true,
    })?;
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            status: WatchFilter::InProgress,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 1);
    let run = required(movie_run);
    service.playback_progress_operation(Op::Save {
        target: run.target,
        run: run.active_run,
        epoch: run.epoch,
        observation: required(Obs::new(0, None)),
    })?;
    let LibraryResult::Page(page) =
        service.library_operation(LibraryOperation::List(LibraryQuery {
            status: WatchFilter::InProgress,
            ..Default::default()
        }))?
    else {
        panic!("page")
    };
    assert_eq!(page.total, 0);
    // Cache-page pruning and membership removal never clear personal data.
    service.connection.execute_batch(
        "DELETE FROM catalog_page_items; UPDATE catalog_pages SET membership_available=0;",
    )?;
    service.library_operation(LibraryOperation::SetMembership {
        title: id(1),
        saved: false,
        restore_added_at: None,
    })?;
    let Out::Applied(retained) = service.playback_progress_operation(Op::Read(run.target))? else {
        panic!("read")
    };
    assert_eq!(required(retained.checkpoint).observation.position_ms(), 0);
    Ok(())
}

#[test]
fn progress_timestamps_advance_beyond_all_prior_checkpoints_despite_older_wall_clock() -> TestResult
{
    use crate::catalog::playback_progress::{
        PlaybackProgressOperation as Op, PlaybackProgressResult as Out,
    };
    use kinema_core::watch_progress::{
        PlaybackObservation as Obs, PlaybackProgressTarget as Target,
    };
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("progress-clock"))?;
    seed(&service)?;
    let movie = Target::Movie(required(TitleId::new(1)));
    let episode = Target::Episode(required(EpisodeId::new(100)));
    let applied = |out| match out {
        Out::Applied(state) => state,
        Out::Stale(_) => panic!("unexpected stale checkpoint"),
    };
    let movie_run = applied(service.playback_progress_operation(Op::BeginRun {
        target: movie,
        start_over: false,
    })?);
    let episode_run = applied(service.playback_progress_operation(Op::BeginRun {
        target: episode,
        start_over: false,
    })?);
    // Both values are deliberately beyond the real wall clock. The episode
    // checkpoint is newer than every watch row and the movie checkpoint.
    service.connection.execute(
        "INSERT INTO movie_playback_progress(title_id,position_ms,updated_at,revision) VALUES(1,700,?1,?2)",
        params![i64::MAX - 10, movie_run.epoch],
    )?;
    service.connection.execute(
        "INSERT INTO episode_playback_progress(episode_id,position_ms,updated_at,revision) VALUES(100,700,?1,?2)",
        params![i64::MAX - 5, episode_run.epoch],
    )?;
    assert_eq!(
        mutation_time(&service.connection.transaction()?)?,
        i64::MAX - 4
    );
    for (run, expected) in [
        (movie_run, i64::MAX - 4),
        (episode_run, i64::MAX - 3),
        (movie_run, i64::MAX - 2),
    ] {
        let saved = applied(service.playback_progress_operation(Op::Save {
            target: run.target,
            run: run.active_run,
            epoch: run.epoch,
            observation: required(Obs::new(100, Some(1000))),
        })?);
        assert_eq!(required(saved.checkpoint).updated_at, expected);
        assert!(!required(saved.watch).watched);
    }
    service.connection.execute(
        "UPDATE movie_playback_progress SET updated_at=?1",
        [i64::MAX],
    )?;
    assert_eq!(
        mutation_time(&service.connection.transaction()?),
        Err(CatalogError::Clock)
    );
    Ok(())
}

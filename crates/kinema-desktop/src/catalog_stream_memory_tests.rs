use super::*;
use crate::catalog::library::{LibraryOperation, LibraryResult};
use crate::catalog::playback_progress::{PlaybackProgressOperation, PlaybackProgressResult};
use kinema_core::library::{EpisodeId, TitleId};
use kinema_core::watch_progress::{PlaybackObservation, PlaybackProgressTarget};

type TestResult = Result<(), Box<dyn std::error::Error>>;

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn required<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("required fixture value is missing"),
    }
}
fn series() -> TitleRef {
    TitleRef::Canonical(required(TitleId::new(2)))
}
fn release(hash: &str, name: &str) -> ReleaseMemory {
    ReleaseMemory {
        info_hash: required(InfoHash::new(hash).ok()),
        name: name.into(),
        quality: Some("1080p".into()),
        size: Some(1_500_000_000),
        file_index: Some(3),
        filename: Some("hint.mkv".into()),
    }
}
fn stream(hash: &str, name: &str) -> StreamMemory {
    StreamMemory {
        release: release(hash, name),
        file_name: Some("Show.S01E02.mkv".into()),
        file_size: Some(700_000_000),
    }
}
fn remember(
    service: &mut CatalogService,
    season: u32,
    episode: u32,
    stream: StreamMemory,
) -> Result<StreamMemoryResult, CatalogError> {
    let season_release = ReleaseMemory {
        file_index: None,
        filename: None,
        ..stream.release.clone()
    };
    service.stream_memory_operation(StreamMemoryOperation::Remember(Box::new(Remember {
        series: series(),
        season,
        episode,
        stream,
        season_release,
    })))
}
fn read(
    service: &mut CatalogService,
    season: u32,
    episode: u32,
) -> Result<(Option<RememberedStream>, Option<RememberedRelease>), CatalogError> {
    match service.stream_memory_operation(StreamMemoryOperation::Read {
        series: series(),
        season,
        episode,
    })? {
        StreamMemoryResult::Memory { episode, season } => Ok((episode, season)),
        other => panic!("unexpected {other:?}"),
    }
}
fn previous(
    service: &mut CatalogService,
    season: u32,
) -> Result<Option<(u32, RememberedRelease)>, CatalogError> {
    match service.stream_memory_operation(StreamMemoryOperation::ReadPreviousSeason {
        series: series(),
        season,
    })? {
        StreamMemoryResult::PreviousSeason(found) => Ok(found),
        other => panic!("unexpected {other:?}"),
    }
}
fn seed(service: &CatalogService) -> Result<(), CatalogError> {
    service.connection.execute_batch(
        "INSERT INTO titles(id,kind,locale,updated_at) VALUES (1,'movie','en-US',1),(2,'series','en-US',1);
         INSERT INTO seasons(id,series_id,series_kind,season_number,locale,updated_at) VALUES (9,2,'series',0,'en-US',1),(10,2,'series',1,'en-US',1),(11,2,'series',2,'en-US',1),(12,2,'series',3,'en-US',1);
         INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (90,9,1,'en-US',1),(100,10,1,'en-US',1),(101,10,2,'en-US',1),(110,11,1,'en-US',1),(120,12,1,'en-US',1);",
    )?;
    Ok(())
}
fn open()
-> Result<(tempfile::TempDir, std::path::PathBuf, CatalogService), Box<dyn std::error::Error>> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog");
    let service = CatalogService::open(&path)?;
    seed(&service)?;
    Ok((dir, path, service))
}
fn count(service: &CatalogService, table: &str) -> Result<i64, rusqlite::Error> {
    service
        .connection
        .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))
}

#[test]
fn upsert_overwrites_both_records_and_reads_return_them() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    assert_eq!(read(&mut service, 1, 2)?, (None, None));
    assert_eq!(
        remember(&mut service, 1, 2, stream(HASH_A, "First"))?,
        StreamMemoryResult::Remembered
    );
    let (episode, season) = read(&mut service, 1, 2)?;
    let episode = required(episode);
    assert_eq!(episode.stream, stream(HASH_A, "First"));
    let season = required(season);
    assert_eq!(season.release.info_hash.as_str(), HASH_A);
    assert_eq!(season.release.file_index, None);
    assert_eq!(season.release.filename, None);

    // Another episode of the season sees the season release but no stream.
    let (other, shared) = read(&mut service, 1, 1)?;
    assert_eq!(other, None);
    assert_eq!(required(shared).release.name, "First");

    let mut replacement = stream(HASH_B, "Second");
    replacement.release.quality = None;
    replacement.release.size = None;
    replacement.release.file_index = None;
    replacement.release.filename = None;
    replacement.file_name = None;
    replacement.file_size = None;
    remember(&mut service, 1, 2, replacement.clone())?;
    let (episode, season) = read(&mut service, 1, 2)?;
    assert_eq!(required(episode).stream, replacement);
    assert_eq!(required(season).release.name, "Second");
    assert_eq!(count(&service, "episode_stream_memory")?, 1);
    assert_eq!(count(&service, "season_release_memory")?, 1);
    // Unknown stays NULL, never 0 or "".
    let nulls: i64 = service.connection.query_row(
        "SELECT (quality IS NULL)+(size IS NULL)+(file_index IS NULL)+(filename IS NULL)+(provider_file_name IS NULL)+(provider_file_size IS NULL) FROM episode_stream_memory",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(nulls, 6);
    Ok(())
}

#[test]
fn previous_season_is_nearest_earlier_season_and_ignores_specials() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    assert_eq!(previous(&mut service, 3)?, None);
    remember(&mut service, 0, 1, stream(HASH_B, "Special"))?;
    assert_eq!(previous(&mut service, 1)?, None);
    assert_eq!(previous(&mut service, 3)?, None);
    remember(&mut service, 1, 1, stream(HASH_A, "One"))?;
    let (number, found) = required(previous(&mut service, 3)?);
    assert_eq!((number, found.release.name.as_str()), (1, "One"));
    remember(&mut service, 2, 1, stream(HASH_B, "Two"))?;
    assert_eq!(required(previous(&mut service, 3)?).0, 2);
    assert_eq!(required(previous(&mut service, 2)?).0, 1);
    assert_eq!(previous(&mut service, 1)?, None);
    Ok(())
}

#[test]
fn invalid_writes_fail_without_touching_the_existing_record() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    remember(&mut service, 1, 1, stream(HASH_A, "Keep"))?;
    let mut bad = [
        stream(HASH_B, ""),
        stream(HASH_B, &"x".repeat(MAX_TEXT_BYTES + 1)),
        StreamMemory {
            file_name: Some("dir/file.mkv".into()),
            ..stream(HASH_B, "Path")
        },
        StreamMemory {
            file_name: Some(String::new()),
            ..stream(HASH_B, "Empty")
        },
        StreamMemory {
            file_size: Some(u64::MAX),
            ..stream(HASH_B, "Huge")
        },
    ]
    .to_vec();
    let mut empty_quality = stream(HASH_B, "Quality");
    empty_quality.release.quality = Some(String::new());
    bad.push(empty_quality);
    for stream in bad {
        assert_eq!(
            remember(&mut service, 1, 1, stream),
            Err(CatalogError::InvalidMetadata)
        );
    }
    // Unknown episode / season / non-series series.
    assert_eq!(
        remember(&mut service, 1, 9, stream(HASH_B, "x")),
        Err(CatalogError::MissingIdentityMapping)
    );
    assert_eq!(
        remember(&mut service, 9, 1, stream(HASH_B, "x")),
        Err(CatalogError::MissingIdentityMapping)
    );
    let movie = service.stream_memory_operation(StreamMemoryOperation::Read {
        series: TitleRef::Canonical(required(TitleId::new(1))),
        season: 1,
        episode: 1,
    });
    assert_eq!(movie, Err(CatalogError::InvalidMetadata));
    // A bad season record rolls back the episode row as well.
    let result =
        service.stream_memory_operation(StreamMemoryOperation::Remember(Box::new(Remember {
            series: series(),
            season: 1,
            episode: 2,
            stream: stream(HASH_B, "Atomic"),
            season_release: release(HASH_B, ""),
        })));
    assert_eq!(result, Err(CatalogError::InvalidMetadata));
    assert_eq!(count(&service, "episode_stream_memory")?, 1);
    let (episode, season) = read(&mut service, 1, 1)?;
    assert_eq!(required(episode).stream.release.name, "Keep");
    assert_eq!(required(season).release.name, "Keep");
    // The database refuses malformed hashes and zero-as-unknown independently.
    for sql in [
        "UPDATE season_release_memory SET info_hash='XYZ'",
        "UPDATE season_release_memory SET info_hash=upper(info_hash)",
        "UPDATE season_release_memory SET filename=''",
        "UPDATE episode_stream_memory SET size=-1",
    ] {
        assert!(service.connection.execute(sql, []).is_err(), "{sql}");
    }
    Ok(())
}

#[test]
fn records_survive_progress_clear_watch_changes_library_removal_and_cleanup() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    remember(&mut service, 1, 1, stream(HASH_A, "Persisted"))?;
    let target = PlaybackProgressTarget::Episode(required(EpisodeId::new(100)));
    let PlaybackProgressResult::Applied(snapshot) =
        service.playback_progress_operation(PlaybackProgressOperation::BeginRun {
            target,
            start_over: false,
        })?
    else {
        panic!("begin")
    };
    service.playback_progress_operation(PlaybackProgressOperation::Save {
        target,
        run: snapshot.active_run,
        epoch: snapshot.epoch,
        observation: required(PlaybackObservation::new(60_000, Some(120_000))),
    })?;
    service.playback_progress_operation(PlaybackProgressOperation::Clear(target))?;
    service.library_operation(LibraryOperation::SetEpisodeWatched {
        episode: crate::catalog::library::EpisodeRef::Canonical(required(EpisodeId::new(100))),
        watched: true,
    })?;
    service.library_operation(LibraryOperation::SetMembership {
        title: series(),
        saved: true,
        restore_added_at: None,
    })?;
    service.library_operation(LibraryOperation::SetMembership {
        title: series(),
        saved: false,
        restore_added_at: None,
    })?;
    // Cache cleanups that touch rows: legacy retirement and page membership.
    let tx = service.connection.transaction()?;
    retire_legacy_caches(&tx)?;
    tx.execute("DELETE FROM catalog_page_items", [])?;
    tx.commit()?;
    let (episode, season) = read(&mut service, 1, 1)?;
    assert_eq!(required(episode).stream.release.name, "Persisted");
    assert!(season.is_some());
    // Catalog rows cannot be deleted from under a personal record.
    assert!(
        service
            .connection
            .execute("DELETE FROM episodes WHERE id=100", [])
            .is_err()
    );
    assert!(
        service
            .connection
            .execute("DELETE FROM seasons WHERE id=10", [])
            .is_err()
    );
    Ok(())
}

#[test]
fn operations_route_through_the_library_protocol() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    let write =
        LibraryOperation::StreamMemory(StreamMemoryOperation::Remember(Box::new(Remember {
            series: series(),
            season: 1,
            episode: 1,
            stream: stream(HASH_A, "Routed"),
            season_release: release(HASH_A, "Routed"),
        })));
    assert!(crate::catalog_worker::local::is_mutation(&write));
    let read_op = LibraryOperation::StreamMemory(StreamMemoryOperation::Read {
        series: series(),
        season: 1,
        episode: 1,
    });
    assert!(!crate::catalog_worker::local::is_mutation(&read_op));
    assert!(matches!(
        service.library_operation(write)?,
        LibraryResult::StreamMemory(StreamMemoryResult::Remembered)
    ));
    assert!(matches!(
        service.library_operation(read_op)?,
        LibraryResult::StreamMemory(StreamMemoryResult::Memory {
            episode: Some(_),
            season: Some(_)
        })
    ));
    Ok(())
}

#[test]
fn identity_merge_carries_records_to_the_winner_newest_first() -> TestResult {
    let (_dir, _path, mut service) = open()?;
    service.connection.execute_batch(
        "INSERT INTO seasons(id,series_id,series_kind,season_number,locale,updated_at) VALUES (20,2,'series',7,'en-US',1),(21,2,'series',8,'en-US',1);
         INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (200,20,1,'en-US',1),(210,21,1,'en-US',1);",
    )?;
    let row = |table: &str, key: i64, hash: &str, at: i64| {
        let column = if table == "episode_stream_memory" {
            "episode_id"
        } else {
            "season_id"
        };
        service.connection.execute(
            &format!(
                "INSERT INTO {table}({column},info_hash,name,updated_at) VALUES(?1,?2,'n',?3)"
            ),
            params![key, hash, at],
        )
    };
    row("episode_stream_memory", 200, HASH_A, 10)?;
    row("episode_stream_memory", 210, HASH_B, 20)?;
    row("season_release_memory", 20, HASH_B, 30)?;
    row("season_release_memory", 21, HASH_A, 5)?;
    let tx = service.connection.transaction()?;
    merge(&tx, Merge::Episode, 200, 210)?;
    merge(&tx, Merge::Season, 20, 21)?;
    tx.commit()?;
    let hash = |sql: &str| -> Result<String, rusqlite::Error> {
        service.connection.query_row(sql, [], |r| r.get(0))
    };
    assert_eq!(
        hash("SELECT info_hash FROM episode_stream_memory WHERE episode_id=200")?,
        HASH_B
    );
    assert_eq!(
        hash("SELECT info_hash FROM season_release_memory WHERE season_id=20")?,
        HASH_B
    );
    assert_eq!(count(&service, "episode_stream_memory")?, 1);
    assert_eq!(count(&service, "season_release_memory")?, 1);
    // Losers can now be deleted despite RESTRICT.
    service
        .connection
        .execute("DELETE FROM episodes WHERE id=210", [])?;
    service
        .connection
        .execute("DELETE FROM seasons WHERE id=21", [])?;
    Ok(())
}

fn synthetic(path: &std::path::Path, version: i64) -> TestResult {
    let mut db = Connection::open(path)?;
    let tx = db.transaction()?;
    schema::migrate(&tx)?;
    if version >= 7 {
        schema::migrate_library(&tx)?;
    }
    if version >= 8 {
        schema::migrate_playback_progress(&tx)?;
    }
    if version < 5 {
        tx.execute_batch("ALTER TABLE metadata_fetches DROP COLUMN facts_json;")?;
    }
    tx.execute_batch(
        "INSERT INTO titles(id,kind,locale,updated_at) VALUES (2,'series','en-US',1);
         INSERT INTO seasons(id,series_id,series_kind,season_number,locale,updated_at) VALUES (10,2,'series',1,'en-US',1);
         INSERT INTO episodes(id,season_id,episode_number,locale,updated_at) VALUES (100,10,1,'en-US',1);",
    )?;
    if version >= 7 {
        tx.execute_batch(
            "INSERT INTO episode_watch_state(episode_id,watched,updated_at) VALUES(100,1,13);",
        )?;
    }
    tx.pragma_update(None, "user_version", version)?;
    tx.commit()?;
    Ok(())
}

#[test]
fn migrates_in_place_from_every_older_normalized_version() -> TestResult {
    for version in 4..SCHEMA_VERSION {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("catalog");
        synthetic(&path, version)?;
        let mut service = CatalogService::open(&path)?;
        let stored: i64 = service
            .connection
            .pragma_query_value(None, "user_version", |r| r.get(0))?;
        assert_eq!(stored, SCHEMA_VERSION, "from v{version}");
        assert_eq!(count(&service, "season_release_memory")?, 0);
        assert_eq!(count(&service, "episode_stream_memory")?, 0);
        let strict: i64 = service.connection.query_row(
            "SELECT count(*) FROM pragma_table_list WHERE name IN ('season_release_memory','episode_stream_memory') AND strict=1",
            [],
            |r| r.get(0),
        )?;
        assert_eq!(strict, 2);
        // Existing catalog and personal rows are untouched and the feature works.
        assert_eq!(count(&service, "episodes")?, 1);
        if version >= 7 {
            let watched: i64 = service.connection.query_row(
                "SELECT watched FROM episode_watch_state WHERE episode_id=100",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(watched, 1);
        }
        remember(&mut service, 1, 1, stream(HASH_A, "Migrated"))?;
        assert_eq!(
            required(read(&mut service, 1, 1)?.0).stream.release.name,
            "Migrated"
        );
        drop(service);
        // Reopening at the current version changes nothing.
        let mut service = CatalogService::open(&path)?;
        assert_eq!(
            required(read(&mut service, 1, 1)?.0).stream.release.name,
            "Migrated"
        );
    }
    Ok(())
}

#[test]
fn failed_migration_rolls_back_and_a_newer_database_is_refused() -> TestResult {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("catalog");
    synthetic(&path, 8)?;
    // A conflicting object makes the v9 DDL fail midway.
    let db = Connection::open(&path)?;
    db.execute_batch("CREATE TABLE episode_stream_memory (x INTEGER);")?;
    drop(db);
    assert!(CatalogService::open(&path).is_err());
    let db = Connection::open(&path)?;
    let version: i64 = db.pragma_query_value(None, "user_version", |r| r.get(0))?;
    assert_eq!(version, 8);
    let created: i64 = db.query_row(
        "SELECT count(*) FROM sqlite_schema WHERE name='season_release_memory'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(created, 0);
    drop(db);

    let newer = dir.path().join("newer");
    let db = Connection::open(&newer)?;
    db.pragma_update(None, "user_version", SCHEMA_VERSION + 1)?;
    drop(db);
    assert!(matches!(
        CatalogService::open(&newer),
        Err(CatalogError::NewerDatabase)
    ));
    Ok(())
}

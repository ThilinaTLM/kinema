//! Conservative v3-only forward import. Historical databases are never opened.
//! Each reduced legacy snapshot is independently validated; ambiguous shared
//! search-title rows are not attributed to a particular upstream observation.
use super::*;
use rusqlite::Transaction;

fn kind(value: &str) -> Option<MediaKind> {
    match value {
        "movie" => Some(MediaKind::Movie),
        "series" | "tv" => Some(MediaKind::Series),
        _ => None,
    }
}
fn attempt(
    tx: &Transaction<'_>,
    import: impl FnOnce() -> Result<(), CatalogError>,
) -> Result<(), CatalogError> {
    tx.execute_batch("SAVEPOINT legacy_snapshot")?;
    match import() {
        Ok(()) => tx.execute_batch("RELEASE legacy_snapshot")?,
        Err(CatalogError::IdentityConflict) => {
            tx.execute_batch("ROLLBACK TO legacy_snapshot; RELEASE legacy_snapshot")?
        }
        Err(error) => {
            tx.execute_batch("ROLLBACK TO legacy_snapshot; RELEASE legacy_snapshot")?;
            return Err(error);
        }
    }
    Ok(())
}
// V3's seven cache tables are ordinary rowid tables. Their rows are never
// modified by this migration, so rowid is a stable transaction-local keyset.
// Finish each single-row read before SAVEPOINT work: a conflict rollback can
// invalidate an outstanding SQLite cursor, even over an unchanged legacy table.
fn scan<T>(
    tx: &Transaction<'_>,
    table: &str,
    columns: &str,
    filter: &str,
    extra_parameters: &[&dyn rusqlite::ToSql],
    mut decode: impl FnMut(&rusqlite::Row<'_>) -> rusqlite::Result<T>,
    mut import: impl FnMut(T) -> Result<(), CatalogError>,
) -> Result<(), CatalogError> {
    let mut last: Option<i64> = None;
    loop {
        let next = {
            // Separate first/continuation predicates let SQLite seek the rowid
            // for every continuation rather than rescanning an OR/NULL filter.
            let after = if last.is_some() {
                "rowid > ?1"
            } else {
                "?1 IS NULL"
            };
            let sql = format!(
                "SELECT rowid,{columns} FROM {table} WHERE ({filter}) AND {after} ORDER BY rowid LIMIT 1"
            );
            let mut stmt = tx.prepare(&sql)?;
            let parameters = std::iter::once(&last as &dyn rusqlite::ToSql)
                .chain(extra_parameters.iter().copied());
            stmt.query_row(rusqlite::params_from_iter(parameters), |row| {
                Ok((row.get::<_, i64>(0)?, decode(row)?))
            })
            .optional()?
        };
        let Some((rowid, value)) = next else {
            break;
        };
        last = Some(rowid);
        attempt(tx, || import(value))?;
    }
    Ok(())
}

pub(super) fn backfill(tx: &Transaction<'_>) -> Result<(), CatalogError> {
    scan(
        tx,
        "cinemeta_v3_searches",
        "kind,query,fetched_at",
        "result_count=0 AND NOT EXISTS(SELECT 1 FROM cinemeta_v3_members m WHERE m.kind=cinemeta_v3_searches.kind AND m.query=cinemeta_v3_searches.query)",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
        |(k, text, time)| {
            let query = SearchQuery::new(kind(&k).ok_or(CatalogError::InvalidCache)?, text)
                .map_err(|_| CatalogError::InvalidCache)?;
            if time < 0 {
                return Err(CatalogError::InvalidCache);
            }
            normalized_pages::write_page(
                tx,
                "cinemeta",
                &normalized_pages::cinemeta_search_key(&query),
                1,
                None,
                &[],
                time,
            )
        },
    )?;
    scan(
        tx,
        "cinemeta_v3_details",
        "kind,imdb_id,fetched_at,payload",
        "1",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
        |(k, external, time, payload)| {
            metadata::backfill_details(
                tx,
                kind(&k).ok_or(CatalogError::InvalidCache)?,
                &external,
                time,
                &payload,
            )
        },
    )?;
    scan(
        tx,
        "cinemeta_v3_browse",
        "kind,offset,fetched_at,payload",
        "1",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, u32>(2)?,
                row.get::<_, i64>(3)?,
                row.get::<_, String>(4)?,
            ))
        },
        |(k, offset, time, payload)| {
            metadata::backfill_browse(
                tx,
                kind(&k).ok_or(CatalogError::InvalidCache)?,
                offset,
                time,
                &payload,
            )
        },
    )?;

    // Bounded multipass scans establish explicit mappings before seasons/pages.
    // Never sort payload rows: that could materialize the whole cache. Exact
    // canonical prefixes prevent search text from changing its tier. The final
    // pass also validates unknown/malformed resource keys.
    let details_prefix = format!("{}:details:", crate::tmdb::CACHE_QUERY_IDENTITY);
    let seasons_prefix = format!("{}:season:", crate::tmdb::CACHE_QUERY_IDENTITY);
    for pass in 0..3 {
        scan(
            tx,
            "tmdb_v1_cache",
            "resource,fetched_at,payload",
            "locale='en-US' AND CASE ?2
               WHEN 0 THEN substr(resource,1,length(?3))=?3
               WHEN 1 THEN substr(resource,1,length(?4))=?4
               ELSE substr(resource,1,length(?3))<>?3 AND substr(resource,1,length(?4))<>?4
             END",
            &[&pass, &details_prefix, &seasons_prefix],
            |row| {
                Ok((
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
            |(resource, time, payload)| {
                let query = parse_tmdb(&resource).ok_or(CatalogError::InvalidCache)?;
                if time < 0 {
                    return Err(CatalogError::InvalidCache);
                }
                let data = crate::tmdb::decode(&payload, &query)
                    .map_err(|_| CatalogError::InvalidCache)?;
                normalized_tmdb::write(tx, &query, &data, time, false)
            },
        )?;
    }
    scan(
        tx,
        "indexer_sources_v1",
        "indexer,target,scope,fetched_at,payload",
        "1",
        &[],
        |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, i64>(4)?,
                row.get::<_, String>(5)?,
            ))
        },
        |(indexer, target, scope, time, payload)| {
            metadata::backfill_sources(tx, &indexer, &target, &scope, time, &payload)
        },
    )?;
    Ok(())
}

fn parse_tmdb(resource: &str) -> Option<crate::tmdb::TmdbQuery> {
    use crate::tmdb::TmdbQuery;
    let rest = resource
        .strip_prefix(crate::tmdb::CACHE_QUERY_IDENTITY)?
        .strip_prefix(':')?;
    let mut parts = rest.splitn(4, ':');
    let operation = parts.next()?;
    let first = parts.next()?;
    let second = parts.next()?;
    let query = match operation {
        "browse" => TmdbQuery::Browse {
            kind: kind(first)?,
            page: second.parse().ok()?,
        },
        "search" => TmdbQuery::Search {
            query: SearchQuery::new(kind(first)?, parts.next()?).ok()?,
            page: second.parse().ok()?,
        },
        "details" => TmdbQuery::Details {
            kind: kind(first)?,
            id: second.parse().ok()?,
        },
        "season" => TmdbQuery::Season {
            series_id: first.parse().ok()?,
            season: second.parse().ok()?,
        },
        _ => return None,
    };
    (crate::tmdb::cache_query_identity(&query).ok()?.as_str() == resource).then_some(query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tmdb::{TmdbData, TmdbDetails, TmdbPage, TmdbQuery, TmdbSeason, TmdbTitle};
    type TestResult = Result<(), Box<dyn Error>>;

    fn legacy(path: &Path) -> Result<Connection, Box<dyn Error>> {
        let db = Connection::open(path)?;
        db.execute_batch(
            "CREATE TABLE cinemeta_v3_searches(kind TEXT,query TEXT,fetched_at INTEGER,result_count INTEGER,PRIMARY KEY(kind,query));
             CREATE TABLE cinemeta_v3_titles(kind TEXT,imdb_id TEXT,name TEXT,overview TEXT,PRIMARY KEY(kind,imdb_id));
             CREATE TABLE cinemeta_v3_members(kind TEXT,query TEXT,position INTEGER,imdb_id TEXT,PRIMARY KEY(kind,query,position));
             CREATE TABLE cinemeta_v3_details(kind TEXT,imdb_id TEXT,fetched_at INTEGER,payload TEXT,PRIMARY KEY(kind,imdb_id));
             CREATE TABLE cinemeta_v3_browse(kind TEXT,offset INTEGER,fetched_at INTEGER,payload TEXT,PRIMARY KEY(kind,offset));
             CREATE TABLE tmdb_v1_cache(resource TEXT,locale TEXT,fetched_at INTEGER,payload TEXT,PRIMARY KEY(resource,locale));
             CREATE TABLE indexer_sources_v1(indexer TEXT,target TEXT,scope TEXT,fetched_at INTEGER,payload TEXT,PRIMARY KEY(indexer,target,scope));
             PRAGMA user_version=3;",
        )?;
        Ok(db)
    }
    fn save_tmdb(
        tx: &rusqlite::Transaction<'_>,
        query: &TmdbQuery,
        data: &TmdbData,
        time: i64,
    ) -> TestResult {
        tx.execute(
            "INSERT INTO tmdb_v1_cache VALUES (?1,'en-US',?2,?3)",
            params![
                crate::tmdb::cache_query_identity(query)?,
                time,
                crate::tmdb::encode(data)?
            ],
        )?;
        Ok(())
    }
    fn seed(db: &mut Connection, count: u32) -> TestResult {
        let tx = db.transaction()?;
        let overview = "Synthetic retained description. ".repeat(128);
        for i in 0..count {
            let imdb = format!("tt{}", 7_000_000 + i);
            tx.execute(
                "INSERT INTO cinemeta_v3_searches VALUES ('movie',?1,1,0)",
                params![format!("empty-{i}")],
            )?;
            let details = serde_json::json!({"id":imdb,"type":"movie","name":"Synthetic movie","description":overview,"genres":[],"videos":[]}).to_string();
            tx.execute(
                "INSERT INTO cinemeta_v3_details VALUES ('movie',?1,10,?2)",
                params![imdb, details],
            )?;
            let browse = serde_json::json!({"metas":[{"id":imdb,"type":"movie","name":"Synthetic movie","description":overview}]}).to_string();
            tx.execute(
                "INSERT INTO cinemeta_v3_browse VALUES ('movie',?1,15,?2)",
                params![i, browse],
            )?;
            tx.execute(
                "INSERT INTO indexer_sources_v1 VALUES ('torrentio',?1,'public-default-v1',50,?2)",
                params![format!("movie:{imdb}"), r#"{"candidates":[],"omitted":0}"#],
            )?;
            let movie = TmdbTitle {
                id: u64::from(1_000 + i),
                kind: MediaKind::Movie,
                name: "Synthetic movie".into(),
                overview: Some(overview.clone()),
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            };
            let series = TmdbTitle {
                id: u64::from(5_000 + i),
                kind: MediaKind::Series,
                name: "Synthetic series".into(),
                overview: Some(overview.clone()),
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            };
            // Physical insertion order deliberately opposes dependency order.
            save_tmdb(
                &tx,
                &TmdbQuery::Search {
                    query: SearchQuery::new(
                        MediaKind::Movie,
                        format!("query-{i}:details:season:"),
                    )?,
                    page: 1,
                },
                &TmdbData::Page(TmdbPage {
                    titles: vec![movie.clone()],
                    page: 1,
                    total_pages: 1,
                }),
                40,
            )?;
            save_tmdb(
                &tx,
                &TmdbQuery::Season {
                    series_id: series.id,
                    season: 0,
                },
                &TmdbData::Season(TmdbSeason {
                    poster_url: None,
                    series_id: series.id,
                    season: 0,
                    episodes: vec![],
                }),
                30,
            )?;
            save_tmdb(
                &tx,
                &TmdbQuery::Details {
                    kind: MediaKind::Movie,
                    id: movie.id,
                },
                &TmdbData::Details(TmdbDetails {
                    backdrop_url: None,
                    title: movie,
                    imdb: Some(ImdbId::new(imdb)?),
                    genres: vec![],
                    seasons: vec![],
                }),
                20,
            )?;
            save_tmdb(
                &tx,
                &TmdbQuery::Details {
                    kind: MediaKind::Series,
                    id: series.id,
                },
                &TmdbData::Details(TmdbDetails {
                    backdrop_url: None,
                    title: series,
                    imdb: Some(ImdbId::new(format!("tt{}", 8_000_000 + i))?),
                    genres: vec![],
                    seasons: vec![],
                }),
                20,
            )?;
        }
        tx.commit()?;
        Ok(())
    }
    fn count(db: &Connection, table: &str) -> Result<i64, rusqlite::Error> {
        db.query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
            row.get(0)
        })
    }

    #[test]
    fn many_legacy_rows_stream_through_every_resource_tier_in_dependency_order() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("synthetic.sqlite3");
        let mut db = legacy(&path)?;
        const ROWS: u32 = 96;
        seed(&mut db, ROWS)?;
        drop(db);
        let service = CatalogService::open(&path)?;
        let db = &service.connection;
        for (table, expected) in [
            ("titles", 2 * ROWS),
            ("seasons", ROWS),
            ("catalog_pages", 3 * ROWS),
            ("indexer_snapshots", ROWS),
            ("cinemeta_v3_details", ROWS),
            ("cinemeta_v3_browse", ROWS),
            ("tmdb_v1_cache", 4 * ROWS),
            ("indexer_sources_v1", ROWS),
        ] {
            assert_eq!(count(db, table)?, i64::from(expected), "{table}");
        }
        // Identifier supplier times expose the actual processing tiers: pages
        // must confirm movie mappings after details, seasons after series details.
        for (kind, expected) in [("movie", 40), ("series", 30)] {
            let wrong:i64=db.query_row("SELECT count(*) FROM title_identifiers WHERE namespace='tmdb' AND media_kind=?1 AND observed_at<>?2",params![kind,expected],|row|row.get(0))?;
            assert_eq!(wrong, 0, "{kind}");
        }
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            4
        );
        assert_eq!(count(db, "pragma_foreign_key_check")?, 0);
        Ok(())
    }

    #[test]
    fn conflict_savepoint_rolls_back_only_that_resource_and_cursor_continues() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("synthetic.sqlite3");
        let mut db = legacy(&path)?;
        seed(&mut db, 4)?;
        let tx = db.transaction()?;
        let contradictory = TmdbTitle {
            id: 9_999,
            kind: MediaKind::Movie,
            name: "Contradictory registry mapping".into(),
            overview: None,
            release_info: None,
            poster_url: None,
            display: Default::default(),
            imdb: None,
        };
        save_tmdb(
            &tx,
            &TmdbQuery::Details {
                kind: MediaKind::Movie,
                id: 9_999,
            },
            &TmdbData::Details(TmdbDetails {
                backdrop_url: None,
                title: contradictory,
                imdb: Some(ImdbId::new("tt7000000")?),
                genres: vec![],
                seasons: vec![],
            }),
            21,
        )?;
        tx.commit()?;
        drop(db);
        let service = CatalogService::open(&path)?;
        assert_eq!(count(&service.connection, "titles")?, 8);
        assert_eq!(count(&service.connection, "indexer_snapshots")?, 4);
        let unwanted: i64 = service.connection.query_row(
            "SELECT count(*) FROM metadata_fetches WHERE resource_key=?1",
            params![crate::tmdb::cache_query_identity(&TmdbQuery::Details {
                kind: MediaKind::Movie,
                id: 9_999
            })?],
            |row| row.get(0),
        )?;
        assert_eq!(unwanted, 0);
        assert_eq!(count(&service.connection, "tmdb_v1_cache")?, 17);
        Ok(())
    }

    #[test]
    fn malformed_late_source_rolls_back_all_streamed_imports_and_version() -> TestResult {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("synthetic.sqlite3");
        let mut db = legacy(&path)?;
        seed(&mut db, 12)?;
        db.execute("INSERT INTO indexer_sources_v1 VALUES ('torrentio','movie:tt9999999','public-default-v1',60,'malformed-late-payload')",[])?;
        // Confirm earlier resources were actually imported before the late error,
        // then exercise the same outer transaction's complete rollback.
        {
            let tx = db.transaction()?;
            schema::migrate(&tx)?;
            assert_eq!(backfill(&tx), Err(CatalogError::InvalidCache));
            assert_eq!(count(&tx, "titles")?, 24);
            assert_eq!(count(&tx, "indexer_snapshots")?, 12);
            tx.rollback()?;
        }
        drop(db);
        assert!(matches!(
            CatalogService::open(&path),
            Err(CatalogError::InvalidCache)
        ));
        let db = Connection::open(&path)?;
        assert_eq!(
            db.pragma_query_value(None, "user_version", |row| row.get::<_, i64>(0))?,
            3
        );
        let normalized: i64 = db.query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='titles'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(normalized, 0);
        assert_eq!(count(&db, "cinemeta_v3_details")?, 12);
        assert_eq!(count(&db, "tmdb_v1_cache")?, 48);
        assert_eq!(count(&db, "indexer_sources_v1")?, 13);
        let malformed: String = db.query_row(
            "SELECT payload FROM indexer_sources_v1 WHERE target='movie:tt9999999'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(malformed, "malformed-late-payload");
        Ok(())
    }
    #[test]
    fn legacy_known_detail_fields_keep_detail_quality_without_rich_completeness() -> TestResult {
        use super::super::{normalized::*, normalized_pages::*};
        use kinema_core::catalog_normalized::{FieldValue, Quality};
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("synthetic.sqlite3");
        let mut db = legacy(&path)?;
        let cinemeta_id = ImdbId::new("tt9100001")?;
        let old_poster = "https://images.example.test/detail.jpg";
        let payload=serde_json::json!({"id":cinemeta_id.as_str(),"type":"movie","name":"Legacy Cinemeta detail","poster":old_poster,"genres":["Drama"],"videos":[]}).to_string();
        db.execute(
            "INSERT INTO cinemeta_v3_details VALUES ('movie',?1,10,?2)",
            params![cinemeta_id.as_str(), payload],
        )?;
        let detail_query = TmdbQuery::Details {
            kind: MediaKind::Movie,
            id: 7001,
        };
        let old_tmdb_poster = "https://image.tmdb.org/t/p/w500/detail.jpg";
        let tmdb_title = TmdbTitle {
            id: 7001,
            kind: MediaKind::Movie,
            name: "Legacy TMDB detail".into(),
            overview: None,
            release_info: None,
            poster_url: Some(old_tmdb_poster.into()),
            display: Default::default(),
            imdb: None,
        };
        let season_query = TmdbQuery::Season {
            series_id: 8001,
            season: 0,
        };
        let tx = db.transaction()?;
        save_tmdb(
            &tx,
            &detail_query,
            &TmdbData::Details(TmdbDetails {
                backdrop_url: None,
                title: tmdb_title.clone(),
                imdb: None,
                genres: vec!["Drama".into()],
                seasons: vec![],
            }),
            20,
        )?;
        save_tmdb(
            &tx,
            &season_query,
            &TmdbData::Season(TmdbSeason {
                poster_url: None,
                series_id: 8001,
                season: 0,
                episodes: vec![CatalogEpisode {
                    still_url: None,
                    runtime_minutes: None,
                    season: 0,
                    number: 1,
                    name: Some("Known legacy episode".into()),
                    overview: None,
                    released: None,
                }],
            }),
            30,
        )?;
        tx.commit()?;
        drop(db);
        let mut service = CatalogService::open(&path)?;
        assert_eq!(service.read_details(MediaKind::Movie, &cinemeta_id)?, None);
        assert_eq!(service.read_tmdb(&detail_query)?, None);
        assert_eq!(service.read_tmdb(&season_query)?, None);
        {
            let tx = service.connection.transaction()?;
            let mut summary = imdb_observation(&CatalogTitle::new(
                cinemeta_id.clone(),
                MediaKind::Movie,
                "Newer Cinemeta summary",
                None,
            )?);
            summary.fields.push((
                "poster_url".into(),
                FieldValue::Text("https://images.example.test/summary.jpg".into()),
            ));
            write_page(
                &tx,
                "cinemeta",
                "synthetic-summary-v1",
                1,
                None,
                &[summary],
                100,
            )?;
            let mut newer = tmdb_title;
            newer.name = "Newer TMDB summary".into();
            newer.poster_url = Some("https://image.tmdb.org/t/p/w500/summary.jpg".into());
            normalized_tmdb::write(
                &tx,
                &TmdbQuery::Browse {
                    kind: MediaKind::Movie,
                    page: 1,
                },
                &TmdbData::Page(TmdbPage {
                    titles: vec![newer],
                    page: 1,
                    total_pages: 1,
                }),
                100,
                true,
            )?;
            for (namespace, external, name, poster, time) in [
                (
                    "imdb",
                    cinemeta_id.as_str(),
                    "Legacy Cinemeta detail",
                    old_poster,
                    10,
                ),
                ("tmdb", "7001", "Legacy TMDB detail", old_tmdb_poster, 20),
            ] {
                let id =
                    title_id(&tx, MediaKind::Movie, namespace, external)?.ok_or("missing title")?;
                let record = CatalogService::title_record(&tx, id)?;
                assert_eq!(record.text("name"), Some(name));
                assert_eq!(record.text("poster_url"), Some(poster));
                for field in ["name", "poster_url", "genres_json"] {
                    let origin = record
                        .provenance
                        .iter()
                        .find(|origin| origin.field_name == field)
                        .ok_or("missing detail origin")?;
                    assert_eq!(origin.quality, Quality::Detail);
                    assert_eq!(origin.observed_at, time);
                }
                assert!(!record.fields.contains_key("logo_url"));
            }
            let series =
                title_id(&tx, MediaKind::Series, "tmdb", "8001")?.ok_or("missing series")?;
            let children = CatalogService::episode_records(&tx, series, Some(0))?;
            assert_eq!(children[0].1.text("name"), Some("Known legacy episode"));
            assert_eq!(children[0].1.provenance[0].quality, Quality::Detail);
            tx.commit()?;
        }
        // Retained fields and newer page successes still cannot satisfy the
        // selected provider's missing rich detail/season representation.
        assert_eq!(service.read_details(MediaKind::Movie, &cinemeta_id)?, None);
        assert_eq!(service.read_tmdb(&detail_query)?, None);
        assert_eq!(service.read_tmdb(&season_query)?, None);
        Ok(())
    }
}

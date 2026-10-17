//! Additional synthetic failure and child-coordinate acceptance cases.
use super::*;
use super::{normalized::*, normalized_pages::*};
use kinema_core::catalog_normalized::{FieldValue, Quality};
type TestResult = Result<(), Box<dyn Error>>;

#[test]
fn season_date_year_pairs_refresh_atomically_and_partial_conflict_is_explicit() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    for (time, date, year) in [(1, "2025-06-03", 2025), (2, "2026-07-04", 2026)] {
        let tx = service.connection.transaction()?;
        let item = imdb_observation(&CatalogTitle::new(
            ImdbId::new("tt2000001")?,
            MediaKind::Series,
            "Series",
            None,
        )?);
        let title = resolve_title(&tx, &item, time)?;
        let f = fetch(
            &tx,
            Target::Title(title),
            "cinemeta",
            "series-v1",
            Quality::Detail,
            false,
            time,
        )?;
        import_title(&tx, title, &item, "cinemeta", Quality::Detail, f, time)?;
        let id = season(&tx, title, 0, time)?;
        normalized_pairs::fields(
            &tx,
            Target::Season(id),
            &[
                ("air_date".into(), FieldValue::Text(date.into())),
                ("air_year".into(), FieldValue::Integer(year)),
            ],
            "cinemeta",
            Quality::Detail,
            f,
            time,
        )?;
        tx.commit()?;
    }
    let tx = service.connection.transaction()?;
    let id: i64 = tx.query_row("SELECT id FROM seasons", [], |r| r.get(0))?;
    let f: i64 = tx.query_row("SELECT id FROM metadata_fetches", [], |r| r.get(0))?;
    assert_eq!(
        normalized_pairs::fields(
            &tx,
            Target::Season(id),
            &[("air_date".into(), FieldValue::Text("2027-01-02".into()))],
            "cinemeta",
            Quality::Detail,
            f,
            3
        ),
        Err(CatalogError::InvalidMetadata)
    );
    let (date, year): (String, i64) =
        tx.query_row("SELECT air_date,air_year FROM seasons", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })?;
    assert_eq!((date.as_str(), year), ("2026-07-04", 2026));
    Ok(())
}

#[test]
fn failure_state_is_not_empty_or_success_and_repeated_failure_retains_success_time() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let query = SearchQuery::new(MediaKind::Movie, "Exact synthetic query")?;
    normalized_fetches::search(&mut service, &query, CatalogError::Network);
    assert_eq!(service.read(&query)?, None);
    let (success, error): (Option<i64>, String) = service.connection.query_row(
        "SELECT fetched_at,last_error_code FROM metadata_fetches",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(success, None);
    assert_eq!(error, "network");
    {
        let tx = service.connection.transaction()?;
        write_page(
            &tx,
            "cinemeta",
            &cinemeta_search_key(&query),
            1,
            None,
            &[],
            123,
        )?;
        tx.commit()?;
    }
    normalized_fetches::search(&mut service, &query, CatalogError::InvalidMetadata);
    let saved = service.read(&query)?.ok_or("missing authoritative empty")?;
    assert!(saved.titles.is_empty());
    assert_eq!(saved.fetched_at, 123);
    let (success, kind, error): (i64, String, String) = service.connection.query_row(
        "SELECT fetched_at,success_kind,last_error_code FROM metadata_fetches",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert_eq!(
        (success, kind.as_str(), error.as_str()),
        (123, "empty", "invalid_metadata")
    );
    Ok(())
}

#[test]
fn stale_generation_mid_transaction_rolls_back_every_new_canonical_row() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let cancel = AtomicBool::new(false);
    {
        let tx = service.connection.transaction()?;
        let title = CatalogTitle::new(
            ImdbId::new("tt2000002")?,
            MediaKind::Movie,
            "Obsolete",
            None,
        )?;
        write_page(
            &tx,
            "cinemeta",
            "obsolete-v1",
            1,
            None,
            &[imdb_observation(&title)],
            10,
        )?;
        cancel.store(true, Ordering::Release);
        assert_eq!(
            check_cancellation(Some(&cancel)),
            Err(CatalogError::Canceled)
        );
        // The production precommit guard returns here and drops the transaction.
    }
    for table in [
        "titles",
        "catalog_pages",
        "metadata_fetches",
        "title_identifiers",
        "field_provenance",
        "catalog_page_items",
    ] {
        let count: i64 =
            service
                .connection
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |r| r.get(0))?;
        assert_eq!(count, 0, "{table}");
    }
    Ok(())
}

#[test]
fn child_external_id_cannot_be_reparented_or_renumbered() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    let item = imdb_observation(&CatalogTitle::new(
        ImdbId::new("tt2000003")?,
        MediaKind::Series,
        "Series",
        None,
    )?);
    let title = resolve_title(&tx, &item, 1)?;
    let f = fetch(
        &tx,
        Target::Title(title),
        "tmdb",
        "series-v1",
        Quality::Detail,
        false,
        1,
    )?;
    import_title(&tx, title, &item, "tmdb", Quality::Detail, f, 1)?;
    let first = season(&tx, title, 1, 1)?;
    let second = season(&tx, title, 2, 1)?;
    let ident = kinema_core::catalog_normalized::ExternalIdentifier {
        namespace: "tmdb".into(),
        external_id: "95".into(),
    };
    identifier(&tx, Target::Season(first), None, &ident, f, 1)?;
    assert_eq!(
        identifier(&tx, Target::Season(second), None, &ident, f, 2),
        Err(CatalogError::IdentityConflict)
    );
    let first_ep = episode(&tx, first, 1, 1)?;
    let second_ep = episode(&tx, second, 1, 1)?;
    identifier(&tx, Target::Episode(first_ep), None, &ident, f, 1)?;
    assert_eq!(
        identifier(&tx, Target::Episode(second_ep), None, &ident, f, 2),
        Err(CatalogError::IdentityConflict)
    );
    Ok(())
}

#[test]
fn rich_title_read_exposes_origins_and_supplier_scoped_rating_without_fallback() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let tx = service.connection.transaction()?;
    let item = imdb_observation(&CatalogTitle::new(
        ImdbId::new("tt2000004")?,
        MediaKind::Movie,
        "Rich",
        None,
    )?);
    let id = resolve_title(&tx, &item, 1)?;
    let f = fetch(
        &tx,
        Target::Title(id),
        "cinemeta",
        "details-v1",
        Quality::Detail,
        false,
        1,
    )?;
    import_title(&tx, id, &item, "cinemeta", Quality::Detail, f, 1)?;
    let raw = serde_json::json!({"id":"tt2000004","type":"movie","logo":"https://images.example.test/logo.png","imdbRating":"7.4","genres":[]});
    normalized_rich::write(&tx, id, &normalized_rich::cinemeta(&raw)?, "cinemeta", f, 1)?;
    let record = CatalogService::title_record(&tx, id)?;
    assert_eq!(
        record.text("logo_url"),
        Some("https://images.example.test/logo.png")
    );
    assert_eq!(record.ratings[0].observation.source, "imdb");
    assert_eq!(record.ratings[0].observation.supplied_by, "cinemeta");
    assert!(
        record
            .provenance
            .iter()
            .any(|origin| origin.field_name == "logo_url" && origin.supplied_by == "cinemeta")
    );
    Ok(())
}

#[test]
fn deleted_nonnull_field_provenance_is_invalid_cache_not_unattributed_metadata() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let query = SearchQuery::new(MediaKind::Movie, "origin")?;
    {
        let tx = service.connection.transaction()?;
        let title = CatalogTitle::new(
            ImdbId::new("tt2000005")?,
            MediaKind::Movie,
            "Origin required",
            None,
        )?;
        write_page(
            &tx,
            "cinemeta",
            &cinemeta_search_key(&query),
            1,
            None,
            &[imdb_observation(&title)],
            1,
        )?;
        tx.commit()?;
    }
    service
        .connection
        .execute("DELETE FROM field_provenance WHERE field_name='name'", [])?;
    assert_eq!(service.read(&query), Err(CatalogError::InvalidCache));
    Ok(())
}

#[test]
fn selected_provider_missing_field_retains_actual_other_supplier_origin_without_detail_fallback()
-> TestResult {
    use crate::tmdb::{TmdbData, TmdbDetails, TmdbQuery, TmdbTitle};
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let title = CatalogTitle::new(
        ImdbId::new("tt2000006")?,
        MediaKind::Movie,
        "Rich Cinemeta",
        Some("Cinemeta overview".into()),
    )?;
    {
        let tx = service.connection.transaction()?;
        let details = TitleDetails {
            title,
            poster_url: Some("https://images.example.test/retained.jpg".into()),
            background_url: None,
            release_info: None,
            genres: vec![],
            episodes: vec![],
        };
        write_cinemeta_details(&tx, &details, 1, true)?;
        tx.commit()?;
    }
    let query = TmdbQuery::Details {
        kind: MediaKind::Movie,
        id: 99,
    };
    assert_eq!(service.read_tmdb(&query)?, None);
    service.save_tmdb(
        &query,
        TmdbData::Details(TmdbDetails {
            title: TmdbTitle {
                id: 99,
                kind: MediaKind::Movie,
                name: "Selected TMDB".into(),
                overview: None,
                release_info: None,
                poster_url: None,
                display: Default::default(),
                imdb: None,
            },
            imdb: Some(ImdbId::new("tt2000006")?),
            genres: vec![],
            seasons: vec![],
        }),
    )?;
    let tx = service.connection.transaction()?;
    let id = title_id(&tx, MediaKind::Movie, "tmdb", "99")?.ok_or("missing id")?;
    let record = CatalogService::title_record(&tx, id)?;
    for field in ["overview", "poster_url"] {
        let origin = record
            .provenance
            .iter()
            .find(|origin| origin.field_name == field)
            .ok_or("missing origin")?;
        assert_eq!(origin.supplied_by, "cinemeta");
        assert_eq!(origin.observed_at, 1);
        assert_eq!(origin.quality, Quality::Detail);
    }
    assert_eq!(record.text("overview"), Some("Cinemeta overview"));
    let name = record
        .provenance
        .iter()
        .find(|origin| origin.field_name == "name")
        .ok_or("missing name origin")?;
    assert_eq!(name.supplied_by, "tmdb");
    Ok(())
}

#[test]
fn unmapped_source_target_is_explicit_unavailability_before_indexer_initialization() -> TestResult {
    let dir = tempfile::tempdir()?;
    let mut service = CatalogService::open(&dir.path().join("catalog"))?;
    let target = PlaybackTarget::Movie(ImdbId::new("tt2000007")?);
    assert_eq!(
        service.refresh_sources(IndexerKind::Torrentio, &target),
        Err(CatalogError::MissingIdentityMapping)
    );
    assert!(service.indexers.is_none());
    Ok(())
}

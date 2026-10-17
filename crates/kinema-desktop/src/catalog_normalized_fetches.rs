//! Sanitized failed-attempt state, separate from successful snapshots.
use super::*;
use crate::tmdb::TmdbQuery;

enum RequestTarget {
    Page {
        provider: &'static str,
        key: String,
        number: u32,
    },
    Title {
        provider: &'static str,
        key: String,
        kind: MediaKind,
        namespace: &'static str,
        external: String,
    },
    Season {
        key: String,
        series: u64,
        number: u32,
    },
}
fn error_code(error: CatalogError) -> &'static str {
    match error {
        CatalogError::IdentityConflict => "identity_conflict",
        CatalogError::InvalidResponse | CatalogError::InvalidMetadata => "invalid_metadata",
        CatalogError::TmdbMissingCredential => "missing_credential",
        CatalogError::TmdbCredentialDenied => "credential_denied",
        CatalogError::TmdbCredentialUnavailable => "credential_unavailable",
        CatalogError::Tmdb(crate::tmdb::TmdbError::RejectedCredential) => "credential_rejected",
        CatalogError::Tmdb(crate::tmdb::TmdbError::RateLimited) => "rate_limited",
        CatalogError::Tmdb(crate::tmdb::TmdbError::Malformed) => "invalid_metadata",
        CatalogError::Network | CatalogError::Tmdb(crate::tmdb::TmdbError::Network) => "network",
        CatalogError::HttpStatus | CatalogError::Tmdb(crate::tmdb::TmdbError::HttpStatus) => {
            "http_status"
        }
        CatalogError::ResponseTooLarge | CatalogError::Tmdb(crate::tmdb::TmdbError::TooLarge) => {
            "response_too_large"
        }
        _ => "resource_failed",
    }
}
fn failed(
    service: &mut CatalogService,
    target: RequestTarget,
    error: CatalogError,
) -> Result<(), CatalogError> {
    if error == CatalogError::Canceled {
        return Ok(());
    }
    service.ensure_active()?;
    let (_, time) = super::metadata::timestamp_now()?;
    let cancellation = service.cancellation.clone();
    let tx = service.connection.transaction()?;
    let (column, id, provider, key) = match target {
        RequestTarget::Page {
            provider,
            key,
            number,
        } => {
            tx.execute("INSERT INTO catalog_pages(provider,query_key,page_number,result_count,membership_available,updated_at) VALUES (?1,?2,?3,NULL,0,?4) ON CONFLICT(provider,query_key,page_number) DO NOTHING",params![provider,key,number,time])?;
            let id=tx.query_row("SELECT id FROM catalog_pages WHERE provider=?1 AND query_key=?2 AND page_number=?3",params![provider,key,number],|r|r.get::<_,i64>(0))?;
            ("page_id", id, provider, key)
        }
        RequestTarget::Title {
            provider,
            key,
            kind,
            namespace,
            external,
        } => {
            let Some(id) = super::normalized::title_id(&tx, kind, namespace, &external)? else {
                return Ok(());
            };
            ("title_id", id, provider, key)
        }
        RequestTarget::Season {
            key,
            series,
            number,
        } => {
            let id:Option<i64>=tx.query_row("SELECT s.id FROM seasons s JOIN title_identifiers i ON i.title_id=s.series_id WHERE i.namespace='tmdb' AND i.media_kind='series' AND i.external_id=?1 AND s.season_number=?2",params![series.to_string(),number],|r|r.get(0)).optional()?;
            let Some(id) = id else {
                return Ok(());
            };
            ("season_id", id, "tmdb", key)
        }
    };
    let existing:Option<i64>=tx.query_row(&format!("SELECT id FROM metadata_fetches WHERE {column}=?1 AND provider=?2 AND resource_key=?3 AND locale='en-US'"),params![id,provider,key],|r|r.get(0)).optional()?;
    if let Some(id) = existing {
        tx.execute(
            "UPDATE metadata_fetches SET last_attempt_at=?2,last_error_code=?3 WHERE id=?1",
            params![id, time, error_code(error)],
        )?;
    } else {
        tx.execute(&format!("INSERT INTO metadata_fetches({column},provider,resource_key,locale,last_attempt_at,last_error_code) VALUES (?1,?2,?3,'en-US',?4,?5)"),params![id,provider,key,time,error_code(error)])?;
    }
    check_cancellation(cancellation.as_deref())?;
    tx.commit()?;
    Ok(())
}
pub(super) fn search(service: &mut CatalogService, query: &SearchQuery, error: CatalogError) {
    let _ = failed(
        service,
        RequestTarget::Page {
            provider: "cinemeta",
            key: super::normalized_pages::cinemeta_search_key(query),
            number: 1,
        },
        error,
    );
}
pub(super) fn browse(
    service: &mut CatalogService,
    kind: MediaKind,
    offset: u32,
    error: CatalogError,
) {
    let _ = failed(
        service,
        RequestTarget::Page {
            provider: "cinemeta",
            key: super::normalized_pages::cinemeta_browse_key(kind, offset),
            number: 1,
        },
        error,
    );
}
pub(super) fn details(
    service: &mut CatalogService,
    kind: MediaKind,
    id: &ImdbId,
    error: CatalogError,
) {
    let _ = failed(
        service,
        RequestTarget::Title {
            provider: "cinemeta",
            key: super::normalized_pages::cinemeta_detail_key(kind, id),
            kind,
            namespace: "imdb",
            external: id.as_str().into(),
        },
        error,
    );
}
pub(super) fn tmdb(service: &mut CatalogService, query: &TmdbQuery, error: CatalogError) {
    let Ok(key) = crate::tmdb::cache_query_identity(query) else {
        return;
    };
    let target = match query {
        TmdbQuery::Browse { page, .. }
        | TmdbQuery::Search { page, .. }
        | TmdbQuery::Similar { page, .. } => RequestTarget::Page {
            provider: "tmdb",
            key,
            number: *page,
        },
        TmdbQuery::Details { kind, id } => RequestTarget::Title {
            provider: "tmdb",
            key,
            kind: *kind,
            namespace: "tmdb",
            external: id.to_string(),
        },
        TmdbQuery::Season { series_id, season } => RequestTarget::Season {
            key,
            series: *series_id,
            number: *season,
        },
    };
    let _ = failed(service, target, error);
}

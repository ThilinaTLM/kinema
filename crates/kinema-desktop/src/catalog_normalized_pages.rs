//! Ordered provider membership and canonical screen reconstruction.
use super::normalized::*;
use super::*;
use kinema_core::catalog_normalized::{Quality, TitleObservation};
use rusqlite::Transaction;

pub(super) fn cinemeta_search_key(query: &SearchQuery) -> String {
    // JSON encoding preserves delimiters, case and exact query semantics.
    serde_json::json!([
        "cinemeta-v3:catalog-v1",
        "search",
        kind_text(query.kind()),
        query.text(),
        "top",
        "en-US",
        "exact"
    ])
    .to_string()
}
pub(super) fn cinemeta_browse_key(kind: MediaKind, offset: u32) -> String {
    // Cinemeta offsets are not page numbers. Encode the offset explicitly; the
    // saved resource has one local page, numbered one.
    serde_json::json!([
        "cinemeta-v3:catalog-v1",
        "browse",
        kind_text(kind),
        "top",
        "en-US",
        "offset",
        offset
    ])
    .to_string()
}
pub(super) fn cinemeta_detail_key(kind: MediaKind, id: &ImdbId) -> String {
    format!(
        "cinemeta-v3:detail-v1:{}:{}:en-US",
        kind_text(kind),
        id.as_str()
    )
}

#[expect(
    clippy::too_many_arguments,
    reason = "Explicit transaction, resource identity, observation and merge policy context"
)]
pub(super) fn write_page(
    tx: &Transaction<'_>,
    provider: &str,
    key: &str,
    page: u32,
    total: Option<u32>,
    observations: &[TitleObservation],
    time: i64,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    tx.execute("INSERT INTO catalog_pages(provider,query_key,page_number,total_pages,result_count,membership_available,updated_at) VALUES (?1,?2,?3,?4,NULL,0,?5) ON CONFLICT(provider,query_key,page_number) DO NOTHING",params![provider,key,page,total,time])?;
    let id: i64 = tx.query_row(
        "SELECT id FROM catalog_pages WHERE provider=?1 AND query_key=?2 AND page_number=?3",
        params![provider, key, page],
        |row| row.get(0),
    )?;
    let fetch_id = fetch(
        tx,
        Target::Page(id),
        provider,
        key,
        Quality::Summary,
        observations.is_empty(),
        time,
    )?;
    let mut ids = Vec::with_capacity(observations.len());
    let mut seen = HashSet::new();
    for observation in observations {
        let title = resolve_title(tx, observation, time, priority)?;
        if !seen.insert(title) {
            return Err(CatalogError::InvalidMetadata);
        }
        import_title(
            tx,
            title,
            observation,
            provider,
            Quality::Summary,
            fetch_id,
            time,
            priority,
        )?;
        ids.push(title);
    }
    let old: Vec<i64> = {
        let mut stmt = tx.prepare(
            "SELECT title_id FROM catalog_page_items WHERE page_id=?1 ORDER BY position",
        )?;
        stmt.query_map(params![id], |row| row.get(0))?
            .collect::<Result<_, _>>()?
    };
    tx.execute("UPDATE catalog_pages SET updated_at=CASE WHEN result_count IS NOT ?2 OR total_pages IS NOT ?3 OR NOT membership_available OR ?4 THEN ?5 ELSE updated_at END,result_count=?2,total_pages=?3,membership_available=1 WHERE id=?1",params![id,ids.len() as i64,total,old != ids,time])?;
    tx.execute(
        "DELETE FROM catalog_page_items WHERE page_id=?1",
        params![id],
    )?;
    for (position, title) in ids.iter().enumerate() {
        tx.execute(
            "INSERT INTO catalog_page_items(page_id,position,title_id) VALUES (?1,?2,?3)",
            params![id, position as i64, title],
        )?;
    }
    Ok(())
}

pub(super) struct PageRows {
    pub ids: Vec<i64>,
    pub fetched_at: u64,
    pub total: Option<u32>,
}
pub(super) fn read_page(
    tx: &Transaction<'_>,
    provider: &str,
    key: &str,
    page: u32,
) -> Result<Option<PageRows>, CatalogError> {
    type SavedPage = (i64, Option<i64>, bool, Option<u32>, Option<i64>);
    let row: Option<SavedPage> = tx.query_row("SELECT p.id,p.result_count,p.membership_available,p.total_pages,f.fetched_at FROM catalog_pages p LEFT JOIN metadata_fetches f ON f.page_id=p.id AND f.provider=p.provider AND f.resource_key=p.query_key AND f.locale='en-US' WHERE p.provider=?1 AND p.query_key=?2 AND p.page_number=?3",params![provider,key,page],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let Some((id, count, available, total, time)) = row else {
        return Ok(None);
    };
    if !available {
        return Ok(None);
    }
    let count = count
        .and_then(|n| usize::try_from(n).ok())
        .ok_or(CatalogError::InvalidCache)?;
    let fetched_at = time
        .and_then(|n| u64::try_from(n).ok())
        .ok_or(CatalogError::InvalidCache)?;
    let mut stmt = tx.prepare(
        "SELECT position,title_id FROM catalog_page_items WHERE page_id=?1 ORDER BY position",
    )?;
    let mut rows = stmt.query(params![id])?;
    let mut ids = Vec::new();
    let mut seen = HashSet::new();
    while let Some(row) = rows.next()? {
        let position: i64 = row.get(0)?;
        let title: i64 = row.get(1)?;
        if position != ids.len() as i64 || !seen.insert(title) {
            return Err(CatalogError::InvalidCache);
        }
        ids.push(title);
    }
    if ids.len() != count {
        return Err(CatalogError::InvalidCache);
    }
    Ok(Some(PageRows {
        ids,
        fetched_at,
        total,
    }))
}

pub(super) fn imdb_title(
    tx: &Transaction<'_>,
    id: i64,
    kind: MediaKind,
) -> Result<CatalogTitle, CatalogError> {
    let record = CatalogService::title_record(tx, id)?;
    if record.kind != kind {
        return Err(CatalogError::InvalidCache);
    }
    CatalogService::display_title(&record)
}
pub(super) fn read_cinemeta_page(
    service: &mut CatalogService,
    kind: MediaKind,
    key: &str,
) -> Result<Option<CachedSearch>, CatalogError> {
    let tx = service.connection.transaction()?;
    let Some(page) = read_page(&tx, "cinemeta", key, 1)? else {
        return Ok(None);
    };
    let titles = page
        .ids
        .into_iter()
        .map(|id| imdb_title(&tx, id, kind))
        .collect::<Result<_, _>>()?;
    Ok(Some(CachedSearch {
        titles,
        fetched_at: page.fetched_at,
    }))
}
pub(super) fn detail_time(
    tx: &Transaction<'_>,
    target: Target,
    provider: &str,
    resource: &str,
) -> Result<Option<u64>, CatalogError> {
    // Target's concrete column is deliberately selected here, not caller SQL.
    let (column, id) = match target {
        Target::Title(id) => ("title_id", id),
        Target::Season(id) => ("season_id", id),
        _ => return Err(CatalogError::InvalidCache),
    };
    let time: Option<Option<i64>> = tx.query_row(&format!("SELECT fetched_at FROM metadata_fetches WHERE {column}=?1 AND provider=?2 AND resource_key=?3 AND locale='en-US' AND completeness='detail'"),params![id,provider,resource],|row| row.get(0)).optional()?;
    time.flatten()
        .map(|t| u64::try_from(t).map_err(|_| CatalogError::InvalidCache))
        .transpose()
}
pub(super) fn read_cinemeta_details(
    service: &mut CatalogService,
    kind: MediaKind,
    external: &ImdbId,
) -> Result<Option<CachedDetails>, CatalogError> {
    let tx = service.connection.transaction()?;
    let Some(id) = title_id(&tx, kind, "imdb", external.as_str())? else {
        return Ok(None);
    };
    let Some(fetched_at) = detail_time(
        &tx,
        Target::Title(id),
        "cinemeta",
        &cinemeta_detail_key(kind, external),
    )?
    else {
        return Ok(None);
    };
    let title = imdb_title(&tx, id, kind)?;
    let (poster_url,background_url,release_info,genres): (Option<String>,Option<String>,Option<String>,Option<String>) = tx.query_row("SELECT poster_url,backdrop_url,CASE WHEN kind='movie' THEN COALESCE(release_date,CAST(release_year AS TEXT)) ELSE COALESCE(first_air_date,CAST(first_air_year AS TEXT)) END,genres_json FROM titles WHERE id=?1",params![id],|row|Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?)))?;
    let genres = genres
        .map(|text| serde_json::from_str(&text).map_err(|_| CatalogError::InvalidCache))
        .transpose()?
        .unwrap_or_default();
    let episodes = read_episodes(&tx, id, None)?;
    Ok(Some(CachedDetails {
        details: TitleDetails {
            facts: read_facts(&tx, id, "cinemeta", &cinemeta_detail_key(kind, external))?,
            title,
            poster_url,
            background_url,
            release_info,
            genres,
            episodes,
        },
        fetched_at,
    }))
}

pub(super) fn write_cinemeta_details(
    tx: &Transaction<'_>,
    details: &TitleDetails,
    time: i64,
    complete: bool,
    priority: &kinema_core::provider::ProviderPriority,
) -> Result<(), CatalogError> {
    let mut observation = imdb_observation(&details.title);
    for (name, value) in [
        ("poster_url", details.poster_url.as_ref()),
        ("backdrop_url", details.background_url.as_ref()),
    ] {
        if let Some(value) = value {
            observation.fields.push((
                name.into(),
                kinema_core::catalog_normalized::FieldValue::Text(value.clone()),
            ));
        }
    }
    if !details.genres.is_empty() {
        observation.fields.push((
            "genres_json".into(),
            kinema_core::catalog_normalized::FieldValue::Labels(details.genres.clone()),
        ));
    }
    if let Some(release) = &details.release_info {
        let prefix = if details.title.kind() == MediaKind::Movie {
            "release"
        } else {
            "first_air"
        };
        if kinema_core::catalog_normalized::valid_date(release) {
            observation.fields.push((
                format!("{prefix}_date"),
                kinema_core::catalog_normalized::FieldValue::Text(release.clone()),
            ));
        }
        let years = cinemeta_years(release);
        if let Some(year) = years.map(|(first, _)| first) {
            observation.fields.push((
                format!("{prefix}_year"),
                kinema_core::catalog_normalized::FieldValue::Integer(year),
            ));
        }
    }
    if details.title.kind() == MediaKind::Series
        && let Some(last) = details
            .release_info
            .as_deref()
            .and_then(cinemeta_years)
            .and_then(|(_, last)| last)
    {
        observation.fields.push((
            "last_air_year".into(),
            kinema_core::catalog_normalized::FieldValue::Integer(last),
        ));
    }
    let id = resolve_title(tx, &observation, time, priority)?;
    // Old reduced snapshots are useful evidence, not proof richer optional DTO
    // fields were checked. Backfill therefore records summary completeness,
    // independently of the detail origin of its supplied, known fields.
    let completeness = if complete {
        Quality::Detail
    } else {
        Quality::Summary
    };
    let fetch_id = fetch(
        tx,
        Target::Title(id),
        "cinemeta",
        &cinemeta_detail_key(details.title.kind(), details.title.id()),
        completeness,
        false,
        time,
    )?;
    import_title(
        tx,
        id,
        &observation,
        "cinemeta",
        Quality::Detail,
        fetch_id,
        time,
        priority,
    )?;
    write_facts(tx, fetch_id, &details.facts)?;
    import_episodes(
        tx,
        id,
        &details.episodes,
        "cinemeta",
        fetch_id,
        time,
        priority,
    )?;
    Ok(())
}

pub(super) fn cinemeta_years(text: &str) -> Option<(i64, Option<i64>)> {
    fn year(text: &str) -> Option<i64> {
        (text.len() == 4 && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<i64>().ok())
            .flatten()
            .filter(|year| (1..=9999).contains(year))
    }
    if let Some(first) = year(text) {
        return Some((first, None));
    }
    if kinema_core::catalog_normalized::valid_date(text) {
        return Some((year(&text[..4])?, None));
    }
    let (first, last) = text.split_once('–').or_else(|| text.split_once('-'))?;
    let first = year(first.trim())?;
    let last = if last.trim().is_empty() {
        None
    } else {
        Some(year(last.trim())?)
    };
    if last.is_some_and(|last| last < first) {
        return None;
    }
    Some((first, last))
}

pub(super) fn write_facts(
    tx: &Transaction<'_>,
    fetch_id: i64,
    facts: &kinema_core::catalog::TitleFacts,
) -> Result<(), CatalogError> {
    if !facts.is_valid() {
        return Err(CatalogError::InvalidMetadata);
    }
    let payload = serde_json::to_string(&crate::tmdb::FactsPayload::from(facts))
        .map_err(|_| CatalogError::InvalidMetadata)?;
    if payload.len() > 64000 {
        return Err(CatalogError::InvalidMetadata);
    }
    tx.execute(
        "UPDATE metadata_fetches SET facts_json=?2 WHERE id=?1",
        params![fetch_id, payload],
    )?;
    Ok(())
}
pub(super) fn read_facts(
    tx: &Transaction<'_>,
    title: i64,
    provider: &str,
    resource: &str,
) -> Result<kinema_core::catalog::TitleFacts, CatalogError> {
    let payload: Option<String> = tx.query_row("SELECT facts_json FROM metadata_fetches WHERE title_id=?1 AND provider=?2 AND resource_key=?3 AND locale='en-US'", params![title,provider,resource], |r| r.get(0))?;
    let Some(payload) = payload else {
        return Ok(Default::default());
    };
    if payload.len() > 64000 {
        return Err(CatalogError::InvalidCache);
    }
    serde_json::from_str::<crate::tmdb::FactsPayload>(&payload)
        .map_err(|_| CatalogError::InvalidCache)?
        .normalize()
        .map_err(|_| CatalogError::InvalidCache)
}

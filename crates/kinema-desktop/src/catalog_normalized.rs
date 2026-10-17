//! Canonical catalog transaction primitives. Provider caches are legacy evidence,
//! never the source for current screens after migration.
use super::*;
use kinema_core::catalog_normalized::{
    ExternalIdentifier, FieldValue, Quality, RatingObservation, TitleObservation, accepts_field,
    valid_date,
};
use rusqlite::Transaction;

pub(super) const TITLE_FIELDS: &[&str] = &[
    "name",
    "original_name",
    "tagline",
    "overview",
    "release_date",
    "release_year",
    "runtime_minutes",
    "genres_json",
    "poster_url",
    "backdrop_url",
    "logo_url",
    "original_language",
    "status",
    "first_air_date",
    "first_air_year",
    "last_air_date",
    "last_air_year",
    "season_count",
    "episode_count",
];
pub(super) const SEASON_FIELDS: &[&str] = &[
    "name",
    "overview",
    "air_date",
    "air_year",
    "poster_url",
    "episode_count",
];
pub(super) const EPISODE_FIELDS: &[&str] = &[
    "name",
    "overview",
    "air_date",
    "runtime_minutes",
    "production_code",
    "still_url",
];

#[derive(Clone, Copy)]
pub(super) enum Target {
    Title(i64),
    Season(i64),
    Episode(i64),
    Page(i64),
}
impl Target {
    fn column(self) -> &'static str {
        match self {
            Self::Title(_) => "title_id",
            Self::Season(_) => "season_id",
            Self::Episode(_) => "episode_id",
            Self::Page(_) => "page_id",
        }
    }
    fn table(self) -> &'static str {
        match self {
            Self::Title(_) => "titles",
            Self::Season(_) => "seasons",
            Self::Episode(_) => "episodes",
            Self::Page(_) => "catalog_pages",
        }
    }
    fn id(self) -> i64 {
        match self {
            Self::Title(id) | Self::Season(id) | Self::Episode(id) | Self::Page(id) => id,
        }
    }
}

pub(super) fn fetch(
    tx: &Transaction<'_>,
    target: Target,
    provider: &str,
    resource: &str,
    quality: Quality,
    empty: bool,
    time: i64,
) -> Result<i64, CatalogError> {
    let sql = format!(
        "SELECT id FROM metadata_fetches WHERE {}=?1 AND provider=?2 AND resource_key=?3 AND locale='en-US'",
        target.column()
    );
    let id: Option<i64> = tx
        .query_row(&sql, params![target.id(), provider, resource], |row| {
            row.get(0)
        })
        .optional()?;
    let success = if empty { "empty" } else { "data" };
    if let Some(id) = id {
        tx.execute("UPDATE metadata_fetches SET fetched_at=?2,success_kind=?3,completeness=?4,last_attempt_at=?2,last_error_code=NULL WHERE id=?1", params![id,time,success,quality.as_str()])?;
        Ok(id)
    } else {
        tx.execute(&format!("INSERT INTO metadata_fetches({},provider,resource_key,locale,fetched_at,success_kind,completeness,last_attempt_at) VALUES (?1,?2,?3,'en-US',?4,?5,?6,?4)", target.column()), params![target.id(),provider,resource,time,success,quality.as_str()])?;
        Ok(tx.last_insert_rowid())
    }
}

pub(super) fn resolve_title(
    tx: &Transaction<'_>,
    observation: &TitleObservation,
    time: i64,
) -> Result<i64, CatalogError> {
    if observation.identifiers.is_empty() {
        return Err(CatalogError::InvalidMetadata);
    }
    let mut owners = Vec::new();
    let mut namespaces = HashSet::new();
    for identifier in &observation.identifiers {
        if !identifier.valid() || !namespaces.insert(&identifier.namespace) {
            return Err(CatalogError::InvalidMetadata);
        }
        if identifier.namespace == "imdb" {
            let conflict: bool = tx.query_row("SELECT EXISTS(SELECT 1 FROM episode_identifiers WHERE namespace='imdb' AND external_id=?1) OR EXISTS(SELECT 1 FROM title_identifiers WHERE namespace='imdb' AND external_id=?1 AND media_kind<>?2)", params![identifier.external_id,kind_text(observation.kind)], |row| row.get(0))?;
            if conflict {
                return Err(CatalogError::IdentityConflict);
            }
        }
        let found: Option<i64> = tx.query_row("SELECT title_id FROM title_identifiers WHERE namespace=?1 AND media_kind=?2 AND external_id=?3", params![identifier.namespace,kind_text(observation.kind),identifier.external_id], |row| row.get(0)).optional()?;
        if let Some(found) = found
            && !owners.contains(&found)
        {
            owners.push(found);
        }
    }
    if owners.len() > 1 {
        if !observation
            .identifiers
            .iter()
            .any(|id| id.namespace == "imdb")
            || !observation
                .identifiers
                .iter()
                .any(|id| id.namespace == "tmdb")
        {
            return Err(CatalogError::IdentityConflict);
        }
        return super::normalized_merge::reconcile(tx, owners[0], owners[1], time);
    }
    if let Some(id) = owners.first() {
        return Ok(*id);
    }
    tx.execute(
        "INSERT INTO titles(kind,locale,updated_at) VALUES (?1,'en-US',?2)",
        params![kind_text(observation.kind), time],
    )?;
    Ok(tx.last_insert_rowid())
}

pub(super) fn identifier(
    tx: &Transaction<'_>,
    target: Target,
    kind: Option<MediaKind>,
    identifier: &ExternalIdentifier,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    if !identifier.valid() {
        return Err(CatalogError::InvalidMetadata);
    }
    let supplier: String = tx.query_row(
        "SELECT provider FROM metadata_fetches WHERE id=?1",
        params![fetch_id],
        |row| row.get(0),
    )?;
    validate_supplier(tx, target, &supplier, fetch_id)?;
    let (table, column) = match target {
        Target::Title(_) => ("title_identifiers", "title_id"),
        Target::Season(_) => ("season_identifiers", "season_id"),
        Target::Episode(_) => ("episode_identifiers", "episode_id"),
        Target::Page(_) => return Err(CatalogError::InvalidMetadata),
    };
    if identifier.namespace == "imdb" {
        if matches!(target, Target::Season(_)) {
            return Err(CatalogError::InvalidMetadata);
        }
        let other = if matches!(target, Target::Title(_)) {
            "episode_identifiers"
        } else {
            "title_identifiers"
        };
        let exists: bool = tx.query_row(
            &format!(
                "SELECT EXISTS(SELECT 1 FROM {other} WHERE namespace='imdb' AND external_id=?1)"
            ),
            params![identifier.external_id],
            |row| row.get(0),
        )?;
        if exists {
            return Err(CatalogError::IdentityConflict);
        }
    }
    let scope = if matches!(target, Target::Title(_)) {
        " AND media_kind=?3"
    } else {
        ""
    };
    let sql = format!("SELECT {column} FROM {table} WHERE namespace=?1 AND external_id=?2{scope}");
    let found: Option<i64> = if let Some(kind) = kind {
        tx.query_row(
            &sql,
            params![
                identifier.namespace,
                identifier.external_id,
                kind_text(kind)
            ],
            |row| row.get(0),
        )
        .optional()?
    } else {
        tx.query_row(
            &sql,
            params![identifier.namespace, identifier.external_id],
            |row| row.get(0),
        )
        .optional()?
    };
    if found.is_some_and(|id| id != target.id()) {
        return Err(CatalogError::IdentityConflict);
    }
    let existing: Option<String> = tx
        .query_row(
            &format!("SELECT external_id FROM {table} WHERE {column}=?1 AND namespace=?2"),
            params![target.id(), identifier.namespace],
            |row| row.get(0),
        )
        .optional()?;
    if existing
        .as_deref()
        .is_some_and(|id| id != identifier.external_id)
    {
        return Err(CatalogError::IdentityConflict);
    }
    if found.is_some() {
        tx.execute(&format!("UPDATE {table} SET fetch_state_id=?1,observed_at=?2 WHERE {column}=?3 AND namespace=?4"), params![fetch_id,time,target.id(),identifier.namespace])?;
    } else if let Some(kind) = kind {
        tx.execute("INSERT INTO title_identifiers(namespace,media_kind,external_id,title_id,fetch_state_id,observed_at) VALUES (?1,?2,?3,?4,?5,?6)",params![identifier.namespace,kind_text(kind),identifier.external_id,target.id(),fetch_id,time])?;
    } else {
        tx.execute(&format!("INSERT INTO {table}(namespace,external_id,{column},fetch_state_id,observed_at) VALUES (?1,?2,?3,?4,?5)"),params![identifier.namespace,identifier.external_id,target.id(),fetch_id,time])?;
    }
    Ok(())
}

pub(super) fn field(
    tx: &Transaction<'_>,
    target: Target,
    field_value: (&str, &FieldValue),
    provider: &str,
    quality: Quality,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    let (name, value) = field_value;
    let allowed = match target {
        Target::Title(_) => TITLE_FIELDS,
        Target::Season(_) => SEASON_FIELDS,
        Target::Episode(_) => EPISODE_FIELDS,
        Target::Page(_) => return Err(CatalogError::InvalidMetadata),
    };
    if !allowed.contains(&name) {
        return Err(CatalogError::InvalidMetadata);
    }
    let numeric = name.ends_with("_year") || name.ends_with("_count") || name == "runtime_minutes";
    if !matches!(
        (numeric, name == "genres_json", value),
        (true, false, FieldValue::Integer(_))
            | (false, true, FieldValue::Labels(_))
            | (false, false, FieldValue::Text(_))
    ) {
        return Err(CatalogError::InvalidMetadata);
    }
    validate_supplier(tx, target, provider, fetch_id)?;
    let value = match value {
        FieldValue::Text(text) => {
            if text.trim().is_empty() {
                return Ok(());
            }
            if text.contains('\0')
                || text.len() > 80_000
                || (name.ends_with("_date") && !valid_date(text))
            {
                return Err(CatalogError::InvalidMetadata);
            }
            if name.ends_with("_url") {
                super::metadata::validate_image(Some(text.clone()))?;
            }
            rusqlite::types::Value::Text(text.clone())
        }
        FieldValue::Integer(number) => {
            if *number < 0 || (name == "runtime_minutes" && *number == 0) {
                return Err(CatalogError::InvalidMetadata);
            }
            rusqlite::types::Value::Integer(*number)
        }
        FieldValue::Labels(labels) => {
            if name != "genres_json"
                || labels.len() > 64
                || labels.iter().any(|s| s.trim().is_empty() || s.len() > 400)
            {
                return Err(CatalogError::InvalidMetadata);
            }
            let mut seen = HashSet::new();
            let labels: Vec<_> = labels.iter().filter(|s| seen.insert(s.as_str())).collect();
            rusqlite::types::Value::Text(
                serde_json::to_string(&labels).map_err(|_| CatalogError::InvalidMetadata)?,
            )
        }
    };
    let current: Option<(String, i64)> = tx
        .query_row(
            &format!(
                "SELECT quality,observed_at FROM field_provenance WHERE {}=?1 AND field_name=?2",
                target.column()
            ),
            params![target.id(), name],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    let current = current.map(|(q, t)| {
        (
            if q == "detail" {
                Quality::Detail
            } else {
                Quality::Summary
            },
            t,
        )
    });
    if !accepts_field(current, quality, time) {
        return Ok(());
    }
    tx.execute(&format!("UPDATE {} SET {name}=?1,updated_at=CASE WHEN {name} IS NOT ?1 THEN ?2 ELSE updated_at END WHERE id=?3",target.table()),params![value,time,target.id()])?;
    tx.execute(
        &format!(
            "DELETE FROM field_provenance WHERE {}=?1 AND field_name=?2",
            target.column()
        ),
        params![target.id(), name],
    )?;
    tx.execute(&format!("INSERT INTO field_provenance({},field_name,supplied_by,quality,fetch_state_id,observed_at) VALUES (?1,?2,?3,?4,?5,?6)",target.column()),params![target.id(),name,provider,quality.as_str(),fetch_id,time])?;
    Ok(())
}

pub(super) fn rating(
    tx: &Transaction<'_>,
    target: Target,
    rating: &RatingObservation,
    provider: &str,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    if !rating.valid() || rating.supplied_by != provider || matches!(target, Target::Page(_)) {
        return Err(CatalogError::InvalidMetadata);
    }
    validate_supplier(tx, target, provider, fetch_id)?;
    type SavedRating = (i64, Option<f64>, Option<i64>, Option<i64>, i64);
    let old: Option<SavedRating> = tx.query_row(&format!("SELECT id,value,vote_count,source_updated_at,observed_at FROM ratings WHERE {}=?1 AND rating_source=?2 AND metric=?3 AND supplied_by=?4",target.column()),params![target.id(),rating.source,rating.metric,provider],|row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?))).optional()?;
    let votes = rating.vote_count.map(|v| v as i64);
    if let Some((id, value, count, upstream, observed)) = old {
        if time < observed {
            return Ok(());
        }
        let changed =
            value != rating.value || count != votes || upstream != rating.source_updated_at;
        tx.execute("UPDATE ratings SET value=?2,vote_count=?3,source_updated_at=?4,fetch_state_id=?5,observed_at=?6,updated_at=CASE WHEN ?7 THEN ?6 ELSE updated_at END WHERE id=?1",params![id,rating.value,votes,rating.source_updated_at,fetch_id,time,changed])?;
    } else {
        tx.execute(&format!("INSERT INTO ratings({},rating_source,metric,supplied_by,value,vote_count,source_updated_at,fetch_state_id,observed_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?9)",target.column()),params![target.id(),rating.source,rating.metric,provider,rating.value,votes,rating.source_updated_at,fetch_id,time])?;
    }
    Ok(())
}

pub(super) fn import_title(
    tx: &Transaction<'_>,
    id: i64,
    observation: &TitleObservation,
    provider: &str,
    quality: Quality,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    for ident in &observation.identifiers {
        identifier(
            tx,
            Target::Title(id),
            Some(observation.kind),
            ident,
            fetch_id,
            time,
        )?;
    }
    super::normalized_pairs::fields(
        tx,
        Target::Title(id),
        &observation.fields,
        provider,
        quality,
        fetch_id,
        time,
    )?;
    for observation in &observation.ratings {
        rating(tx, Target::Title(id), observation, provider, fetch_id, time)?;
    }
    Ok(())
}

pub(super) fn imdb_observation(title: &CatalogTitle) -> TitleObservation {
    let mut fields = vec![("name".into(), FieldValue::Text(title.name().into()))];
    if let Some(overview) = title.overview() {
        fields.push(("overview".into(), FieldValue::Text(overview.into())));
    }
    TitleObservation {
        kind: title.kind(),
        identifiers: vec![ExternalIdentifier {
            namespace: "imdb".into(),
            external_id: title.id().as_str().into(),
        }],
        fields,
        ratings: vec![],
    }
}

pub(super) fn season(
    tx: &Transaction<'_>,
    series: i64,
    number: u32,
    time: i64,
) -> Result<i64, CatalogError> {
    tx.execute("INSERT INTO seasons(series_id,series_kind,season_number,locale,updated_at) VALUES (?1,'series',?2,'en-US',?3) ON CONFLICT(series_id,season_number) DO NOTHING",params![series,number,time])?;
    Ok(tx.query_row(
        "SELECT id FROM seasons WHERE series_id=?1 AND season_number=?2",
        params![series, number],
        |row| row.get(0),
    )?)
}
pub(super) fn episode(
    tx: &Transaction<'_>,
    season: i64,
    number: u32,
    time: i64,
) -> Result<i64, CatalogError> {
    if number == 0 {
        return Err(CatalogError::InvalidMetadata);
    }
    tx.execute("INSERT INTO episodes(season_id,episode_number,locale,updated_at) VALUES (?1,?2,'en-US',?3) ON CONFLICT(season_id,episode_number) DO NOTHING",params![season,number,time])?;
    Ok(tx.query_row(
        "SELECT id FROM episodes WHERE season_id=?1 AND episode_number=?2",
        params![season, number],
        |row| row.get(0),
    )?)
}

pub(super) fn import_episodes(
    tx: &Transaction<'_>,
    series: i64,
    episodes: &[CatalogEpisode],
    provider: &str,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    for item in episodes {
        let season_id = season(tx, series, item.season, time)?;
        let id = episode(tx, season_id, item.number, time)?;
        if let Some(runtime) = item.runtime_minutes {
            field(
                tx,
                Target::Episode(id),
                ("runtime_minutes", &FieldValue::Integer(i64::from(runtime))),
                provider,
                Quality::Detail,
                fetch_id,
                time,
            )?;
        }

        for (name, value) in [
            ("still_url", item.still_url.as_deref()),
            ("name", item.name.as_deref()),
            ("overview", item.overview.as_deref()),
            (
                "air_date",
                item.released
                    .as_deref()
                    .and_then(|s| s.get(..10))
                    .filter(|s| valid_date(s)),
            ),
        ] {
            if let Some(value) = value {
                field(
                    tx,
                    Target::Episode(id),
                    (name, &FieldValue::Text(value.into())),
                    provider,
                    Quality::Detail,
                    fetch_id,
                    time,
                )?;
            }
        }
    }
    Ok(())
}

pub(super) fn read_episodes(
    tx: &Transaction<'_>,
    series: i64,
    only_season: Option<u32>,
) -> Result<Vec<CatalogEpisode>, CatalogError> {
    Ok(CatalogService::episode_records(tx, series, only_season)?
        .into_iter()
        .map(|(season, record)| CatalogEpisode {
            still_url: record.text("still_url").map(str::to_owned),
            runtime_minutes: record
                .integer("runtime_minutes")
                .and_then(|n| u32::try_from(n).ok()),
            season,
            number: record.number,
            name: record.text("name").map(str::to_owned),
            overview: record.text("overview").map(str::to_owned),
            released: record.text("air_date").map(str::to_owned),
        })
        .collect())
}

pub(super) fn title_id(
    tx: &Transaction<'_>,
    kind: MediaKind,
    namespace: &str,
    external: &str,
) -> Result<Option<i64>, CatalogError> {
    Ok(tx.query_row("SELECT title_id FROM title_identifiers WHERE namespace=?1 AND media_kind=?2 AND external_id=?3",params![namespace,kind_text(kind),external],|row| row.get(0)).optional()?)
}

fn validate_supplier(
    tx: &Transaction<'_>,
    target: Target,
    provider: &str,
    fetch_id: i64,
) -> Result<(), CatalogError> {
    let (title, season, episode, page, supplier): (
        Option<i64>,
        Option<i64>,
        Option<i64>,
        Option<i64>,
        String,
    ) = tx.query_row(
        "SELECT title_id,season_id,episode_id,page_id,provider FROM metadata_fetches WHERE id=?1",
        params![fetch_id],
        |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        },
    )?;
    if supplier != provider {
        return Err(CatalogError::InvalidMetadata);
    }
    let valid=match target {
        Target::Title(id)=>title==Some(id) || if let Some(season)=season {tx.query_row("SELECT series_id=?1 FROM seasons WHERE id=?2",params![id,season],|row|row.get(0))?}else{false} || if let Some(page)=page {tx.query_row("SELECT provider=?1 AND query_key=(SELECT resource_key FROM metadata_fetches WHERE id=?2) FROM catalog_pages WHERE id=?3",params![provider,fetch_id,page],|row|row.get(0))?}else{false},
        Target::Season(id)=>season==Some(id) || if let Some(title)=title {tx.query_row("SELECT series_id=?1 FROM seasons WHERE id=?2",params![title,id],|row|row.get(0))?}else{false},
        Target::Episode(id)=>episode==Some(id) || tx.query_row("SELECT (?1 IS NOT NULL AND s.series_id=?1) OR (?2 IS NOT NULL AND s.id=?2) FROM episodes e JOIN seasons s ON s.id=e.season_id WHERE e.id=?3",params![title,season,id],|row|row.get(0))?,
        Target::Page(_)=>false,
    };
    if valid {
        Ok(())
    } else {
        Err(CatalogError::InvalidMetadata)
    }
}

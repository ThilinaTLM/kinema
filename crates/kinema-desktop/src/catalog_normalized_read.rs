//! Rich domain read APIs shared by the existing UI projections.
use super::normalized::{EPISODE_FIELDS, SEASON_FIELDS, TITLE_FIELDS, Target};
use super::*;
use kinema_core::catalog_normalized::{
    CanonicalChild, CanonicalTitle, ChildKind, ExternalIdentifier, FieldOrigin, FieldValue,
    Quality, RatingObservation, RetainedRating, valid_date,
};
use rusqlite::{Transaction, types::ValueRef};
use std::collections::BTreeMap;

struct Parts {
    fields: BTreeMap<String, FieldValue>,
    identifiers: Vec<ExternalIdentifier>,
    provenance: Vec<FieldOrigin>,
    ratings: Vec<RetainedRating>,
}
fn parts(tx: &Transaction<'_>, target: Target) -> Result<Parts, CatalogError> {
    let (table, column, identifiers, id, allowed) = match target {
        Target::Title(id) => ("titles", "title_id", "title_identifiers", id, TITLE_FIELDS),
        Target::Season(id) => (
            "seasons",
            "season_id",
            "season_identifiers",
            id,
            SEASON_FIELDS,
        ),
        Target::Episode(id) => (
            "episodes",
            "episode_id",
            "episode_identifiers",
            id,
            EPISODE_FIELDS,
        ),
        Target::Page(_) => return Err(CatalogError::InvalidCache),
    };
    let mut stmt = tx.prepare(&format!(
        "SELECT {} FROM {table} WHERE id=?1",
        allowed.join(",")
    ))?;
    let mut rows = stmt.query(params![id])?;
    let row = rows.next()?.ok_or(CatalogError::InvalidCache)?;
    let mut fields = BTreeMap::new();
    for (index, &name) in allowed.iter().enumerate() {
        let numeric =
            name.ends_with("_year") || name.ends_with("_count") || name == "runtime_minutes";
        let value = match row.get_ref(index)? {
            ValueRef::Null => continue,
            ValueRef::Integer(value) if numeric => {
                if value < 0
                    || (name == "runtime_minutes" && value == 0)
                    || (name.ends_with("_year") && !(1..=9999).contains(&value))
                {
                    return Err(CatalogError::InvalidCache);
                }
                FieldValue::Integer(value)
            }
            ValueRef::Text(bytes) if !numeric => {
                let text = std::str::from_utf8(bytes).map_err(|_| CatalogError::InvalidCache)?;
                if text.trim().is_empty()
                    || text.len() > 80_000
                    || text.contains('\0')
                    || (name.ends_with("_date") && !valid_date(text))
                {
                    return Err(CatalogError::InvalidCache);
                }
                if name.ends_with("_url") {
                    super::metadata::validate_image(Some(text.into()))
                        .map_err(|_| CatalogError::InvalidCache)?;
                }
                if name == "genres_json" {
                    let labels: Vec<String> =
                        serde_json::from_str(text).map_err(|_| CatalogError::InvalidCache)?;
                    let mut seen = HashSet::new();
                    if labels.len() > 64
                        || labels
                            .iter()
                            .any(|s| s.trim().is_empty() || s.len() > 400 || !seen.insert(s))
                    {
                        return Err(CatalogError::InvalidCache);
                    }
                    FieldValue::Labels(labels)
                } else {
                    FieldValue::Text(text.into())
                }
            }
            _ => return Err(CatalogError::InvalidCache),
        };
        fields.insert(name.into(), value);
    }
    let mut stmt = tx.prepare(&format!(
        "SELECT namespace,external_id FROM {identifiers} WHERE {column}=?1 ORDER BY namespace"
    ))?;
    let identifiers = stmt
        .query_map(params![id], |r| {
            Ok(ExternalIdentifier {
                namespace: r.get(0)?,
                external_id: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if identifiers.iter().any(|id| !id.valid()) {
        return Err(CatalogError::InvalidCache);
    }
    let mut stmt=tx.prepare(&format!("SELECT field_name,supplied_by,quality,fetch_state_id,observed_at FROM field_provenance WHERE {column}=?1 ORDER BY field_name"))?;
    let provenance = stmt
        .query_map(params![id], |r| {
            Ok(FieldOrigin {
                field_name: r.get(0)?,
                supplied_by: r.get(1)?,
                quality: if r.get::<_, String>(2)? == "detail" {
                    Quality::Detail
                } else {
                    Quality::Summary
                },
                fetch_state_id: r.get(3)?,
                observed_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if provenance.len() != fields.len()
        || provenance
            .iter()
            .any(|origin| !fields.contains_key(&origin.field_name))
    {
        return Err(CatalogError::InvalidCache);
    }
    let mut stmt=tx.prepare(&format!("SELECT rating_source,metric,supplied_by,value,vote_count,source_updated_at,fetch_state_id,observed_at,updated_at FROM ratings WHERE {column}=?1 ORDER BY rating_source,metric,supplied_by"))?;
    let ratings = stmt
        .query_map(params![id], |r| {
            Ok(RetainedRating {
                observation: RatingObservation {
                    source: r.get(0)?,
                    metric: r.get(1)?,
                    supplied_by: r.get(2)?,
                    value: r.get(3)?,
                    vote_count: r
                        .get::<_, Option<i64>>(4)?
                        .map(u64::try_from)
                        .transpose()
                        .map_err(|_| rusqlite::Error::IntegralValueOutOfRange(4, -1))?,
                    source_updated_at: r.get(5)?,
                },
                fetch_state_id: r.get(6)?,
                observed_at: r.get(7)?,
                updated_at: r.get(8)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if ratings.iter().any(|rating| !rating.observation.valid()) {
        return Err(CatalogError::InvalidCache);
    }
    Ok(Parts {
        fields,
        identifiers,
        provenance,
        ratings,
    })
}
impl CatalogService {
    pub(crate) fn title_record(
        tx: &Transaction<'_>,
        id: i64,
    ) -> Result<CanonicalTitle, CatalogError> {
        let (kind, updated_at): (String, i64) = tx.query_row(
            "SELECT kind,updated_at FROM titles WHERE id=?1",
            params![id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let kind = match kind.as_str() {
            "movie" => MediaKind::Movie,
            "series" => MediaKind::Series,
            _ => return Err(CatalogError::InvalidCache),
        };
        let Parts {
            fields,
            identifiers,
            provenance,
            ratings,
        } = parts(tx, Target::Title(id))?;
        Ok(CanonicalTitle {
            id,
            kind,
            identifiers,
            fields,
            provenance,
            ratings,
            updated_at,
        })
    }
    pub(crate) fn season_records(
        tx: &Transaction<'_>,
        series: i64,
    ) -> Result<Vec<CanonicalChild>, CatalogError> {
        let mut stmt=tx.prepare("SELECT id,season_number,updated_at FROM seasons WHERE series_id=?1 ORDER BY season_number")?;
        let rows = stmt
            .query_map(params![series], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, u32>(1)?,
                    r.get::<_, i64>(2)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, number, updated_at)| {
                let Parts {
                    fields,
                    identifiers,
                    provenance,
                    ratings,
                } = parts(tx, Target::Season(id))?;
                Ok(CanonicalChild {
                    id,
                    parent_id: series,
                    number,
                    kind: ChildKind::Season,
                    fields,
                    identifiers,
                    provenance,
                    ratings,
                    updated_at,
                })
            })
            .collect()
    }
    pub(crate) fn episode_records(
        tx: &Transaction<'_>,
        series: i64,
        only_season: Option<u32>,
    ) -> Result<Vec<(u32, CanonicalChild)>, CatalogError> {
        let mut stmt=tx.prepare("SELECT e.id,e.season_id,e.episode_number,e.updated_at,s.season_number FROM episodes e JOIN seasons s ON s.id=e.season_id WHERE s.series_id=?1 AND (?2 IS NULL OR s.season_number=?2) ORDER BY s.season_number,e.episode_number")?;
        let rows = stmt
            .query_map(params![series, only_season], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, u32>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, u32>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        rows.into_iter()
            .map(|(id, parent_id, number, updated_at, season)| {
                let Parts {
                    fields,
                    identifiers,
                    provenance,
                    ratings,
                } = parts(tx, Target::Episode(id))?;
                Ok((
                    season,
                    CanonicalChild {
                        id,
                        parent_id,
                        number,
                        kind: ChildKind::Episode,
                        fields,
                        identifiers,
                        provenance,
                        ratings,
                        updated_at,
                    },
                ))
            })
            .collect()
    }
}

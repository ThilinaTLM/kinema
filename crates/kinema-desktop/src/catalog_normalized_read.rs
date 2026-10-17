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
    /// Project retained facts only. Rating source and metric, not supplier, define IMDb.
    pub(crate) fn display_title(record: &CanonicalTitle) -> Result<CatalogTitle, CatalogError> {
        let id = record
            .identifiers
            .iter()
            .find(|id| id.namespace == "imdb")
            .ok_or(CatalogError::InvalidCache)?;
        let title = validate_title(
            id.external_id.clone(),
            record.kind,
            record
                .text("name")
                .ok_or(CatalogError::InvalidCache)?
                .into(),
            record.text("overview").map(str::to_owned),
        )
        .map_err(|_| CatalogError::InvalidCache)?;
        Ok(title.with_display(Self::display_facts(record)))
    }

    /// Also usable for TMDB titles without an IMDb identifier. Unknown IMDb ID
    /// and unknown IMDb rating are independent; only explicit IMDb scores project.
    pub(crate) fn display_facts(record: &CanonicalTitle) -> kinema_core::catalog::CatalogDisplay {
        use kinema_core::catalog::{CatalogDisplay, ImdbRating};
        let rating = record
            .ratings
            .iter()
            .filter(|r| {
                r.observation.source == "imdb"
                    && r.observation.metric == "score"
                    && r.observation.value.is_some()
            })
            .max_by_key(|r| (r.observed_at, r.updated_at, &r.observation.supplied_by))
            .and_then(|r| r.observation.value)
            .and_then(ImdbRating::new);
        let genres = match record.fields.get("genres_json") {
            Some(FieldValue::Labels(labels)) => labels.clone(),
            _ => Vec::new(),
        };
        CatalogDisplay {
            year: record
                .integer(if record.kind == MediaKind::Movie {
                    "release_year"
                } else {
                    "first_air_year"
                })
                .and_then(|v| u16::try_from(v).ok()),
            poster_url: record.text("poster_url").map(str::to_owned),
            imdb_rating: rating,
            runtime_minutes: record
                .integer("runtime_minutes")
                .and_then(|v| u16::try_from(v).ok()),
            genres,
            season_count: record
                .integer("season_count")
                .and_then(|v| u32::try_from(v).ok()),
        }
    }

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

#[cfg(test)]
mod display_tests {
    use super::*;
    #[test]
    fn stored_provider_facts_project_without_tmdb_rating_substitution()
    -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempfile::tempdir()?;
        let mut service = CatalogService::open(&dir.path().join("catalog"))?;
        let title = CatalogTitle::new(
            ImdbId::new("tt1234567")?,
            MediaKind::Movie,
            "Synthetic",
            None,
        )?;
        let details = TitleDetails {
            title,
            poster_url: Some("https://example.com/poster.png".into()),
            background_url: None,
            release_info: Some("2024".into()),
            genres: vec!["Drama".into()],
            episodes: vec![],
        };
        let tx = service.connection.transaction()?;
        normalized_pages::write_cinemeta_details(&tx, &details, 10, true)?;
        let id = normalized::title_id(&tx, MediaKind::Movie, "imdb", "tt1234567")?
            .ok_or("missing title")?;
        let fetch_id: i64 = tx.query_row(
            "SELECT id FROM metadata_fetches WHERE title_id=?1 AND provider='cinemeta'",
            params![id],
            |row| row.get(0),
        )?;
        // Synthetic provider document, never a real provider/authentication request.
        let document = serde_json::json!({"type":"movie", "runtime":"93 min", "released":"2024-03-12", "genres":["Drama"], "imdbRating":"0.0"});
        let records = normalized_rich::cinemeta(&document)?;
        normalized_rich::write(&tx, id, &records, "cinemeta", fetch_id, 10)?;
        let record = CatalogService::title_record(&tx, id)?;
        let projected = CatalogService::display_title(&record)?;
        assert_eq!(projected.year(), Some(2024));
        assert_eq!(projected.runtime_minutes(), Some(93));
        assert_eq!(
            projected.poster_url(),
            Some("https://example.com/poster.png")
        );
        assert_eq!(projected.genres(), &["Drama"]);
        assert_eq!(projected.imdb_rating(), Some(0.0));
        assert_eq!(projected.season_count(), None);
        let mut record = record;
        record.identifiers.clear();
        assert_eq!(
            CatalogService::display_facts(&record)
                .imdb_rating
                .map(kinema_core::catalog::ImdbRating::value),
            Some(0.0)
        );
        assert!(CatalogService::display_title(&record).is_err());
        record.identifiers.push(ExternalIdentifier {
            namespace: "imdb".into(),
            external_id: "tt1234567".into(),
        });
        record.ratings[0].observation.source = "tmdb".into();
        record.ratings[0].observation.value = Some(9.8);
        let projected = CatalogService::display_title(&record)?;
        assert_eq!(projected.imdb_rating(), None);
        assert_eq!(projected.id().as_str(), "tt1234567");
        Ok(())
    }
    #[test]
    fn series_year_and_season_count_remain_optional_facts() -> Result<(), Box<dyn std::error::Error>>
    {
        let record = CanonicalTitle {
            id: 1,
            kind: MediaKind::Series,
            identifiers: vec![ExternalIdentifier {
                namespace: "imdb".into(),
                external_id: "tt1234567".into(),
            }],
            fields: BTreeMap::from([
                ("name".into(), FieldValue::Text("Series".into())),
                ("first_air_year".into(), FieldValue::Integer(2023)),
                ("season_count".into(), FieldValue::Integer(0)),
            ]),
            provenance: vec![],
            ratings: vec![],
            updated_at: 1,
        };
        let projected = CatalogService::display_title(&record)?;
        assert_eq!(projected.year(), Some(2023));
        assert_eq!(projected.season_count(), Some(0));
        assert_eq!(projected.imdb_rating(), None);
        Ok(())
    }
}

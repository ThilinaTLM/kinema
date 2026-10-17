//! User-approved verified-ID reconciliation. Never infer identity from metadata.
use super::normalized::{TITLE_FIELDS, Target};
use super::*;
use kinema_core::catalog_normalized::{Quality, accepts_field};
use rusqlite::{Transaction, types::Value};

fn entity(target: Target) -> (&'static str, &'static str, &'static str, i64) {
    match target {
        Target::Title(id) => ("titles", "title_id", "title_identifiers", id),
        Target::Season(id) => ("seasons", "season_id", "season_identifiers", id),
        Target::Episode(id) => ("episodes", "episode_id", "episode_identifiers", id),
        Target::Page(_) => unreachable!("pages are never reconciled"),
    }
}

fn ids(tx: &Transaction<'_>, sql: &str, id: i64) -> Result<Vec<(i64, i64)>, CatalogError> {
    let mut stmt = tx.prepare(sql)?;
    Ok(stmt
        .query_map(params![id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?)
}
fn validate_identifiers(
    tx: &Transaction<'_>,
    target: Target,
    other: i64,
) -> Result<(), CatalogError> {
    let (_, column, table, id) = entity(target);
    let conflict:bool=tx.query_row(&format!("SELECT EXISTS(SELECT 1 FROM {table} a JOIN {table} b ON a.namespace=b.namespace WHERE a.{column}=?1 AND b.{column}=?2 AND a.external_id<>b.external_id)"),params![id,other],|r|r.get(0))?;
    if conflict {
        Err(CatalogError::IdentityConflict)
    } else {
        Ok(())
    }
}
fn validate_graph(tx: &Transaction<'_>, target: Target, other: i64) -> Result<(), CatalogError> {
    validate_identifiers(tx, target, other)?;
    let (_, _, _, id) = entity(target);
    match target {
        Target::Title(_) => {
            let kinds: (String, String) = tx.query_row(
                "SELECT a.kind,b.kind FROM titles a,titles b WHERE a.id=?1 AND b.id=?2",
                params![id, other],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )?;
            if kinds.0 != kinds.1 {
                return Err(CatalogError::IdentityConflict);
            }
            for (loser, number) in ids(
                tx,
                "SELECT id,season_number FROM seasons WHERE series_id=?1",
                other,
            )? {
                let winner: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM seasons WHERE series_id=?1 AND season_number=?2",
                        params![id, number],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(winner) = winner {
                    validate_graph(tx, Target::Season(winner), loser)?;
                }
            }
        }
        Target::Season(_) => {
            let coordinates:(i64,i64)=tx.query_row("SELECT a.season_number,b.season_number FROM seasons a,seasons b WHERE a.id=?1 AND b.id=?2",params![id,other],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if coordinates.0 != coordinates.1 {
                return Err(CatalogError::IdentityConflict);
            }
            for (loser, number) in ids(
                tx,
                "SELECT id,episode_number FROM episodes WHERE season_id=?1",
                other,
            )? {
                let winner: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM episodes WHERE season_id=?1 AND episode_number=?2",
                        params![id, number],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(winner) = winner {
                    validate_graph(tx, Target::Episode(winner), loser)?;
                }
            }
        }
        Target::Episode(_) => {
            let coordinates:(i64,i64)=tx.query_row("SELECT a.episode_number,b.episode_number FROM episodes a,episodes b WHERE a.id=?1 AND b.id=?2",params![id,other],|r|Ok((r.get(0)?,r.get(1)?)))?;
            if coordinates.0 != coordinates.1 {
                return Err(CatalogError::IdentityConflict);
            }
        }
        Target::Page(_) => return Err(CatalogError::IdentityConflict),
    }
    Ok(())
}

/// Caller has validated an explicit shared IMDb mapping. The survivor is stable
/// (lowest local ID), independent of provider selection or arrival order.
pub(super) fn reconcile(
    tx: &Transaction<'_>,
    first: i64,
    second: i64,
    time: i64,
) -> Result<i64, CatalogError> {
    let (winner, loser) = if first < second {
        (first, second)
    } else {
        (second, first)
    };
    validate_graph(tx, Target::Title(winner), loser)?;
    merge_entity(tx, Target::Title(winner), loser, time)?;
    Ok(winner)
}

fn merge_entity(
    tx: &Transaction<'_>,
    target: Target,
    other: i64,
    time: i64,
) -> Result<(), CatalogError> {
    let (table, column, identifiers, id) = entity(target);
    // Move/merge children before parents. The verified parent mapping makes
    // equal explicit coordinates compatible; external-ID conflicts were checked.
    match target {
        Target::Title(_) => {
            for (child, number) in ids(
                tx,
                "SELECT id,season_number FROM seasons WHERE series_id=?1",
                other,
            )? {
                let existing: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM seasons WHERE series_id=?1 AND season_number=?2",
                        params![id, number],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(existing) = existing {
                    merge_entity(tx, Target::Season(existing), child, time)?;
                } else {
                    tx.execute(
                        "UPDATE seasons SET series_id=?1 WHERE id=?2",
                        params![id, child],
                    )?;
                }
            }
        }
        Target::Season(_) => {
            for (child, number) in ids(
                tx,
                "SELECT id,episode_number FROM episodes WHERE season_id=?1",
                other,
            )? {
                let existing: Option<i64> = tx
                    .query_row(
                        "SELECT id FROM episodes WHERE season_id=?1 AND episode_number=?2",
                        params![id, number],
                        |r| r.get(0),
                    )
                    .optional()?;
                if let Some(existing) = existing {
                    merge_entity(tx, Target::Episode(existing), child, time)?;
                } else {
                    tx.execute(
                        "UPDATE episodes SET season_id=?1 WHERE id=?2",
                        params![id, child],
                    )?;
                }
            }
        }
        _ => {}
    }
    merge_fields(tx, target, other, time)?;
    merge_ratings(tx, target, other)?;
    merge_fetches(tx, target, other)?;
    tx.execute(
        &format!("UPDATE {identifiers} SET {column}=?1 WHERE {column}=?2"),
        params![id, other],
    )?;
    match target {
        Target::Title(_) => {
            merge_pages(tx, id, other, time)?;
            merge_snapshots(tx, "movie_id", id, other)?;
        }
        Target::Episode(_) => merge_snapshots(tx, "episode_id", id, other)?,
        _ => {}
    }
    tx.execute(&format!("DELETE FROM {table} WHERE id=?1"), params![other])?;
    Ok(())
}

fn merge_fields(
    tx: &Transaction<'_>,
    target: Target,
    other: i64,
    time: i64,
) -> Result<(), CatalogError> {
    let (table, column, _, id) = entity(target);
    let fields: &[&str] = match target {
        Target::Title(_) => TITLE_FIELDS,
        Target::Season(_) => &[
            "name",
            "overview",
            "air_date",
            "air_year",
            "poster_url",
            "episode_count",
        ],
        Target::Episode(_) => &[
            "name",
            "overview",
            "air_date",
            "runtime_minutes",
            "production_code",
            "still_url",
        ],
        _ => return Err(CatalogError::IdentityConflict),
    };
    let mut chosen = Vec::new();
    let mut transfers = Vec::new();
    for &name in fields {
        let value: Value = tx.query_row(
            &format!("SELECT {name} FROM {table} WHERE id=?1"),
            params![other],
            |r| r.get(0),
        )?;
        if value == Value::Null {
            continue;
        }
        let origin:Option<(i64,String,i64)>=tx.query_row(&format!("SELECT id,quality,observed_at FROM field_provenance WHERE {column}=?1 AND field_name=?2"),params![other,name],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
        let Some((provenance, quality, observed)) = origin else {
            return Err(CatalogError::InvalidCache);
        };
        let old:Option<(String,i64)>=tx.query_row(&format!("SELECT quality,observed_at FROM field_provenance WHERE {column}=?1 AND field_name=?2"),params![id,name],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        let quality = if quality == "detail" {
            Quality::Detail
        } else {
            Quality::Summary
        };
        if accepts_field(
            old.map(|(q, t)| {
                (
                    if q == "detail" {
                        Quality::Detail
                    } else {
                        Quality::Summary
                    },
                    t,
                )
            }),
            quality,
            observed,
        ) {
            chosen.push((name, value));
            transfers.push((name, provenance));
        }
    }
    // One statement keeps correlated date/year constraints valid. A pair made
    // inconsistent by independently eligible observations is explicitly rejected.
    if !chosen.is_empty() {
        let pairs: &[(&str, &str)] = match target {
            Target::Title(_) => &[
                ("release_date", "release_year"),
                ("first_air_date", "first_air_year"),
                ("last_air_date", "last_air_year"),
            ],
            Target::Season(_) => &[("air_date", "air_year")],
            _ => &[],
        };
        for &(date, year) in pairs {
            let d: Value = if let Some((_, v)) = chosen.iter().find(|(n, _)| *n == date) {
                v.clone()
            } else {
                tx.query_row(
                    &format!("SELECT {date} FROM {table} WHERE id=?1"),
                    params![id],
                    |r| r.get(0),
                )?
            };
            let y: Value = if let Some((_, v)) = chosen.iter().find(|(n, _)| *n == year) {
                v.clone()
            } else {
                tx.query_row(
                    &format!("SELECT {year} FROM {table} WHERE id=?1"),
                    params![id],
                    |r| r.get(0),
                )?
            };
            if let (Value::Text(d), Value::Integer(y)) = (d, y)
                && d.get(..4).and_then(|s| s.parse::<i64>().ok()) != Some(y)
            {
                return Err(CatalogError::IdentityConflict);
            }
        }
        let mut changed = false;
        for (name, value) in &chosen {
            let old: Value = tx.query_row(
                &format!("SELECT {name} FROM {table} WHERE id=?1"),
                params![id],
                |r| r.get(0),
            )?;
            changed |= old != *value;
        }
        let assignments = chosen
            .iter()
            .enumerate()
            .map(|(i, (name, _))| format!("{name}=?{}", i + 1))
            .collect::<Vec<_>>()
            .join(",");
        let mut values: Vec<Value> = chosen.into_iter().map(|(_, value)| value).collect();
        let flag = values.len() + 1;
        let clock = flag + 1;
        let owner = clock + 1;
        values.extend([
            Value::Integer(i64::from(changed)),
            Value::Integer(time),
            Value::Integer(id),
        ]);
        let sql = format!(
            "UPDATE {table} SET {assignments},updated_at=CASE WHEN ?{flag} THEN ?{clock} ELSE updated_at END WHERE id=?{owner}"
        );
        tx.execute(&sql, rusqlite::params_from_iter(values))?;
        for (name, provenance) in transfers {
            tx.execute(
                &format!("DELETE FROM field_provenance WHERE {column}=?1 AND field_name=?2"),
                params![id, name],
            )?;
            tx.execute(
                &format!("UPDATE field_provenance SET {column}=?1 WHERE id=?2"),
                params![id, provenance],
            )?;
        }
    }
    tx.execute(
        &format!("DELETE FROM field_provenance WHERE {column}=?1"),
        params![other],
    )?;
    Ok(())
}

fn merge_ratings(tx: &Transaction<'_>, target: Target, other: i64) -> Result<(), CatalogError> {
    let (_, column, _, id) = entity(target);
    let rows: Vec<(i64, String, String, String, i64)> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT id,rating_source,metric,supplied_by,observed_at FROM ratings WHERE {column}=?1"
        ))?;
        stmt.query_map(params![other], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?
    };
    for (rating, source, metric, supplier, observed) in rows {
        let old:Option<(i64,i64)>=tx.query_row(&format!("SELECT id,observed_at FROM ratings WHERE {column}=?1 AND rating_source=?2 AND metric=?3 AND supplied_by=?4"),params![id,source,metric,supplier],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old, time)) = old {
            if time > observed {
                tx.execute("DELETE FROM ratings WHERE id=?1", params![rating])?;
                continue;
            }
            tx.execute("DELETE FROM ratings WHERE id=?1", params![old])?;
        }
        tx.execute(
            &format!("UPDATE ratings SET {column}=?1 WHERE id=?2"),
            params![id, rating],
        )?;
    }
    Ok(())
}

fn merge_fetches(tx: &Transaction<'_>, target: Target, other: i64) -> Result<(), CatalogError> {
    let (_, column, _, id) = entity(target);
    let rows: Vec<(i64, String, String, String)> = {
        let mut stmt = tx.prepare(&format!(
            "SELECT id,provider,resource_key,locale FROM metadata_fetches WHERE {column}=?1"
        ))?;
        stmt.query_map(params![other], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
        })?
        .collect::<Result<_, _>>()?
    };
    for (fetch, provider, key, locale) in rows {
        let old:Option<i64>=tx.query_row(&format!("SELECT id FROM metadata_fetches WHERE {column}=?1 AND provider=?2 AND resource_key=?3 AND locale=?4"),params![id,provider,key,locale],|r|r.get(0)).optional()?;
        if let Some(old) = old {
            // Success and last attempt advance independently; failure never wipes success.
            tx.execute("UPDATE metadata_fetches SET fetched_at=(SELECT fetched_at FROM metadata_fetches WHERE id=?2),success_kind=(SELECT success_kind FROM metadata_fetches WHERE id=?2),completeness=(SELECT completeness FROM metadata_fetches WHERE id=?2) WHERE id=?1 AND COALESCE(fetched_at,-1)<COALESCE((SELECT fetched_at FROM metadata_fetches WHERE id=?2),-1)",params![old,fetch])?;
            tx.execute("UPDATE metadata_fetches SET last_attempt_at=(SELECT last_attempt_at FROM metadata_fetches WHERE id=?2),last_error_code=(SELECT last_error_code FROM metadata_fetches WHERE id=?2) WHERE id=?1 AND COALESCE(last_attempt_at,-1)<COALESCE((SELECT last_attempt_at FROM metadata_fetches WHERE id=?2),-1)",params![old,fetch])?;
            for table in [
                "title_identifiers",
                "season_identifiers",
                "episode_identifiers",
                "field_provenance",
                "ratings",
            ] {
                tx.execute(
                    &format!("UPDATE {table} SET fetch_state_id=?1 WHERE fetch_state_id=?2"),
                    params![old, fetch],
                )?;
            }
            tx.execute("DELETE FROM metadata_fetches WHERE id=?1", params![fetch])?;
        } else {
            tx.execute(
                &format!("UPDATE metadata_fetches SET {column}=?1 WHERE id=?2"),
                params![id, fetch],
            )?;
        }
    }
    Ok(())
}

fn merge_pages(
    tx: &Transaction<'_>,
    winner: i64,
    loser: i64,
    time: i64,
) -> Result<(), CatalogError> {
    let pages: Vec<i64> = {
        let mut stmt =
            tx.prepare("SELECT DISTINCT page_id FROM catalog_page_items WHERE title_id=?1")?;
        stmt.query_map(params![loser], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    for page in pages {
        let mut stmt = tx.prepare(
            "SELECT title_id FROM catalog_page_items WHERE page_id=?1 ORDER BY position",
        )?;
        let rows = stmt
            .query_map(params![page], |r| r.get::<_, i64>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        drop(stmt);
        let mut seen = HashSet::new();
        let rows: Vec<i64> = rows
            .into_iter()
            .map(|id| if id == loser { winner } else { id })
            .filter(|id| seen.insert(*id))
            .collect();
        tx.execute(
            "DELETE FROM catalog_page_items WHERE page_id=?1",
            params![page],
        )?;
        for (position, id) in rows.iter().enumerate() {
            tx.execute(
                "INSERT INTO catalog_page_items(page_id,position,title_id) VALUES (?1,?2,?3)",
                params![page, position as i64, id],
            )?;
        }
        tx.execute("UPDATE catalog_pages SET result_count=CASE WHEN membership_available THEN ?2 ELSE result_count END,updated_at=?3 WHERE id=?1",params![page,rows.len() as i64,time])?;
    }
    Ok(())
}
fn merge_snapshots(
    tx: &Transaction<'_>,
    column: &str,
    winner: i64,
    loser: i64,
) -> Result<(), CatalogError> {
    let rows: Vec<(i64, String, String, String, i64)> = {
        let mut stmt=tx.prepare(&format!("SELECT id,indexer,query_key,cache_scope,fetched_at FROM indexer_snapshots WHERE {column}=?1"))?;
        stmt.query_map(params![loser], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })?
        .collect::<Result<_, _>>()?
    };
    for (snapshot, indexer, key, scope, time) in rows {
        let old:Option<(i64,i64)>=tx.query_row(&format!("SELECT id,fetched_at FROM indexer_snapshots WHERE {column}=?1 AND indexer=?2 AND query_key=?3 AND cache_scope=?4"),params![winner,indexer,key,scope],|r|Ok((r.get(0)?,r.get(1)?))).optional()?;
        if let Some((old, fetched)) = old {
            if fetched > time {
                tx.execute(
                    "DELETE FROM indexer_snapshots WHERE id=?1",
                    params![snapshot],
                )?;
                continue;
            }
            tx.execute("DELETE FROM indexer_snapshots WHERE id=?1", params![old])?;
        }
        tx.execute(
            &format!("UPDATE indexer_snapshots SET {column}=?1 WHERE id=?2"),
            params![winner, snapshot],
        )?;
    }
    Ok(())
}

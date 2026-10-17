//! Atomic date/year pairs, retaining independent per-field merge eligibility.
use super::normalized::{Target, field};
use super::*;
use kinema_core::catalog_normalized::{FieldValue, Quality, accepts_field, valid_date};
use rusqlite::Transaction;

pub(super) fn fields(
    tx: &Transaction<'_>,
    target: Target,
    values: &[(String, FieldValue)],
    provider: &str,
    quality: Quality,
    fetch_id: i64,
    time: i64,
) -> Result<(), CatalogError> {
    let (table, column, id, pairs): (&str, &str, i64, &[(&str, &str)]) = match target {
        Target::Title(id) => (
            "titles",
            "title_id",
            id,
            &[
                ("release_date", "release_year"),
                ("first_air_date", "first_air_year"),
                ("last_air_date", "last_air_year"),
            ],
        ),
        Target::Season(id) => ("seasons", "season_id", id, &[("air_date", "air_year")]),
        _ => {
            for (name, value) in values {
                field(tx, target, (name, value), provider, quality, fetch_id, time)?;
            }
            return Ok(());
        }
    };
    for &(date_name, year_name) in pairs {
        let date = values.iter().find(|(name, _)| name == date_name);
        let year = values.iter().find(|(name, _)| name == year_name);
        if date.is_none() && year.is_none() {
            continue;
        }
        let accepted = |name: &str| -> Result<bool, CatalogError> {
            let old:Option<(String,i64)>=tx.query_row(&format!("SELECT quality,observed_at FROM field_provenance WHERE {column}=?1 AND field_name=?2"),params![id,name],|row|Ok((row.get(0)?,row.get(1)?))).optional()?;
            Ok(accepts_field(
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
                time,
            ))
        };
        let (mut final_date, mut final_year): (Option<String>, Option<i64>) = tx.query_row(
            &format!("SELECT {date_name},{year_name} FROM {table} WHERE id=?1"),
            params![id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        if let Some((_, value)) = date
            && accepted(date_name)?
        {
            let FieldValue::Text(value) = value else {
                return Err(CatalogError::InvalidMetadata);
            };
            if !value.trim().is_empty() {
                if !valid_date(value) {
                    return Err(CatalogError::InvalidMetadata);
                }
                final_date = Some(value.clone());
            }
        }
        if let Some((_, value)) = year
            && accepted(year_name)?
        {
            let FieldValue::Integer(value) = value else {
                return Err(CatalogError::InvalidMetadata);
            };
            if !(1..=9999).contains(value) {
                return Err(CatalogError::InvalidMetadata);
            }
            final_year = Some(*value);
        }
        if final_date
            .as_ref()
            .zip(final_year)
            .is_some_and(|(date, year)| date[..4].parse::<i64>().ok() != Some(year))
        {
            return Err(CatalogError::InvalidMetadata);
        }
        tx.execute(&format!("UPDATE {table} SET {date_name}=?1,{year_name}=?2,updated_at=CASE WHEN {date_name} IS NOT ?1 OR {year_name} IS NOT ?2 THEN ?3 ELSE updated_at END WHERE id=?4"),params![final_date,final_year,time,id])?;
    }
    for (name, value) in values {
        field(tx, target, (name, value), provider, quality, fetch_id, time)?;
    }
    Ok(())
}

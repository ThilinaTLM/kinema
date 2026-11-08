//! Persisted stream memory (personal records, schema v9). Call only on the catalog worker.
//!
//! Raw strings are stored, never parsed traits: `kinema-core` derives traits at
//! read time. Two overwrite-only records exist:
//! - per season (`season_release_memory`, keyed by `seasons.id`): the last played
//!   release;
//! - per episode (`episode_stream_memory`, keyed by `episodes.id`): the last
//!   played stream.
//!
//! Both are keyed by canonical row IDs, the same convention as
//! `episode_playback_progress`, and are independent of watched state and progress.
//! They never hold URLs, debrid file IDs or provider file IDs, and are never logged.
#![allow(
    dead_code,
    reason = "Playback integration consumes this staged repository protocol"
)]
use super::library::{TitleRef, title_id};
use super::*;
use kinema_core::sources::InfoHash;
use rusqlite::{Transaction, TransactionBehavior};

/// Upper bound for every stored text field, in bytes.
const MAX_TEXT_BYTES: usize = 1024;

/// What is needed to rebuild a `SourceCandidate` (without seeders).
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ReleaseMemory {
    pub info_hash: InfoHash,
    /// Release name as listed by the indexer.
    pub name: String,
    pub quality: Option<String>,
    pub size: Option<u64>,
    /// Indexer file index hint; `None` when the indexer gave none.
    pub file_index: Option<u32>,
    /// Indexer file name hint; `None` when the indexer gave none.
    pub filename: Option<String>,
}
/// The last played stream of one episode: its release plus the resolved file.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StreamMemory {
    pub release: ReleaseMemory,
    /// Resolved provider file basename (no directories), when known.
    pub file_name: Option<String>,
    pub file_size: Option<u64>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RememberedRelease {
    pub release: ReleaseMemory,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RememberedStream {
    pub stream: StreamMemory,
    pub updated_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Remember {
    pub series: TitleRef,
    pub season: u32,
    pub episode: u32,
    pub stream: StreamMemory,
    /// The season record; callers may drop per-episode file hints here.
    pub season_release: ReleaseMemory,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StreamMemoryOperation {
    /// Overwrite both records atomically. The episode and its season must exist
    /// in the catalog; otherwise `MissingIdentityMapping`.
    Remember(Box<Remember>),
    /// Missing rows are `None`, not errors.
    Read {
        series: TitleRef,
        season: u32,
        episode: u32,
    },
    /// The nearest earlier season (number >= 1, below `season`) with a record.
    ReadPreviousSeason { series: TitleRef, season: u32 },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum StreamMemoryResult {
    Remembered,
    Memory {
        episode: Option<RememberedStream>,
        season: Option<RememberedRelease>,
    },
    PreviousSeason(Option<(u32, RememberedRelease)>),
}

#[derive(Clone, Copy)]
pub(super) enum Merge {
    Season,
    Episode,
}

fn text(value: &str) -> Result<&str, CatalogError> {
    if value.is_empty() || value.len() > MAX_TEXT_BYTES || value.contains('\0') {
        return Err(CatalogError::InvalidMetadata);
    }
    Ok(value)
}
fn optional_text(value: &Option<String>) -> Result<Option<&str>, CatalogError> {
    value.as_deref().map(text).transpose()
}
fn basename(value: &Option<String>) -> Result<Option<&str>, CatalogError> {
    match optional_text(value)? {
        Some(name) if name.contains(['/', '\\']) => Err(CatalogError::InvalidMetadata),
        other => Ok(other),
    }
}
fn size(value: Option<u64>) -> Result<Option<i64>, CatalogError> {
    value
        .map(|v| i64::try_from(v).map_err(|_| CatalogError::InvalidMetadata))
        .transpose()
}
type ReleaseParams = (
    String,
    String,
    Option<String>,
    Option<i64>,
    Option<u32>,
    Option<String>,
);
fn release_params(release: &ReleaseMemory) -> Result<ReleaseParams, CatalogError> {
    Ok((
        release.info_hash.as_str().to_owned(),
        text(&release.name)?.to_owned(),
        optional_text(&release.quality)?.map(str::to_owned),
        size(release.size)?,
        release.file_index,
        optional_text(&release.filename)?.map(str::to_owned),
    ))
}
fn read_release(row: &rusqlite::Row<'_>, at: usize) -> Result<ReleaseMemory, CatalogError> {
    let hash: String = row.get(at)?;
    let to_u64 = |v: Option<i64>| v.map(u64::try_from).transpose();
    Ok(ReleaseMemory {
        info_hash: InfoHash::new(hash).map_err(|_| CatalogError::InvalidCache)?,
        name: row.get(at + 1)?,
        quality: row.get(at + 2)?,
        size: to_u64(row.get(at + 3)?).map_err(|_| CatalogError::InvalidCache)?,
        file_index: row.get(at + 4)?,
        filename: row.get(at + 5)?,
    })
}

fn series(tx: &Transaction<'_>, reference: &TitleRef) -> Result<i64, CatalogError> {
    let id = title_id(tx, reference)?;
    let kind: String = tx.query_row("SELECT kind FROM titles WHERE id=?1", [id], |r| r.get(0))?;
    if kind != "series" {
        return Err(CatalogError::InvalidMetadata);
    }
    Ok(id)
}
fn season_row(tx: &Transaction<'_>, series: i64, season: u32) -> Result<Option<i64>, CatalogError> {
    Ok(tx
        .query_row(
            "SELECT id FROM seasons WHERE series_id=?1 AND season_number=?2",
            params![series, season],
            |r| r.get(0),
        )
        .optional()?)
}
fn episode_row(
    tx: &Transaction<'_>,
    season_id: i64,
    episode: u32,
) -> Result<Option<i64>, CatalogError> {
    Ok(tx
        .query_row(
            "SELECT id FROM episodes WHERE season_id=?1 AND episode_number=?2",
            params![season_id, episode],
            |r| r.get(0),
        )
        .optional()?)
}
fn season_memory(
    tx: &Transaction<'_>,
    season_id: i64,
) -> Result<Option<RememberedRelease>, CatalogError> {
    tx.query_row(
        "SELECT info_hash,name,quality,size,file_index,filename,updated_at FROM season_release_memory WHERE season_id=?1",
        [season_id],
        |r| Ok((read_release(r, 0), r.get::<_, i64>(6)?)),
    )
    .optional()?
    .map(|(release, updated_at)| {
        Ok(RememberedRelease {
            release: release?,
            updated_at,
        })
    })
    .transpose()
}
fn episode_memory(
    tx: &Transaction<'_>,
    episode_id: i64,
) -> Result<Option<RememberedStream>, CatalogError> {
    tx.query_row(
        "SELECT info_hash,name,quality,size,file_index,filename,provider_file_name,provider_file_size,updated_at FROM episode_stream_memory WHERE episode_id=?1",
        [episode_id],
        |r| Ok((read_release(r, 0), (r.get::<_, Option<String>>(6)?, r.get::<_, Option<i64>>(7)?, r.get::<_, i64>(8)?))),
    )
    .optional()?
    .map(|(release, (file_name, file_size, updated_at))| {
        Ok(RememberedStream {
            stream: StreamMemory {
                release: release?,
                file_name,
                file_size: file_size
                    .map(u64::try_from)
                    .transpose()
                    .map_err(|_| CatalogError::InvalidCache)?,
            },
            updated_at,
        })
    })
    .transpose()
}

/// Move a merge loser's record to the winner (newest wins), before the RESTRICTed
/// loser row is deleted. Runs inside the merge transaction.
pub(super) fn merge(
    tx: &Transaction<'_>,
    which: Merge,
    winner: i64,
    loser: i64,
) -> Result<(), CatalogError> {
    let (table, column, columns) = match which {
        Merge::Season => (
            "season_release_memory",
            "season_id",
            "info_hash,name,quality,size,file_index,filename,updated_at",
        ),
        Merge::Episode => (
            "episode_stream_memory",
            "episode_id",
            "info_hash,name,quality,size,file_index,filename,provider_file_name,provider_file_size,updated_at",
        ),
    };
    // Pre-v9 backfill merges run before these tables exist; there is nothing to move.
    let exists: bool = tx.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |r| r.get(0),
    )?;
    if !exists {
        return Ok(());
    }
    let set = columns
        .split(',')
        .map(|c| format!("{c}=excluded.{c}"))
        .collect::<Vec<_>>()
        .join(",");
    tx.execute(
        &format!("INSERT INTO {table}({column},{columns}) SELECT ?1,{columns} FROM {table} WHERE {column}=?2 ON CONFLICT({column}) DO UPDATE SET {set} WHERE excluded.updated_at>{table}.updated_at"),
        params![winner, loser],
    )?;
    tx.execute(&format!("DELETE FROM {table} WHERE {column}=?1"), [loser])?;
    Ok(())
}

impl CatalogService {
    pub(crate) fn stream_memory_operation(
        &mut self,
        operation: StreamMemoryOperation,
    ) -> Result<StreamMemoryResult, CatalogError> {
        let behavior = match operation {
            StreamMemoryOperation::Remember(_) => TransactionBehavior::Immediate,
            _ => TransactionBehavior::Deferred,
        };
        let tx = self.connection.transaction_with_behavior(behavior)?;
        let result = match operation {
            StreamMemoryOperation::Remember(write) => {
                let Remember {
                    series: reference,
                    season,
                    episode,
                    stream,
                    season_release,
                } = *write;
                // Validate everything before touching rows.
                let (hash, name, quality, release_size, index, filename) =
                    release_params(&stream.release)?;
                let provider_name = basename(&stream.file_name)?.map(str::to_owned);
                let provider_size = size(stream.file_size)?;
                let (s_hash, s_name, s_quality, s_size, s_index, s_filename) =
                    release_params(&season_release)?;
                let series = series(&tx, &reference)?;
                let season_id =
                    season_row(&tx, series, season)?.ok_or(CatalogError::MissingIdentityMapping)?;
                let episode_id = episode_row(&tx, season_id, episode)?
                    .ok_or(CatalogError::MissingIdentityMapping)?;
                let now = library::wall_millis()?;
                tx.execute(
                    "INSERT INTO season_release_memory(season_id,info_hash,name,quality,size,file_index,filename,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8) ON CONFLICT(season_id) DO UPDATE SET info_hash=excluded.info_hash,name=excluded.name,quality=excluded.quality,size=excluded.size,file_index=excluded.file_index,filename=excluded.filename,updated_at=excluded.updated_at",
                    params![season_id, s_hash, s_name, s_quality, s_size, s_index, s_filename, now],
                )?;
                tx.execute(
                    "INSERT INTO episode_stream_memory(episode_id,info_hash,name,quality,size,file_index,filename,provider_file_name,provider_file_size,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(episode_id) DO UPDATE SET info_hash=excluded.info_hash,name=excluded.name,quality=excluded.quality,size=excluded.size,file_index=excluded.file_index,filename=excluded.filename,provider_file_name=excluded.provider_file_name,provider_file_size=excluded.provider_file_size,updated_at=excluded.updated_at",
                    params![episode_id, hash, name, quality, release_size, index, filename, provider_name, provider_size, now],
                )?;
                StreamMemoryResult::Remembered
            }
            StreamMemoryOperation::Read {
                series: reference,
                season,
                episode,
            } => {
                let series = series(&tx, &reference)?;
                match season_row(&tx, series, season)? {
                    None => StreamMemoryResult::Memory {
                        episode: None,
                        season: None,
                    },
                    Some(season_id) => StreamMemoryResult::Memory {
                        episode: episode_row(&tx, season_id, episode)?
                            .map(|id| episode_memory(&tx, id))
                            .transpose()?
                            .flatten(),
                        season: season_memory(&tx, season_id)?,
                    },
                }
            }
            StreamMemoryOperation::ReadPreviousSeason {
                series: reference,
                season,
            } => {
                let series = series(&tx, &reference)?;
                let found: Option<(i64, u32)> = tx
                    .query_row(
                        "SELECT s.id,s.season_number FROM seasons s JOIN season_release_memory m ON m.season_id=s.id WHERE s.series_id=?1 AND s.season_number>=1 AND s.season_number<?2 ORDER BY s.season_number DESC LIMIT 1",
                        params![series, season],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .optional()?;
                StreamMemoryResult::PreviousSeason(
                    found
                        .map(|(id, number)| {
                            season_memory(&tx, id)?
                                .map(|record| (number, record))
                                .ok_or(CatalogError::InvalidCache)
                        })
                        .transpose()?,
                )
            }
        };
        tx.commit()?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "catalog_stream_memory_tests.rs"]
mod tests;

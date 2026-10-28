//! Durable, synchronous repository protocol. Call only on the catalog worker.
//! A stale response may be retried with its new epoch ONLY when active_run still
//! equals the caller's run ID. Preserve suppression across that handshake.
#![allow(
    dead_code,
    reason = "Playback worker integration consumes this staged repository protocol"
)]
use super::*;
use kinema_core::{
    library::WatchState,
    watch_progress::{PlaybackObservation, PlaybackProgressTarget},
};
use rusqlite::{Transaction, TransactionBehavior};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlaybackCheckpoint {
    pub observation: PlaybackObservation,
    pub updated_at: i64,
    pub revision: i64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct PlaybackProgressSnapshot {
    pub target: PlaybackProgressTarget,
    pub watch: Option<WatchState>,
    pub epoch: i64,
    /// Zero means there is no authorized playback run.
    pub active_run: i64,
    pub auto_mark_suppressed: bool,
    pub checkpoint: Option<PlaybackCheckpoint>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybackProgressOperation {
    Read(PlaybackProgressTarget),
    /// Call only when playback is ready; failed preparation must not reset progress.
    BeginRun {
        target: PlaybackProgressTarget,
        start_over: bool,
    },
    Save {
        target: PlaybackProgressTarget,
        run: i64,
        epoch: i64,
        observation: PlaybackObservation,
    },
    Clear(PlaybackProgressTarget),
    /// Optionally flush the last confirmed sample, then revoke the run atomically.
    CompleteRun {
        target: PlaybackProgressTarget,
        run: i64,
        epoch: i64,
        observation: Option<PlaybackObservation>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PlaybackProgressResult {
    Applied(PlaybackProgressSnapshot),
    Stale(PlaybackProgressSnapshot),
}
fn tables(target: PlaybackProgressTarget) -> (&'static str, &'static str, i64) {
    match target {
        PlaybackProgressTarget::Movie(id) => ("movie", "title_id", id.get()),
        PlaybackProgressTarget::Episode(id) => ("episode", "episode_id", id.get()),
    }
}
/// Batched snapshots share the caller's transaction and avoid per-card queries.
pub(super) fn read_many(
    tx: &Transaction<'_>,
    movie: bool,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, PlaybackProgressSnapshot>, CatalogError> {
    use kinema_core::library::{EpisodeId, TitleId};
    let mut result = std::collections::HashMap::new();
    let (kind, key, parent, restriction) = if movie {
        ("movie", "title_id", "titles", "AND p.kind='movie'")
    } else {
        ("episode", "episode_id", "episodes", "")
    };
    for chunk in ids.chunks(500) {
        if chunk.is_empty() {
            continue;
        }
        let sql = format!(
            "SELECT p.id,w.watched,w.updated_at,coalesce(w.revision,0),coalesce(w.active_run,0),coalesce(w.auto_mark_suppressed,0),c.position_ms,c.duration_ms,c.updated_at,c.revision FROM {parent} p LEFT JOIN {kind}_watch_state w ON w.{key}=p.id LEFT JOIN {kind}_playback_progress c ON c.{key}=p.id WHERE p.id IN ({}) {restriction}",
            vec!["?"; chunk.len()].join(",")
        );
        let mut stmt = tx.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<bool>>(1)?,
                r.get::<_, Option<i64>>(2)?,
                r.get::<_, i64>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, bool>(5)?,
                r.get::<_, Option<i64>>(6)?,
                r.get::<_, Option<i64>>(7)?,
                r.get::<_, Option<i64>>(8)?,
                r.get::<_, Option<i64>>(9)?,
            ))
        })?;
        for row in rows {
            let (
                id,
                watched,
                time,
                epoch,
                active_run,
                auto_mark_suppressed,
                position,
                duration,
                updated_at,
                revision,
            ) = row?;
            let target = if movie {
                PlaybackProgressTarget::Movie(TitleId::new(id).ok_or(CatalogError::InvalidCache)?)
            } else {
                PlaybackProgressTarget::Episode(
                    EpisodeId::new(id).ok_or(CatalogError::InvalidCache)?,
                )
            };
            let checkpoint = position
                .map(|position| {
                    Ok::<_, CatalogError>(PlaybackCheckpoint {
                        observation: PlaybackObservation::new(position, duration)
                            .ok_or(CatalogError::InvalidCache)?,
                        updated_at: updated_at.ok_or(CatalogError::InvalidCache)?,
                        revision: revision.ok_or(CatalogError::InvalidCache)?,
                    })
                })
                .transpose()?;
            result.insert(
                id,
                PlaybackProgressSnapshot {
                    target,
                    watch: watched.zip(time).map(|(watched, updated_at)| WatchState {
                        watched,
                        updated_at,
                    }),
                    epoch,
                    active_run,
                    auto_mark_suppressed,
                    checkpoint,
                },
            );
        }
    }
    Ok(result)
}
pub(super) fn read(
    tx: &Transaction<'_>,
    target: PlaybackProgressTarget,
) -> Result<PlaybackProgressSnapshot, CatalogError> {
    let (kind, _, id) = tables(target);
    read_many(tx, kind == "movie", &[id])?
        .remove(&id)
        .ok_or(CatalogError::MissingIdentityMapping)
}
fn ensure_state(tx: &Transaction<'_>, kind: &str, key: &str, id: i64) -> Result<(), CatalogError> {
    tx.execute(&format!("INSERT INTO {kind}_watch_state({key},watched,updated_at) VALUES (?1,0,0) ON CONFLICT({key}) DO NOTHING"),[id])?;
    Ok(())
}
/// Shared by movie, episode and frozen bulk manual mutations.
pub(super) fn manual_watch(
    tx: &Transaction<'_>,
    kind: &str,
    key: &str,
    id: i64,
    watched: bool,
    time: i64,
) -> Result<(), CatalogError> {
    ensure_state(tx, kind, key, id)?;
    tx.execute(&format!("UPDATE {kind}_watch_state SET watched=?2,updated_at=?3,revision=revision+1,auto_mark_suppressed=CASE WHEN ?2=0 THEN 1 ELSE auto_mark_suppressed END WHERE {key}=?1"),params![id,watched,time])?;
    tx.execute(&format!("UPDATE {kind}_playback_progress SET revision=(SELECT revision FROM {kind}_watch_state WHERE {key}=?1) WHERE {key}=?1"),[id])?;
    Ok(())
}
fn save(
    tx: &Transaction<'_>,
    state: PlaybackProgressSnapshot,
    observation: PlaybackObservation,
    now: i64,
) -> Result<(), CatalogError> {
    let (kind, key, id) = tables(state.target);
    tx.execute(&format!("INSERT INTO {kind}_playback_progress({key},position_ms,duration_ms,updated_at,revision) VALUES (?1,?2,?3,?4,?5) ON CONFLICT({key}) DO UPDATE SET position_ms=excluded.position_ms,duration_ms=excluded.duration_ms,updated_at=excluded.updated_at,revision=excluded.revision"),params![id,observation.position_ms(),observation.duration_ms(),now,state.epoch])?;
    if !state.watch.is_some_and(|w| w.watched)
        && observation.qualifies_watched(state.auto_mark_suppressed)
    {
        tx.execute(
            &format!("UPDATE {kind}_watch_state SET watched=1,updated_at=?2 WHERE {key}=?1"),
            params![id, now],
        )?;
    }
    Ok(())
}
impl CatalogService {
    pub(crate) fn playback_progress_operation(
        &mut self,
        operation: PlaybackProgressOperation,
    ) -> Result<PlaybackProgressResult, CatalogError> {
        // IMMEDIATE serializes read/compare/write even across application processes.
        let tx = self
            .connection
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        use PlaybackProgressOperation::*;
        let target = match operation {
            Read(t) | Clear(t) => t,
            BeginRun { target, .. } | Save { target, .. } | CompleteRun { target, .. } => target,
        };
        let state = read(&tx, target)?;
        let (kind, key, id) = tables(target);
        if let Save { run, epoch, .. } | CompleteRun { run, epoch, .. } = operation
            && (run == 0 || run != state.active_run || epoch != state.epoch)
        {
            tx.commit()?;
            return Ok(PlaybackProgressResult::Stale(state));
        }
        if !matches!(operation, Read(_)) {
            ensure_state(&tx, kind, key, id)?;
            let now = library::mutation_time(&tx)?;
            match operation {
                BeginRun { start_over, .. } => {
                    tx.execute(&format!("UPDATE {kind}_watch_state SET revision=revision+1,active_run=revision+1,auto_mark_suppressed=0 WHERE {key}=?1"),[id])?;
                    if start_over {
                        tx.execute(
                            &format!("DELETE FROM {kind}_playback_progress WHERE {key}=?1"),
                            [id],
                        )?;
                    }
                }
                Clear(_) => {
                    tx.execute(
                        &format!(
                            "UPDATE {kind}_watch_state SET revision=revision+1 WHERE {key}=?1"
                        ),
                        [id],
                    )?;
                    tx.execute(
                        &format!("DELETE FROM {kind}_playback_progress WHERE {key}=?1"),
                        [id],
                    )?;
                }
                Save { observation, .. } => save(&tx, state, observation, now)?,
                CompleteRun { observation, .. } => {
                    if let Some(observation) = observation {
                        save(&tx, state, observation, now)?;
                    }
                    tx.execute(&format!("UPDATE {kind}_watch_state SET revision=revision+1,active_run=0 WHERE {key}=?1"),[id])?;
                }
                Read(_) => unreachable!(),
            }
            // Retag preserved checkpoints whenever authorization advances.
            tx.execute(&format!("UPDATE {kind}_playback_progress SET revision=(SELECT revision FROM {kind}_watch_state WHERE {key}=?1) WHERE {key}=?1"),[id])?;
        }
        let result = read(&tx, target)?;
        tx.commit()?;
        Ok(PlaybackProgressResult::Applied(result))
    }
}

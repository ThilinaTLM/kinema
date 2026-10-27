//! Local personal data; all operations execute on the catalog owner, without HTTP.
use super::*;
use kinema_core::{
    catalog_normalized::{CanonicalChild, CanonicalTitle, ExternalIdentifier},
    library::*,
};
use rusqlite::Transaction;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum TitleRef {
    Canonical(TitleId),
    External {
        kind: MediaKind,
        identifier: ExternalIdentifier,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum EpisodeRef {
    Canonical(EpisodeId),
    #[allow(
        dead_code,
        reason = "Verified provider-episode lookup is part of the storage protocol; current screens use coordinates"
    )]
    External(ExternalIdentifier),
    Coordinates {
        title: TitleRef,
        season: u32,
        episode: u32,
    },
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersonalTitle {
    pub title_id: TitleId,
    pub kind: MediaKind,
    pub added_at: Option<i64>,
    pub movie_watch: Option<WatchState>,
    pub show_summary: Option<ShowSummary>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PersonalEpisode {
    pub episode_id: EpisodeId,
    pub watch: Option<WatchState>,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LibraryEntry {
    pub metadata: CanonicalTitle,
    pub personal: PersonalTitle,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LibraryPage {
    pub entries: Vec<LibraryEntry>,
    pub total: usize,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalEpisode {
    pub season: u32,
    pub metadata: CanonicalChild,
    pub personal: PersonalEpisode,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalDetail {
    pub metadata: CanonicalTitle,
    pub personal: PersonalTitle,
    pub seasons: Vec<CanonicalChild>,
    pub episodes: Vec<LocalEpisode>,
}
/// Confirmation freezes this exact canonical episode set; later arrivals are excluded.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct SeasonSnapshot {
    pub title_id: TitleId,
    pub season_id: SeasonId,
    pub season: u32,
    pub episodes: Vec<EpisodeId>,
    /// Verified identity anchors, not merely reusable SQLite row IDs.
    pub title_identifiers: Vec<ExternalIdentifier>,
    /// Original episode numbers, parallel to `episodes`.
    pub episode_numbers: Vec<u32>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum LibraryOperation {
    ReadTitles(Vec<TitleRef>),
    ReadEpisodes(Vec<EpisodeRef>),
    List(LibraryQuery),
    Detail(TitleRef),
    SetMembership {
        title: TitleRef,
        saved: bool,
        restore_added_at: Option<i64>,
    },
    SetMovieWatched {
        title: TitleRef,
        watched: bool,
    },
    SetEpisodeWatched {
        episode: EpisodeRef,
        watched: bool,
    },
    SeasonSnapshot {
        title: TitleRef,
        season: u32,
    },
    SetSeasonWatched {
        snapshot: SeasonSnapshot,
        watched: bool,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum LibraryResult {
    /// Positional batch outcomes, including duplicate refs. Only unresolved
    /// identity is per-ref; storage/cache failures fail the operation itself.
    Titles(Vec<Result<PersonalTitle, CatalogError>>),
    Episodes(Vec<Result<PersonalEpisode, CatalogError>>),
    Page(LibraryPage),
    Detail(Box<LocalDetail>),
    Membership {
        personal: PersonalTitle,
        previous_added_at: Option<i64>,
    },
    Movie(PersonalTitle),
    Episode(PersonalEpisode),
    SeasonSnapshot(SeasonSnapshot),
    Season(Vec<PersonalEpisode>),
}

fn title_id(tx: &Transaction<'_>, reference: &TitleRef) -> Result<i64, CatalogError> {
    let found: Option<i64> = match reference {
        TitleRef::Canonical(id) => tx
            .query_row("SELECT id FROM titles WHERE id=?1", [id.get()], |r| {
                r.get(0)
            })
            .optional()?,
        TitleRef::External { kind, identifier } => {
            if !identifier.valid() {
                return Err(CatalogError::MissingIdentityMapping);
            }
            tx.query_row("SELECT title_id FROM title_identifiers WHERE namespace=?1 AND media_kind=?2 AND external_id=?3", params![identifier.namespace, kind_name(*kind), identifier.external_id], |r|r.get(0)).optional()?
        }
    };
    found.ok_or(CatalogError::MissingIdentityMapping)
}
fn kind_name(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Movie => "movie",
        MediaKind::Series => "series",
    }
}
fn episode_id(tx: &Transaction<'_>, reference: &EpisodeRef) -> Result<i64, CatalogError> {
    let found: Option<i64> = match reference {
        EpisodeRef::Canonical(id) => tx.query_row("SELECT id FROM episodes WHERE id=?1", [id.get()], |r|r.get(0)).optional()?,
        EpisodeRef::External(identifier) => {
            if !identifier.valid() { return Err(CatalogError::MissingIdentityMapping); }
            tx.query_row("SELECT episode_id FROM episode_identifiers WHERE namespace=?1 AND external_id=?2", params![identifier.namespace, identifier.external_id], |r|r.get(0)).optional()?
        }
        EpisodeRef::Coordinates {title, season, episode} => tx.query_row("SELECT e.id FROM episodes e JOIN seasons s ON s.id=e.season_id WHERE s.series_id=?1 AND s.season_number=?2 AND e.episode_number=?3", params![title_id(tx,title)?,season,episode], |r|r.get(0)).optional()?,
    };
    found.ok_or(CatalogError::MissingIdentityMapping)
}
fn wall_millis() -> Result<i64, CatalogError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| CatalogError::Clock)?
            .as_millis(),
    )
    .map_err(|_| CatalogError::Clock)
}
fn mutation_time(tx: &Transaction<'_>) -> Result<i64, CatalogError> {
    let last: i64 = tx.query_row("SELECT max(value) FROM (SELECT coalesce(max(updated_at),0) value FROM movie_watch_state UNION ALL SELECT coalesce(max(updated_at),0) FROM episode_watch_state)", [], |r|r.get(0))?;
    next_watch_timestamp(wall_millis()?, last).ok_or(CatalogError::Clock)
}
fn watch(
    tx: &Transaction<'_>,
    table: &str,
    column: &str,
    id: i64,
) -> Result<Option<WatchState>, CatalogError> {
    Ok(tx
        .query_row(
            &format!("SELECT watched,updated_at FROM {table} WHERE {column}=?1"),
            [id],
            |r| {
                Ok(WatchState {
                    watched: r.get(0)?,
                    updated_at: r.get(1)?,
                })
            },
        )
        .optional()?)
}
fn episode_personal(tx: &Transaction<'_>, id: i64) -> Result<PersonalEpisode, CatalogError> {
    Ok(PersonalEpisode {
        episode_id: EpisodeId::new(id).ok_or(CatalogError::InvalidCache)?,
        watch: watch(tx, "episode_watch_state", "episode_id", id)?,
    })
}

#[path = "catalog_library_query.rs"]
mod query;
use query::{detail, listing, personal, snapshot};

impl CatalogService {
    /// Mutations commit before returning. Accepted requests must not be canceled by navigation.
    pub(crate) fn library_operation(
        &mut self,
        operation: LibraryOperation,
    ) -> Result<LibraryResult, CatalogError> {
        let tx = self.connection.transaction()?;
        let result = match operation {
            LibraryOperation::ReadTitles(refs) => {
                if refs.len() > 500 {
                    return Err(CatalogError::InvalidMetadata);
                }
                LibraryResult::Titles(query::batch_titles(&tx, &refs)?)
            }
            LibraryOperation::ReadEpisodes(refs) => {
                if refs.len() > MAX_EPISODES {
                    return Err(CatalogError::InvalidMetadata);
                }
                LibraryResult::Episodes(query::batch_episodes(&tx, &refs)?)
            }
            LibraryOperation::List(query) => LibraryResult::Page(listing(&tx, query)?),
            LibraryOperation::Detail(reference) => {
                LibraryResult::Detail(Box::new(detail(&tx, title_id(&tx, &reference)?)?))
            }
            LibraryOperation::SetMembership {
                title,
                saved,
                restore_added_at,
            } => {
                let id = title_id(&tx, &title)?;
                let previous_added_at = personal(&tx, id)?.added_at;
                if saved {
                    let time = restore_added_at.unwrap_or(wall_millis()?);
                    if time < 0 {
                        return Err(CatalogError::Clock);
                    }
                    tx.execute("INSERT INTO library_entries(title_id,added_at) VALUES (?1,?2) ON CONFLICT(title_id) DO NOTHING",params![id,time])?;
                } else {
                    tx.execute("DELETE FROM library_entries WHERE title_id=?1", [id])?;
                }
                LibraryResult::Membership {
                    personal: personal(&tx, id)?,
                    previous_added_at,
                }
            }
            LibraryOperation::SetMovieWatched { title, watched } => {
                let id = title_id(&tx, &title)?;
                let state = personal(&tx, id)?;
                if state.kind != MediaKind::Movie {
                    return Err(CatalogError::InvalidMetadata);
                }
                let time = mutation_time(&tx)?;
                tx.execute("INSERT INTO movie_watch_state(title_id,watched,updated_at) VALUES (?1,?2,?3) ON CONFLICT(title_id) DO UPDATE SET watched=excluded.watched,updated_at=excluded.updated_at",params![id,watched,time])?;
                LibraryResult::Movie(personal(&tx, id)?)
            }
            LibraryOperation::SetEpisodeWatched { episode, watched } => {
                let id = episode_id(&tx, &episode)?;
                let time = mutation_time(&tx)?;
                write_episode(&tx, id, watched, time)?;
                LibraryResult::Episode(episode_personal(&tx, id)?)
            }
            LibraryOperation::SeasonSnapshot { title, season } => {
                LibraryResult::SeasonSnapshot(snapshot(&tx, title_id(&tx, &title)?, season)?)
            }
            LibraryOperation::SetSeasonWatched { snapshot, watched } => {
                if snapshot.episodes.is_empty() || snapshot.episodes.len() > MAX_EPISODES {
                    return Err(CatalogError::InvalidMetadata);
                }
                if snapshot.title_identifiers.is_empty()
                    || snapshot.episode_numbers.len() != snapshot.episodes.len()
                {
                    return Err(CatalogError::MissingIdentityMapping);
                }
                for identifier in &snapshot.title_identifiers {
                    let resolved = title_id(
                        &tx,
                        &TitleRef::External {
                            kind: MediaKind::Series,
                            identifier: identifier.clone(),
                        },
                    )?;
                    if resolved != snapshot.title_id.get() {
                        return Err(CatalogError::MissingIdentityMapping);
                    }
                }
                // Validate frozen coordinates, not just reusable row IDs. New arrivals
                // (and updated advertised totals) do not expand or invalidate this set.
                let acquired: Option<bool> = tx.query_row("SELECT EXISTS(SELECT 1 FROM metadata_fetches f WHERE f.season_id=s.id AND f.fetched_at IS NOT NULL AND f.completeness='detail' AND f.success_kind IN ('data','empty') AND f.last_error_code IS NULL) FROM seasons s WHERE s.id=?1 AND s.series_id=?2 AND s.season_number=?3",params![snapshot.season_id.get(),snapshot.title_id.get(),snapshot.season],|r|r.get(0)).optional()?;
                let acquired = acquired.ok_or(CatalogError::MissingIdentityMapping)?;
                if !acquired {
                    return Err(CatalogError::InvalidMetadata);
                }
                let mut unique = HashSet::new();
                for (id, number) in snapshot.episodes.iter().zip(&snapshot.episode_numbers) {
                    if !unique.insert(*id) {
                        return Err(CatalogError::InvalidMetadata);
                    }
                    let valid:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM episodes e JOIN seasons s ON s.id=e.season_id WHERE e.id=?1 AND s.id=?2 AND s.series_id=?3 AND s.season_number=?4 AND e.episode_number=?5)",params![id.get(),snapshot.season_id.get(),snapshot.title_id.get(),snapshot.season,number],|r|r.get(0))?;
                    if !valid {
                        return Err(CatalogError::MissingIdentityMapping);
                    }
                }
                let time = mutation_time(&tx)?;
                let mut states = Vec::new();
                for id in snapshot.episodes {
                    write_episode(&tx, id.get(), watched, time)?;
                    states.push(episode_personal(&tx, id.get())?);
                }
                LibraryResult::Season(states)
            }
        };
        tx.commit()?;
        Ok(result)
    }
}
fn write_episode(
    tx: &Transaction<'_>,
    id: i64,
    watched: bool,
    time: i64,
) -> Result<(), CatalogError> {
    tx.execute("INSERT INTO episode_watch_state(episode_id,watched,updated_at) VALUES (?1,?2,?3) ON CONFLICT(episode_id) DO UPDATE SET watched=excluded.watched,updated_at=excluded.updated_at",params![id,watched,time])?;
    Ok(())
}

#[cfg(test)]
#[path = "catalog_library_tests.rs"]
mod tests;

use super::*;
use std::collections::HashMap;

// One SQL summary pass for all requested cards; no per-card watch/count queries.
const PERSONAL: &str = r#"
WITH season_stats AS (
 SELECT s.series_id, count(*) seasons, sum(s.season_number<>0) regular_seasons,
 sum(coalesce(s.episode_count,0)) advertised,
 sum(CASE WHEN s.season_number<>0 THEN coalesce(s.episode_count,0) ELSE 0 END) regular_advertised,
 sum(CASE WHEN s.episode_count IS NULL OR s.episode_count<>(SELECT count(*) FROM episodes e WHERE e.season_id=s.id)
 OR NOT EXISTS(SELECT 1 FROM metadata_fetches f WHERE f.season_id=s.id AND f.fetched_at IS NOT NULL AND f.completeness='detail' AND f.success_kind IN ('data','empty') AND f.last_error_code IS NULL) THEN 1 ELSE 0 END) invalid
 FROM seasons s GROUP BY s.series_id
), episode_stats AS (
 SELECT s.series_id,count(*) total,sum(coalesce(w.watched,0)) watched,
 max(CASE WHEN coalesce(w.watched,0)=0 AND c.position_ms>0 THEN 1 ELSE 0 END) has_resume
 FROM episodes e JOIN seasons s ON s.id=e.season_id LEFT JOIN episode_watch_state w ON w.episode_id=e.id LEFT JOIN episode_playback_progress c ON c.episode_id=e.id GROUP BY s.series_id
)
SELECT t.id,t.kind,l.added_at,m.watched,m.updated_at,coalesce(e.watched,0),coalesce(e.total,0),
 CASE WHEN t.episode_count IS NOT NULL AND t.season_count IS NOT NULL
 AND (t.episode_count=coalesce(e.total,0) OR t.episode_count=ss.regular_advertised)
 AND t.season_count=ss.regular_seasons AND ss.advertised=e.total AND ss.invalid=0
 AND EXISTS(SELECT 1 FROM metadata_fetches f WHERE f.title_id=t.id AND f.fetched_at IS NOT NULL AND f.completeness='detail' AND f.success_kind='data' AND f.last_error_code IS NULL)
 THEN 1 ELSE 0 END,
 CASE WHEN t.kind='movie' THEN coalesce(m.watched,0)=0 AND EXISTS(SELECT 1 FROM movie_playback_progress c WHERE c.title_id=t.id AND c.position_ms>0) ELSE coalesce(e.has_resume,0) END
 FROM titles t LEFT JOIN library_entries l ON l.title_id=t.id LEFT JOIN movie_watch_state m ON m.title_id=t.id
 LEFT JOIN season_stats ss ON ss.series_id=t.id LEFT JOIN episode_stats e ON e.series_id=t.id
"#;
fn personal_rows(tx: &Transaction<'_>, ids: &[i64]) -> Result<Vec<PersonalTitle>, CatalogError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; ids.len()].join(",");
    let mut stmt = tx.prepare(&format!("{PERSONAL} WHERE t.id IN ({placeholders})"))?;
    let rows = stmt
        .query_map(rusqlite::params_from_iter(ids.iter()), |r| {
            let id: i64 = r.get(0)?;
            let kind: String = r.get(1)?;
            let watched: Option<bool> = r.get(3)?;
            let time: Option<i64> = r.get(4)?;
            Ok((
                id,
                kind,
                r.get::<_, Option<i64>>(2)?,
                watched.zip(time),
                r.get::<_, u32>(5)? as usize,
                r.get::<_, u32>(6)? as usize,
                r.get::<_, bool>(7)?,
                r.get::<_, bool>(8)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let progress = playback_progress::read_many(tx, true, ids)?;
    rows.into_iter()
        .map(
            |(id, kind, added_at, watch, watched, total, complete, has_resume)| {
                let kind = match kind.as_str() {
                    "movie" => MediaKind::Movie,
                    "series" => MediaKind::Series,
                    _ => return Err(CatalogError::InvalidCache),
                };
                Ok(PersonalTitle {
                    title_id: TitleId::new(id).ok_or(CatalogError::InvalidCache)?,
                    kind,
                    added_at,
                    has_resume,
                    playback_progress: progress.get(&id).copied(),
                    movie_watch: watch.map(|(watched, updated_at)| WatchState {
                        watched,
                        updated_at,
                    }),
                    show_summary: (kind == MediaKind::Series).then_some(ShowSummary {
                        watched,
                        total,
                        complete,
                    }),
                })
            },
        )
        .collect()
}
pub(super) fn personal(tx: &Transaction<'_>, id: i64) -> Result<PersonalTitle, CatalogError> {
    personal_rows(tx, &[id])?
        .pop()
        .ok_or(CatalogError::MissingIdentityMapping)
}
pub(super) fn batch_titles(
    tx: &Transaction<'_>,
    refs: &[TitleRef],
) -> Result<Vec<Result<PersonalTitle, CatalogError>>, CatalogError> {
    let mut output = Vec::with_capacity(refs.len());
    for chunk in refs.chunks(250) {
        output.extend(batch_titles_chunk(tx, chunk)?);
    }
    Ok(output)
}
fn batch_titles_chunk(
    tx: &Transaction<'_>,
    refs: &[TitleRef],
) -> Result<Vec<Result<PersonalTitle, CatalogError>>, CatalogError> {
    // Resolve identifiers in one bounded SQL query rather than issuing one per card.
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut values = Vec::<rusqlite::types::Value>::new();
    let mut clauses = Vec::new();
    for reference in refs {
        match reference {
            TitleRef::Canonical(id) => {
                clauses.push("(t.id=?)".to_owned());
                values.push(id.get().into());
            }
            TitleRef::External { kind, identifier } => {
                if !identifier.valid() {
                    clauses.push("0".to_owned());
                    continue;
                }
                clauses.push("(i.media_kind=? AND i.namespace=? AND i.external_id=?)".to_owned());
                values.extend([
                    kind_name(*kind).to_owned().into(),
                    identifier.namespace.clone().into(),
                    identifier.external_id.clone().into(),
                ]);
            }
        }
    }
    let mut stmt=tx.prepare(&format!("SELECT DISTINCT t.id FROM titles t LEFT JOIN title_identifiers i ON i.title_id=t.id WHERE {}",clauses.join(" OR ")))?;
    let ids = stmt
        .query_map(rusqlite::params_from_iter(values), |r| r.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    let by_id: HashMap<_, _> = personal_rows(tx, &ids)?
        .into_iter()
        .map(|p| (p.title_id.get(), p))
        .collect();
    // Recover input ordering, including duplicate refs, without another SQL pass.
    let mut mappings = HashMap::new();
    let mut stmt=tx.prepare(&format!("SELECT title_id,media_kind,namespace,external_id FROM title_identifiers WHERE title_id IN ({})",vec!["?";ids.len()].join(",")))?;
    for row in stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (id, kind, ns, external) = row?;
        if by_id.contains_key(&id) {
            mappings.insert((kind, ns, external), id);
        }
    }
    Ok(refs
        .iter()
        .map(|reference| {
            if matches!(reference, TitleRef::External { identifier, .. } if !identifier.valid()) {
                return Err(CatalogError::MissingIdentityMapping);
            }
            let id = match reference {
                TitleRef::Canonical(id) => Some(id.get()),
                TitleRef::External { kind, identifier } => mappings
                    .get(&(
                        kind_name(*kind).into(),
                        identifier.namespace.clone(),
                        identifier.external_id.clone(),
                    ))
                    .copied(),
            }
            .ok_or(CatalogError::MissingIdentityMapping)?;
            by_id
                .get(&id)
                .cloned()
                .ok_or(CatalogError::MissingIdentityMapping)
        })
        .collect())
}
pub(super) fn batch_episodes(
    tx: &Transaction<'_>,
    refs: &[EpisodeRef],
) -> Result<Vec<Result<PersonalEpisode, CatalogError>>, CatalogError> {
    if refs.is_empty() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::with_capacity(refs.len());
    for chunk in refs.chunks(150) {
        let mut selects = Vec::new();
        let mut values = Vec::<rusqlite::types::Value>::new();
        for (index, reference) in chunk.iter().enumerate() {
            let invalid = match reference {
                EpisodeRef::External(identifier)
                | EpisodeRef::Coordinates {
                    title: TitleRef::External { identifier, .. },
                    ..
                } => !identifier.valid(),
                _ => false,
            };
            if invalid {
                selects.push(format!("SELECT {index} ordinal,NULL id"));
                continue;
            }
            let select = match reference {
                EpisodeRef::Canonical(id) => {
                    values.push(id.get().into());
                    "SELECT id FROM episodes WHERE id=?".to_owned()
                }
                EpisodeRef::External(identifier) => {
                    values.extend([
                        identifier.namespace.clone().into(),
                        identifier.external_id.clone().into(),
                    ]);
                    "SELECT episode_id id FROM episode_identifiers WHERE namespace=? AND external_id=?".to_owned()
                }
                EpisodeRef::Coordinates {
                    title,
                    season,
                    episode,
                } => {
                    let predicate = match title {
                        TitleRef::Canonical(id) => {
                            values.push(id.get().into());
                            "s.series_id=?"
                        }
                        TitleRef::External { kind, identifier } => {
                            values.extend([
                                kind_name(*kind).to_owned().into(),
                                identifier.namespace.clone().into(),
                                identifier.external_id.clone().into(),
                            ]);
                            "s.series_id=(SELECT title_id FROM title_identifiers WHERE media_kind=? AND namespace=? AND external_id=?)"
                        }
                    };
                    values.extend([i64::from(*season).into(), i64::from(*episode).into()]);
                    format!(
                        "SELECT e.id FROM episodes e JOIN seasons s ON s.id=e.season_id WHERE {predicate} AND s.season_number=? AND e.episode_number=?"
                    )
                }
            };
            selects.push(format!("SELECT {index} ordinal,({select}) id"));
        }
        // At most 750 parameters, below the portable 999-variable limit.
        let mut stmt = tx.prepare(&format!("{} ORDER BY ordinal", selects.join(" UNION ALL ")))?;
        let resolved = stmt
            .query_map(rusqlite::params_from_iter(values), |r| {
                r.get::<_, Option<i64>>(1)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        for id in resolved {
            ids.push(id);
        }
    }
    let mut states = HashMap::new();
    // Chunk to remain beneath SQLite's portable bind-variable limit.
    let resolved_ids = ids.iter().flatten().copied().collect::<Vec<_>>();
    for chunk in resolved_ids.chunks(500) {
        let progress = playback_progress::read_many(tx, false, chunk)?;
        let mut stmt=tx.prepare(&format!("SELECT e.id,w.watched,w.updated_at FROM episodes e LEFT JOIN episode_watch_state w ON w.episode_id=e.id WHERE e.id IN ({})",vec!["?";chunk.len()].join(",")))?;
        for row in stmt.query_map(rusqlite::params_from_iter(chunk), |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, Option<bool>>(1)?
                    .zip(r.get::<_, Option<i64>>(2)?),
            ))
        })? {
            let (id, watch) = row?;
            states.insert(
                id,
                PersonalEpisode {
                    episode_id: EpisodeId::new(id).ok_or(CatalogError::InvalidCache)?,
                    playback_progress: *progress.get(&id).ok_or(CatalogError::InvalidCache)?,
                    watch: watch.map(|(watched, updated_at)| WatchState {
                        watched,
                        updated_at,
                    }),
                },
            );
        }
    }
    Ok(ids
        .iter()
        .map(|id| {
            id.and_then(|id| states.get(&id).cloned())
                .ok_or(CatalogError::MissingIdentityMapping)
        })
        .collect())
}
pub(super) fn listing(
    tx: &Transaction<'_>,
    query: LibraryQuery,
) -> Result<LibraryPage, CatalogError> {
    // SQLite lower is ASCII-only: include non-ASCII candidates and finish Unicode
    // lowercase matching locally, still before pagination. No LIKE wildcards.
    let needle = query.search.to_lowercase();
    let mut stmt=tx.prepare("SELECT t.id,coalesce(t.name,''),coalesce(t.release_year,t.first_air_year,0) FROM library_entries l JOIN titles t ON t.id=l.title_id WHERE instr(lower(coalesce(t.name,'')),lower(?1))>0 OR t.name GLOB '*[^ -~]*'")?;
    let candidates = stmt
        .query_map([&query.search], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut states = HashMap::new();
    for chunk in candidates.chunks(500) {
        let ids = chunk.iter().map(|v| v.0).collect::<Vec<_>>();
        for state in personal_rows(tx, &ids)? {
            states.insert(state.title_id.get(), state);
        }
    }
    let mut filtered = candidates
        .into_iter()
        .filter(|(id, name, _)| {
            let state = &states[id];
            let status = if state.kind == MediaKind::Movie {
                Some(if state.movie_watch.is_some_and(|w| w.watched) {
                    WatchFilter::Watched
                } else if state.has_resume {
                    WatchFilter::InProgress
                } else {
                    WatchFilter::Unwatched
                })
            } else if state.has_resume {
                Some(WatchFilter::InProgress)
            } else {
                state.show_summary.and_then(ShowSummary::status)
            };
            name.to_lowercase().contains(&needle)
                && query.kind.accepts(state.kind)
                && (query.status == WatchFilter::Any || status == Some(query.status))
        })
        .collect::<Vec<_>>();
    filtered.sort_by(|a, b| match query.sort {
        LibrarySort::RecentlyAdded => states[&b.0]
            .added_at
            .cmp(&states[&a.0].added_at)
            .then(a.0.cmp(&b.0)),
        LibrarySort::Title => {
            a.1.to_lowercase()
                .cmp(&b.1.to_lowercase())
                .then(a.0.cmp(&b.0))
        }
        LibrarySort::NewestRelease => b.2.cmp(&a.2).then(a.0.cmp(&b.0)),
    });
    let total = filtered.len();
    let entries = filtered
        .into_iter()
        .skip(query.offset)
        .take(query.limit.clamp(1, 500))
        .map(|(id, _, _)| {
            Ok(LibraryEntry {
                metadata: CatalogService::title_record(tx, id)?,
                personal: states.remove(&id).ok_or(CatalogError::InvalidCache)?,
            })
        })
        .collect::<Result<_, CatalogError>>()?;
    Ok(LibraryPage { entries, total })
}
pub(super) fn detail(tx: &Transaction<'_>, id: i64) -> Result<LocalDetail, CatalogError> {
    let metadata = CatalogService::title_record(tx, id)?;
    let personal = personal(tx, id)?;
    let seasons = CatalogService::season_records(tx, id)?;
    let records = CatalogService::episode_records(tx, id, None)?;
    let refs = records
        .iter()
        .map(|(_, e)| {
            EpisodeId::new(e.id)
                .map(EpisodeRef::Canonical)
                .ok_or(CatalogError::InvalidCache)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let states = batch_episodes(tx, &refs)?
        .into_iter()
        .collect::<Result<Vec<_>, CatalogError>>()?;
    let episodes = records
        .into_iter()
        .zip(states)
        .map(|((season, metadata), personal)| LocalEpisode {
            season,
            metadata,
            personal,
        })
        .collect();
    Ok(LocalDetail {
        metadata,
        personal,
        seasons,
        episodes,
    })
}
pub(super) fn snapshot(
    tx: &Transaction<'_>,
    id: i64,
    season: u32,
) -> Result<SeasonSnapshot, CatalogError> {
    let found:Option<(i64,Option<u32>,bool)>=tx.query_row("SELECT s.id,s.episode_count,EXISTS(SELECT 1 FROM metadata_fetches f WHERE f.season_id=s.id AND f.fetched_at IS NOT NULL AND f.completeness='detail' AND f.success_kind IN ('data','empty') AND f.last_error_code IS NULL) FROM seasons s WHERE s.series_id=?1 AND s.season_number=?2",params![id,season],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional()?;
    let (season_id, count, evidence) = found.ok_or(CatalogError::MissingIdentityMapping)?;
    let mut stmt = tx.prepare("SELECT namespace,external_id FROM title_identifiers WHERE title_id=?1 AND media_kind='series' ORDER BY namespace")?;
    let title_identifiers = stmt
        .query_map([id], |r| {
            Ok(ExternalIdentifier {
                namespace: r.get(0)?,
                external_id: r.get(1)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    if title_identifiers.is_empty()
        || title_identifiers
            .iter()
            .any(|identifier| !identifier.valid())
    {
        return Err(CatalogError::MissingIdentityMapping);
    }
    let mut stmt = tx.prepare(
        "SELECT id,episode_number FROM episodes WHERE season_id=?1 ORDER BY episode_number,id",
    )?;
    let pairs = stmt
        .query_map([season_id], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, u32>(1)?))
        })?
        .map(|r| {
            let (id, number) = r?;
            Ok((
                EpisodeId::new(id).ok_or(CatalogError::InvalidCache)?,
                number,
            ))
        })
        .collect::<Result<Vec<_>, CatalogError>>()?;
    let (episodes, episode_numbers): (Vec<_>, Vec<_>) = pairs.into_iter().unzip();
    if !evidence || count.map(|n| n as usize) != Some(episodes.len()) || episodes.is_empty() {
        return Err(CatalogError::InvalidMetadata);
    }
    Ok(SeasonSnapshot {
        title_id: TitleId::new(id).ok_or(CatalogError::InvalidCache)?,
        season_id: SeasonId::new(season_id).ok_or(CatalogError::InvalidCache)?,
        season,
        episodes,
        title_identifiers,
        episode_numbers,
    })
}

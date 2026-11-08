use super::*;
use crate::catalog::stream_memory::{ReleaseMemory, StreamMemory};
use kinema_core::episode_continuity::SeasonListing;

const SESSION: &str = "s1";

fn series() -> ImdbId {
    ImdbId::new("tt0000001").unwrap_or_else(|e| panic!("fixture id: {e}"))
}
fn episode(season: u32, episode: u32) -> PlaybackTarget {
    PlaybackTarget::Episode {
        series: series(),
        season,
        episode,
    }
}
fn hash(n: u32) -> InfoHash {
    InfoHash::new(format!("{n:040x}")).unwrap_or_else(|e| panic!("fixture hash: {e}"))
}
fn candidate(n: u32, name: &str, index: Option<u32>, file: Option<&str>) -> SourceCandidate {
    SourceCandidate {
        torrent: TorrentSource {
            hash: hash(n),
            file_index: index,
            filename: file.map(str::to_owned),
        },
        name: name.into(),
        quality: Some("1080p".into()),
        size: Some(1_000),
        seeders: Some(5),
    }
}
fn listing() -> Listing {
    Listing {
        seasons: vec![
            SeasonListing {
                number: 1,
                advertised: Some(3),
                episodes: vec![1, 2, 3],
            },
            SeasonListing {
                number: 2,
                advertised: Some(2),
                episodes: vec![],
            },
        ],
        names: HashMap::from([((1, 3), "Third".to_owned())]),
    }
}
fn played(n: u32, name: &str) -> Played {
    Played {
        reference: ReferenceRelease {
            name: name.into(),
            file_name: None,
            size: Some(1_000),
        },
        hash: hash(n),
    }
}
fn pack(markers: bool) -> Kept {
    let files: Vec<RemoteFile> = (1..=3)
        .map(|n| RemoteFile {
            id: format!("f{n}"),
            path: if markers {
                format!("Show.S01E0{n}.mkv")
            } else {
                format!("Show/0{n}.mkv")
            },
            size: 10,
        })
        .collect();
    Kept {
        provider: DebridKind::RealDebrid,
        files: files.into(),
        hash: hash(1),
    }
}

/// Opens a window on S1E2 with its listing loaded.
fn open(resolver: &mut Resolver) {
    let effects = resolver.subject_changed(SESSION, Some(&episode(1, 2)));
    let token = effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a listing read: {effects:?}"));
    resolver.listing_loaded(token, Some(listing()));
}
fn publishes(effects: &[Effect]) -> Vec<Dock> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Publish { dock, .. } => Some(*dock),
            _ => None,
        })
        .collect()
}
fn outcomes(effects: &[Effect]) -> Vec<&Outcome> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::Outcome(outcome) => Some(outcome),
            _ => None,
        })
        .collect()
}
fn indexer_token(effects: &[Effect]) -> Token {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::QueryIndexer { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("an indexer query: {effects:?}"))
}
fn dock(previous: NeighbourState, next: NeighbourState) -> Dock {
    Dock { previous, next }
}

// ── neighbour state ──

#[test]
fn episodes_publish_enabled_and_disabled_from_the_listing() {
    let mut resolver = Resolver::default();
    let effects = resolver.subject_changed(SESSION, Some(&episode(1, 2)));
    // No listing yet: nothing is published, so the dock keeps its hidden default.
    assert!(publishes(&effects).is_empty());
    let token = effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("listing read"));
    let effects = resolver.listing_loaded(token, Some(listing()));
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Enabled)]
    );
    // The first episode has no previous; the last has no next.
    let first = resolver.subject_changed("s2", Some(&episode(1, 1)));
    assert_eq!(
        publishes(&first),
        vec![dock(NeighbourState::Disabled, NeighbourState::Enabled)],
        "a cached listing publishes at once"
    );
    let last = resolver.subject_changed("s3", Some(&episode(2, 2)));
    assert_eq!(
        publishes(&last),
        vec![dock(NeighbourState::Enabled, NeighbourState::Disabled)]
    );
}

#[test]
fn movies_and_untracked_files_are_hidden() {
    let mut resolver = Resolver::default();
    let movie = PlaybackTarget::Movie(series());
    for target in [Some(&movie), None] {
        let effects = resolver.subject_changed(SESSION, target);
        assert_eq!(publishes(&effects), vec![Dock::default()]);
        assert!(
            !effects
                .iter()
                .any(|e| matches!(e, Effect::ReadListing { .. }))
        );
    }
}

#[test]
fn unknown_or_failed_listings_never_invent_a_neighbour() {
    let mut resolver = Resolver::default();
    let effects = resolver.subject_changed(SESSION, Some(&episode(1, 2)));
    let token = effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("listing read"));
    // A failed read publishes nothing.
    assert!(resolver.listing_loaded(token, None).is_empty());
    // A listing that lacks the current season: Disabled both ways, not Enabled.
    let mut resolver = Resolver::default();
    let effects = resolver.subject_changed(SESSION, Some(&episode(5, 1)));
    let token = effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("listing read"));
    let effects = resolver.listing_loaded(token, Some(listing()));
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Disabled, NeighbourState::Disabled)]
    );
}

#[test]
fn a_changed_listing_republishes_only_when_the_neighbours_change() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    let effects = resolver.listing_changed(&series());
    let token = match effects.as_slice() {
        [Effect::ReadListing { token, .. }] => *token,
        other => panic!("one re-read: {other:?}"),
    };
    // Same answer: no publish.
    assert!(
        publishes(&resolver.listing_loaded(token, Some(listing()))).is_empty(),
        "unchanged neighbours are not republished"
    );
    let effects = resolver.listing_changed(&series());
    let token = match effects.as_slice() {
        [Effect::ReadListing { token, .. }] => *token,
        other => panic!("one re-read: {other:?}"),
    };
    let mut shorter = listing();
    shorter.seasons[0].episodes = vec![1, 2];
    shorter.seasons[0].advertised = Some(2);
    shorter.seasons.truncate(1);
    let effects = resolver.listing_loaded(token, Some(shorter));
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Disabled)]
    );
}

#[test]
fn a_late_listing_for_a_replaced_window_is_ignored() {
    let mut resolver = Resolver::default();
    let first = resolver.subject_changed(SESSION, Some(&episode(1, 2)));
    let stale = first
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("listing read"));
    // The same session now plays another episode (a swap re-keys, but a new
    // subject on the same id is the same case).
    resolver.subject_changed(SESSION, Some(&episode(1, 3)));
    assert!(resolver.listing_loaded(stale, Some(listing())).is_empty());
    resolver.close(SESSION);
    assert!(resolver.listing_loaded(stale, Some(listing())).is_empty());
}

// ── requests ──

#[test]
fn a_request_goes_busy_and_queries_the_indexer_for_the_neighbour() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p.WEB-DL.x265-GRP"));
    let effects = resolver.request(
        SESSION,
        &episode(1, 2),
        NeighbourDirection::Next,
        Some(pack(true)),
    );
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Busy)]
    );
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::QueryIndexer { target, .. } if *target == episode(1, 3)
    )));
    // The window's own stream is the reference: no memory read.
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::ReadMemory { .. }))
    );
}

#[test]
fn a_second_request_while_busy_is_ignored() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    assert_eq!(resolver.job_count(), 1);
    let again = resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Previous, None);
    // Only the dock is corrected (Previous stays Enabled, Next stays Busy).
    assert_eq!(
        again,
        vec![Effect::Publish {
            session: SESSION.into(),
            dock: dock(NeighbourState::Enabled, NeighbourState::Busy)
        }]
    );
    assert_eq!(resolver.job_count(), 1, "no second job");
}

#[test]
fn a_direction_without_a_neighbour_is_not_requested() {
    let mut resolver = Resolver::default();
    let effects = resolver.subject_changed(SESSION, Some(&episode(1, 1)));
    let token = effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadListing { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("listing read"));
    resolver.listing_loaded(token, Some(listing()));
    let effects = resolver.request(SESSION, &episode(1, 1), NeighbourDirection::Previous, None);
    assert_eq!(resolver.job_count(), 0);
    assert!(outcomes(&effects).is_empty());
}

#[test]
fn the_reference_comes_from_memory_when_the_window_has_no_stream() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    let effects = resolver.request(
        SESSION,
        &episode(1, 2),
        NeighbourDirection::Next,
        Some(pack(true)),
    );
    let token = indexer_token(&effects);
    assert!(effects.iter().any(|e| matches!(
        e,
        Effect::ReadMemory {
            read: MemoryRead::Episode {
                season: 1,
                episode: 3,
                ..
            },
            ..
        }
    )));
    // No season record: the previous season is read next.
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: None,
            season: None,
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::ReadMemory {
            read: MemoryRead::PreviousSeason { season: 1, .. },
            ..
        }]
    ));
    // The indexer answers first; the decision waits for the memory.
    let waiting = resolver.indexer_result(
        token,
        Ok(vec![candidate(
            9,
            "Show.S01E03.1080p.WEB-DL.x265-GRP",
            None,
            None,
        )]),
    );
    assert!(outcomes(&waiting).is_empty());
    let remembered = RememberedRelease {
        release: release(7, "Show.S01.1080p.WEB-DL.x265-GRP"),
        updated_at: 1,
    };
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::PreviousSeason(Some((1, remembered)))),
    );
    // Same group and resolution as the remembered release: a strong match.
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision:
                    Decision::Prepare {
                        candidate, label, ..
                    },
                ..
            },
        ] => {
            assert_eq!(candidate.torrent.hash, hash(9));
            assert_eq!(label, "Same release group · 1080p WEB-DL");
        }
        other => panic!("a strong match: {other:?}"),
    }
}

fn release(n: u32, name: &str) -> ReleaseMemory {
    ReleaseMemory {
        info_hash: hash(n),
        name: name.into(),
        quality: Some("1080p".into()),
        size: Some(1_000),
        file_index: None,
        filename: None,
    }
}

// ── each NeighbourChoice ──

/// Runs a Next request on S1E2 with the given indexer answer.
fn resolve(
    indexer: Result<Vec<SourceCandidate>, IndexerFailed>,
    kept: Option<Kept>,
    played_name: &str,
) -> (Resolver, Vec<Effect>) {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, played_name));
    let effects = resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, kept);
    let token = indexer_token(&effects);
    let effects = resolver.indexer_result(token, indexer);
    (resolver, effects)
}

#[test]
fn same_pack_plays_the_kept_file() {
    let row = candidate(1, "Show.S01.1080p-GRP", Some(3), Some("Show/03.mkv"));
    let (_, effects) = resolve(Ok(vec![row]), Some(pack(false)), "Show.S01.1080p-GRP");
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                target,
                decision:
                    Decision::PlayKept {
                        file_index,
                        label,
                        candidate,
                    },
                origin: Origin::Window { direction, .. },
            },
        ] => {
            assert_eq!(*target, episode(1, 3));
            assert_eq!(*file_index, 2);
            assert_eq!(label, "Same pack");
            assert_eq!(candidate.torrent.hash, hash(1));
            assert_eq!(*direction, NeighbourDirection::Next);
        }
        other => panic!("same pack: {other:?}"),
    }
}

#[test]
fn an_indexer_failure_falls_back_to_the_kept_files_marker() {
    let (_, effects) = resolve(Err(IndexerFailed), Some(pack(true)), "Show.S01.1080p-GRP");
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision:
                    Decision::PlayKept {
                        file_index,
                        label,
                        candidate,
                    },
                ..
            },
        ] => {
            assert_eq!(*file_index, 2);
            assert_eq!(label, "Same pack · by episode number");
            // Enough to record what played, from the kept pack alone.
            assert_eq!(candidate.torrent.hash, hash(1));
            assert_eq!(candidate.size, Some(10));
        }
        other => panic!("kept file: {other:?}"),
    }
}

#[test]
fn an_indexer_failure_without_a_unique_file_is_unavailable_and_distinct_from_empty() {
    let (mut failed, effects) = resolve(Err(IndexerFailed), Some(pack(false)), "Show.S01-GRP");
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision: Decision::Unavailable { label },
                ..
            },
        ] => assert_eq!(label, "Couldn't find a stream for this episode"),
        other => panic!("unavailable: {other:?}"),
    }
    // The dock is usable again at once.
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Enabled)]
    );
    // And the request can be made again.
    let again = failed.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    assert!(
        again
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. }))
    );
    // An empty answer is a different reason.
    let (_, effects) = resolve(Ok(Vec::new()), Some(pack(false)), "Show.S01-GRP");
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision: Decision::Unavailable { label },
                ..
            },
        ] => assert_eq!(label, "No streams found for this episode"),
        other => panic!("unavailable: {other:?}"),
    }
}

#[test]
fn a_strong_match_is_prepared_with_a_picker_to_fall_back_on() {
    let other = candidate(
        9,
        "Show.S01E03.1080p.WEB-DL.x265-GRP",
        Some(1),
        Some("Show.S01E03.mkv"),
    );
    let (_, effects) = resolve(
        Ok(vec![other.clone()]),
        Some(pack(true)),
        "Show.S01.1080p.WEB-DL.x265-GRP",
    );
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision:
                    Decision::Prepare {
                        candidate,
                        label,
                        picker,
                    },
                ..
            },
        ] => {
            assert_eq!(candidate, &other);
            assert_eq!(label, "Same release group · 1080p WEB-DL");
            assert_eq!(picker.reference.name, "Show.S01.1080p.WEB-DL.x265-GRP");
            assert_eq!(picker.exclude, Some(hash(1)));
        }
        other => panic!("strong match: {other:?}"),
    }
}

#[test]
fn no_strong_match_opens_the_picker_with_the_reference() {
    let weak = candidate(9, "Show.S01E03.720p.HDTV.x264-OTHER", None, None);
    let (_, effects) = resolve(Ok(vec![weak]), None, "Show.S01.1080p.WEB-DL.x265-GRP");
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision: Decision::Picker(spec),
                ..
            },
        ] => {
            assert_eq!(spec.label, "No close match — pick a stream");
            assert_eq!(spec.reference.name, "Show.S01.1080p.WEB-DL.x265-GRP");
        }
        other => panic!("picker: {other:?}"),
    }
    // A picker keeps the dock busy: the coordinator settles it on dismissal.
    assert!(
        publishes(&effects).is_empty(),
        "busy stays until the picker is dismissed"
    );
}

#[test]
fn settle_returns_the_dock_to_enabled_and_drops_the_work() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01-GRP"));
    resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    let effects = resolver.settle(SESSION);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CancelIndexer { target, .. } if *target == episode(1, 3)))
    );
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Enabled)]
    );
    assert_eq!(resolver.job_count(), 0);
}

// ── stale and late results ──

#[test]
fn late_results_after_close_swap_or_settle_are_ignored() {
    for end in 0..3 {
        let mut resolver = Resolver::default();
        open(&mut resolver);
        resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
        let effects = resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
        let token = indexer_token(&effects);
        match end {
            0 => {
                resolver.close(SESSION);
            }
            1 => {
                resolver.subject_changed(SESSION, Some(&episode(1, 3)));
            }
            _ => {
                resolver.settle(SESSION);
            }
        }
        let late = resolver.indexer_result(
            token,
            Ok(vec![candidate(9, "Show.S01E03.1080p-GRP", None, None)]),
        );
        assert!(late.is_empty(), "end {end}: {late:?}");
        assert!(resolver.memory_result(token, None).is_empty());
    }
}

#[test]
fn an_unknown_token_and_a_repeated_answer_do_nothing() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01-GRP"));
    let effects = resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    let token = indexer_token(&effects);
    assert!(resolver.indexer_result(token + 100, Ok(vec![])).is_empty());
    let first = resolver.indexer_result(token, Ok(vec![]));
    assert!(!first.is_empty());
    assert!(resolver.indexer_result(token, Ok(vec![])).is_empty());
}

// ── prefetch ──

fn start_prefetch(resolver: &mut Resolver, kept: Option<Kept>) -> Vec<Effect> {
    let early = PlaybackObservation::new(700, Some(1_000)).unwrap_or_else(|| panic!("obs"));
    assert!(resolver.progress(SESSION, early, kept.clone()).is_empty());
    let due = PlaybackObservation::new(800, Some(1_000)).unwrap_or_else(|| panic!("obs"));
    resolver.progress(SESSION, due, kept)
}

#[test]
fn prefetch_starts_at_eighty_percent_once_and_only_resolves() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    let effects = start_prefetch(&mut resolver, Some(pack(false)));
    // Only reads: an indexer query for the next episode, and no dock change.
    assert!(
        effects
            .iter()
            .all(|e| matches!(e, Effect::QueryIndexer { .. } | Effect::ReadMemory { .. })),
        "{effects:?}"
    );
    assert!(publishes(&effects).is_empty());
    let token = indexer_token(&effects);
    // A second tick does not start a second prefetch.
    let due = PlaybackObservation::new(900, Some(1_000)).unwrap_or_else(|| panic!("obs"));
    assert!(resolver.progress(SESSION, due, None).is_empty());
    // An unknown duration never prefetches.
    let unknown = PlaybackObservation::new(900, None).unwrap_or_else(|| panic!("obs"));
    assert!(resolver.progress(SESSION, unknown, None).is_empty());
    // The answer is cached and produces no outcome at all.
    let row = candidate(1, "Show.S01.1080p-GRP", Some(3), Some("Show/03.mkv"));
    let effects = resolver.indexer_result(token, Ok(vec![row]));
    assert!(effects.is_empty(), "prefetch never acts: {effects:?}");
    assert!(matches!(
        resolver.prefetched(SESSION),
        Some(Decision::PlayKept { file_index: 2, .. })
    ));
}

#[test]
fn a_request_after_prefetch_uses_the_cache_without_another_query() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    let effects = start_prefetch(&mut resolver, Some(pack(false)));
    let token = indexer_token(&effects);
    let row = candidate(1, "Show.S01.1080p-GRP", Some(3), Some("Show/03.mkv"));
    resolver.indexer_result(token, Ok(vec![row]));
    let effects = resolver.request(
        SESSION,
        &episode(1, 2),
        NeighbourDirection::Next,
        Some(pack(false)),
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. } | Effect::ReadMemory { .. }))
    );
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::PlayKept { file_index: 2, .. },
            ..
        }]
    ));
    // Previous is never prefetched: it is resolved live.
    let mut other = Resolver::default();
    open(&mut other);
    other.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    let effects = other.request(SESSION, &episode(1, 2), NeighbourDirection::Previous, None);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. }))
    );
}

#[test]
fn a_request_while_prefetching_adopts_the_running_query() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    let effects = start_prefetch(&mut resolver, Some(pack(false)));
    let token = indexer_token(&effects);
    let effects = resolver.request(
        SESSION,
        &episode(1, 2),
        NeighbourDirection::Next,
        Some(pack(false)),
    );
    // Busy at once, and no second query.
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Busy)]
    );
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. }))
    );
    let row = candidate(1, "Show.S01.1080p-GRP", Some(3), Some("Show/03.mkv"));
    let effects = resolver.indexer_result(token, Ok(vec![row]));
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::PlayKept { .. },
            ..
        }]
    ));
}

#[test]
fn the_prefetch_cache_is_invalidated_by_swap_close_account_and_listing_changes() {
    let cached = |resolver: &mut Resolver| {
        open(resolver);
        resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
        let effects = start_prefetch(resolver, Some(pack(false)));
        let token = indexer_token(&effects);
        let row = candidate(1, "Show.S01.1080p-GRP", Some(3), Some("Show/03.mkv"));
        resolver.indexer_result(token, Ok(vec![row]));
        assert!(resolver.prefetched(SESSION).is_some());
    };
    // A swap (subject change) drops it.
    let mut resolver = Resolver::default();
    cached(&mut resolver);
    resolver.subject_changed(SESSION, Some(&episode(1, 3)));
    assert!(resolver.prefetched(SESSION).is_none());
    // Close drops it.
    let mut resolver = Resolver::default();
    cached(&mut resolver);
    resolver.close(SESSION);
    assert!(resolver.prefetched(SESSION).is_none());
    // An account change drops it.
    let mut resolver = Resolver::default();
    cached(&mut resolver);
    resolver.invalidate();
    assert!(resolver.prefetched(SESSION).is_none());
    // A listing in which the next episode is gone drops it.
    let mut resolver = Resolver::default();
    cached(&mut resolver);
    let effects = resolver.listing_changed(&series());
    let token = match effects.as_slice() {
        [Effect::ReadListing { token, .. }] => *token,
        other => panic!("re-read: {other:?}"),
    };
    let mut shorter = listing();
    shorter.seasons[0].episodes = vec![1, 2];
    shorter.seasons[0].advertised = Some(2);
    shorter.seasons.truncate(1);
    resolver.listing_loaded(token, Some(shorter));
    assert!(resolver.prefetched(SESSION).is_none());
}

#[test]
fn a_running_prefetch_is_cancelled_with_its_window_and_a_late_answer_is_dropped() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
    let effects = start_prefetch(&mut resolver, None);
    let token = indexer_token(&effects);
    let effects = resolver.close(SESSION);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CancelIndexer { token: t, .. } if *t == token))
    );
    assert!(resolver.indexer_result(token, Ok(vec![])).is_empty());
    assert!(resolver.prefetched(SESSION).is_none());
}

#[test]
fn failed_or_empty_prefetch_answers_are_not_cached() {
    for answer in [Err(IndexerFailed), Ok(Vec::new())] {
        let mut resolver = Resolver::default();
        open(&mut resolver);
        resolver.set_played(SESSION, played(1, "Show.S01.1080p-GRP"));
        let effects = start_prefetch(&mut resolver, Some(pack(true)));
        let token = indexer_token(&effects);
        assert!(resolver.indexer_result(token, answer).is_empty());
        assert!(resolver.prefetched(SESSION).is_none());
        // A later request asks the indexer again.
        let effects = resolver.request(
            SESSION,
            &episode(1, 2),
            NeighbourDirection::Next,
            Some(pack(true)),
        );
        assert!(
            effects
                .iter()
                .any(|e| matches!(e, Effect::QueryIndexer { .. }))
        );
    }
}

// ── Detail ──

fn stream_memory() -> RememberedStream {
    RememberedStream {
        stream: StreamMemory {
            release: ReleaseMemory {
                file_index: Some(4),
                filename: Some("hint.mkv".into()),
                ..release(7, "Show.S01.1080p.WEB-DL.x265-GRP")
            },
            file_name: Some("Show.S01E02.mkv".into()),
            file_size: Some(10),
        },
        updated_at: 1,
    }
}

fn read_token(effects: &[Effect]) -> Token {
    effects
        .iter()
        .find_map(|e| match e {
            Effect::ReadMemory { token, .. } => Some(*token),
            _ => None,
        })
        .unwrap_or_else(|| panic!("a memory read: {effects:?}"))
}

#[test]
fn detail_prepares_the_remembered_episode_stream_directly() {
    let mut resolver = Resolver::default();
    let effects = resolver.detail(7, &episode(1, 2));
    let token = read_token(&effects);
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: Some(stream_memory()),
            season: None,
        }),
    );
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                origin: Origin::Detail { ticket: 7 },
                decision: Decision::Direct { candidate },
                ..
            },
        ] => {
            assert_eq!(candidate.torrent.hash, hash(7));
            assert_eq!(candidate.torrent.file_index, Some(4));
            // The provider file's basename is the best hint.
            assert_eq!(
                candidate.torrent.filename.as_deref(),
                Some("Show.S01E02.mkv")
            );
        }
        other => panic!("direct: {other:?}"),
    }
    // No indexer query was made.
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. }))
    );
}

#[test]
fn detail_uses_the_season_release_through_the_indexer() {
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(7, &episode(1, 2)));
    let season = RememberedRelease {
        release: release(7, "Show.S01.1080p.WEB-DL.x265-GRP"),
        updated_at: 1,
    };
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: None,
            season: Some(season),
        }),
    );
    assert!(matches!(
        effects.as_slice(),
        [Effect::QueryIndexer { target, .. }] if *target == episode(1, 2)
    ));
    // The remembered torrent is listed: the same pack, prepared directly.
    let row = candidate(7, "Show.S01.1080p.WEB-DL.x265-GRP", Some(2), Some("02.mkv"));
    let effects = resolver.indexer_result(token, Ok(vec![row.clone()]));
    match outcomes(&effects).as_slice() {
        [
            Outcome {
                decision:
                    Decision::Prepare {
                        candidate, label, ..
                    },
                origin: Origin::Detail { .. },
                ..
            },
        ] => {
            assert_eq!(candidate, &row);
            assert_eq!(label, "Same pack");
        }
        other => panic!("same pack: {other:?}"),
    }
}

#[test]
fn detail_falls_back_to_the_ranked_picker_or_the_plain_one() {
    // A remembered release with no close match: the ranked picker.
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(1, &episode(2, 1)));
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: None,
            season: None,
        }),
    );
    // Nothing for the season: the previous season's release.
    assert!(matches!(
        effects.as_slice(),
        [Effect::ReadMemory {
            read: MemoryRead::PreviousSeason { season: 2, .. },
            ..
        }]
    ));
    let effects = resolver.memory_result(
        token,
        Some(StreamMemoryResult::PreviousSeason(Some((
            1,
            RememberedRelease {
                release: release(7, "Show.S01.1080p.WEB-DL.x265-GRP"),
                updated_at: 1,
            },
        )))),
    );
    assert!(matches!(effects.as_slice(), [Effect::QueryIndexer { .. }]));
    let weak = candidate(9, "Show.S02E01.720p.HDTV.x264-OTHER", None, None);
    let effects = resolver.indexer_result(token, Ok(vec![weak]));
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::Picker(_),
            origin: Origin::Detail { .. },
            ..
        }]
    ));

    // Nothing remembered at all: today's picker, with no indexer query.
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(2, &episode(1, 1)));
    resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: None,
            season: None,
        }),
    );
    let effects = resolver.memory_result(token, Some(StreamMemoryResult::PreviousSeason(None)));
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::Fallback,
            ..
        }]
    ));
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, Effect::QueryIndexer { .. }))
    );

    // A failed memory read also falls back instead of hanging.
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(3, &episode(1, 1)));
    let effects = resolver.memory_result(token, None);
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::Fallback,
            ..
        }]
    ));
}

#[test]
fn detail_indexer_failure_is_unavailable_not_a_picker_of_nothing() {
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(1, &episode(1, 2)));
    let season = RememberedRelease {
        release: release(7, "Show.S01.1080p-GRP"),
        updated_at: 1,
    };
    resolver.memory_result(
        token,
        Some(StreamMemoryResult::Memory {
            episode: None,
            season: Some(season),
        }),
    );
    let effects = resolver.indexer_result(token, Err(IndexerFailed));
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            decision: Decision::Unavailable { .. },
            origin: Origin::Detail { .. },
            ..
        }]
    ));
}

#[test]
fn leaving_detail_cancels_its_wait_and_ignores_late_answers() {
    let mut resolver = Resolver::default();
    let token = read_token(&resolver.detail(4, &episode(1, 2)));
    resolver.cancel_detail(4);
    assert_eq!(resolver.job_count(), 0);
    assert!(
        resolver
            .memory_result(
                token,
                Some(StreamMemoryResult::Memory {
                    episode: Some(stream_memory()),
                    season: None
                })
            )
            .is_empty()
    );
}

#[test]
fn an_account_change_releases_waiting_detail_and_window_requests() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01-GRP"));
    resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    resolver.detail(9, &episode(1, 1));
    let effects = resolver.invalidate();
    assert_eq!(resolver.job_count(), 0);
    assert_eq!(
        publishes(&effects),
        vec![dock(NeighbourState::Enabled, NeighbourState::Enabled)]
    );
    assert!(matches!(
        outcomes(&effects).as_slice(),
        [Outcome {
            origin: Origin::Detail { ticket: 9 },
            decision: Decision::Fallback,
            ..
        }]
    ));
}

#[test]
fn state_of_vanished_windows_is_collected() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    resolver.set_played(SESSION, played(1, "Show.S01-GRP"));
    resolver.request(SESSION, &episode(1, 2), NeighbourDirection::Next, None);
    let effects = resolver.retain_live(|_| false);
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::CancelIndexer { .. }))
    );
    assert!(resolver.played(SESSION).is_none());
    assert_eq!(resolver.job_count(), 0);
}

#[test]
fn episode_names_come_from_the_listing() {
    let mut resolver = Resolver::default();
    open(&mut resolver);
    assert_eq!(
        resolver.episode_name(&series(), 1, 3).as_deref(),
        Some("Third")
    );
    assert_eq!(resolver.episode_name(&series(), 1, 1), None);
}

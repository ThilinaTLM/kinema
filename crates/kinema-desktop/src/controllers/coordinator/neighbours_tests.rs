//! The continuity glue against a fixture coordinator: no worker, no network.
//! What would go to the workers is recorded (`flow.sent`), and answers are fed
//! back through the same entry points the workers use.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "Tests fail immediately when fixture setup or assertions are invalid"
)]
use super::*;
use crate::{
    app::{AppEvent, AppOutcome},
    catalog::{
        library::{LocalEpisode, PersonalEpisode},
        playback_progress::PlaybackProgressSnapshot,
        stream_memory::{ReleaseMemory, RememberedRelease, StreamMemoryResult},
    },
    catalog_worker::SourceLane,
    controllers::coordinator::PersonalTitle,
    controllers::coordinator::{continuity::KeptPreparation, resolver::Token},
    media::PlaybackSession,
    ui::views::player::{NeighbourState, Neighbours as Dock},
};
use ::core::prelude::v1::test;
use kinema_core::{
    catalog_normalized::{CanonicalChild, CanonicalTitle, ChildKind, FieldValue},
    library::{EpisodeId, TitleId},
    sources::{DebridKind, InfoHash, RemoteFile, TorrentSource},
    watch_progress::PlaybackProgressTarget,
};
use std::{cell::RefCell, rc::Rc};

const W: &str = "window-1";
const URL: &str = "http://127.0.0.1:1/private-capability";

fn series() -> ImdbId {
    ImdbId::new("tt1234567").unwrap()
}
fn episode(episode: u32) -> PlaybackTarget {
    PlaybackTarget::Episode {
        series: series(),
        season: 1,
        episode,
    }
}
fn hash(n: u32) -> InfoHash {
    InfoHash::new(format!("{n:040x}")).unwrap()
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
        size: Some(2_000),
        seeders: Some(7),
    }
}
fn progress(id: i64) -> PlaybackProgressSnapshot {
    PlaybackProgressSnapshot {
        target: PlaybackProgressTarget::Episode(EpisodeId::new(id).unwrap()),
        watch: None,
        epoch: 0,
        active_run: 0,
        auto_mark_suppressed: false,
        checkpoint: None,
    }
}
fn child(id: i64, parent: i64, number: u32, kind: ChildKind, name: &str) -> CanonicalChild {
    let mut fields = std::collections::BTreeMap::new();
    fields.insert("name".to_owned(), FieldValue::Text(name.into()));
    CanonicalChild {
        id,
        parent_id: parent,
        number,
        kind,
        fields,
        identifiers: vec![],
        provenance: vec![],
        ratings: vec![],
        updated_at: 0,
    }
}
/// A series with one season of `count` episodes.
fn detail(count: u32) -> LocalDetail {
    let mut season = child(10, 1, 1, ChildKind::Season, "Season 1");
    season.fields.insert(
        "episode_count".into(),
        FieldValue::Integer(i64::from(count)),
    );
    LocalDetail {
        metadata: CanonicalTitle {
            id: 1,
            kind: MediaKind::Series,
            identifiers: vec![],
            fields: Default::default(),
            provenance: vec![],
            ratings: vec![],
            updated_at: 0,
        },
        personal: PersonalTitle {
            title_id: TitleId::new(1).unwrap(),
            kind: MediaKind::Series,
            added_at: None,
            has_resume: false,
            playback_progress: None,
            movie_watch: None,
            show_summary: None,
        },
        seasons: vec![season],
        episodes: (1..=count)
            .map(|n| LocalEpisode {
                season: 1,
                metadata: child(
                    100 + i64::from(n),
                    10,
                    n,
                    ChildKind::Episode,
                    &format!("Episode {n}"),
                ),
                personal: PersonalEpisode {
                    episode_id: EpisodeId::new(i64::from(n)).unwrap(),
                    watch: None,
                    playback_progress: progress(i64::from(n)),
                },
            })
            .collect(),
    }
}
fn pack() -> Vec<RemoteFile> {
    (1..=3)
        .map(|n| RemoteFile {
            id: n.to_string(),
            path: format!("Show/0{n}.mkv"),
            size: 10,
        })
        .collect()
}

fn kept_pack(files: Vec<RemoteFile>) -> KeptPreparation {
    KeptPreparation {
        id: 5,
        provider: DebridKind::RealDebrid,
        source: TorrentSource {
            hash: hash(1),
            file_index: None,
            filename: None,
        },
        files: files.into(),
    }
}

struct Harness {
    cx: TestAppContext,
    c: Entity<AppCoordinator>,
    events: Rc<RefCell<Vec<CoordinatorEvent>>>,
}

impl Harness {
    /// A window playing S1E2 of a 3-episode season whose pack it keeps.
    fn new() -> Self {
        let mut cx = TestAppContext::single();
        let c = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        let events: Rc<RefCell<Vec<CoordinatorEvent>>> = Default::default();
        let log = events.clone();
        cx.update(|cx| {
            cx.subscribe(&c, move |_, event: &CoordinatorEvent, _| {
                log.borrow_mut().push(event.clone())
            })
            .detach()
        });
        c.update(&mut cx, |c, _| {
            c.fixture_attach_mailbox();
            c.fixture_window(W, URL, "The Show", episode(2), progress(2));
            c.fixture_kept_pack(W, kept_pack(pack()));
            c.flow.resolver.set_played(
                W,
                Played {
                    reference: ReferenceRelease {
                        name: "Show.S01.1080p.WEB-DL.x265-GRP".into(),
                        file_name: None,
                        size: Some(2_000),
                    },
                    hash: hash(1),
                },
            );
        });
        Self { cx, c, events }
    }

    fn run<R>(
        &mut self,
        f: impl FnOnce(&mut AppCoordinator, &mut Context<AppCoordinator>) -> R,
    ) -> R {
        self.c.update(&mut self.cx, f)
    }
    fn sent(&mut self) -> Vec<Sent> {
        self.run(|c, _| std::mem::take(&mut c.flow.sent))
    }
    fn commands(&mut self) -> Vec<crate::app::AppCommand> {
        self.run(|c, _| c.fixture_drain_requests())
    }
    fn dock(&mut self) -> Option<Dock> {
        self.run(|c, _| c.fixture_probe(W).neighbours)
    }
    /// Opens the window's subject and answers its listing read.
    fn open(&mut self) {
        self.run(|c, cx| c.announce_subject(W, Some(episode(2)), cx));
        self.answer_listing(Ok(detail(3)));
    }
    fn answer_listing(&mut self, result: Result<LocalDetail, String>) {
        let (id, operation) = self
            .sent()
            .into_iter()
            .rev()
            .find_map(|sent| match sent {
                Sent::Local(id, op @ LibraryOperation::Detail(_)) => Some((id, op)),
                _ => None,
            })
            .expect("a listing read");
        self.feed_local(
            id,
            operation,
            result.map(|d| LibraryResult::Detail(Box::new(d))),
        );
    }
    fn feed_local(
        &mut self,
        id: u64,
        operation: LibraryOperation,
        result: Result<LibraryResult, String>,
    ) {
        let event = LocalEvent {
            request: LocalRequest {
                request_id: id,
                generation: 0,
                operation,
            },
            revision: 0,
            result,
        };
        self.run(|c, cx| assert!(c.apply_flow_event(&event, cx), "a continuity answer"));
    }
    /// Asks for the next episode and returns the indexer token it waits on.
    fn request(&mut self, direction: NeighbourDirection) -> Token {
        self.run(|c, cx| c.request_neighbour(W, 0, direction, cx));
        self.indexer_tokens().pop().expect("an indexer query")
    }
    /// The episodes the continuity indexer lane was asked about, in order.
    fn indexer_targets(&mut self) -> Vec<PlaybackTarget> {
        self.run(|c, _| {
            c.flow
                .sent
                .iter()
                .filter_map(|sent| match sent {
                    Sent::Indexer(_, target) => Some(target.clone()),
                    _ => None,
                })
                .collect()
        })
    }
    fn indexer_tokens(&mut self) -> Vec<Token> {
        self.run(|c, _| {
            c.flow
                .sent
                .iter()
                .filter_map(|sent| match sent {
                    Sent::Indexer(token, _) => Some(*token),
                    _ => None,
                })
                .collect()
        })
    }
    fn answer_indexer(
        &mut self,
        token: Token,
        target: PlaybackTarget,
        candidates: Option<Vec<SourceCandidate>>,
    ) {
        let result = match candidates {
            Some(candidates) => Ok(MetadataSnapshot::Sources(crate::catalog::CachedSources {
                candidates,
                omitted: 0,
                fetched_at: 1,
            })),
            None => Err("offline".to_owned()),
        };
        let event = MetadataEvent {
            generation: token,
            action: MetadataAction::Sources {
                indexer: IndexerKind::Torrentio,
                target,
                lane: SourceLane::Continuity,
            },
            result,
            saved: false,
            refreshing: false,
        };
        self.run(|c, cx| c.flow_indexer_event(event, cx));
    }
    fn prepared_event(&mut self, generation: u64, selection: FileSelection) {
        let prepared = crate::providers::PreparedTorrent {
            provider: DebridKind::RealDebrid,
            source: TorrentSource {
                hash: hash(9),
                file_index: None,
                filename: None,
            },
            job_id: "job".into(),
            files: pack(),
        };
        self.run(|c, cx| {
            c.fixture_apply_app(
                AppEvent {
                    generation,
                    outcome: AppOutcome::Prepared {
                        prepared,
                        target: episode(3),
                        selection,
                    },
                },
                cx,
            )
        });
    }
    fn picker_opened(&self) -> usize {
        self.events
            .borrow()
            .iter()
            .filter(|e| matches!(e, CoordinatorEvent::OpenStreamPicker))
            .count()
    }
}

// ── neighbour state ──

#[test]
fn opening_a_window_publishes_neighbours_from_the_catalog_listing() {
    let mut h = Harness::new();
    h.run(|c, cx| c.announce_subject(W, Some(episode(2)), cx));
    // Nothing is invented before the listing arrives.
    assert_eq!(h.dock(), None);
    h.answer_listing(Ok(detail(3)));
    assert_eq!(
        h.dock(),
        Some(Dock {
            previous: NeighbourState::Enabled,
            next: NeighbourState::Enabled
        })
    );
    // The last episode has no next.
    h.run(|c, cx| c.announce_subject(W, Some(episode(3)), cx));
    assert_eq!(
        h.dock(),
        Some(Dock {
            previous: NeighbourState::Enabled,
            next: NeighbourState::Disabled
        })
    );
}

#[test]
fn movies_untracked_items_and_unreadable_listings_are_not_enabled() {
    let mut h = Harness::new();
    h.run(|c, cx| c.announce_subject(W, Some(PlaybackTarget::Movie(series())), cx));
    assert_eq!(h.dock(), Some(Dock::default()));
    h.run(|c, cx| c.announce_subject(W, None, cx));
    assert_eq!(h.dock(), Some(Dock::default()));
    // A listing the catalog cannot give (no episode rows) leaves the dock hidden.
    let mut h = Harness::new();
    h.run(|c, cx| c.announce_subject(W, Some(episode(2)), cx));
    h.answer_listing(Err("missing".into()));
    assert_eq!(h.dock(), None);
}

#[test]
fn a_catalog_change_republishes_the_neighbours() {
    let mut h = Harness::new();
    h.run(|c, cx| c.announce_subject(W, Some(episode(3)), cx));
    h.answer_listing(Ok(detail(3)));
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Disabled));
    // A new season arrives in the catalog: the last episode gains a next.
    let mut grown = detail(3);
    grown
        .seasons
        .push(child(11, 1, 2, ChildKind::Season, "Season 2"));
    grown.seasons[1]
        .fields
        .insert("episode_count".into(), FieldValue::Integer(4));
    h.run(|c, cx| c.refresh_personal_after_catalog(cx));
    h.answer_listing(Ok(grown));
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Enabled));
}

// ── each choice ──

#[test]
fn same_pack_unlocks_the_kept_file_into_the_same_window() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    // The neighbour's own episode is what the indexer is asked about.
    assert_eq!(h.indexer_targets(), [episode(3)]);
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Busy));
    h.answer_indexer(
        token,
        episode(3),
        Some(vec![candidate(
            1,
            "Show.S01.1080p.WEB-DL.x265-GRP",
            Some(3),
            Some("Show/03.mkv"),
        )]),
    );
    let commands = h.commands();
    let kept = commands
        .iter()
        .find_map(|command| match &command.action {
            AppAction::PlayKeptFile {
                preparation,
                file_id,
            } => Some((*preparation, file_id.clone(), command.generation)),
            _ => None,
        })
        .expect("a kept-file play");
    assert_eq!((kept.0, kept.1.as_str()), (5, "3"));
    // It swaps into the window, and the dock says how it was chosen once ready.
    assert!(h.run(|c, _| c.is_routed(kept.2)));
    assert_eq!(
        h.run(|c, _| c.flow.notes.get(W).cloned()).as_deref(),
        Some("Same pack")
    );
    // No new window and no new preparation were asked for.
    assert!(
        !commands
            .iter()
            .any(|command| matches!(command.action, AppAction::Prepare { .. }))
    );
}

#[test]
fn an_indexer_failure_uses_the_strict_marker_rule_on_the_kept_pack() {
    let mut h = Harness::new();
    h.run(|c, _| {
        let marked = (1..=3)
            .map(|n| RemoteFile {
                id: n.to_string(),
                path: format!("Show.S01E0{n}.mkv"),
                size: 10,
            })
            .collect();
        c.fixture_kept_pack(W, kept_pack(marked));
    });
    h.open();
    let token = h.request(NeighbourDirection::Next);
    h.answer_indexer(token, episode(3), None);
    let commands = h.commands();
    assert!(commands.iter().any(|command| matches!(
        &command.action,
        AppAction::PlayKeptFile { file_id, .. } if file_id == "3"
    )));
    assert_eq!(
        h.run(|c, _| c.flow.notes.get(W).cloned()).as_deref(),
        Some("Same pack · by episode number")
    );
}

#[test]
fn an_unavailable_neighbour_reports_the_reason_and_frees_the_control() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    // The pack has no marker files and the indexer failed: nothing to play.
    h.answer_indexer(token, episode(3), None);
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Enabled));
    assert!(h.commands().is_empty());
    // An empty answer is a different reason from a failed one, and a new
    // request can be made at once.
    let token = h.request(NeighbourDirection::Next);
    h.answer_indexer(token, episode(3), Some(vec![]));
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Enabled));
}

#[test]
fn a_strong_match_is_prepared_and_its_unique_file_played_into_the_window() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    let strong = candidate(9, "Show.S01E03.1080p.WEB-DL.x265-GRP", None, None);
    h.answer_indexer(token, episode(3), Some(vec![strong.clone()]));
    let commands = h.commands();
    let prepare = commands
        .iter()
        .find(|command| matches!(command.action, AppAction::Prepare { .. }))
        .expect("a preparation");
    let AppAction::Prepare { candidate, target } = &prepare.action else {
        unreachable!()
    };
    assert_eq!((candidate, target), (&strong, &episode(3)));
    // The file is unique: it plays, keeping the preparation, routed to the window.
    h.prepared_event(prepare.generation, FileSelection::Unique(2));
    let play = h
        .commands()
        .into_iter()
        .find(|command| matches!(command.action, AppAction::PlayFile { .. }))
        .expect("the file play");
    assert!(matches!(
        &play.action,
        AppAction::PlayFile { file_id, keep: true, .. } if file_id == "3"
    ));
    assert!(h.run(|c, _| c.is_routed(play.generation)));
    assert_eq!(h.picker_opened(), 0);
}

#[test]
fn a_strong_match_with_no_unique_file_becomes_the_picker() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    let strong = candidate(9, "Show.S01E03.1080p.WEB-DL.x265-GRP", None, None);
    h.answer_indexer(token, episode(3), Some(vec![strong]));
    let prepare = h
        .commands()
        .into_iter()
        .find(|command| matches!(command.action, AppAction::Prepare { .. }))
        .expect("a preparation");
    h.prepared_event(prepare.generation, FileSelection::Ambiguous(vec![0, 1]));
    // Never a guess: the stream list opens and nothing was played.
    assert_eq!(h.picker_opened(), 1);
    assert!(
        !h.commands()
            .iter()
            .any(|command| matches!(command.action, AppAction::PlayFile { .. }))
    );
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Busy));
}

#[test]
fn no_strong_match_opens_the_ranked_picker_and_dismissing_it_frees_the_control() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    let weak = candidate(9, "Show.S01E03.720p.HDTV.x264-OTHER", None, None);
    h.answer_indexer(token, episode(3), Some(vec![weak]));
    assert_eq!(h.picker_opened(), 1);
    let picker = h.run(|c, _| c.stream_picker().cloned()).expect("a picker");
    assert_eq!(picker.target, episode(3));
    assert_eq!(picker.spec.label, "No close match — pick a stream");
    assert_eq!(picker.spec.reference.name, "Show.S01.1080p.WEB-DL.x265-GRP");
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Busy));
    // Back / Esc on the stream list.
    h.run(|c, cx| c.close_stream_picker(cx));
    assert!(h.run(|c, _| c.stream_picker().is_none()));
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Enabled));
}

// ── the picker plays into the existing window ──

#[test]
fn a_stream_picked_from_the_picker_is_routed_into_the_player_window() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    let weak = candidate(9, "Show.S01E03.720p.HDTV.x264-OTHER", None, None);
    h.answer_indexer(token, episode(3), Some(vec![weak.clone()]));
    // The Streams page prepares the picked stream like any other.
    let preparation = h
        .run(|c, cx| {
            c.prepare(
                "The Show".into(),
                Some("Episode 3".into()),
                episode(3),
                weak,
                PlaybackIntent::Normal,
                cx,
            )
        })
        .expect("a preparation");
    h.prepared_event(preparation, FileSelection::Unique(2));
    let play = h.run(|c, cx| {
        c.command(
            AppAction::PlayFile {
                preparation,
                file_id: "3".into(),
                keep: true,
            },
            cx,
        )
    });
    let play = play.expect("the file play");
    assert!(h.run(|c, _| c.is_routed(play)), "routed into the window");
    // A stream for another episode is not routed.
    let other = h
        .run(|c, cx| {
            c.prepare(
                "The Show".into(),
                None,
                episode(1),
                candidate(8, "Show.S01E01.720p-X", None, None),
                PlaybackIntent::Normal,
                cx,
            )
        })
        .expect("a preparation");
    h.prepared_event(other, FileSelection::Unique(0));
    let play = h
        .run(|c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation: other,
                    file_id: "1".into(),
                    keep: true,
                },
                cx,
            )
        })
        .expect("the file play");
    assert!(
        !h.run(|c, _| c.is_routed(play)),
        "another episode opens normally"
    );
}

#[test]
fn closing_the_window_cancels_its_picker_work_and_late_answers() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    h.run(|c, cx| c.embedded_player_closed(W.into(), cx));
    h.sent();
    // The indexer's late answer finds nothing waiting.
    h.answer_indexer(
        token,
        episode(3),
        Some(vec![candidate(
            9,
            "Show.S01E03.1080p.WEB-DL.x265-GRP",
            None,
            None,
        )]),
    );
    assert!(!h.commands().iter().any(|command| matches!(
        command.action,
        AppAction::Prepare { .. } | AppAction::PlayKeptFile { .. }
    )));
    assert!(h.run(|c, _| c.stream_picker().is_none()));
}

// ── indexer lane ──

#[test]
fn continuity_queries_go_one_at_a_time_and_interactive_first() {
    let mut h = Harness::new();
    h.open();
    // A prefetch at 80% is queued behind nothing; a request while it runs waits
    // for its slot, but adopts the same query instead of asking twice.
    let observation = PlaybackObservation::new(900, Some(1_000)).unwrap();
    h.run(|c, cx| c.flow_progress(W, observation, cx));
    assert_eq!(h.indexer_tokens().len(), 1);
    h.run(|c, cx| c.request_neighbour(W, 0, NeighbourDirection::Next, cx));
    assert_eq!(
        h.indexer_tokens().len(),
        1,
        "the request adopted the prefetch"
    );
    assert_eq!(h.dock().map(|d| d.next), Some(NeighbourState::Busy));
}

#[test]
fn a_saved_listing_waits_for_the_fresh_one_but_survives_its_failure() {
    let mut h = Harness::new();
    h.open();
    let token = h.request(NeighbourDirection::Next);
    let strong = candidate(9, "Show.S01E03.1080p.WEB-DL.x265-GRP", None, None);
    let saved = |generation, result| MetadataEvent {
        generation,
        action: MetadataAction::Sources {
            indexer: IndexerKind::Torrentio,
            target: episode(3),
            lane: SourceLane::Continuity,
        },
        result,
        saved: true,
        refreshing: true,
    };
    // The saved copy alone does not decide.
    h.run(|c, cx| {
        c.flow_indexer_event(
            saved(
                token,
                Ok(MetadataSnapshot::Sources(crate::catalog::CachedSources {
                    candidates: vec![strong.clone()],
                    omitted: 0,
                    fetched_at: 1,
                })),
            ),
            cx,
        )
    });
    assert!(h.commands().is_empty());
    // The refresh failed: the saved copy is better than nothing.
    let mut failed = saved(token, Err("offline".into()));
    failed.saved = false;
    failed.refreshing = false;
    h.run(|c, cx| c.flow_indexer_event(failed, cx));
    assert!(
        h.commands()
            .iter()
            .any(|command| matches!(command.action, AppAction::Prepare { .. }))
    );
}

// ── prefetch ──

#[test]
fn prefetch_resolves_without_touching_the_account_and_a_request_uses_it() {
    let mut h = Harness::new();
    h.open();
    let early = PlaybackObservation::new(500, Some(1_000)).unwrap();
    h.run(|c, cx| c.flow_progress(W, early, cx));
    assert!(h.indexer_tokens().is_empty(), "below 80% nothing starts");
    let due = PlaybackObservation::new(800, Some(1_000)).unwrap();
    h.run(|c, cx| c.flow_progress(W, due, cx));
    let token = h.indexer_tokens().pop().expect("a prefetch query");
    h.answer_indexer(
        token,
        episode(3),
        Some(vec![candidate(
            1,
            "Show.S01.1080p.WEB-DL.x265-GRP",
            Some(3),
            Some("Show/03.mkv"),
        )]),
    );
    // Resolve-only: no magnet, no unlock, no download request.
    assert!(h.commands().is_empty());
    assert!(h.dock().is_some_and(|d| d.next == NeighbourState::Enabled));
    // The user commits: the cached choice plays at once, with no second query.
    h.sent();
    h.run(|c, cx| c.request_neighbour(W, 0, NeighbourDirection::Next, cx));
    assert!(h.indexer_tokens().is_empty());
    assert!(
        h.commands()
            .iter()
            .any(|command| matches!(&command.action, AppAction::PlayKeptFile { file_id, .. } if file_id == "3"))
    );
}

#[test]
fn an_account_change_voids_the_prefetch() {
    let mut h = Harness::new();
    h.open();
    let due = PlaybackObservation::new(800, Some(1_000)).unwrap();
    h.run(|c, cx| c.flow_progress(W, due, cx));
    let token = h.indexer_tokens().pop().expect("a prefetch query");
    h.answer_indexer(
        token,
        episode(3),
        Some(vec![candidate(
            1,
            "Show.S01.1080p.WEB-DL.x265-GRP",
            Some(3),
            Some("Show/03.mkv"),
        )]),
    );
    h.run(|c, cx| {
        c.invalidate_continuity();
        c.flow_invalidated(cx)
    });
    h.sent();
    h.run(|c, cx| c.request_neighbour(W, 0, NeighbourDirection::Next, cx));
    // The cache is gone, so the indexer is asked again.
    assert_eq!(h.indexer_tokens().len(), 1);
}

// ── record on play ──

fn plan(target: PlaybackTarget) -> PlayPlan {
    PlayPlan {
        target,
        candidate: candidate(
            4,
            "Show.S01.1080p.WEB-DL.x265-GRP",
            Some(3),
            Some("Show/03.mkv"),
        ),
        file: Some(ProviderFile {
            name: "Show.S01E03.mkv".into(),
            size: 123,
        }),
    }
}

#[test]
fn remember_holds_the_stream_fields_the_file_and_a_hintless_season_release() {
    let remember = remember_for(&plan(episode(3))).expect("a record");
    assert_eq!((remember.season, remember.episode), (1, 3));
    let release = &remember.stream.release;
    assert_eq!(release.info_hash, hash(4));
    assert_eq!(release.name, "Show.S01.1080p.WEB-DL.x265-GRP");
    assert_eq!(release.quality.as_deref(), Some("1080p"));
    assert_eq!(release.size, Some(2_000));
    assert_eq!(release.file_index, Some(3));
    assert_eq!(release.filename.as_deref(), Some("Show/03.mkv"));
    assert_eq!(
        remember.stream.file_name.as_deref(),
        Some("Show.S01E03.mkv")
    );
    assert_eq!(remember.stream.file_size, Some(123));
    let season = &remember.season_release;
    assert_eq!(season.info_hash, hash(4));
    assert_eq!(
        (season.file_index, season.filename.as_deref()),
        (None, None)
    );
    // Only indexer and provider-listing facts: no URL or session id anywhere.
    let dump = format!("{remember:?}");
    assert!(!dump.contains("http") && !dump.contains("capability") && !dump.contains(W));
}

#[test]
fn remember_cleans_names_and_skips_what_cannot_be_stored() {
    let mut p = plan(episode(3));
    p.file = Some(ProviderFile {
        name: "dir/inner\\Show.mkv".into(),
        size: 1,
    });
    let remember = remember_for(&p).expect("a record");
    assert_eq!(remember.stream.file_name.as_deref(), Some("Show.mkv"));
    // An unnamed release, and movies, are not remembered.
    let mut unnamed = plan(episode(3));
    unnamed.candidate.name.clear();
    assert!(remember_for(&unnamed).is_none());
    assert!(remember_for(&plan(PlaybackTarget::Movie(series()))).is_none());
    // Oversized text is cut at a character boundary, never stored whole.
    let mut long = plan(episode(3));
    long.candidate.name = "é".repeat(2_000);
    let name = remember_for(&long).expect("a record").stream.release.name;
    assert!(name.len() <= 1024 && name.chars().all(|c| c == 'é'));
}

/// Prepares and plays `candidate` for `target` through the coordinator, up to
/// the start event of the play command.
fn start(
    h: &mut Harness,
    target: PlaybackTarget,
    candidate: SourceCandidate,
    embedded: bool,
) -> u64 {
    let preparation = h
        .run(|c, cx| {
            c.prepare(
                "The Show".into(),
                None,
                target.clone(),
                candidate,
                PlaybackIntent::Normal,
                cx,
            )
        })
        .expect("a preparation");
    h.prepared_event(preparation, FileSelection::Unique(1));
    let play = h
        .run(|c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation,
                    file_id: "2".into(),
                    keep: false,
                },
                cx,
            )
        })
        .expect("the file play");
    let session = PlaybackSession {
        id: "started".into(),
        url: URL.into(),
    };
    let outcome = if embedded {
        AppOutcome::EmbeddedPlaybackStarted {
            session,
            file_name: "Show.mkv".into(),
            mode: crate::player::PlayerChoice::Embedded,
            hash_windows: None,
        }
    } else {
        AppOutcome::PlaybackStarted {
            session,
            file_name: "Show.mkv".into(),
        }
    };
    h.run(|c, cx| {
        c.fixture_apply_app(
            AppEvent {
                generation: play,
                outcome,
            },
            cx,
        )
    });
    play
}

fn remembered(h: &mut Harness) -> Vec<crate::catalog::stream_memory::Remember> {
    h.sent()
        .into_iter()
        .filter_map(|sent| match sent {
            Sent::Local(
                _,
                LibraryOperation::StreamMemory(StreamMemoryOperation::Remember(remember)),
            ) => Some(*remember),
            _ => None,
        })
        .collect()
}

#[test]
fn every_episode_start_remembers_its_stream_once_with_the_resolved_file() {
    for embedded in [true, false] {
        let mut h = Harness::new();
        let picked = candidate(
            6,
            "Show.S01.1080p.WEB-DL.x265-GRP",
            Some(2),
            Some("Show/02.mkv"),
        );
        start(&mut h, episode(2), picked, embedded);
        let all = remembered(&mut h);
        assert_eq!(all.len(), 1, "embedded {embedded}");
        let remember = &all[0];
        assert_eq!(remember.stream.release.info_hash, hash(6));
        // The file the preparation resolved, by its basename and size.
        assert_eq!(remember.stream.file_name.as_deref(), Some("02.mkv"));
        assert_eq!(remember.stream.file_size, Some(10));
        // The stream an embedded window plays becomes its own reference.
        if embedded {
            let played = h.run(|c, _| c.flow.resolver.played("started").cloned());
            assert_eq!(played.map(|p| p.hash), Some(hash(6)));
        }
    }
}

#[test]
fn movies_stale_embedded_starts_and_failed_plays_remember_nothing() {
    // A movie.
    let mut h = Harness::new();
    start(
        &mut h,
        PlaybackTarget::Movie(series()),
        candidate(6, "Movie.1080p-GRP", None, None),
        false,
    );
    assert!(remembered(&mut h).is_empty());
    // An embedded start that a newer play superseded is released unseen.
    let mut h = Harness::new();
    let first = h
        .run(|c, cx| {
            c.prepare(
                "The Show".into(),
                None,
                episode(2),
                candidate(6, "Show.S01-GRP", None, None),
                PlaybackIntent::Normal,
                cx,
            )
        })
        .expect("a preparation");
    h.prepared_event(first, FileSelection::Unique(1));
    let old_play = h
        .run(|c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation: first,
                    file_id: "2".into(),
                    keep: false,
                },
                cx,
            )
        })
        .expect("play");
    // A newer play is issued before the old one starts.
    let _newer = h
        .run(|c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation: first,
                    file_id: "3".into(),
                    keep: false,
                },
                cx,
            )
        })
        .expect("play");
    h.run(|c, cx| {
        c.fixture_apply_app(
            AppEvent {
                generation: old_play,
                outcome: AppOutcome::EmbeddedPlaybackStarted {
                    session: PlaybackSession {
                        id: "stale".into(),
                        url: URL.into(),
                    },
                    file_name: "Show.mkv".into(),
                    mode: crate::player::PlayerChoice::Embedded,
                    hash_windows: None,
                },
            },
            cx,
        )
    });
    assert!(remembered(&mut h).is_empty());
    // A failed play drops its plan.
    let mut h = Harness::new();
    let preparation = h
        .run(|c, cx| {
            c.prepare(
                "The Show".into(),
                None,
                episode(2),
                candidate(6, "Show.S01-GRP", None, None),
                PlaybackIntent::Normal,
                cx,
            )
        })
        .expect("a preparation");
    h.prepared_event(preparation, FileSelection::Unique(1));
    let play = h
        .run(|c, cx| {
            c.command(
                AppAction::PlayFile {
                    preparation,
                    file_id: "2".into(),
                    keep: false,
                },
                cx,
            )
        })
        .expect("play");
    h.run(|c, cx| {
        c.fixture_apply_app(
            AppEvent {
                generation: play,
                outcome: AppOutcome::Failed {
                    message: "no".into(),
                },
            },
            cx,
        )
    });
    assert!(h.run(|c, _| c.flow.plans.is_empty()));
    assert!(remembered(&mut h).is_empty());
}

#[test]
fn an_unmapped_episode_or_a_failed_write_stays_quiet() {
    let mut h = Harness::new();
    start(
        &mut h,
        episode(2),
        candidate(6, "Show.S01-GRP", None, None),
        false,
    );
    let (id, operation) = h
        .sent()
        .into_iter()
        .find_map(|sent| match sent {
            Sent::Local(id, op @ LibraryOperation::StreamMemory(_)) => Some((id, op)),
            _ => None,
        })
        .expect("a write");
    for error in [
        crate::catalog::CatalogError::MissingIdentityMapping.to_string(),
        crate::catalog::CatalogError::Storage.to_string(),
    ] {
        h.run(|c, _| {
            c.flow.reads.insert(id, Read::Remember);
        });
        h.feed_local(id, operation.clone(), Err(error));
    }
    h.run(|c, _| {
        assert!(c.personal_notice.is_none() && c.notice.is_none());
        assert!(!c.can_retry_personal_write());
    });
}

// ── Detail: play and resume with memory ──

fn memory_answer(h: &mut Harness, result: Result<StreamMemoryResult, String>) {
    let (id, operation) = h
        .sent()
        .into_iter()
        .rev()
        .find_map(|sent| match sent {
            Sent::Local(id, op @ LibraryOperation::StreamMemory(_)) => Some((id, op)),
            _ => None,
        })
        .expect("a memory read");
    h.feed_local(id, operation, result.map(LibraryResult::StreamMemory));
}

fn season_release(n: u32, name: &str) -> RememberedRelease {
    RememberedRelease {
        release: ReleaseMemory {
            info_hash: hash(n),
            name: name.into(),
            quality: Some("1080p".into()),
            size: Some(2_000),
            file_index: None,
            filename: None,
        },
        updated_at: 1,
    }
}

fn direct_plays(h: &Harness) -> Vec<(u64, DirectPlay)> {
    h.events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            CoordinatorEvent::DirectPlay { ticket, play } => Some((*ticket, play.clone())),
            _ => None,
        })
        .collect()
}

#[test]
fn detail_plays_the_remembered_stream_without_the_indexer() {
    let mut h = Harness::new();
    let ticket = h.run(|c, cx| c.play_episode_direct(episode(3), cx));
    let remembered_stream = crate::catalog::stream_memory::RememberedStream {
        stream: crate::catalog::stream_memory::StreamMemory {
            release: ReleaseMemory {
                file_index: Some(3),
                ..season_release(7, "Show.S01.1080p.WEB-DL.x265-GRP").release
            },
            file_name: Some("Show.S01E03.mkv".into()),
            file_size: Some(10),
        },
        updated_at: 1,
    };
    memory_answer(
        &mut h,
        Ok(StreamMemoryResult::Memory {
            episode: Some(remembered_stream),
            season: None,
        }),
    );
    assert!(h.indexer_tokens().is_empty());
    match direct_plays(&h).as_slice() {
        [
            (
                t,
                DirectPlay::Play {
                    candidate,
                    label: None,
                    ..
                },
            ),
        ] => {
            assert_eq!(*t, ticket);
            assert_eq!(candidate.torrent.hash, hash(7));
            assert_eq!(
                candidate.torrent.filename.as_deref(),
                Some("Show.S01E03.mkv")
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn detail_uses_the_season_release_then_falls_back_to_the_ordinary_list() {
    // A remembered release: the indexer decides, and a strong match plays.
    let mut h = Harness::new();
    h.run(|c, cx| c.play_episode_direct(episode(3), cx));
    memory_answer(
        &mut h,
        Ok(StreamMemoryResult::Memory {
            episode: None,
            season: Some(season_release(7, "Show.S01.1080p.WEB-DL.x265-GRP")),
        }),
    );
    let token = h.indexer_tokens().pop().expect("the indexer is asked");
    let strong = candidate(9, "Show.S01E03.1080p.WEB-DL.x265-GRP", None, None);
    h.answer_indexer(token, episode(3), Some(vec![strong.clone()]));
    assert!(matches!(
        direct_plays(&h).as_slice(),
        [(_, DirectPlay::Play { candidate, label: Some(_), picker: Some(_) })] if *candidate == strong
    ));
    // The indexer failed: the plain list (which shows its own failure).
    let mut h = Harness::new();
    h.run(|c, cx| c.play_episode_direct(episode(3), cx));
    memory_answer(
        &mut h,
        Ok(StreamMemoryResult::Memory {
            episode: None,
            season: Some(season_release(7, "Show.S01-GRP")),
        }),
    );
    let token = h.indexer_tokens().pop().unwrap();
    h.answer_indexer(token, episode(3), None);
    assert!(matches!(
        direct_plays(&h).as_slice(),
        [(_, DirectPlay::Unavailable(_))]
    ));
    // Nothing remembered at all: today's picker, without asking the indexer.
    let mut h = Harness::new();
    h.run(|c, cx| c.play_episode_direct(episode(3), cx));
    memory_answer(
        &mut h,
        Ok(StreamMemoryResult::Memory {
            episode: None,
            season: None,
        }),
    );
    memory_answer(&mut h, Ok(StreamMemoryResult::PreviousSeason(None)));
    assert!(h.indexer_tokens().is_empty());
    assert!(matches!(
        direct_plays(&h).as_slice(),
        [(_, DirectPlay::Fallback)]
    ));
}

#[test]
fn detail_asking_again_or_leaving_ignores_the_earlier_answer() {
    let mut h = Harness::new();
    h.run(|c, cx| c.play_episode_direct(episode(3), cx));
    let (stale_id, stale_op) = h
        .sent()
        .into_iter()
        .find_map(|sent| match sent {
            Sent::Local(id, op @ LibraryOperation::StreamMemory(_)) => Some((id, op)),
            _ => None,
        })
        .expect("a read");
    // Asking for another episode supersedes the first request.
    h.run(|c, cx| c.play_episode_direct(episode(1), cx));
    h.feed_local(
        stale_id,
        stale_op,
        Ok(LibraryResult::StreamMemory(StreamMemoryResult::Memory {
            episode: None,
            season: None,
        })),
    );
    assert!(
        direct_plays(&h).is_empty(),
        "the first request was superseded"
    );
}

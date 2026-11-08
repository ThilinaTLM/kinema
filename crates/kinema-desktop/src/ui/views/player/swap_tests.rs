//! Same-window source swap, neighbour controls and Up next, driven through the
//! real registry, window and coordinator with an in-memory player stand-in.
use super::*;
use crate::{
    app::{AppAction, AppEvent, AppOutcome},
    catalog::playback_progress::{PlaybackCheckpoint, PlaybackProgressSnapshot},
    controllers::coordinator::{
        EpisodeLaunch, NeighbourEvent, SessionProbe, SwapError, SwapRequest, SwapStep,
    },
    media::PlaybackSession,
};
use ::core::prelude::v1::test;
use gpui::TestAppContext;
use kinema_core::{
    catalog::ImdbId,
    library::{EpisodeId, WatchState},
    watch_progress::{PlaybackObservation, PlaybackProgressTarget},
};
use std::cell::RefCell;

const OLD: &str = "old-lease";
const NEW: &str = "new-lease";
const OLD_URL: &str = "http://127.0.0.1:1/media/old";
const NEW_URL: &str = "http://127.0.0.1:1/media/new";

type Requests = Rc<RefCell<Vec<(String, u64, NeighbourDirection)>>>;
struct Recorder(Requests);
impl NeighbourRequests for Recorder {
    fn request(&self, session: &str, generation: u64, direction: NeighbourDirection, _: &mut App) {
        self.0
            .borrow_mut()
            .push((session.to_owned(), generation, direction));
    }
}

fn target(episode: u32) -> PlaybackTarget {
    PlaybackTarget::Episode {
        series: ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("fixture: {e}")),
        season: 1,
        episode,
    }
}
fn snapshot(id: i64, position_ms: Option<i64>) -> PlaybackProgressSnapshot {
    PlaybackProgressSnapshot {
        target: PlaybackProgressTarget::Episode(
            EpisodeId::new(id).unwrap_or_else(|| panic!("fixture episode id")),
        ),
        watch: Some(WatchState {
            watched: false,
            updated_at: 1,
        }),
        epoch: 4,
        active_run: 9,
        auto_mark_suppressed: false,
        checkpoint: position_ms.map(|ms| PlaybackCheckpoint {
            observation: PlaybackObservation::new(ms, Some(1_500_000))
                .unwrap_or_else(|| panic!("fixture observation")),
            updated_at: 2,
            revision: 3,
        }),
    }
}
fn launch(episode: u32) -> EpisodeLaunch {
    EpisodeLaunch {
        title: "The Show".into(),
        episode_title: Some(format!("Episode {episode}")),
        target: target(episode),
        intent: PlaybackIntent::Normal,
        direction: Some(NeighbourDirection::Next),
    }
}
fn track(id: i64, kind: TrackKind, lang: &str, selected: bool) -> kinema_mpv::Track {
    kinema_mpv::Track {
        id,
        kind,
        title: None,
        lang: Some(lang.into()),
        selected,
        external: false,
        forced: false,
        hearing_impaired: false,
        codec: None,
        channels: None,
    }
}

struct Rig {
    cx: TestAppContext,
    coordinator: Entity<AppCoordinator>,
    window: WindowHandle<PlayerWindow>,
    requests: Requests,
    events: Rc<RefCell<Vec<NeighbourEvent>>>,
    generation: u64,
}

impl Rig {
    fn new() -> Self {
        let mut cx = TestAppContext::single();
        cx.update(crate::ui::theme::init);
        cx.update(bind_keys);
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        let old = snapshot(1, Some(60_000));
        coordinator.update(&mut cx, |c, _| {
            c.fixture_attach_mailbox();
            c.fixture_window(OLD, OLD_URL, "The Show", target(1), old);
            c.fixture_lease(NEW, NEW_URL);
        });
        let events = Rc::new(RefCell::new(Vec::new()));
        let log = events.clone();
        cx.update(|cx| {
            cx.subscribe(&coordinator, move |_, event: &NeighbourEvent, _| {
                log.borrow_mut().push(event.clone())
            })
            .detach()
        });
        let context = SessionContext {
            progress: Some(PlaybackLaunch {
                subject: old.target,
                epoch: old.epoch,
                run: old.active_run,
                checkpoint: None,
                watched: false,
                intent: PlaybackIntent::Normal,
            }),
            subtitle_target: Some(target(1)),
        };
        let handle = cx.update(|cx| {
            open_fixture(
                OLD,
                kinema_core::media::playback_title("The Show", Some((1, 1, None))),
                context,
                coordinator.clone(),
                cx,
            )
        });
        let Some(window) = handle else {
            panic!("fixture window did not open")
        };
        let requests: Requests = Default::default();
        let sink = requests.clone();
        let generation = window
            .update(&mut cx, move |view, _, _| {
                view.neighbour_requests = Some(Rc::new(Recorder(sink)));
                // An active, authorized run with an observed position to flush.
                if let Some(p) = view.progress.as_mut() {
                    p.phase = ProgressPhase::Active;
                    p.authorized = true;
                    p.generation = Some(1);
                }
                view.state.position = 120.;
                view.state.position_revision = 1;
                view.state.position_observed = true;
                view.state.duration = Some(1500.);
                view.state.seekable = Some(true);
                view.state.tracks = vec![
                    track(1, TrackKind::Audio, "jpn", true),
                    track(2, TrackKind::Subtitle, "en", true),
                ];
                view.state.tracks_available = true;
                view.lease.as_ref().map_or(0, |(_, g)| *g)
            })
            .unwrap_or(0);
        coordinator.update(&mut cx, |c, _| c.fixture_begin_run(OLD, generation));
        Self {
            cx,
            coordinator,
            window,
            requests,
            events,
            generation,
        }
    }

    fn view<R>(&mut self, f: impl FnOnce(&mut PlayerWindow) -> R) -> R {
        self.window
            .update(&mut self.cx, |view, _, _| f(view))
            .unwrap_or_else(|_| panic!("window gone"))
    }
    fn coordinator<R>(
        &mut self,
        f: impl FnOnce(&mut AppCoordinator, &mut Context<AppCoordinator>) -> R,
    ) -> R {
        self.coordinator.update(&mut self.cx, f)
    }
    fn probe(&mut self, session: &str) -> SessionProbe {
        self.coordinator(|c, _| c.fixture_probe(session))
    }
    fn request(&self, preparation: Option<u64>) -> SwapRequest {
        SwapRequest {
            window: OLD.into(),
            session: PlaybackSession {
                id: NEW.into(),
                url: NEW_URL.into(),
            },
            episode: launch(2),
            preparation,
        }
    }
    /// Starts a swap and acknowledges the old run's final save.
    fn swap(&mut self, preparation: Option<u64>) -> Result<(), SwapError> {
        let result = self.swap_held(preparation);
        self.ack();
        result
    }
    /// Starts a swap without acknowledging the save.
    fn swap_held(&mut self, preparation: Option<u64>) -> Result<(), SwapError> {
        let request = self.request(preparation);
        self.coordinator(|c, cx| c.fixture_swap(request, snapshot(2, None), cx))
    }
    fn releases(&mut self) -> Vec<String> {
        self.coordinator(|c, _| {
            c.fixture_drain_requests()
                .into_iter()
                .filter_map(|command| match command.action {
                    AppAction::ReleaseSession(id) => Some(id),
                    _ => None,
                })
                .collect()
        })
    }
    fn lease(&mut self) -> (String, u64) {
        self.view(|v| v.lease.clone().unwrap_or_default())
    }
    fn loads(&mut self) -> Vec<(String, bool)> {
        self.view(|v| {
            v.fixture_backend
                .as_ref()
                .map_or(vec![], |b| b.loads.clone())
        })
    }
    /// The old run's save is in flight, so the swap must wait for it.
    fn hold_save(&mut self) {
        self.coordinator(|c, _| c.fixture_save_in_flight(OLD, true));
    }
    fn ack(&mut self) {
        self.coordinator(|c, cx| c.fixture_acknowledge_save(OLD, cx));
    }
    fn ready(&mut self) {
        let generation = self.lease().1;
        self.coordinator(|c, cx| c.swap_window_report(NEW, generation, true, cx));
    }
    fn failed(&mut self) {
        // The window stops watching before it reports (see `observe_swap`).
        self.view(|v| v.swap = None);
        let generation = self.lease().1;
        self.coordinator(|c, cx| c.swap_window_report(NEW, generation, false, cx));
    }
}

#[test]
fn swap_runs_the_six_steps_in_order() {
    let mut rig = Rig::new();
    rig.hold_save();
    rig.swap_held(None)
        .unwrap_or_else(|e| panic!("swap: {e:?}"));
    // The flush was handed over and is unacknowledged: nothing else happens yet.
    assert_eq!(
        rig.coordinator(|c, _| c.fixture_swap_log()),
        [SwapStep::FlushOld]
    );
    assert_eq!(rig.lease().0, OLD);
    assert!(rig.loads().is_empty());
    assert!(rig.probe(OLD).swapping);

    rig.ack();
    assert_eq!(
        rig.coordinator(|c, _| c.fixture_swap_log()),
        [
            SwapStep::FlushOld,
            SwapStep::NewLease,
            SwapStep::Rekey,
            SwapStep::NewRun,
            SwapStep::Load
        ]
    );
    // The window now belongs to the new lease, under a new generation, with the
    // new episode's identity; a tracked episode loads paused (resume sequence).
    let (session, generation) = rig.lease();
    assert_eq!(session, NEW);
    assert!(generation > rig.generation);
    assert_eq!(rig.loads(), [(NEW_URL.to_owned(), true)]);
    let (title, subtitle_target, progress_subject) = rig.view(|v| {
        (
            v.title.full.clone(),
            v.subtitle_target.clone(),
            v.progress.as_ref().map(|p| p.launch.subject),
        )
    });
    assert_eq!(title, "The Show · S01E02 · Episode 2");
    assert_eq!(subtitle_target, Some(target(2)));
    assert_eq!(progress_subject, Some(snapshot(2, None).target));
    // The old lease is still held, tombstoned, and not yet released.
    let old = rig.probe(OLD);
    assert!(old.tombstoned && !old.embedded && !old.release_pending);
    assert_eq!(old.run_sealed, Some(true));
    assert!(rig.probe(NEW).embedded);
    assert!(rig.releases().is_empty());

    rig.ready();
    assert_eq!(
        rig.coordinator(|c, _| c.fixture_swap_log()).last(),
        Some(&SwapStep::ReleaseOld)
    );
    assert_eq!(rig.releases(), [OLD]);
}

#[test]
fn the_registry_follows_the_window_to_the_new_session() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    let old_generation = rig.generation;
    let (entries, current) = rig.cx.update(|cx| {
        let registry = cx.global::<PlayerWindows>();
        (
            registry.entries.keys().cloned().collect::<Vec<_>>(),
            registry.entries.get(NEW).map(|e| e.generation),
        )
    });
    assert_eq!(entries, [NEW]);
    assert!(current > Some(old_generation));
    // The OS media session reads the same snapshot: new target, new title.
    let snapshots = rig.cx.update(desktop_snapshots);
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].media.title, "The Show · S01E02 · Episode 2");
    assert_eq!(
        snapshots[0].media.target,
        desktop_target(current.unwrap_or_default())
    );
    assert_ne!(snapshots[0].media.target, desktop_target(old_generation));
}

#[test]
fn late_events_and_samples_from_the_old_lease_are_rejected() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    let new_generation = rig.lease().1;
    let old_generation = rig.generation;
    let old_target = snapshot(1, None).target;
    let new_subject = snapshot(2, None).target;
    let sample = |generation, subject, sequence| PlayerCheckpoint {
        generation,
        subject,
        epoch: 4,
        run: 9,
        sequence,
        observation: PlaybackObservation::new(5_000, Some(1_500_000))
            .unwrap_or_else(|| panic!("observation")),
        force: false,
    };
    rig.coordinator(|c, cx| {
        c.fixture_begin_run(NEW, new_generation);
        let before = c.fixture_requests_made();
        // The old session's run is sealed.
        c.submit_player_checkpoint(OLD.into(), sample(old_generation, old_target, 500), cx);
        assert_eq!(c.fixture_run_state(OLD).map(|r| r.2), Some(false));
        // Old generation, old subject, addressed to the new session.
        c.submit_player_checkpoint(NEW.into(), sample(old_generation, old_target, 1), cx);
        c.submit_player_checkpoint(NEW.into(), sample(new_generation, old_target, 1), cx);
        assert_eq!(c.fixture_run_state(NEW), Some((0, new_generation, false)));
        // A ready/begin message from the old window cannot start a run either.
        c.player_progress_ready(OLD.into(), old_generation, 4, false, cx);
        assert_eq!(c.fixture_requests_made(), before);
        // The new run's own sample is accepted.
        c.submit_player_checkpoint(NEW.into(), sample(new_generation, new_subject, 1), cx);
        assert_eq!(c.fixture_run_state(NEW).map(|r| r.0), Some(1));
    });
    // An authorization for the old generation finds no window.
    rig.cx
        .update(|cx| authorize_progress(OLD, old_generation, 4, 4, 9, cx));
    assert_eq!(
        rig.view(|v| v.progress.as_ref().map(|p| p.authorized)),
        Some(false)
    );
    // The old window generation can no longer close the new entry.
    rig.cx
        .update(|cx| release_window(OLD, old_generation, false, cx));
    assert!(rig.probe(NEW).embedded);
    assert!(rig.releases().is_empty());
}

#[test]
fn the_old_lease_is_released_exactly_once_on_success() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.ready();
    rig.ready();
    rig.coordinator(|c, cx| c.swap_window_report(NEW, 0, true, cx));
    // A later close of the window releases only the new lease.
    rig.coordinator(|c, cx| c.embedded_player_closed(NEW.into(), cx));
    assert_eq!(rig.releases(), [OLD, NEW]);
}

#[test]
fn a_failed_swap_releases_the_new_lease_once_and_keeps_the_old_episode() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.failed();
    rig.failed();
    assert_eq!(rig.releases(), [NEW]);
    // The window is back under the old session and resumes from the saved point.
    let (session, generation) = rig.lease();
    assert_eq!(session, OLD);
    assert!(generation > rig.generation);
    assert_eq!(
        rig.loads(),
        [(NEW_URL.to_owned(), true), (OLD_URL.to_owned(), true)]
    );
    let (title, subject, notice) = rig.view(|v| {
        (
            v.title.full.clone(),
            v.subtitle_target.clone(),
            v.neighbour_notice.is_some(),
        )
    });
    assert_eq!(title, "The Show · S01E01");
    assert_eq!(subject, Some(target(1)));
    assert!(notice, "the dock says the episode could not be opened");
    let old = rig.probe(OLD);
    assert!(old.embedded && !old.tombstoned && !old.release_pending);
    assert_eq!(old.run_sealed, Some(false));
    let new = rig.probe(NEW);
    assert!(!new.embedded && new.run_sealed.is_none());
}

#[test]
fn refused_swaps_release_the_new_lease_and_touch_nothing() {
    let mut rig = Rig::new();
    // Unknown window.
    let mut request = rig.request(None);
    request.window = "nobody".into();
    let result = rig.coordinator(|c, cx| c.fixture_swap(request, snapshot(2, None), cx));
    assert_eq!(result, Err(SwapError::NoWindow));
    assert_eq!(rig.releases(), [NEW]);
    // A movie is not an episode.
    let mut rig = Rig::new();
    let mut request = rig.request(None);
    request.episode.target =
        PlaybackTarget::Movie(ImdbId::new("tt7654321").unwrap_or_else(|e| panic!("fixture: {e}")));
    let result = rig.coordinator(|c, cx| c.fixture_swap(request, snapshot(2, None), cx));
    assert_eq!(result, Err(SwapError::NotAnEpisode));
    assert_eq!(rig.releases(), [NEW]);
    assert_eq!(rig.lease().0, OLD);
    assert!(rig.loads().is_empty());
}

#[test]
fn unsaved_progress_abandons_the_swap_after_the_deadline() {
    let mut rig = Rig::new();
    rig.hold_save();
    rig.swap_held(None)
        .unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.coordinator(|c, cx| c.fixture_expire_flush(OLD, cx));
    assert_eq!(rig.releases(), [NEW]);
    assert_eq!(rig.lease().0, OLD);
    assert!(rig.loads().is_empty());
    let old = rig.probe(OLD);
    assert!(old.embedded && !old.swapping && !old.tombstoned);
}

#[test]
fn closing_mid_swap_releases_both_leases_once() {
    let mut rig = Rig::new();
    rig.fixture_pack(5);
    rig.swap(Some(5)).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.coordinator(|c, cx| c.embedded_player_closed(NEW.into(), cx));
    rig.coordinator(|c, cx| c.embedded_player_closed(NEW.into(), cx));
    let mut released = rig.releases();
    released.sort();
    assert_eq!(released, [NEW, OLD]);
    assert!(rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));
}

impl Rig {
    fn fixture_pack(&mut self, id: u64) {
        self.coordinator(|c, _| c.fixture_keep(OLD, id));
    }
}

#[test]
fn the_kept_pack_follows_the_window_and_is_cancelled_when_the_swap_leaves_it() {
    // Same pack: it moves with the window and stays.
    let mut rig = Rig::new();
    rig.fixture_pack(5);
    rig.swap(Some(5)).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.ready();
    assert_eq!(rig.probe(NEW).kept, Some(5));
    assert_eq!(rig.probe(OLD).kept, None);
    assert!(!rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));

    // Leaving the pack: cancelled once the new episode is ready.
    let mut rig = Rig::new();
    rig.fixture_pack(5);
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    assert!(!rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));
    rig.ready();
    assert!(rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));
    assert_eq!(rig.probe(NEW).kept, None);

    // A failed swap keeps the pack with the old episode.
    let mut rig = Rig::new();
    rig.fixture_pack(5);
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.failed();
    assert_eq!(rig.probe(OLD).kept, Some(5));
    assert!(!rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));
}

#[test]
fn the_kept_pack_is_cancelled_on_close_and_on_account_invalidation() {
    let mut rig = Rig::new();
    rig.fixture_pack(5);
    rig.coordinator(|c, cx| c.embedded_player_closed(OLD.into(), cx));
    assert_eq!(rig.probe(OLD).kept, None);
    assert!(rig.coordinator(|c, _| c.fixture_preparation_cancelled(5)));

    let mut rig = Rig::new();
    rig.fixture_pack(6);
    rig.coordinator(|c, cx| {
        c.fixture_apply_app(
            AppEvent {
                generation: 0,
                outcome: AppOutcome::Invalidated {
                    released: vec![OLD.into()],
                },
            },
            cx,
        )
    });
    assert_eq!(rig.probe(OLD).kept, None);
    assert!(rig.coordinator(|c, _| c.fixture_preparation_cancelled(6)));
}

#[test]
fn audio_and_subtitles_carry_over_by_language_and_flags_after_the_new_tracks_appear() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    // Before the new file reports tracks nothing is selected.
    rig.window
        .update(&mut rig.cx, |view, _, cx| view.observe_carry(cx))
        .unwrap_or_else(|_| panic!("window gone"));
    assert!(rig.view(|v| {
        v.fixture_backend
            .as_ref()
            .is_some_and(|b| b.tracks.is_empty())
    }));
    rig.view(|v| {
        v.state.media_generation = 2;
        v.state.tracks = vec![
            track(7, TrackKind::Audio, "en", true),
            track(8, TrackKind::Audio, "ja", false),
            track(9, TrackKind::Subtitle, "fr", false),
            track(10, TrackKind::Subtitle, "eng", false),
        ];
        v.state.tracks_available = true;
    });
    for _ in 0..2 {
        rig.window
            .update(&mut rig.cx, |view, _, cx| view.observe_carry(cx))
            .unwrap_or_else(|_| panic!("window gone"));
    }
    // By language, not by id; applied once.
    assert_eq!(
        rig.view(|v| v.fixture_backend.as_ref().map(|b| b.tracks.clone())),
        Some(vec![(TrackKind::Audio, 8), (TrackKind::Subtitle, 10)])
    );
}

#[test]
fn dock_buttons_shortcuts_and_os_commands_share_one_request_path() {
    use NeighbourState::*;
    let mut rig = Rig::new();
    let press = |rig: &mut Rig, key: &str| {
        rig.cx
            .update_window(rig.window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                let Ok(keystroke) = Keystroke::parse(key) else {
                    panic!("fixture key")
                };
                window.dispatch_event(
                    KeyDownEvent {
                        keystroke,
                        is_held: false,
                        prefer_character_input: false,
                    }
                    .to_platform_input(),
                    cx,
                );
            })
            .unwrap_or_else(|_| panic!("window gone"));
    };
    let set = |rig: &mut Rig, previous, next| {
        rig.cx
            .update(|cx| set_neighbours(OLD, Neighbours { previous, next }, cx));
    };
    // Hidden and disabled: nothing goes out, from any route.
    for state in [Hidden, Disabled] {
        set(&mut rig, state, state);
        press(&mut rig, "shift-n");
        press(&mut rig, "shift-p");
        let target = desktop_target(rig.lease().1);
        assert!(
            !rig.cx
                .update(|cx| dispatch_transport(target, Transport::Next, cx))
        );
        assert!(rig.requests.borrow().is_empty(), "{state:?}");
    }
    // Enabled: the shortcut requests once and the control turns busy at once.
    set(&mut rig, Enabled, Enabled);
    press(&mut rig, "shift-n");
    press(&mut rig, "shift-n");
    assert_eq!(rig.view(|v| v.neighbours.next), Busy);
    let generation = rig.lease().1;
    assert_eq!(
        *rig.requests.borrow(),
        [(OLD.to_owned(), generation, NeighbourDirection::Next)]
    );
    // The OS command is the same request, for the other direction.
    let target = desktop_target(generation);
    assert!(
        rig.cx
            .update(|cx| dispatch_transport(target, Transport::Previous, cx))
    );
    assert_eq!(rig.requests.borrow().len(), 2);
    assert_eq!(rig.requests.borrow()[1].2, NeighbourDirection::Previous);
    // What the OS is told follows the dock: busy and disabled are not advertised.
    let snapshots = rig.cx.update(desktop_snapshots);
    assert!(!snapshots[0].media.can_next && !snapshots[0].media.can_previous);
    set(&mut rig, Enabled, Disabled);
    let snapshots = rig.cx.update(desktop_snapshots);
    assert!(snapshots[0].media.can_previous && !snapshots[0].media.can_next);
}

#[test]
fn the_coordinator_gates_requests_and_reports_failures_to_the_dock() {
    use NeighbourState::*;
    let mut rig = Rig::new();
    // Not enabled: no event, and the window's state is corrected.
    rig.coordinator(|c, cx| c.request_neighbour(OLD, 0, NeighbourDirection::Next, cx));
    assert!(
        rig.events
            .borrow()
            .iter()
            .all(|e| !matches!(e, NeighbourEvent::Requested { .. }))
    );
    rig.coordinator(|c, cx| {
        c.set_neighbours(
            OLD,
            Neighbours {
                previous: Hidden,
                next: Enabled,
            },
            cx,
        )
    });
    rig.coordinator(|c, cx| c.request_neighbour(OLD, 0, NeighbourDirection::Next, cx));
    assert!(rig.events.borrow().contains(&NeighbourEvent::Requested {
        session: OLD.into(),
        direction: NeighbourDirection::Next
    }));
    assert_eq!(rig.view(|v| v.neighbours.next), Busy);
    // A second request while busy is ignored.
    let before = rig.events.borrow().len();
    rig.coordinator(|c, cx| c.request_neighbour(OLD, 0, NeighbourDirection::Next, cx));
    assert_eq!(rig.events.borrow().len(), before);
    // Failure: usable again, current episode untouched, notice shown.
    rig.coordinator(|c, cx| c.neighbour_failed(OLD, NeighbourDirection::Next, cx));
    assert_eq!(rig.view(|v| v.neighbours.next), Enabled);
    assert_eq!(
        rig.view(|v| v.neighbour_notice_text()),
        Some("Couldn’t open the next episode".to_owned())
    );
    assert_eq!(rig.lease().0, OLD);
}

fn end_of_file(rig: &mut Rig) {
    rig.cx.update(|cx| {
        set_neighbours(
            OLD,
            Neighbours {
                previous: NeighbourState::Hidden,
                next: NeighbourState::Enabled,
            },
            cx,
        )
    });
    rig.window
        .update(&mut rig.cx, |view, _, cx| {
            view.state.ended = true;
            view.observe_up_next(cx);
        })
        .unwrap_or_else(|_| panic!("window gone"));
}

#[test]
fn up_next_counts_down_five_seconds_then_requests_the_next_episode_once() {
    let mut rig = Rig::new();
    end_of_file(&mut rig);
    assert_eq!(
        rig.view(|v| v.up_next.as_ref().map(|c| c.remaining)),
        Some(5)
    );
    // The chrome stays up while the card shows, however long the user is idle.
    rig.cx
        .update_window(rig.window.into(), |view, window, cx| {
            let view = view
                .downcast::<PlayerWindow>()
                .unwrap_or_else(|_| panic!("view"));
            view.update(cx, |view, _| {
                view.state.paused = false;
                view.state.ended = false; // playing-state idle rule, card still shown
                view.last_activity = Instant::now() - Duration::from_secs(3600);
                assert!(view.chrome_visible(window));
            });
        })
        .unwrap_or_else(|_| panic!("window gone"));
    rig.view(|v| v.state.ended = true);
    for second in 1..5 {
        rig.cx.executor().advance_clock(timing::UP_NEXT_TICK);
        rig.cx.run_until_parked();
        assert_eq!(
            rig.view(|v| v.up_next.as_ref().map(|c| c.remaining)),
            Some(5 - second)
        );
        assert!(rig.requests.borrow().is_empty());
    }
    rig.cx.executor().advance_clock(timing::UP_NEXT_TICK);
    rig.cx.run_until_parked();
    assert!(rig.view(|v| v.up_next.is_none()));
    assert_eq!(rig.requests.borrow().len(), 1);
    assert_eq!(rig.requests.borrow()[0].2, NeighbourDirection::Next);
    rig.cx
        .executor()
        .advance_clock(timing::UP_NEXT_COUNTDOWN * 2);
    rig.cx.run_until_parked();
    assert_eq!(rig.requests.borrow().len(), 1);
}

#[test]
fn any_input_cancels_the_up_next_countdown() {
    for input in ["key", "click", "escape", "cancel"] {
        let mut rig = Rig::new();
        end_of_file(&mut rig);
        rig.cx.executor().advance_clock(timing::UP_NEXT_TICK * 2);
        rig.cx.run_until_parked();
        assert!(rig.view(|v| v.up_next.is_some()), "{input}");
        rig.cx
            .update_window(rig.window.into(), |_, window, cx| {
                window.draw(cx).clear(cx);
                match input {
                    "key" | "escape" => {
                        let name = if input == "key" { "a" } else { "escape" };
                        let Ok(keystroke) = Keystroke::parse(name) else {
                            panic!("fixture key")
                        };
                        window.dispatch_event(
                            KeyDownEvent {
                                keystroke,
                                is_held: false,
                                prefer_character_input: false,
                            }
                            .to_platform_input(),
                            cx,
                        );
                    }
                    _ => {}
                }
            })
            .unwrap_or_else(|_| panic!("window gone"));
        match input {
            "click" => {
                rig.window
                    .update(&mut rig.cx, |view, _, cx| view.user_input(cx))
                    .unwrap_or_else(|_| panic!("window gone"));
            }
            "cancel" => rig.view(|v| {
                v.cancel_up_next();
            }),
            _ => {}
        }
        assert!(rig.view(|v| v.up_next.is_none()), "{input}");
        rig.cx
            .executor()
            .advance_clock(timing::UP_NEXT_COUNTDOWN * 2);
        rig.cx.run_until_parked();
        assert!(
            rig.requests.borrow().is_empty(),
            "{input}: no request after input"
        );
        // And it does not come back while the file stays at its end.
        rig.window
            .update(&mut rig.cx, |view, _, cx| view.observe_up_next(cx))
            .unwrap_or_else(|_| panic!("window gone"));
        assert!(rig.view(|v| v.up_next.is_none()), "{input}: dismissed");
    }
}

#[test]
fn play_now_requests_at_once_and_closing_the_window_drops_the_countdown() {
    let mut rig = Rig::new();
    end_of_file(&mut rig);
    rig.window
        .update(&mut rig.cx, |view, _, cx| view.play_up_next(cx))
        .unwrap_or_else(|_| panic!("window gone"));
    assert_eq!(rig.requests.borrow().len(), 1);

    let mut rig = Rig::new();
    end_of_file(&mut rig);
    rig.cx
        .update_window(rig.window.into(), |_, window, _| window.remove_window())
        .unwrap_or_else(|_| panic!("window gone"));
    rig.cx
        .executor()
        .advance_clock(timing::UP_NEXT_COUNTDOWN * 2);
    rig.cx.run_until_parked();
    assert!(rig.requests.borrow().is_empty());
}

fn observe(rig: &mut Rig) {
    rig.window
        .update(&mut rig.cx, |view, _, cx| view.observe_swap(cx))
        .unwrap_or_else(|_| panic!("window gone"));
    rig.cx.run_until_parked();
}

#[test]
fn the_window_reports_a_ready_episode_once_and_the_old_lease_goes() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    // Still the outgoing media: not ready.
    observe(&mut rig);
    assert!(rig.releases().is_empty());
    rig.view(|v| {
        v.state.media_generation = 2;
        v.state.file_loaded = true;
        v.state.loading = false;
        v.state.media_stopped = false;
        if let Some(p) = v.progress.as_mut() {
            p.phase = ProgressPhase::Ready;
        }
    });
    observe(&mut rig);
    observe(&mut rig);
    assert_eq!(rig.releases(), [OLD]);
    assert_eq!(rig.lease().0, NEW);
}

#[test]
fn a_failed_incoming_episode_rolls_back_instead_of_closing_the_window() {
    let mut rig = Rig::new();
    rig.swap(None).unwrap_or_else(|e| panic!("swap: {e:?}"));
    rig.view(|v| {
        v.state.media_generation = 2;
        v.state.media_stopped = true;
        v.state.error = Some("open failed".into());
    });
    observe(&mut rig);
    // The failure is masked until the restored media starts its own lifecycle.
    assert!(rig.view(|v| !v.failed()));
    assert_eq!(rig.releases(), [NEW]);
    assert_eq!(rig.lease().0, OLD);
    rig.view(|v| v.state.media_generation = 3);
    assert!(
        rig.view(|v| v.failed()),
        "an error on later media is real again"
    );
}

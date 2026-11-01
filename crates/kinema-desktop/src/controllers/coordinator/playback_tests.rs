//! Focused application-owned progress protocol tests; no native player or I/O.
use super::*;
use crate::catalog::playback_progress::PlaybackCheckpoint;
use ::core::prelude::v1::test;
use kinema_core::{
    library::{TitleId, WatchState},
    watch_progress::{PlaybackObservation, PlaybackProgressTarget},
};

fn required<T>(value: Option<T>) -> T {
    match value {
        Some(value) => value,
        None => panic!("invalid progress fixture"),
    }
}
fn snapshot(position: Option<i64>) -> PlaybackProgressSnapshot {
    PlaybackProgressSnapshot {
        target: PlaybackProgressTarget::Movie(required(TitleId::new(1))),
        watch: Some(WatchState {
            watched: false,
            updated_at: 10,
        }),
        epoch: 20,
        active_run: 19,
        auto_mark_suppressed: false,
        checkpoint: position.map(|position| PlaybackCheckpoint {
            observation: required(PlaybackObservation::new(position, Some(1000))),
            updated_at: 11,
            revision: 20,
        }),
    }
}

#[test]
fn resume_presence_tracks_positive_incomplete_checkpoints_not_epochs_or_duration() {
    assert!(!resumable(&snapshot(None)));
    assert!(!resumable(&snapshot(Some(0))));
    let mut state = snapshot(Some(500));
    assert!(resumable(&state));
    if let Some(checkpoint) = &mut state.checkpoint {
        checkpoint.observation = required(PlaybackObservation::new(500, None));
    }
    assert!(resumable(&state));
    state.epoch += 1;
    state.auto_mark_suppressed = true;
    assert!(resumable(&state));
    if let Some(watch) = &mut state.watch {
        watch.watched = true;
    }
    assert!(!resumable(&state));
    if let Some(watch) = &mut state.watch {
        watch.watched = false;
    }
    assert!(resumable(&state));
    state.checkpoint = None; // Clear / ready start-over.
    assert!(!resumable(&state));
}

#[cfg(feature = "ui-probe-test")]
mod integration {
    use super::*;
    use ::core::prelude::v1::test;
    use gpui::{AppContext, TestAppContext};

    const SESSION: &str = "progress-fixture";
    const REQUEST: u64 = (1 << 63) | 99;

    fn run() -> Run {
        Run {
            snapshot: snapshot(Some(500)),
            generation: 7,
            sequence: 10,
            in_flight: true,
            latest: None,
            retries: 0,
            retry_at: None,
            ready: None,
        }
    }
    fn sample(sequence: u64, position: i64) -> PlayerCheckpoint {
        PlayerCheckpoint {
            generation: 7,
            subject: snapshot(None).target,
            epoch: 20,
            run: 19,
            sequence,
            observation: required(PlaybackObservation::new(position, Some(1000))),
            force: false,
        }
    }
    fn save_event(
        sample: PlayerCheckpoint,
        result: Result<PlaybackProgressResult, String>,
    ) -> LocalEvent {
        LocalEvent {
            request: LocalRequest {
                request_id: REQUEST,
                generation: 0,
                operation: LibraryOperation::PlaybackProgress(PlaybackProgressOperation::Save {
                    target: sample.subject,
                    run: sample.run,
                    epoch: sample.epoch,
                    observation: sample.observation,
                }),
            },
            revision: 12,
            result: result.map(LibraryResult::PlaybackProgress),
        }
    }
    fn pending_save(c: &mut AppCoordinator, sample: PlayerCheckpoint) {
        c.playback.pending.insert(
            REQUEST,
            PendingProgress::Save {
                session: SESSION.into(),
                sample,
            },
        );
    }

    #[test]
    fn applying_movie_snapshots_updates_personal_resume_presence_without_losing_checkpoint() {
        use crate::controllers::coordinator::{
            BrowseTitle, LocalBrowseTitle, PersonalLoad, PersonalTitle,
        };
        use kinema_core::catalog_normalized::CanonicalTitle;
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, _| {
            let title = BrowseTitle::Local(LocalBrowseTitle::new(CanonicalTitle {
                id: 1,
                kind: MediaKind::Movie,
                identifiers: vec![],
                fields: Default::default(),
                provenance: vec![],
                ratings: vec![],
                updated_at: 0,
            }));
            c.fixture_personal(
                &title,
                PersonalTitle {
                    title_id: required(TitleId::new(1)),
                    kind: MediaKind::Movie,
                    added_at: None,
                    movie_watch: snapshot(None).watch,
                    playback_progress: Some(snapshot(None)),
                    show_summary: None,
                    has_resume: false,
                },
                vec![],
            );
            for (position, watched, expected) in [
                (Some(500), false, true),
                (Some(500), true, false),
                (Some(500), false, true),
                (Some(0), false, false),
                (None, false, false),
            ] {
                let mut next = snapshot(position);
                if let Some(watch) = &mut next.watch {
                    watch.watched = watched;
                }
                c.apply_playback_snapshot(next);
                let PersonalLoad::Ready(personal) = c.personal(&title).state else {
                    panic!("personal snapshot missing")
                };
                assert_eq!(personal.has_resume, expected);
                assert_eq!(personal.playback_progress, Some(next));
            }
        });
    }

    #[test]
    fn samples_reject_stale_identity_generation_epoch_and_sequence_then_keep_backward_position() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            c.playback.runs.insert(SESSION.into(), run());
            let mut wrong_generation = sample(11, 900);
            wrong_generation.generation += 1;
            let mut wrong_target = sample(11, 900);
            wrong_target.subject = PlaybackProgressTarget::Movie(required(TitleId::new(2)));
            let mut wrong_epoch = sample(11, 900);
            wrong_epoch.epoch += 1;
            let mut wrong_run = sample(11, 900);
            wrong_run.run += 1;
            for stale in [
                sample(9, 900),
                sample(10, 900),
                wrong_generation,
                wrong_target,
                wrong_epoch,
                wrong_run,
            ] {
                c.submit_player_checkpoint(SESSION.into(), stale, cx);
            }
            assert_eq!(c.playback.runs[SESSION].sequence, 10);
            assert!(c.playback.runs[SESSION].latest.is_none());
            assert_eq!(c.playback.next, 0);
            c.submit_player_checkpoint(SESSION.into(), sample(11, 800), cx);
            c.submit_player_checkpoint(SESSION.into(), sample(12, 100), cx);
            c.submit_player_checkpoint(SESSION.into(), sample(11, 900), cx);
            let latest = required(c.playback.runs[SESSION].latest.as_ref());
            assert_eq!(latest.sequence, 12);
            assert_eq!(latest.observation.position_ms(), 100);
            assert_eq!(c.playback.next, 0); // In-flight coalescing never submits concurrently.
        });
    }

    #[test]
    fn forced_checkpoint_attempts_submission_during_four_second_backoff() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            let mut state = run();
            state.in_flight = false;
            state.retries = 3;
            let backoff = Instant::now() + Duration::from_secs(4);
            state.retry_at = Some(backoff);
            c.playback.runs.insert(SESSION.into(), state);
            c.submit_player_checkpoint(SESSION.into(), sample(11, 600), cx);
            assert_eq!(c.playback.next, 0);
            assert_eq!(c.playback.runs[SESSION].retry_at, Some(backoff));
            let mut final_sample = sample(12, 200);
            final_sample.force = true;
            c.submit_player_checkpoint(SESSION.into(), final_sample, cx);
            // Fixture has no worker: incremented ID proves the immediate attempt,
            // and its retained observation proves submission failure was not loss.
            assert_eq!(c.playback.next, 1);
            assert!(!c.playback.runs[SESSION].in_flight);
            assert_eq!(
                required(c.playback.runs[SESSION].latest.as_ref())
                    .observation
                    .position_ms(),
                200
            );
            assert!(c.notice.is_some());
        });
    }

    #[test]
    fn ready_during_save_is_latched_and_attempted_after_acknowledgement() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            c.playback.runs.insert(SESSION.into(), run());
            c.player_progress_ready(SESSION.into(), 8, 20, true, cx);
            assert_eq!(c.playback.runs[SESSION].ready, Some((8, 20, true)));
            assert_eq!(c.playback.next, 0);
            pending_save(c, sample(10, 500));
            assert!(c.apply_playback_event(
                &save_event(
                    sample(10, 500),
                    Ok(PlaybackProgressResult::Applied(snapshot(Some(500))))
                ),
                cx
            ));
            assert_eq!(c.playback.next, 1); // Begin attempted, unavailable fixture worker.
            assert_eq!(c.playback.runs[SESSION].generation, 8);
            assert!(c.playback.runs[SESSION].ready.is_none());
            assert!(!c.playback.runs[SESSION].in_flight);
        });
    }

    #[test]
    fn latched_ready_survives_manual_epoch_advance_in_stale_save_acknowledgement() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            c.playback.runs.insert(SESSION.into(), run());
            c.player_progress_ready(SESSION.into(), 8, 20, true, cx);
            pending_save(c, sample(10, 500));
            let mut manual = snapshot(Some(500));
            manual.epoch = 21;
            manual.auto_mark_suppressed = true;
            assert!(c.apply_playback_event(
                &save_event(sample(10, 500), Ok(PlaybackProgressResult::Stale(manual))),
                cx
            ));
            // The explicit new run must handshake at the new durable epoch,
            // rather than consuming Ready and silently rejecting its old epoch.
            assert_eq!(c.playback.next, 1);
            assert_eq!(c.playback.runs[SESSION].generation, 8);
        });
    }

    #[test]
    fn successful_begin_discards_old_run_retry_and_coalesced_samples() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            let mut state = run();
            state.latest = Some(sample(11, 400));
            state.retries = 2;
            state.retry_at = Some(Instant::now() + Duration::from_secs(2));
            c.playback.runs.insert(SESSION.into(), state);
            c.playback.pending.insert(
                REQUEST,
                PendingProgress::Begin {
                    session: SESSION.into(),
                    generation: 7,
                    epoch: 20,
                },
            );
            let mut begun = snapshot(Some(500));
            begun.epoch = 21;
            begun.active_run = 21;
            let event = LocalEvent {
                request: LocalRequest {
                    request_id: REQUEST,
                    generation: 0,
                    operation: LibraryOperation::PlaybackProgress(
                        PlaybackProgressOperation::BeginRun {
                            target: begun.target,
                            start_over: false,
                        },
                    ),
                },
                revision: 12,
                result: Ok(LibraryResult::PlaybackProgress(
                    PlaybackProgressResult::Applied(begun),
                )),
            };
            assert!(c.apply_playback_event(&event, cx));
            let state = &c.playback.runs[SESSION];
            assert_eq!(state.snapshot, begun);
            assert_eq!(state.sequence, 0);
            assert!(!state.in_flight);
            assert!(state.latest.is_none());
            assert_eq!(state.retries, 0);
            assert!(state.retry_at.is_none());
        });
    }

    #[test]
    fn stale_save_discards_queued_pre_reset_samples_and_preserves_suppression() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            let mut state = run();
            state.latest = Some(sample(12, 999));
            c.playback.runs.insert(SESSION.into(), state);
            pending_save(c, sample(11, 950));
            let mut manual = snapshot(Some(500));
            manual.epoch = 21;
            manual.auto_mark_suppressed = true;
            assert!(c.apply_playback_event(
                &save_event(sample(11, 950), Ok(PlaybackProgressResult::Stale(manual))),
                cx
            ));
            let state = &c.playback.runs[SESSION];
            assert_eq!(state.snapshot, manual);
            assert!(state.latest.is_none());
            assert!(!state.in_flight);
            assert_eq!(c.playback.next, 0);
        });
    }

    #[test]
    fn late_prior_generation_acknowledgement_cannot_release_current_in_flight_write() {
        let mut cx = TestAppContext::single();
        let owner = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        owner.update(&mut cx, |c, cx| {
            let mut state = run();
            state.generation = 8;
            state.latest = Some(sample(12, 200));
            c.playback.runs.insert(SESSION.into(), state);
            pending_save(c, sample(11, 900));
            let mut old = snapshot(Some(900));
            old.epoch = 19;
            assert!(c.apply_playback_event(
                &save_event(sample(11, 900), Ok(PlaybackProgressResult::Applied(old))),
                cx
            ));
            let state = &c.playback.runs[SESSION];
            assert_eq!(state.snapshot, snapshot(Some(500)));
            assert!(state.in_flight);
            assert_eq!(
                required(state.latest.as_ref()).observation.position_ms(),
                200
            );
        });
    }
}

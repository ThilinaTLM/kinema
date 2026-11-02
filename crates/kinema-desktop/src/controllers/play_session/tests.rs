use super::{AfterPrepare, after_prepare};
use kinema_core::sources::{FileSelection, RemoteFile};

fn files() -> Vec<RemoteFile> {
    ["a", "b", "c"]
        .into_iter()
        .map(|id| RemoteFile {
            id: id.into(),
            path: format!("{id}.mkv"),
            size: 1,
        })
        .collect()
}
#[test]
fn a_unique_selection_plays_that_file_without_asking() {
    assert_eq!(
        after_prepare(&FileSelection::Unique(1), &files()),
        AfterPrepare::Play("b".into())
    );
}
#[test]
fn anything_else_shows_the_clear_message_never_a_picker() {
    let message = |selection| match after_prepare(&selection, &files()) {
        AfterPrepare::Message(m) => m,
        other => panic!("expected a message, got {other:?}"),
    };
    let expected = "Couldn't identify the file in this torrent — try another stream.";
    assert_eq!(message(FileSelection::NotFound), expected);
    assert_eq!(message(FileSelection::Ambiguous(vec![0, 2])), expected);
    // An index outside the prepared files is never played.
    assert_eq!(message(FileSelection::Unique(9)), expected);
}
#[test]
fn byte_labels_are_bounded_and_provider_names_are_human_readable() {
    assert_eq!(super::format_bytes(0), "0 B");
    assert_eq!(super::format_bytes(1024), "1.0 KiB");
    assert_eq!(super::format_bytes(1_048_576), "1.0 MiB");
    assert_eq!(super::format_bytes(1_073_741_824), "1.0 GiB");
    assert_eq!(
        super::provider_name(kinema_core::sources::DebridKind::RealDebrid),
        "Real-Debrid"
    );
}
#[cfg(feature = "ui-probe-test")]
#[test]
fn account_invalidation_clears_the_preparation() {
    use gpui::{AppContext, TestAppContext};
    let mut cx = TestAppContext::single();
    let coordinator = cx.new(|cx| super::AppCoordinator::fixture(vec![], None, cx));
    let session =
        cx.new(|cx| super::PlaySession::fixture(coordinator.clone(), "play-live-ready", cx));
    coordinator.update(&mut cx, |_, cx| {
        cx.emit(super::CoordinatorEvent::App(crate::app::AppEvent {
            generation: 0,
            outcome: super::AppOutcome::Invalidated { released: vec![] },
        }))
    });
    cx.run_until_parked();
    session.update(&mut cx, |session, _| {
        assert!(session.preparation.is_none());
        assert!(session.pending.is_none());
        assert!(session.missing);
    });
}
#[cfg(feature = "ui-probe-test")]
mod switch {
    use super::super::{AppCoordinator, AppOutcome, CoordinatorEvent, PlaySession};
    use crate::app::{AppEvent, CredentialState};
    use gpui::{AppContext, Entity, TestAppContext};

    fn emit(cx: &mut TestAppContext, coordinator: &Entity<AppCoordinator>, event: AppEvent) {
        coordinator.update(cx, |_, cx| cx.emit(CoordinatorEvent::App(event)));
        cx.run_until_parked();
    }
    fn invalidated() -> AppEvent {
        AppEvent {
            generation: 0,
            outcome: AppOutcome::Invalidated { released: vec![] },
        }
    }
    fn switching(cx: &mut TestAppContext) -> (Entity<AppCoordinator>, Entity<PlaySession>) {
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        let sheet =
            cx.new(|cx| PlaySession::fixture(coordinator.clone(), "play-live-switching", cx));
        (coordinator, sheet)
    }

    #[test]
    fn own_provider_switch_does_not_abandon_the_queued_preparation() {
        let mut cx = TestAppContext::single();
        let (coordinator, sheet) = switching(&mut cx);
        emit(&mut cx, &coordinator, invalidated());
        sheet.update(&mut cx, |sheet, _| {
            assert_eq!(sheet.pending, Some(100));
            assert!(!sheet.missing);
        });
        emit(
            &mut cx,
            &coordinator,
            AppEvent {
                generation: 50,
                outcome: AppOutcome::Settings {
                    settings: Default::default(),
                    credentials: vec![],
                    tmdb_credential: CredentialState::Missing,
                },
            },
        );
        // After the switch settles, a later account change is a real invalidation.
        emit(&mut cx, &coordinator, invalidated());
        sheet.update(&mut cx, |sheet, _| {
            assert_eq!(sheet.switch, None);
            assert!(sheet.pending.is_none());
            assert!(sheet.missing);
        });
    }

    #[test]
    fn failed_provider_switch_cancels_preparation_with_the_old_account() {
        let mut cx = TestAppContext::single();
        let (coordinator, sheet) = switching(&mut cx);
        emit(
            &mut cx,
            &coordinator,
            AppEvent {
                generation: 50,
                outcome: AppOutcome::Failed {
                    message: "settings unavailable".into(),
                },
            },
        );
        sheet.update(&mut cx, |sheet, _| {
            assert!(sheet.pending.is_none());
            assert_eq!(
                sheet.error.as_deref(),
                Some("Couldn't switch to Real-Debrid: settings unavailable")
            );
        });
    }
}

#[cfg(feature = "ui-probe-test")]
mod events {
    use super::super::{AppOutcome, CoordinatorEvent, PlaySession};
    use crate::{
        app::AppEvent, controllers::coordinator::AppCoordinator, providers::PreparedTorrent,
    };
    use gpui::{AppContext, TestAppContext};
    use kinema_core::{
        catalog::ImdbId,
        sources::{DebridKind, FileSelection, InfoHash, PlaybackTarget, RemoteFile, TorrentSource},
    };

    fn prepared() -> PreparedTorrent {
        PreparedTorrent {
            provider: DebridKind::RealDebrid,
            source: TorrentSource {
                hash: InfoHash::new("a".repeat(40)).unwrap_or_else(|e| panic!("hash: {e}")),
                file_index: None,
                filename: None,
            },
            job_id: "job".into(),
            files: vec![RemoteFile {
                id: "1".into(),
                path: "movie.mkv".into(),
                size: 1,
            }],
        }
    }
    fn outcome(selection: FileSelection) -> AppOutcome {
        AppOutcome::Prepared {
            prepared: prepared(),
            target: PlaybackTarget::Movie(
                ImdbId::new("tt1234567").unwrap_or_else(|e| panic!("id: {e}")),
            ),
            selection,
        }
    }
    fn run(selection: FileSelection) -> (Option<String>, Option<u64>) {
        let mut cx = TestAppContext::single();
        let coordinator = cx.new(|cx| AppCoordinator::fixture(vec![], None, cx));
        let session =
            cx.new(|cx| PlaySession::fixture(coordinator.clone(), "play-live-preparing", cx));
        coordinator.update(&mut cx, |_, cx| {
            cx.emit(CoordinatorEvent::App(AppEvent {
                generation: 100,
                outcome: outcome(selection),
            }))
        });
        cx.run_until_parked();
        session.update(&mut cx, |s, _| (s.error.clone(), s.pending))
    }

    #[test]
    fn non_unique_selection_shows_the_message_and_issues_no_playback() {
        for selection in [FileSelection::NotFound, FileSelection::Ambiguous(vec![0])] {
            let (error, pending) = run(selection);
            assert_eq!(
                error.as_deref(),
                Some("Couldn't identify the file in this torrent — try another stream.")
            );
            assert_eq!(pending, None);
        }
    }

    #[test]
    fn unique_selection_submits_play_file_immediately() {
        // The offline coordinator has no application worker, so the submitted
        // PlayFile is refused; that refusal proves it was attempted and that
        // no file-choice step intervened.
        let (error, pending) = run(FileSelection::Unique(0));
        assert_eq!(pending, None);
        assert_eq!(
            error.as_deref(),
            Some("Application service unavailable or busy. Retry.")
        );
    }
}

#[cfg(feature = "ui-probe-test")]
#[test]
fn inline_phases_follow_the_workflow_state() {
    use super::{PlayPhase, PlaySession};
    use gpui::{AppContext, TestAppContext};
    let mut cx = TestAppContext::single();
    let coordinator = cx.new(|cx| super::AppCoordinator::fixture(vec![], None, cx));
    let phase = |cx: &mut TestAppContext, scene: &str| {
        let session = cx.new(|cx| PlaySession::fixture(coordinator.clone(), scene, cx));
        session.read_with(cx, |s, _| (s.phase(), s.busy()))
    };
    assert_eq!(
        phase(&mut cx, "play-live-preparing"),
        (PlayPhase::Preparing, true)
    );
    assert_eq!(
        phase(&mut cx, "play-live-ready"),
        (PlayPhase::Starting, true)
    );
    assert!(matches!(
        phase(&mut cx, "play-live-failure"),
        (PlayPhase::Failed(_), false)
    ));
    assert!(matches!(
        phase(&mut cx, "play-live-missing-creds"),
        (PlayPhase::Blocked(_), false)
    ));
}

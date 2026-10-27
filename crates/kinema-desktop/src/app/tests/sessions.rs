//! Offline replay, dismissal, and lease-failure integration tests.
use super::*;

#[test]
fn replay_reuses_lease_and_released_sessions_cannot_replay_or_reveal()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("film ; literal.mkv");
    std::fs::write(&file, b"fixture")?;
    let trace = Arc::new(PlaybackTrace::default());
    let worker = composed_worker(root.path(), String::new(), trace.clone(), false, false)?;
    let AppOutcome::PlaybackStarted { session, .. } =
        command_result(&worker, 1, AppAction::PlayLocal { path: file })?
    else {
        return Err("local launch failed".into());
    };
    assert!(matches!(
        command_result(
            &worker,
            2,
            AppAction::ReplaySession {
                session_id: session.id.clone()
            }
        )?,
        AppOutcome::SessionRelaunched { .. }
    ));
    let urls = trace
        .launched
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    assert_eq!(urls, vec![session.url.clone(), session.url]);
    command_result(&worker, 3, AppAction::ReleaseSession(session.id.clone()))?;
    assert!(matches!(
        command_result(
            &worker,
            4,
            AppAction::ReplaySession {
                session_id: session.id.clone()
            }
        )?,
        AppOutcome::Failed { .. }
    ));
    assert!(matches!(
        command_result(
            &worker,
            5,
            AppAction::RevealSession {
                session_id: session.id
            }
        )?,
        AppOutcome::Failed { .. }
    ));
    assert_eq!(
        trace
            .launched
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len(),
        2
    );
    Ok(())
}
#[test]
fn dismissed_completed_preparation_is_not_launchable() -> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let trace = Arc::new(PlaybackTrace::default());
    let worker = composed_worker(root.path(), String::new(), trace.clone(), false, false)?;
    assert!(matches!(
        command_result(
            &worker,
            1,
            AppAction::Prepare {
                target: episode(1)?,
                candidate: fixture_candidate()?
            }
        )?,
        AppOutcome::Prepared { .. }
    ));
    assert!(worker.cancel_request(1));
    assert!(matches!(
        command_result(
            &worker,
            2,
            AppAction::PlayFile {
                preparation: 1,
                file_id: "episode-1".into()
            }
        )?,
        AppOutcome::Failed { .. }
    ));
    assert!(
        trace
            .launched
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_empty()
    );
    Ok(())
}
#[test]
fn dismissal_cancels_only_the_matching_queued_generation() {
    let worker = unstarted_worker();
    assert!(worker.submit(AppCommand {
        generation: 1,
        action: AppAction::PlayLocal {
            path: "first".into()
        }
    }));
    assert!(worker.submit(AppCommand {
        generation: 2,
        action: AppAction::LoadSettings
    }));
    assert!(worker.cancel_request(1));
    let mail = worker
        .shared
        .mailbox
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert!(mail.requests[0].cancel.load(Ordering::Acquire));
    assert!(!mail.requests[1].cancel.load(Ordering::Acquire));
}
#[test]
fn replay_launch_failure_does_not_release_or_duplicate_existing_session()
-> Result<(), Box<dyn std::error::Error>> {
    let root = tempfile::tempdir()?;
    let file = root.path().join("fixture.mkv");
    std::fs::write(&file, b"fixture")?;
    let fail = Arc::new(AtomicBool::new(false));
    let launch_fail = fail.clone();
    let mut app = Application {
        paths: Some(AppPaths {
            config: root.path().into(),
            data: root.path().into(),
            cache: root.path().into(),
        }),
        settings: Some(Settings {
            player: player::PlayerChoice::Mpv,
            ..Settings::default()
        }),
        io: PlaybackIo {
            launch: Box::new(move |_, _| {
                if launch_fail.load(Ordering::Acquire) {
                    Err(player::PlayerError::SpawnFailed)
                } else {
                    Ok(None)
                }
            }),
            ..PlaybackIo::default()
        },
        ..Application::default()
    };
    let AppOutcome::PlaybackStarted { session, .. } = app.execute(
        AppCommand {
            generation: 1,
            action: AppAction::PlayLocal { path: file },
        },
        Arc::new(AtomicBool::new(false)),
        Arc::new(AtomicBool::new(false)),
    )?
    else {
        return Err("fixture launch failed".into());
    };
    fail.store(true, Ordering::Release);
    assert!(
        app.execute(
            AppCommand {
                generation: 2,
                action: AppAction::ReplaySession {
                    session_id: session.id.clone()
                }
            },
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false))
        )
        .is_err()
    );
    assert_eq!(app.sessions.len(), 1);
    assert_eq!(app.media()?.session_updates().len(), 1);
    assert_eq!(app.media()?.session(&session.id)?.url, session.url);
    Ok(())
}

#[test]
fn unknown_replay_and_reveal_fail_before_initializing_any_services() {
    let mut app = Application::default();
    for action in [
        AppAction::ReplaySession {
            session_id: "unknown".into(),
        },
        AppAction::RevealSession {
            session_id: "unknown".into(),
        },
    ] {
        assert!(
            app.execute(
                AppCommand {
                    generation: 1,
                    action
                },
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false))
            )
            .is_err()
        );
        assert!(app.media.is_none());
        assert!(app.paths.is_none());
        assert!(app.settings.is_none());
    }
}

#[test]
fn matching_active_generation_is_canceled_without_stopping_worker() {
    let worker = unstarted_worker();
    let active = Arc::new(AtomicBool::new(false));
    {
        let mut mail = worker
            .shared
            .mailbox
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        mail.active_generation = Some(7);
        mail.active = Some(active.clone());
    }
    assert!(worker.cancel_request(7));
    assert!(active.load(Ordering::Acquire));
    assert!(!worker.is_stopped());
}

#[test]
fn unknown_cancellations_do_not_allocate_unbounded_control_work() {
    let worker = unstarted_worker();
    for generation in 1..1000 {
        assert!(!worker.cancel_request(generation));
    }
    let mail = worker
        .shared
        .mailbox
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    assert!(mail.discarded_preparations.is_empty());
}

#[test]
fn replay_preserves_original_player_across_preference_changes()
-> Result<(), Box<dyn std::error::Error>> {
    for original in [
        player::PlayerChoice::Embedded,
        player::PlayerChoice::Mpv,
        player::PlayerChoice::Vlc,
    ] {
        let root = tempfile::tempdir()?;
        let file = root.path().join("fixture.mkv");
        std::fs::write(&file, b"fixture")?;
        let launches = Arc::new(std::sync::Mutex::new(Vec::new()));
        let recorded = launches.clone();
        let mut app = Application {
            paths: Some(AppPaths {
                config: root.path().into(),
                data: root.path().into(),
                cache: root.path().into(),
            }),
            settings: Some(Settings {
                player: original.clone(),
                ..Settings::default()
            }),
            io: PlaybackIo {
                launch: Box::new(move |choice, _| {
                    recorded
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .push(choice.clone());
                    Ok(None)
                }),
                ..PlaybackIo::default()
            },
            ..Application::default()
        };
        let execute = |app: &mut Application, generation, action| {
            app.execute(
                AppCommand { generation, action },
                Arc::new(AtomicBool::new(false)),
                Arc::new(AtomicBool::new(false)),
            )
        };
        let initial = execute(&mut app, 1, AppAction::PlayLocal { path: file })?;
        let embedded = original.is_managed();
        let session = match initial {
            AppOutcome::EmbeddedPlaybackStarted {
                session,
                file_name,
                mode,
            } if embedded => {
                assert_eq!(mode, original);
                assert_eq!(file_name, "fixture.mkv");
                session
            }
            AppOutcome::PlaybackStarted { session, .. } if !embedded => session,
            _ => return Err("wrong initial launch backend".into()),
        };
        assert_eq!(app.sessions.len(), 1);
        assert_eq!(app.sessions[0].player, original);
        // Simulate an accepted preference save without involving disk I/O.
        app.settings.as_mut().ok_or("settings missing")?.player = if embedded {
            player::PlayerChoice::Vlc
        } else {
            player::PlayerChoice::Embedded
        };
        let replay = execute(
            &mut app,
            2,
            AppAction::ReplaySession {
                session_id: session.id.clone(),
            },
        )?;
        if embedded {
            let AppOutcome::EmbeddedPlaybackStarted {
                session: replay,
                file_name,
                mode,
            } = replay
            else {
                return Err("embedded replay changed backend".into());
            };
            assert_eq!(mode, original);
            assert_eq!(replay.id, session.id);
            assert_eq!(replay.url, session.url);
            assert_eq!(file_name, "fixture.mkv");
            assert!(
                launches
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_empty()
            );
            assert!(app.children.is_empty());
        } else {
            assert!(matches!(replay, AppOutcome::SessionRelaunched { .. }));
            assert_eq!(
                *launches.lock().unwrap_or_else(|e| e.into_inner()),
                vec![original.clone(), original]
            );
        }
        assert_eq!(app.sessions.len(), 1);
        execute(&mut app, 3, AppAction::ReleaseSession(session.id.clone()))?;
        assert!(app.sessions.is_empty());
        assert!(app.media()?.session(&session.id).is_err());
    }
    Ok(())
}

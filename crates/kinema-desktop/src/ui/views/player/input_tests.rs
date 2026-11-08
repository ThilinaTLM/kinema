use super::*;
use ::core::prelude::v1::test;
use gpui::TestAppContext;

fn key(window: &mut Window, cx: &mut App, name: &str, held: bool, release: bool) -> bool {
    let Ok(keystroke) = Keystroke::parse(name) else {
        panic!("invalid fixture key")
    };
    let consumed = !window
        .dispatch_event(
            KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: held,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        )
        .propagate;
    if release {
        window.dispatch_event(KeyUpEvent { keystroke }.to_platform_input(), cx);
    }
    consumed
}
fn mouse(
    window: &mut Window,
    cx: &mut App,
    position: Point<Pixels>,
    button: MouseButton,
    count: usize,
) {
    window.dispatch_event(
        MouseDownEvent {
            position,
            button,
            click_count: count,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseUpEvent {
            position,
            button,
            click_count: count,
            ..Default::default()
        }
        .to_platform_input(),
        cx,
    );
}
#[test]
fn real_dispatch_repeat_help_focus_and_surface_hit_testing()
-> Result<(), Box<dyn std::error::Error>> {
    let mut cx = TestAppContext::single();
    cx.update(crate::ui::theme::init);
    cx.update(bind_keys);
    let window = cx.add_window(|window, cx| {
        let mut view = PlayerWindow::new_mode(
            kinema_core::media::playback_title("Fixture", None),
            None,
            PlayerChoice::Embedded,
            window,
            cx,
        );
        view.startup_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        view.state.loading = false;
        if let Some(title_id) = kinema_core::library::TitleId::new(1) {
            view.progress = Some(Progress::new(PlaybackLaunch {
                subject: kinema_core::watch_progress::PlaybackProgressTarget::Movie(title_id),
                epoch: 1,
                run: 1,
                checkpoint: None,
                watched: false,
                intent: PlaybackIntent::Normal,
            }));
        }
        view._updates = Task::ready(());
        view
    });
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        key(window, cx, "f", false, false);
        assert!(window.is_fullscreen());
        key(window, cx, "f", true, false);
        assert!(window.is_fullscreen(), "held toggle must be suppressed");
        window.dispatch_event(
            KeyUpEvent {
                keystroke: Keystroke::parse("f")?,
            }
            .to_platform_input(),
            cx,
        );
        key(window, cx, "f", false, true);
        assert!(!window.is_fullscreen());
        for symbol in [
            "[",
            "]",
            "#",
            "shift-j",
            "ctrl-left",
            "shift-left",
            "shift-3->#",
        ] {
            assert!(
                key(window, cx, symbol, false, true),
                "registered logical key {symbol}"
            );
        }
        // Replay captured native X11 structs, not a parsed shift-/ pair.
        let native_help_key = |window: &mut Window, cx: &mut App| {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke {
                        key: "?".into(),
                        key_char: Some("?".into()),
                        modifiers: Modifiers::default(),
                    },
                    is_held: false,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                KeyUpEvent {
                    keystroke: Keystroke {
                        key: "/".into(),
                        key_char: Some("/".into()),
                        modifiers: Modifiers::default(),
                    },
                }
                .to_platform_input(),
                cx,
            );
        };
        native_help_key(window, cx);
        view.update(cx, |view, _| assert!(view.help_focus.is_some()));
        key(window, cx, "tab", false, true);
        key(window, cx, "escape", false, true);
        key(window, cx, "?", true, false);
        view.update(cx, |view, _| {
            assert!(
                view.help_focus.is_none(),
                "native repeat cannot reopen help"
            )
        });
        native_help_key(window, cx);
        view.update(cx, |view, _| {
            assert!(
                view.help_focus.is_some(),
                "native normalized symbol reopens after mismatched keyup"
            )
        });
        key(window, cx, "escape", false, true);
        key(window, cx, "?", false, true);
        view.update(cx, |view, _| assert!(view.help_focus.is_some()));
        key(window, cx, "f", false, true);
        assert!(!window.is_fullscreen(), "help must isolate shortcuts");
        key(window, cx, "tab", false, true);
        view.update(cx, |view, _| {
            assert!(view.help_dismiss_focus.is_focused(window))
        });
        mouse(window, cx, point(px(200.), px(100.)), MouseButton::Left, 2);
        assert!(!window.is_fullscreen(), "help excludes video gestures");
        key(window, cx, "escape", false, true);
        view.update(cx, |view, _| assert!(view.surface_focus.is_focused(window)));
        key(window, cx, "?", false, true);
        key(window, cx, "tab", false, true);
        key(window, cx, "enter", false, true);
        view.update(cx, |view, _| {
            assert!(
                view.help_focus.is_none(),
                "help close is keyboard accessible"
            )
        });
        // Key actions invalidate the frame; native rendering draws before
        // the next pointer event. Refresh stale overlay hitboxes in the mock.
        window.draw(cx).clear(cx);
        let center = point(
            window.viewport_size().width / 2.,
            window.viewport_size().height / 2.,
        );
        mouse(window, cx, center, MouseButton::Right, 1);
        view.update(cx, |view, _| {
            assert!(
                view.progress.as_ref().is_some_and(|p| !p.play),
                "right down/up toggles once"
            )
        });
        mouse(window, cx, point(px(30.), px(25.)), MouseButton::Right, 1);
        view.update(cx, |view, _| {
            assert!(
                view.progress.as_ref().is_some_and(|p| !p.play),
                "header right click must not toggle"
            )
        });
        mouse(
            window,
            cx,
            point(
                window.viewport_size().width - px(25.),
                window.viewport_size().height / 2. - px(90.),
            ),
            MouseButton::Right,
            1,
        );
        view.update(cx, |view, _| {
            assert!(
                view.progress.as_ref().is_some_and(|p| !p.play),
                "volume panel padding excludes surface gestures"
            )
        });
        mouse(window, cx, center, MouseButton::Left, 1);
        assert!(!window.is_fullscreen());
        mouse(window, cx, center, MouseButton::Left, 2);
        assert!(
            window.is_fullscreen(),
            "native double click toggles exactly once"
        );
        mouse(window, cx, point(px(30.), px(25.)), MouseButton::Left, 2);
        assert!(
            window.is_fullscreen(),
            "header must exclude surface gestures"
        );
        // An unrelated focused widget inside the player owns its key semantics.
        let foreign_focus = cx.focus_handle();
        foreign_focus.focus(window, cx);
        key(window, cx, "f", false, true);
        assert!(window.is_fullscreen());
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    Ok(())
}
#[test]
fn file_drops_reach_the_player_over_the_video_surface() -> Result<(), Box<dyn std::error::Error>> {
    let mut cx = TestAppContext::single();
    cx.update(crate::ui::theme::init);
    cx.update(bind_keys);
    let window = cx.add_window(|window, cx| {
        let mut view = PlayerWindow::new_mode(
            kinema_core::media::playback_title("Fixture", None),
            None,
            PlayerChoice::Embedded,
            window,
            cx,
        );
        view.startup_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        view.state.loading = false;
        view._updates = Task::ready(());
        view
    });
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        let center = point(
            window.viewport_size().width / 2.,
            window.viewport_size().height / 2.,
        );
        let enter = |window: &mut Window, cx: &mut App, name: &str| {
            window.dispatch_event(
                FileDropEvent::Entered {
                    position: center,
                    paths: ExternalPaths(vec![std::path::PathBuf::from(name)].into()),
                }
                .to_platform_input(),
                cx,
            );
        };
        enter(window, cx, "movie.mkv");
        view.update(cx, |view, _| assert_eq!(view.drop_hover, Some(false)));
        window.dispatch_event(FileDropEvent::Exited.to_platform_input(), cx);
        view.update(cx, |view, _| assert_eq!(view.drop_hover, None));
        enter(window, cx, "movie.en.srt");
        view.update(cx, |view, _| assert_eq!(view.drop_hover, Some(true)));
        window.draw(cx).clear(cx);
        window.dispatch_event(
            FileDropEvent::Submit { position: center }.to_platform_input(),
            cx,
        );
        // The drop reaches the root over the video; with no media loaded it is
        // refused explicitly rather than queued.
        view.update(cx, |view, _| {
            assert_eq!(view.drop_hover, None);
            assert!(matches!(
                view.subtitle_notice,
                Some((SubtitleNotice::NotReady, _))
            ));
            assert!(view.subtitle_pending.is_none());
        });
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    Ok(())
}
#[test]
fn typing_in_the_search_sheet_never_fires_player_shortcuts()
-> Result<(), Box<dyn std::error::Error>> {
    use super::search_model::SearchModel;
    let mut cx = TestAppContext::single();
    cx.update(crate::ui::theme::init);
    cx.update(bind_keys);
    cx.update(crate::ui::primitives::text_input::bind_keys);
    let window = cx.add_window(|window, cx| {
        let mut view = PlayerWindow::new_mode(
            kinema_core::media::playback_title("Fixture", None),
            None,
            PlayerChoice::Embedded,
            window,
            cx,
        );
        view.startup_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        view.state.loading = false;
        view._updates = Task::ready(());
        view
    });
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        // Text mode: no catalog identity, so opening starts no service call.
        view.update(cx, |view, cx| {
            let model = SearchModel::new(vec!["en".into()], false);
            view.show_search_fixture(model, true, None, window, cx);
        });
        window.draw(cx).clear(cx);
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    cx.run_until_parked();
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        let input = view.update(cx, |view, cx| {
            view.search
                .as_ref()
                .and_then(|sheet| sheet.query.clone())
                .map(|input| input.focus_handle(cx))
        });
        let input = input.ok_or("text field expected")?;
        assert!(input.is_focused(window), "sheet focuses its field");
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    cx.simulate_keystrokes(window.into(), "k f m space");
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        assert!(!window.is_fullscreen(), "F types, it does not fullscreen");
        view.update(cx, |view, _| {
            assert!(view.search.is_some());
            assert!(view.help_focus.is_none(), "? types, it does not open help");
            assert!(!view.control_notice, "no shortcut handler ran");
            assert!(view.progress.is_none());
        });
        let typed = view.update(cx, |view, cx| {
            view.search
                .as_ref()
                .and_then(|sheet| sheet.query.as_ref())
                .map(|input| input.read(cx).text().to_owned())
        });
        assert_eq!(typed.as_deref(), Some("kfm "), "keys reach the field");
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    // Esc closes the sheet and returns focus to the player.
    cx.simulate_keystrokes(window.into(), "escape");
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        view.update(cx, |view, _| {
            assert!(view.search.is_none(), "Esc closes the sheet");
            assert!(view.state.position == 0.);
        });
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    cx.run_until_parked();
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        view.update(cx, |view, _| {
            assert!(
                view.surface_focus.is_focused(window),
                "focus returns to the video"
            )
        });
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    Ok(())
}
#[test]
fn an_observed_online_add_labels_its_track_and_closes_the_sheet()
-> Result<(), Box<dyn std::error::Error>> {
    use super::search_model::{Problem, SearchModel};
    let mut cx = TestAppContext::single();
    cx.update(crate::ui::theme::init);
    cx.update(bind_keys);
    let window = cx.add_window(|window, cx| {
        let mut view = PlayerWindow::new_mode(
            kinema_core::media::playback_title("Fixture", None),
            None,
            PlayerChoice::Embedded,
            window,
            cx,
        );
        view.startup_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        view.state.loading = false;
        view._updates = Task::ready(());
        view
    });
    let result = |id: u64| kinema_core::subtitle_search::SubtitleResult {
        file_id: id,
        language: "en".into(),
        release: format!("Release {id}"),
        file_name: None,
        download_count: 0,
        hearing_impaired: false,
        foreign_parts_only: false,
        ai_translated: false,
        machine_translated: false,
        from_trusted: false,
        moviehash_match: false,
        format: None,
    };
    let track = |id| kinema_mpv::Track {
        id,
        kind: TrackKind::Subtitle,
        title: Some("Release 1".into()),
        lang: Some("en".into()),
        selected: true,
        external: true,
        forced: false,
        hearing_impaired: false,
        codec: Some("subrip".into()),
        channels: None,
    };
    cx.update_window(window.into(), |view, window, cx| {
        let view = view
            .downcast::<PlayerWindow>()
            .map_err(|_| "expected player fixture")?;
        window.draw(cx).clear(cx);
        view.update(cx, |view, cx| {
            let mut model = SearchModel::new(vec!["en".into()], true);
            model.finish_search(0, Ok(vec![result(1)]));
            view.show_search_fixture(model, false, None, window, cx);
            let search = view.search.as_mut().map(|s| &mut s.model);
            let Some(model) = search else { return };
            let generation = model.begin_add(1).unwrap_or_default();
            assert!(generation > 0);
            view.subtitle_pending = Some(SubtitlePending {
                media_generation: 0,
                external: 0,
                error: None,
                skipped: 0,
                since: Instant::now(),
                online: Some(Vec::new()),
            });
            // A failure first: the sheet stays open with a distinct problem.
            view.state.error = Some("command failed".into());
            assert!(view.observe_subtitles(Instant::now()));
            let model = view.search.as_ref().map(|s| s.model.clone());
            assert_eq!(
                model.map(|m| (m.adding, m.add_problem)),
                Some((None, Some(Problem::Unusable)))
            );
            view.state.error = None;
            if let Some(model) = view.search.as_mut().map(|s| &mut s.model) {
                model.begin_add(1);
            }
            view.subtitle_pending = Some(SubtitlePending {
                media_generation: 0,
                external: 0,
                error: None,
                skipped: 0,
                since: Instant::now(),
                online: Some(Vec::new()),
            });
            view.state.tracks = vec![track(7)];
            assert!(view.observe_subtitles(Instant::now()));
            assert!(view.search.is_none(), "success closes the sheet");
            assert_eq!(view.online.track_ids(0), [7]);
            let tracks: Vec<_> = view.state.tracks.iter().collect();
            let online = view.online.track_ids(0);
            let labels = track_labels(TrackKind::Subtitle, &tracks, None, online);
            assert_eq!(labels[0].detail.as_deref(), Some("OpenSubtitles · SRT"));
            assert_eq!(labels[0].primary, "English");
            // A reload drops the attribution with the track.
            assert!(view.online.track_ids(1).is_empty());
        });
        Ok::<_, Box<dyn std::error::Error>>(())
    })??;
    Ok(())
}

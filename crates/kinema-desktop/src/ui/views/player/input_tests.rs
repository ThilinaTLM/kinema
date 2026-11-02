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

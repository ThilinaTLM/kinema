//! Explicit Quit routing, separate from Main's close-to-background policy.
use super::main_window::{V3About, V3Quit, V3Settings};
use crate::controllers::coordinator::AppCoordinator;
use crate::{
    ui::primitives::button::{Button, ButtonVariant},
    ui::primitives::sheet::Sheet,
};
use gpui::{prelude::FluentBuilder, *};

/// Install once for the application's main window, using its shared coordinator.
pub(crate) fn install(coordinator: &Entity<AppCoordinator>, main: AnyWindowHandle, cx: &mut App) {
    crate::controllers::lifecycle::install(coordinator, main, cx);
    let quit = coordinator.downgrade();
    cx.on_action(move |_: &V3Quit, cx| {
        if let Some(coordinator) = quit.upgrade() {
            let _ = main.update(cx, |_, window, cx| request(&coordinator, window, cx));
        }
    });
    let settings = coordinator.downgrade();
    cx.on_action(move |_: &V3Settings, cx| {
        if let Some(coordinator) = settings.upgrade() {
            coordinator.update(cx, |c, cx| c.open_settings(cx));
        }
    });
    let about = coordinator.downgrade();
    cx.on_action(move |_: &V3About, cx| {
        if let Some(coordinator) = about.upgrade() {
            coordinator.update(cx, |c, cx| c.about(cx));
        }
    });
    // Unexpected Main destruction cannot leave an inaccessible Settings-only app.
    cx.on_window_closed(move |cx, closed| {
        if closed == main.window_id() {
            exit(cx);
        }
    })
    .detach();
    #[cfg(target_os = "macos")]
    cx.set_menus([Menu::new("Kinema").items([
        MenuItem::action("About Kinema", V3About),
        MenuItem::separator(),
        MenuItem::action("Settings", V3Settings),
        MenuItem::separator(),
        MenuItem::action("Quit Kinema", V3Quit),
    ])]);
}

/// Explicit Quit (or idle Main close): restore Main for the existing confirmation,
/// then finish persistence before exiting. Never route this operation to hiding.
pub(crate) fn request(coordinator: &Entity<AppCoordinator>, window: &mut Window, cx: &mut App) {
    crate::controllers::lifecycle::cancel(cx);
    crate::controllers::lifecycle::restore_current(window, cx);
    if coordinator.update(cx, |c, cx| c.request_quit(cx)) {
        finish(cx);
    }
}

// GPUI's TestPlatform::quit is a no-op. Record calls at the platform boundary
// in interaction tests, without substituting the coordinator or its quit gate.
#[cfg(all(test, feature = "ui-probe-test"))]
#[derive(Default)]
struct ExitRequests(usize);
#[cfg(all(test, feature = "ui-probe-test"))]
impl Global for ExitRequests {}

fn finish(cx: &mut App) {
    use crate::controllers::lifecycle::Lifecycle;
    use smol::future::race;
    if cx.global::<Lifecycle>().quitting {
        return;
    }
    crate::controllers::lifecycle::begin_shutdown(cx);
    let callback = cx.global::<Lifecycle>().final_flush.clone();
    let coordinator = cx.global::<Lifecycle>().coordinator.clone();
    let Some(callback) = callback else {
        tracing::warn!(
            category = "quit_flush_unavailable",
            "Final progress flush callback is not installed"
        );
        exit(cx);
        return;
    };
    let flush = callback(&coordinator, cx);
    let task = cx.spawn(async move |cx| {
        let timeout = cx
            .background_executor()
            .timer(std::time::Duration::from_secs(2));
        if race(
            async {
                flush.await;
                while !cx.update(|cx| crate::controllers::lifecycle::shutdown_complete(cx)) {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(20))
                        .await;
                }
                false
            },
            async {
                timeout.await;
                true
            },
        )
        .await
        {
            tracing::warn!(
                category = "quit_shutdown_timeout",
                "Final progress flush or desktop teardown exceeded quit deadline"
            );
        }
        cx.update(exit);
    });
    cx.global_mut::<Lifecycle>().flush = Some(task);
}

fn exit(cx: &mut App) {
    if cx.has_global::<crate::controllers::lifecycle::Lifecycle>() {
        crate::controllers::lifecycle::begin_shutdown(cx);
    }
    #[cfg(all(test, feature = "ui-probe-test"))]
    {
        if !cx.has_global::<ExitRequests>() {
            cx.set_global(ExitRequests::default());
        }
        cx.global_mut::<ExitRequests>().0 += 1;
    }
    cx.quit();
}

/// Confirmation for a pending launch without active sessions (lab fixture).
#[cfg_attr(not(feature = "ui-probe"), allow(dead_code))]
pub(crate) fn confirmation(
    coordinator: Entity<AppCoordinator>,
    cancel_focus: FocusHandle,
) -> Sheet {
    confirmation_for(coordinator, cancel_focus, 0)
}

pub(crate) fn confirmation_for(
    coordinator: Entity<AppCoordinator>,
    cancel_focus: FocusHandle,
    active_sessions: usize,
) -> Sheet {
    confirmation_for_close(coordinator, cancel_focus, active_sessions, false)
}

pub(crate) fn confirmation_for_close(
    coordinator: Entity<AppCoordinator>,
    cancel_focus: FocusHandle,
    active_sessions: usize,
    close_fallback: bool,
) -> Sheet {
    let body = if close_fallback {
        "Kinema cannot hide Main safely because a usable tray or Main-only hide capability is unavailable. Minimize keeps playback and pending launches running. Quit stops them; Cancel keeps Main open.".to_owned()
    } else {
        match active_sessions {
        0 => "A launch is still pending. Quitting cancels it and your player may lose access to the media.".to_owned(),
        1 => "1 playback or download is still in progress. Quitting stops it and your player may lose access to the media.".to_owned(),
        n => format!("{n} playbacks or downloads are still in progress. Quitting stops them and your player may lose access to the media."),
    }
    };
    let cancel = coordinator.clone();
    let minimize = coordinator.clone();
    let dismiss = coordinator;
    Sheet::new("quit-sheet", "Quit Kinema?")
        .body(body)
        .initial_focus(cancel_focus.clone())
        .on_dismiss(move |_, cx| {
            crate::controllers::lifecycle::cancel(cx);
            dismiss.update(cx, |c, cx| {
                c.quit_requested = false;
                cx.notify();
            });
        })
        .footer(
            Button::new("cancel-quit", "Cancel")
                .focus_handle(cancel_focus)
                .on_click(move |_, _, cx| {
                    crate::controllers::lifecycle::cancel(cx);
                    cancel.update(cx, |c, cx| {
                        c.quit_requested = false;
                        cx.notify();
                    });
                }),
        )
        .footer(
            Button::new("confirm-quit", "Quit")
                .variant(ButtonVariant::Danger)
                .on_click(|_, _, cx| finish(cx)),
        )
        .when(close_fallback, |sheet| {
            sheet.footer(
                Button::new("minimize-main", "Minimize").on_click(move |_, window, cx| {
                    crate::controllers::lifecycle::cancel(cx);
                    minimize.update(cx, |c, cx| {
                        c.quit_requested = false;
                        cx.notify();
                    });
                    window.minimize_window();
                }),
            )
        })
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::*;
    use crate::ui::theme;
    use crate::ui::views::{main_window::MainWindow, settings_window};
    use ::core::prelude::v1::test;

    fn setup(cx: &mut TestAppContext) -> (WindowHandle<MainWindow>, Entity<AppCoordinator>) {
        cx.update(|cx| {
            theme::init(cx);
            crate::ui::views::main_window::bind_keys(cx);
        });
        let coordinator = cx.new(|cx| AppCoordinator::fixture(Vec::new(), None, cx));
        cx.update(|cx| settings_window::install(&coordinator, cx));
        let main = cx
            .add_window(|window, cx| MainWindow::with_coordinator(coordinator.clone(), window, cx));
        (main, coordinator)
    }

    fn exits(cx: &mut TestAppContext) -> usize {
        cx.update(|cx| {
            if cx.has_global::<ExitRequests>() {
                cx.global::<ExitRequests>().0
            } else {
                0
            }
        })
    }

    fn open_settings(main: WindowHandle<MainWindow>, cx: &mut TestAppContext) -> AnyWindowHandle {
        cx.simulate_keystrokes(main.into(), "secondary-,");
        cx.run_until_parked();
        cx.windows()
            .into_iter()
            .find(|window| window.window_id() != main.window_id())
            .unwrap()
    }

    #[gpui::test]
    fn closing_main_without_sessions_quits_even_with_settings_open(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        let _settings = open_settings(main, cx);
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        // Native removal is vetoed: the application quit operation closes all
        // windows on real platforms, not just the window requesting closure.
        assert!(!visual.simulate_close());
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(exits(cx), 1);
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
    }

    #[gpui::test]
    fn quit_shortcut_from_settings_routes_through_main(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        let settings = open_settings(main, cx);
        settings
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.simulate_keystrokes(settings, "secondary-q");
        cx.run_until_parked();
        cx.background_executor
            .advance_clock(std::time::Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(exits(cx), 1);
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
        cx.update(|cx| assert_eq!(cx.active_window(), Some(main.into())));
    }

    fn assert_pending_confirmation(
        main: WindowHandle<MainWindow>,
        coordinator: &Entity<AppCoordinator>,
        cx: &mut TestAppContext,
    ) {
        cx.run_until_parked();
        assert_eq!(exits(cx), 0);
        coordinator.read_with(cx, |c, _| {
            assert_eq!(c.active_sessions(), 0);
            assert!(c.needs_quit_confirmation());
            assert!(c.quit_requested);
        });
        cx.update(|cx| assert_eq!(cx.active_window(), Some(main.into())));
        main.update(cx, |view, window, _| {
            assert!(view.quit_cancel_is_focused(window));
        })
        .unwrap();
    }

    #[gpui::test]
    fn closing_main_with_pending_launch_keeps_main_and_focuses_cancel(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        coordinator.update(cx, |c, cx| c.fixture_pending_launch(cx));
        let settings = open_settings(main, cx);
        settings
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        assert!(!visual.simulate_close());
        assert_pending_confirmation(main, &coordinator, cx);
        assert!(settings.update(cx, |_, _, _| ()).is_ok());
        cx.simulate_keystrokes(main.into(), "escape");
        cx.run_until_parked();
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
        assert!(main.update(cx, |_, _, _| ()).is_ok());
        assert_eq!(exits(cx), 0);
    }

    #[gpui::test]
    fn quit_from_settings_with_pending_launch_activates_main_and_focuses_cancel(
        cx: &mut TestAppContext,
    ) {
        let (main, coordinator) = setup(cx);
        coordinator.update(cx, |c, cx| c.fixture_pending_launch(cx));
        let settings = open_settings(main, cx);
        settings
            .update(cx, |_, window, _| window.activate_window())
            .unwrap();
        cx.simulate_keystrokes(settings, "secondary-q");
        assert_pending_confirmation(main, &coordinator, cx);
        assert!(settings.update(cx, |_, _, _| ()).is_ok());
        cx.simulate_keystrokes(main.into(), "escape");
        cx.run_until_parked();
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
        assert_eq!(exits(cx), 0);
    }

    #[gpui::test]
    fn usable_tray_hides_main_without_quit_confirmation(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        coordinator.update(cx, |c, cx| c.fixture_pending_launch(cx));
        let hides = std::rc::Rc::new(std::cell::Cell::new(0));
        let seen = hides.clone();
        let restores = std::rc::Rc::new(std::cell::Cell::new(0));
        let restored = restores.clone();
        cx.update(|cx| {
            crate::controllers::lifecycle::set_main_hider(
                Some(std::rc::Rc::new(move |_| {
                    seen.set(seen.get() + 1);
                    true
                })),
                cx,
            );
            crate::controllers::lifecycle::set_main_restorer(
                std::rc::Rc::new(move |_| {
                    restored.set(restored.get() + 1);
                    true
                }),
                cx,
            );
            crate::controllers::lifecycle::set_tray_available(true, cx);
        });
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        assert!(!visual.simulate_close());
        assert_eq!(hides.get(), 1);
        assert_eq!(exits(cx), 0);
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
        cx.update(crate::controllers::lifecycle::host_lost);
        assert_eq!(restores.get(), 1);
        cx.update(crate::controllers::lifecycle::host_lost);
        assert_eq!(restores.get(), 1);
        assert!(main.update(cx, |_, _, _| ()).is_ok());
        assert_eq!(exits(cx), 0);
    }

    #[gpui::test]
    fn expired_host_health_never_hides_main(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        coordinator.update(cx, |c, cx| c.fixture_pending_launch(cx));
        let hides = std::rc::Rc::new(std::cell::Cell::new(0));
        let hidden = hides.clone();
        cx.update(|cx| {
            crate::controllers::lifecycle::set_main_hider(
                Some(std::rc::Rc::new(move |_| {
                    hidden.set(hidden.get() + 1);
                    true
                })),
                cx,
            );
            crate::controllers::lifecycle::set_main_restorer(std::rc::Rc::new(|_| true), cx);
            crate::controllers::lifecycle::set_tray_health(
                Some(std::time::Instant::now() - std::time::Duration::from_secs(1)),
                cx,
            );
        });
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        assert!(!visual.simulate_close());
        assert_pending_confirmation(main, &coordinator, cx);
        assert_eq!(hides.get(), 0);
    }

    #[gpui::test]
    fn tray_without_main_only_hide_uses_explicit_fallback(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        coordinator.update(cx, |c, cx| c.fixture_pending_launch(cx));
        cx.update(|cx| crate::controllers::lifecycle::set_tray_available(true, cx));
        let mut visual = VisualTestContext::from_window(main.into(), cx);
        assert!(!visual.simulate_close());
        assert_pending_confirmation(main, &coordinator, cx);
        assert!(cx.update(|cx| crate::controllers::lifecycle::is_close_fallback(cx)));
        cx.simulate_keystrokes(main.into(), "escape");
        assert!(!cx.update(|cx| crate::controllers::lifecycle::is_close_fallback(cx)));
    }

    #[gpui::test]
    fn quit_waits_for_registered_final_flush(cx: &mut TestAppContext) {
        let _ = setup(cx);
        cx.run_until_parked();
        let flushed = std::rc::Rc::new(std::cell::Cell::new(false));
        let observed = flushed.clone();
        cx.update(|cx| {
            crate::controllers::lifecycle::set_final_flush(
                std::rc::Rc::new(move |_, cx| {
                    let observed = observed.clone();
                    cx.spawn(async move |_| {
                        observed.set(true);
                    })
                }),
                cx,
            )
        });
        cx.update(crate::controllers::lifecycle::request_quit);
        cx.run_until_parked();
        assert!(flushed.get());
        assert_eq!(exits(cx), 1);
    }

    #[gpui::test]
    fn stalled_final_flush_is_bounded_and_quit_is_not_duplicated(cx: &mut TestAppContext) {
        let _ = setup(cx);
        cx.run_until_parked();
        cx.update(|cx| {
            crate::controllers::lifecycle::set_final_flush(
                std::rc::Rc::new(|_, cx| {
                    cx.spawn(async move |_| {
                        smol::future::pending::<()>().await;
                    })
                }),
                cx,
            )
        });
        cx.update(crate::controllers::lifecycle::request_quit);
        cx.run_until_parked();
        assert_eq!(exits(cx), 0);
        cx.update(crate::controllers::lifecycle::request_quit);
        cx.background_executor
            .advance_clock(std::time::Duration::from_secs(3));
        cx.run_until_parked();
        assert_eq!(exits(cx), 1);
    }

    #[gpui::test]
    fn closing_settings_leaves_main_and_coordinator_alive(cx: &mut TestAppContext) {
        let (main, coordinator) = setup(cx);
        let settings = open_settings(main, cx);
        settings
            .update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        assert_eq!(exits(cx), 0);
        assert!(main.update(cx, |_, _, _| ()).is_ok());
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
    }

    struct ConfirmationFixture {
        coordinator: Entity<AppCoordinator>,
        cancel: FocusHandle,
    }
    impl Render for ConfirmationFixture {
        fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .relative()
                .when(self.coordinator.read(cx).quit_requested, |el| {
                    el.child(confirmation(self.coordinator.clone(), self.cancel.clone()))
                })
        }
    }

    #[gpui::test]
    fn confirmation_focuses_cancel_and_escape_cancels(cx: &mut TestAppContext) {
        cx.update(theme::init);
        let coordinator = cx.new(|cx| {
            let mut c = AppCoordinator::fixture(Vec::new(), None, cx);
            c.quit_requested = true;
            c
        });
        let sheet = cx.add_window(|_, cx| ConfirmationFixture {
            coordinator: coordinator.clone(),
            cancel: cx.focus_handle().tab_stop(true),
        });
        cx.run_until_parked();
        sheet
            .update(cx, |view, window, cx| {
                assert!(view.cancel.is_focused(window));
                assert!(view.coordinator.read(cx).quit_requested);
            })
            .unwrap();
        cx.simulate_keystrokes(sheet.into(), "escape");
        cx.run_until_parked();
        assert!(!coordinator.read_with(cx, |c, _| c.quit_requested));
        assert_eq!(exits(cx), 0);
    }

    #[gpui::test]
    fn programmatic_main_removal_also_requests_application_exit(cx: &mut TestAppContext) {
        let (main, _) = setup(cx);
        let _settings = open_settings(main, cx);
        main.update(cx, |_, window, _| window.remove_window())
            .unwrap();
        cx.run_until_parked();
        assert_eq!(exits(cx), 1);
    }
}

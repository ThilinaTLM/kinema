//! Application-wide quit routing. Main owns the gate; Settings never owns shutdown.
use super::{
    coordinator::AppCoordinator,
    main_window::{V3About, V3Quit, V3Settings},
};
use crate::widgets::{
    button::{Button, ButtonVariant},
    sheet::Sheet,
};
use gpui::*;

/// Install once for the application's main window, using its shared coordinator.
pub(crate) fn install(coordinator: &Entity<AppCoordinator>, main: AnyWindowHandle, cx: &mut App) {
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
    // Also cover programmatic removal of Main: a Settings-only app is never valid.
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
        MenuItem::action("Settings…", V3Settings),
        MenuItem::separator(),
        MenuItem::action("Quit Kinema", V3Quit),
    ])]);
}

/// Always veto native removal: either quit the entire app, or keep Main alive
/// and activate it so the confirmation (with Cancel initially focused) is visible.
pub(crate) fn request(coordinator: &Entity<AppCoordinator>, window: &mut Window, cx: &mut App) {
    window.activate_window();
    cx.activate(true);
    if coordinator.update(cx, |c, cx| c.request_quit(cx)) {
        exit(cx);
    }
}

// GPUI's TestPlatform::quit is a no-op. Record calls at the platform boundary
// in interaction tests, without substituting the coordinator or its quit gate.
#[cfg(all(test, feature = "ui-probe-test"))]
#[derive(Default)]
struct ExitRequests(usize);
#[cfg(all(test, feature = "ui-probe-test"))]
impl Global for ExitRequests {}

fn exit(cx: &mut App) {
    #[cfg(all(test, feature = "ui-probe-test"))]
    {
        if !cx.has_global::<ExitRequests>() {
            cx.set_global(ExitRequests::default());
        }
        cx.global_mut::<ExitRequests>().0 += 1;
    }
    cx.quit();
}

pub(crate) fn confirmation(
    coordinator: Entity<AppCoordinator>,
    cancel_focus: FocusHandle,
) -> Sheet {
    let cancel = coordinator.clone();
    let dismiss = coordinator;
    Sheet::new("quit-sheet", "Quit Kinema?")
        .body("Playback or downloads are still in progress. Quitting stops them and your player may lose access to the media.")
        .initial_focus(cancel_focus.clone())
        .on_dismiss(move |_, cx| {
            dismiss.update(cx, |c, cx| {
                c.quit_requested = false;
                cx.notify();
            });
        })
        .footer(
            Button::new("cancel-quit", "Cancel")
                .focus_handle(cancel_focus)
                .on_click(move |_, _, cx| {
                    cancel.update(cx, |c, cx| {
                        c.quit_requested = false;
                        cx.notify();
                    });
                }),
        )
        .footer(Button::new("confirm-quit", "Quit")
                .variant(ButtonVariant::Danger)
                .on_click(|_, _, cx| exit(cx)))
}

#[cfg(all(test, feature = "ui-probe-test"))]
#[allow(clippy::unwrap_used)]
mod interaction_tests {
    use super::*;
    use crate::screens::{main_window::MainWindow, settings_window};
    use crate::widgets::tokens;
    use ::core::prelude::v1::test;
    use gpui::prelude::*;

    fn setup(cx: &mut TestAppContext) -> (WindowHandle<MainWindow>, Entity<AppCoordinator>) {
        cx.update(|cx| {
            tokens::init(cx);
            crate::screens::main_window::bind_keys(cx);
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
        cx.update(tokens::init);
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

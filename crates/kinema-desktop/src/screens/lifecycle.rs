//! Application lifetime is independent of Main's visibility and player windows.
mod native;
mod runtime;

use super::coordinator::AppCoordinator;
use gpui::*;
use std::{
    rc::Rc,
    time::{Duration, Instant},
};

/// A host-provided, Main-only hide operation. Return false if unsupported/failed.
/// Never hide the application: independent players must remain visible.
pub type MainHider = Rc<dyn Fn(&mut Window) -> bool>;
pub type MainRestorer = Rc<dyn Fn(&mut Window) -> bool>;
/// Begin final progress persistence without blocking the UI thread.
pub type FinalFlush = Rc<dyn Fn(&Entity<AppCoordinator>, &mut App) -> Task<()>>;

pub fn set_final_flush(flush: FinalFlush, cx: &mut App) {
    cx.global_mut::<Lifecycle>().final_flush = Some(flush);
}

pub(crate) struct Lifecycle {
    // Strong application-global ownership, including while Main is hidden.
    pub(crate) coordinator: Entity<AppCoordinator>,
    main: AnyWindowHandle,
    tray_health_until: Option<Instant>,
    health_watch: Option<Task<()>>,
    desktop: Option<runtime::DesktopRuntime>,
    desktop_task: Option<Task<()>>,
    desktop_shutdown_complete: bool,
    hider: Option<MainHider>,
    restorer: Option<MainRestorer>,
    hidden: bool,
    background_notice_shown: bool,
    pub(crate) close_fallback: bool,
    pub(crate) quitting: bool,
    pub(crate) flush: Option<Task<()>>,
    pub(crate) final_flush: Option<FinalFlush>,
}
impl Global for Lifecycle {}

pub(crate) fn install(coordinator: &Entity<AppCoordinator>, main: AnyWindowHandle, cx: &mut App) {
    cx.set_global(Lifecycle {
        coordinator: coordinator.clone(),
        main,
        tray_health_until: None,
        health_watch: None,
        desktop: None,
        desktop_task: None,
        desktop_shutdown_complete: true,
        hider: None,
        restorer: None,
        hidden: false,
        background_notice_shown: false,
        close_fallback: false,
        quitting: false,
        flush: None,
        final_flush: Some(Rc::new(|coordinator, cx| {
            coordinator.update(cx, |coordinator, cx| coordinator.flush_playback_on_quit(cx))
        })),
    });
    let watch = cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let keep_running = cx.update(|cx| {
                let owner = cx.global::<Lifecycle>();
                if owner.quitting {
                    return false;
                }
                if owner.hidden && !tray_usable(cx) {
                    host_lost(cx);
                }
                true
            });
            if !keep_running {
                break;
            }
        }
    });
    cx.global_mut::<Lifecycle>().health_watch = Some(watch);
}

/// Start real native adapters once Main's native window/application loop exists.
/// Offline probes/tests deliberately do not call this bootstrap operation.
pub fn start_runtime(window: &Window, cx: &mut App) {
    runtime::start(window, cx);
}

pub(crate) fn begin_shutdown(cx: &mut App) {
    let owner = cx.global_mut::<Lifecycle>();
    owner.quitting = true;
    owner.tray_health_until = None;
    if let Some(runtime) = &mut owner.desktop {
        runtime.shutdown();
    }
}

pub(crate) fn shutdown_complete(cx: &App) -> bool {
    cx.global::<Lifecycle>().desktop_shutdown_complete
}

pub(crate) fn install_native_visibility(window: &Window, cx: &mut App) {
    native::install(window, cx);
}

fn tray_usable(cx: &App) -> bool {
    cx.global::<Lifecycle>()
        .tray_health_until
        .is_some_and(|until| Instant::now() < until)
}

/// Publish an adapter-observed health lease. Stale health must never authorize hiding.
pub fn set_tray_health(valid_until: Option<Instant>, cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    cx.global_mut::<Lifecycle>().tray_health_until = valid_until;
    if !tray_usable(cx) {
        host_lost(cx);
    }
}

/// Report actual host usability, not merely successful tray construction.
pub fn set_tray_available(available: bool, cx: &mut App) {
    set_tray_health(
        available.then(|| Instant::now() + Duration::from_secs(5)),
        cx,
    );
}

pub fn set_main_hider(hider: Option<MainHider>, cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    let lost = hider.is_none();
    cx.global_mut::<Lifecycle>().hider = hider;
    if lost {
        host_lost(cx);
    }
}

/// Install the matching native show operation before enabling Main-only hide.
pub fn set_main_restorer(restorer: MainRestorer, cx: &mut App) {
    cx.global_mut::<Lifecycle>().restorer = Some(restorer);
}

/// Tray Show Main and OS activation share the same restoration path.
pub fn show_main(cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    let main = cx.global::<Lifecycle>().main;
    let _ = main.update(cx, |_, window, cx| restore_current(window, cx));
}

/// Tray Settings reuses the existing independent Settings-window route.
pub fn show_settings(cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    let coordinator = cx.global::<Lifecycle>().coordinator.clone();
    coordinator.update(cx, |c, cx| c.open_settings(cx));
}

/// Restore Main before opening its Transfers panel.
pub fn show_transfers(cx: &mut App) {
    show_main(cx);
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    // The existing toolbar owns the panel; its programmatic action is installed
    // by Main without introducing a separate transfers view or worker.
    let main = cx.global::<Lifecycle>().main;
    let _ = main.update(cx, |_, window, cx| {
        window.dispatch_action(Box::new(super::main_window::V3Transfers), cx);
    });
}

/// Explicit tray Quit always uses Main's confirmation, never close-to-background.
pub fn request_quit(cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    let owner = cx.global::<Lifecycle>();
    let main = owner.main;
    let coordinator = owner.coordinator.clone();
    let _ = main.update(cx, |_, window, cx| {
        super::quit::request(&coordinator, window, cx)
    });
}

pub(crate) fn restore_current(window: &mut Window, cx: &mut App) {
    let restorer = cx.global::<Lifecycle>().restorer.clone();
    let was_hidden = cx.global::<Lifecycle>().hidden;
    let restored = restorer.is_some_and(|restore| restore(window));
    // Keep retrying an unsuccessful native remap after host loss. Merely
    // activating a still-unmapped X11 window is not restoration.
    cx.global_mut::<Lifecycle>().hidden = was_hidden && !restored;
    window.activate_window();
    cx.activate(true);
}

/// Call only on observed host loss; this module does not claim to detect it.
pub fn host_lost(cx: &mut App) {
    if !cx.has_global::<Lifecycle>() {
        return;
    }
    cx.global_mut::<Lifecycle>().tray_health_until = None;
    if cx.global::<Lifecycle>().hidden {
        show_main(cx);
    }
}

pub(crate) fn close_main(coordinator: &Entity<AppCoordinator>, window: &mut Window, cx: &mut App) {
    if !coordinator.read(cx).needs_quit_confirmation() {
        super::quit::request(coordinator, window, cx);
        return;
    }
    let hider = tray_usable(cx)
        .then_some(())
        .filter(|_| cx.global::<Lifecycle>().restorer.is_some())
        .and_then(|_| cx.global::<Lifecycle>().hider.clone());
    if hider.is_some_and(|hide| hide(window)) {
        cx.global_mut::<Lifecycle>().hidden = true;
        if !cx.global::<Lifecycle>().background_notice_shown {
            cx.global_mut::<Lifecycle>().background_notice_shown = true;
            cx.show_system_notification(SystemNotification {
                tag: "kinema-background".into(),
                title: "Kinema is still running".into(),
                body: "Playback continues. Use the tray menu to show Kinema or Quit.".into(),
                actions: Vec::new(),
            });
        }
        return;
    }
    cx.global_mut::<Lifecycle>().close_fallback = true;
    window.activate_window();
    cx.activate(true);
    coordinator.update(cx, |c, cx| {
        c.request_quit(cx);
    });
}

pub(crate) fn is_close_fallback(cx: &App) -> bool {
    cx.has_global::<Lifecycle>() && cx.global::<Lifecycle>().close_fallback
}

pub(crate) fn cancel(cx: &mut App) {
    if cx.has_global::<Lifecycle>() {
        cx.global_mut::<Lifecycle>().close_fallback = false;
    }
}

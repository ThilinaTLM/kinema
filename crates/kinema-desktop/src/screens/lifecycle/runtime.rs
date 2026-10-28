//! App-owned coordination, intentionally outside every coordinator entity borrow.
use super::Lifecycle;
use crate::{
    desktop_integration::{
        self as desktop, Command, CommandReceiver, Playback, PlayerTarget, Selection,
        inhibition::{Activity, PlayerActivity},
        runtime::{IconData, Runtime, RuntimeConfig, RuntimeSnapshot},
    },
    screens::player_window,
};
use gpui::*;
use std::{
    collections::{HashMap, HashSet},
    time::{Duration, Instant},
};

pub(super) struct DesktopRuntime {
    runtime: Runtime,
    commands: CommandReceiver,
    selection: Selection,
    known: HashSet<PlayerTarget>,
    activity: ActivityTracker,
    warned_native: bool,
    warned_inhibition: bool,
}

pub(crate) fn start(_: &Window, cx: &mut App) {
    if cx.global::<Lifecycle>().desktop_task.is_some() {
        return;
    }
    cx.global_mut::<Lifecycle>().desktop_shutdown_complete = false;
    let task = cx.spawn(async move |cx| {
        // Embedded PNG decoding and Windows registry access never run on GPUI's
        // native event-loop thread. This owned startup task is cancelled on drop.
        let icon = cx
            .background_executor()
            .spawn(async { IconData::load_for_current_platform() })
            .await;
        let started = cx.update(|cx| {
            if cx.global::<Lifecycle>().quitting {
                cx.global_mut::<Lifecycle>().desktop_shutdown_complete = true;
                return false;
            }
            let main = cx.global::<Lifecycle>().main;
            let prepared = icon.and_then(|icon| {
                main.update(cx, |_, window, _| config(window, icon))
                    .map_err(|error| error.to_string())
            });
            let (sender, commands) = desktop::command_channel(128);
            match prepared.and_then(|config| Runtime::start_after_native_ready(sender, config)) {
                Ok(runtime) => {
                    cx.global_mut::<Lifecycle>().desktop = Some(DesktopRuntime {
                        runtime,
                        commands,
                        selection: Selection::default(),
                        known: HashSet::new(),
                        activity: ActivityTracker::default(),
                        warned_native: false,
                        warned_inhibition: false,
                    });
                    true
                }
                Err(_) => {
                    cx.global_mut::<Lifecycle>().desktop_shutdown_complete = true;
                    tracing::warn!(
                        category = "desktop_start",
                        "Desktop adapters could not start; playback remains available"
                    );
                    super::set_tray_health(None, cx);
                    false
                }
            }
        });
        if !started {
            return;
        }
        loop {
            let done = cx.update(|cx| {
                let Some(mut desktop) = cx.global_mut::<Lifecycle>().desktop.take() else {
                    return true;
                };
                let done = desktop.tick(cx);
                cx.global_mut::<Lifecycle>().desktop_shutdown_complete = done;
                cx.global_mut::<Lifecycle>().desktop = Some(desktop);
                done
            });
            if done {
                break;
            }
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
        }
    });
    cx.global_mut::<Lifecycle>().desktop_task = Some(task);
}

impl DesktopRuntime {
    fn tick(&mut self, cx: &mut App) -> bool {
        if cx.global::<Lifecycle>().quitting {
            self.runtime.shutdown();
            return self.runtime.shutdown_complete();
        }
        let players = player_window::desktop_snapshots(cx);
        let current: HashSet<_> = players.iter().map(|p| p.media.target).collect();
        for target in self.known.difference(&current) {
            self.selection.closed(*target);
        }
        for player in &players {
            if !self.known.contains(&player.media.target) {
                self.selection.started(player.media.target);
            }
        }
        self.known = current;
        for target in player_window::take_focus_events(cx) {
            self.selection.focus(target);
        }
        // Commands may close player windows and update their coordinator. Never
        // run this while that coordinator (or the lifecycle global) is borrowed.
        for _ in 0..128 {
            let Ok(command) = self.commands.try_recv() else {
                break;
            };
            if !desktop::command_matches_selection(&command, self.selection.selected()) {
                continue;
            }
            match command {
                Command::ShowMain => super::show_main(cx),
                Command::HideMain => super::hide_main(cx),
                Command::Settings => super::show_settings(cx),
                Command::Transfers => super::show_transfers(cx),
                Command::Quit => super::request_quit(cx),
                Command::TrayHostLost => super::host_lost(cx),
                Command::ShowPlayer(target) => {
                    player_window::show_player(target, cx);
                }
                Command::SelectPlayer(target) => {
                    self.selection.focus(target);
                }
                Command::Transport { target, action } => {
                    player_window::dispatch_transport(target, action, cx);
                }
            }
            if cx.global::<Lifecycle>().quitting {
                self.runtime.shutdown();
                return self.runtime.shutdown_complete();
            }
        }
        // A Stop command above may have removed a player. Resample before
        // publishing, so stale generations never remain the OS control target.
        let players = player_window::desktop_snapshots(cx);
        let current: HashSet<_> = players.iter().map(|p| p.media.target).collect();
        for target in self.known.difference(&current) {
            self.selection.closed(*target);
        }
        self.known = current;
        self.activity
            .buffering
            .retain(|target, _| self.known.contains(target));
        self.activity
            .playing
            .retain(|target| self.known.contains(target));
        let now = Instant::now();
        let activities = players
            .iter()
            .map(|player| {
                let target = player.media.target;
                let activity = self.activity.observe(
                    target,
                    player.paused,
                    player.playing && player.media.playback == Playback::Playing,
                    player.buffering,
                    player.error,
                    now,
                );
                PlayerActivity { target, activity }
            })
            .collect();
        self.runtime.update(RuntimeSnapshot {
            main_hidden: cx.global::<Lifecycle>().hidden,
            players: players.into_iter().map(|p| p.media).collect(),
            selected: self.selection.selected(),
            activities,
        });
        let status = self.runtime.poll();
        super::set_tray_health(
            status
                .tray_usable_at(now)
                .then_some(status.tray_valid_until)
                .flatten(),
            cx,
        );
        if status.native_error.is_some() && !self.warned_native {
            self.warned_native = true;
            tracing::warn!(
                category = "desktop_capability",
                "Some native desktop controls are unavailable"
            );
        }
        if status.inhibition_error.is_some() && !self.warned_inhibition {
            self.warned_inhibition = true;
            tracing::warn!(
                category = "desktop_inhibition",
                "Playback idle inhibition is unavailable"
            );
            let coordinator = cx.global::<Lifecycle>().coordinator.clone();
            coordinator.update(cx, |c, cx| {
                if c.notice.is_none() {
                    c.notice = Some("Kinema could not prevent automatic screen locking during playback. Your system's power settings still apply.".into());
                    cx.notify();
                }
            });
        }
        false
    }

    pub(super) fn shutdown(&mut self) {
        self.runtime.shutdown();
    }
}

#[cfg(not(target_os = "windows"))]
fn config(_: &Window, icon: IconData) -> RuntimeConfig {
    RuntimeConfig::new(icon)
}

#[cfg(target_os = "windows")]
fn config(window: &Window, icon: IconData) -> RuntimeConfig {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use std::{num::NonZeroIsize, rc::Rc};
    struct RetainedMainHost {
        hwnd: NonZeroIsize,
        // Main's native close is always vetoed; the app-global lifecycle retains
        // this view/HWND while hidden and detaches media before explicit Quit.
        _view: Entity<super::super::main_window::MainWindow>,
    }
    impl desktop::media_session::WindowsMediaHost for RetainedMainHost {
        fn hwnd(&self) -> NonZeroIsize {
            self.hwnd
        }
    }
    let mut config = RuntimeConfig::new(icon);
    if let (Ok(handle), Some(Some(view))) = (
        HasWindowHandle::window_handle(window),
        window.root::<super::super::main_window::MainWindow>(),
    ) && let RawWindowHandle::Win32(handle) = handle.as_raw()
    {
        config.media_host = Some(Rc::new(RetainedMainHost {
            hwnd: handle.hwnd,
            _view: view,
        }));
    }
    config
}

#[derive(Default)]
struct ActivityTracker {
    playing: HashSet<PlayerTarget>,
    buffering: HashMap<PlayerTarget, Instant>,
}
impl ActivityTracker {
    fn observe(
        &mut self,
        target: PlayerTarget,
        paused: bool,
        playing: bool,
        buffering: bool,
        error: bool,
        now: Instant,
    ) -> Activity {
        if error || paused {
            self.buffering.remove(&target);
            self.playing.remove(&target);
            Activity::Inactive
        } else if buffering && self.playing.contains(&target) {
            Activity::Buffering {
                since: *self.buffering.entry(target).or_insert(now),
            }
        } else if playing {
            self.buffering.remove(&target);
            self.playing.insert(target);
            Activity::Playing
        } else {
            self.buffering.remove(&target);
            self.playing.remove(&target);
            Activity::Inactive
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn buffering_grace_requires_prior_play_and_does_not_slide() {
        let mut tracker = ActivityTracker::default();
        let target = PlayerTarget {
            session_id: 1,
            generation: 1,
        };
        let now = Instant::now();
        assert!(matches!(
            tracker.observe(target, false, false, true, false, now),
            Activity::Inactive
        ));
        assert!(matches!(
            tracker.observe(target, false, true, false, false, now),
            Activity::Playing
        ));
        assert!(
            matches!(tracker.observe(target, false, false, true, false, now), Activity::Buffering { since } if since == now)
        );
        assert!(
            matches!(tracker.observe(target, false, false, true, false, now + Duration::from_secs(35)), Activity::Buffering { since } if since == now)
        );
        assert!(matches!(
            tracker.observe(target, true, false, true, false, now),
            Activity::Inactive
        ));
        assert!(matches!(
            tracker.observe(target, false, false, true, false, now),
            Activity::Inactive
        ));
    }

    #[test]
    fn one_paused_or_failed_player_does_not_release_another() {
        let mut tracker = ActivityTracker::default();
        let one = PlayerTarget {
            session_id: 1,
            generation: 1,
        };
        let two = PlayerTarget {
            session_id: 2,
            generation: 2,
        };
        let now = Instant::now();
        tracker.observe(one, false, true, false, false, now);
        tracker.observe(two, false, true, false, false, now);
        tracker.observe(one, true, false, false, false, now);
        assert!(tracker.playing.contains(&two));
        tracker.observe(one, false, false, false, true, now);
        assert!(tracker.playing.contains(&two));
    }
}

//! Managed player windows: registry, desktop snapshots, open/close and progress routing.
use super::*;

#[derive(Default)]
pub(super) struct PlayerWindows {
    pub(super) next: u64,
    pub(super) entries: HashMap<String, LeaseWindow>,
    pub(super) focus_events: VecDeque<PlayerTarget>,
}
impl Global for PlayerWindows {}
pub(super) struct LeaseWindow {
    pub(super) generation: u64,
    pub(super) handle: WindowHandle<PlayerWindow>,
    pub(super) coordinator: Entity<AppCoordinator>,
    pub(super) mode: PlayerChoice,
}

/// Public, sanitized state for application-owned media routing and inhibition.
#[derive(Clone, Debug)]
pub(crate) struct ManagedPlayerSnapshot {
    pub media: PlayerSnapshot,
    pub paused: bool,
    pub playing: bool,
    pub buffering: bool,
    pub error: bool,
}

pub(super) fn desktop_target(generation: u64) -> PlayerTarget {
    // Opaque process-local identity. Never parse or publish a provider session ID.
    PlayerTarget {
        session_id: generation,
        generation,
    }
}

/// In-memory snapshots only; does not sample progress, perform I/O, or select a player.
pub(crate) fn desktop_snapshots(cx: &mut App) -> Vec<ManagedPlayerSnapshot> {
    let mut entries: Vec<_> = cx
        .try_global::<PlayerWindows>()
        .map(|registry| {
            registry
                .entries
                .iter()
                .map(|(session, entry)| (session.clone(), entry.generation, entry.handle))
                .collect()
        })
        .unwrap_or_default();
    entries.sort_by_key(|(_, generation, _)| *generation);
    entries
        .into_iter()
        .filter_map(|(session_id, generation, handle)| {
            handle
                .update(cx, |view, _, _| {
                    if view.lease.as_ref().is_none_or(|(session, current)| {
                        session != &session_id || *current != generation
                    }) {
                        return None;
                    }
                    let state = view
                        .player
                        .as_ref()
                        .and_then(|owner| owner.player.as_ref())
                        .map_or_else(|| view.state.clone(), Player::state);
                    let failed = terminal_failure(view.error.is_some(), &state);
                    let mut media = desktop_snapshot(
                        desktop_target(generation),
                        &view.title,
                        view.progress.is_some(),
                        &state,
                    );
                    if failed {
                        media.playback = DesktopPlayback::Stopped;
                        media.seekable = false;
                    }
                    Some(ManagedPlayerSnapshot {
                        playing: media.playback == DesktopPlayback::Playing
                            && !state.paused_for_cache,
                        media,
                        paused: state.paused,
                        buffering: !failed && (state.loading || state.paused_for_cache),
                        error: view.error.is_some()
                            || state.error.is_some()
                            || state.failure_reason.is_some(),
                    })
                })
                .ok()
                .flatten()
        })
        .collect()
}

pub(super) fn desktop_snapshot(
    target: PlayerTarget,
    title: &PlaybackTitle,
    catalog_bound: bool,
    state: &PlaybackState,
) -> PlayerSnapshot {
    let ready = state.file_loaded
        && !state.loading
        && !state.media_stopped
        && !state.stopped
        && !terminal_failure(false, state);
    PlayerSnapshot {
        target,
        title: if catalog_bound {
            title.full.clone()
        } else {
            "Local video".into()
        },
        description: None,
        playback: if !ready {
            DesktopPlayback::Stopped
        } else if state.paused {
            DesktopPlayback::Paused
        } else {
            DesktopPlayback::Playing
        },
        position: media_time(state.position).unwrap_or_default(),
        duration: state.duration.and_then(media_time).filter(|v| !v.is_zero()),
        volume: normalized_volume(state.volume / 100.0).unwrap_or(0.0),
        seekable: desktop_can_seek(state),
    }
}
pub(super) fn desktop_can_seek(state: &PlaybackState) -> bool {
    state.position_observed
        && state.position.is_finite()
        && state.position >= 0.0
        && state.file_loaded
        && !state.loading
        && !state.media_stopped
        && !state.stopped
        && state.seekable == Some(true)
        && !terminal_failure(false, state)
}
pub(super) fn bounded_seek(position: f64, duration: Option<f64>) -> f64 {
    let position = position.max(0.0);
    duration
        .filter(|v| v.is_finite() && *v > 0.0)
        .map_or(position, |duration| position.min(duration))
}
pub(super) fn media_time(seconds: f64) -> Option<Duration> {
    Duration::try_from_secs_f64(seconds).ok()
}
pub(super) fn desktop_handle(target: PlayerTarget, cx: &App) -> Option<WindowHandle<PlayerWindow>> {
    cx.try_global::<PlayerWindows>()?
        .entries
        .values()
        .find(|entry| desktop_target(entry.generation) == target)
        .map(|entry| entry.handle)
}

/// Returns false for stale targets or currently unsupported/invalid transport.
/// True means routed, not backend acknowledgement; command failures use the
/// player's existing safe, nonfatal control notice.
pub(crate) fn dispatch_transport(target: PlayerTarget, action: Transport, cx: &mut App) -> bool {
    let Some(handle) = desktop_handle(target, cx) else {
        return false;
    };
    handle
        .update(cx, |view, _, cx| {
            if view
                .lease
                .as_ref()
                .is_none_or(|(_, generation)| desktop_target(*generation) != target)
            {
                return false;
            }
            view.desktop_transport(action, cx)
        })
        .unwrap_or(false)
}
pub(crate) fn show_player(target: PlayerTarget, cx: &mut App) -> bool {
    let Some(handle) = desktop_handle(target, cx) else {
        return false;
    };
    handle
        .update(cx, |view, window, cx| {
            if view
                .lease
                .as_ref()
                .is_none_or(|(_, generation)| desktop_target(*generation) != target)
            {
                return false;
            }
            window.show_window();
            window.activate_window();
            view.activity(cx);
            true
        })
        .unwrap_or(false)
}
pub(super) fn record_player_focus(generation: u64, cx: &mut App) {
    if cx.has_global::<PlayerWindows>() {
        let registry = cx.global_mut::<PlayerWindows>();
        let target = desktop_target(generation);
        registry.focus_events.retain(|event| *event != target);
        registry.focus_events.push_back(target);
    }
}
/// Drain activation events after reconciling snapshot starts/closes. Sampling
/// cannot steal explicit selection; closed generations are discarded here.
pub(crate) fn take_focus_events(cx: &mut App) -> Vec<PlayerTarget> {
    if !cx.has_global::<PlayerWindows>() {
        return Vec::new();
    }
    let registry = cx.global_mut::<PlayerWindows>();
    registry
        .focus_events
        .drain(..)
        .filter(|target| {
            registry
                .entries
                .values()
                .any(|entry| desktop_target(entry.generation) == *target)
        })
        .collect()
}

/// Replaying a lease activates its existing window without reloading its URL.
pub fn open(
    session_id: String,
    mode: PlayerChoice,
    url: String,
    title: PlaybackTitle,
    progress: Option<PlaybackLaunch>,
    coordinator: Entity<AppCoordinator>,
    cx: &mut App,
) -> Result<(), String> {
    if !mode.is_managed() {
        return Err("External players cannot own a managed player window".into());
    }
    if !cx.has_global::<PlayerWindows>() {
        cx.set_global(PlayerWindows::default());
        cx.on_window_closed(|cx, id| {
            let key = cx
                .global::<PlayerWindows>()
                .entries
                .iter()
                .find(|(_, entry)| entry.handle.window_id() == id)
                .map(|(key, entry)| (key.clone(), entry.generation));
            if let Some((key, generation)) = key {
                release_window(&key, generation, false, cx);
            }
        })
        .detach();
    }
    if cx
        .global::<PlayerWindows>()
        .entries
        .get(&session_id)
        .is_some_and(|entry| entry.mode != mode)
    {
        return Err("A managed playback lease cannot change player mode".into());
    }
    if let Some(handle) = cx
        .global::<PlayerWindows>()
        .entries
        .get(&session_id)
        .map(|entry| entry.handle)
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Ok(());
    }
    let generation = {
        let registry = cx.global_mut::<PlayerWindows>();
        registry.next += 1;
        registry.next
    };
    let options = WindowOptions {
        // Keep the installed application identity for Wayland grouping. GPUI's
        // per-window icon is used on X11; other platforms may use the app icon.
        app_id: Some("dev.tlmtech.kinema".into()),
        icon: player_window_icon(),
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(player::WINDOW_WIDTH, player::WINDOW_HEIGHT),
            cx,
        ))),
        window_min_size: Some(size(player::MIN_WIDTH, player::MIN_HEIGHT)),
        titlebar: Some(TitlebarOptions {
            title: Some(title.full.clone().into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            cx.new(|cx| PlayerWindow::new_mode(title, Some(url), mode.clone(), window, cx))
        })
        .map_err(|error| format!("Could not open embedded player window: {error}"))?;
    let initially_active = handle
        .update(cx, |view, window, _| {
            view.lease = Some((session_id.clone(), generation));
            view.progress = progress.map(Progress::new);
            view.progress_owner = Some(coordinator.clone());
            window.is_window_active()
        })
        .unwrap_or(false);
    cx.global_mut::<PlayerWindows>().entries.insert(
        session_id,
        LeaseWindow {
            generation,
            handle,
            coordinator,
            mode,
        },
    );
    if initially_active {
        record_player_focus(generation, cx);
    }
    Ok(())
}

pub(super) fn player_window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    image::load_from_memory(include_bytes!(
        "../../../../assets/brand/png/256/dev.tlmtech.kinema-player.png"
    ))
    .inspect_err(|_| {
        tracing::warn!(
            category = "icon_decode",
            "Could not decode the player icon; continuing without it"
        );
    })
    .ok()
    .map(|image| std::sync::Arc::new(image.into_rgba8()))
}

pub(super) fn is_current(current: Option<u64>, closing: u64) -> bool {
    current == Some(closing)
}
pub(super) fn release_window(session_id: &str, generation: u64, failed: bool, cx: &mut App) {
    if !is_current(
        cx.global::<PlayerWindows>()
            .entries
            .get(session_id)
            .map(|entry| entry.generation),
        generation,
    ) {
        return;
    }
    if let Some(entry) = cx.global_mut::<PlayerWindows>().entries.remove(session_id) {
        cx.global_mut::<PlayerWindows>()
            .focus_events
            .retain(|event| *event != desktop_target(generation));
        let session_id = session_id.to_owned();
        cx.defer(move |cx| {
            entry.coordinator.update(cx, |coordinator, cx| {
                if failed {
                    coordinator.embedded_player_failed(session_id, cx);
                } else {
                    coordinator.embedded_player_closed(session_id, cx);
                }
            })
        });
    }
}

/// Coordinator-driven close: remove the lease before the native-close callback,
/// so releasing a session cannot recursively release it a second time.
pub fn close_session(session_id: &str, cx: &mut App) {
    let entry = cx
        .try_global::<PlayerWindows>()
        .and_then(|registry| registry.entries.get(session_id))
        .map(|entry| (entry.generation, entry.handle));
    if let Some((generation, handle)) = entry
        && is_current(
            cx.global::<PlayerWindows>()
                .entries
                .get(session_id)
                .map(|entry| entry.generation),
            generation,
        )
    {
        cx.global_mut::<PlayerWindows>().entries.remove(session_id);
        cx.global_mut::<PlayerWindows>()
            .focus_events
            .retain(|event| *event != desktop_target(generation));
        let _ = handle.update(cx, |view, window, cx| {
            view.flush_progress(cx);
            view.lease = None;
            window.remove_window();
        });
    }
}

/// Snapshot all live players before application-owned shutdown draining.
/// Delivery is deferred to avoid reentering an updating coordinator. The caller
/// must yield one app-effects turn before deciding that the writer is drained.
pub fn checkpoint_all(cx: &mut App) {
    let handles: Vec<_> = cx
        .try_global::<PlayerWindows>()
        .map(|registry| {
            registry
                .entries
                .values()
                .map(|entry| entry.handle)
                .collect()
        })
        .unwrap_or_default();
    for handle in handles {
        let _ = handle.update(cx, |view, _, cx| view.flush_progress(cx));
    }
}

/// Invalidate queued observations immediately; continuing playback requires an
/// application-owned durable epoch handshake before it can save again.
pub fn invalidate_progress(session: &str, generation: u64, epoch: i64, cx: &mut App) {
    update_progress_window(session, cx, |view, _| {
        if view
            .lease
            .as_ref()
            .is_none_or(|(_, current)| *current != generation)
        {
            return;
        }
        if let Some(p) = &mut view.progress
            && p.launch.epoch == epoch
        {
            p.authorized = false;
            p.last = None;
        }
    });
}
pub fn authorize_progress(
    session: &str,
    generation: u64,
    old_epoch: i64,
    new_epoch: i64,
    active_run: i64,
    cx: &mut App,
) {
    update_progress_window(session, cx, |view, cx| {
        if view
            .lease
            .as_ref()
            .is_none_or(|(_, current)| *current != generation)
        {
            return;
        }
        let Some(p) = &mut view.progress else {
            return;
        };
        if p.launch.epoch != old_epoch || new_epoch < old_epoch || active_run <= 0 {
            return;
        }
        p.launch.epoch = new_epoch;
        p.launch.run = active_run;
        p.authorized = true;
        p.notice = false;
        if matches!(p.phase, ProgressPhase::Reset | ProgressPhase::Ready) {
            p.last = None;
            if p.phase == ProgressPhase::Reset {
                p.launch.checkpoint = None;
            }
            p.phase = ProgressPhase::Active;
            let play = p.play;
            view.presentation_command(|player| player.set_paused(!play), cx);
        }
        cx.notify();
    });
}
pub fn progress_notice(session: &str, generation: u64, failed: bool, cx: &mut App) {
    update_progress_window(session, cx, |view, cx| {
        if view
            .lease
            .as_ref()
            .is_none_or(|(_, current)| *current != generation)
        {
            return;
        }
        if let Some(p) = &mut view.progress {
            p.notice = failed;
            cx.notify();
        }
    });
}
pub(super) fn update_progress_window(
    session: &str,
    cx: &mut App,
    update: impl FnOnce(&mut PlayerWindow, &mut Context<PlayerWindow>),
) {
    let handle = cx
        .try_global::<PlayerWindows>()
        .and_then(|registry| registry.entries.get(session))
        .map(|entry| entry.handle);
    if let Some(handle) = handle {
        let _ = handle.update(cx, |view, _, cx| update(view, cx));
    }
}

/// Even an initialized result delivered after the window closed must shut down
/// on a worker. This wrapper also covers cancellation of the initialization task.
pub(super) struct BackgroundPlayer {
    pub(super) player: Option<Player>,
    pub(super) executor: BackgroundExecutor,
    pub(super) stopped: Arc<std::sync::atomic::AtomicBool>,
}
impl Drop for BackgroundPlayer {
    fn drop(&mut self) {
        if let Some(player) = self.player.take() {
            let stopped = self.stopped.clone();
            self.executor
                .spawn(async move {
                    drop(player);
                    stopped.store(true, std::sync::atomic::Ordering::Release);
                })
                .detach();
        }
    }
}

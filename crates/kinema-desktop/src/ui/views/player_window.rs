//! Separate, lease-owned Cinema player. libmpv initialization and destruction
//! (including worker joins) never run on GPUI's foreground executor.
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::controllers::playback_progress;
use crate::{
    desktop_integration::{
        Playback as DesktopPlayback, PlayerSnapshot, PlayerTarget, Transport, normalized_volume,
    },
    player::PlayerChoice,
};
use gpui::{prelude::*, *};
use kinema_core::media::PlaybackTitle;
#[cfg(feature = "ui-probe")]
use kinema_core::media::playback_title;
use kinema_mpv::{Config, PlaybackFailure, PlaybackState, Player, TrackKind};
use playback_progress::PlaybackIntent;
use playback_progress::{Phase as ProgressPhase, Progress, duration_mismatch};
pub(crate) use playback_progress::{PlaybackLaunch, PlayerCheckpoint};

#[cfg(feature = "gpu-surface")]
use kinema_mpv::ResolutionPolicy;

use super::player_controls::{
    self as dock, Scrub, VOLUME_STEP, chapter_index, chapter_label, chapter_ticks,
    displayed_position, displayed_volume, format_clock, format_remaining, ratio_in_span,
    scrub_settled, speed_label, step_volume, subtitle_position, track_label, volume_at,
    volume_fraction, volume_rail_height, volume_settled,
};
use crate::controllers::coordinator::AppCoordinator;
use crate::{
    ui::primitives::button::{Button, ButtonVariant},
    ui::primitives::icon::{Icon, IconName},
    ui::primitives::menu::{Menu, MenuItem},
    ui::theme::typography::TypeExt,
    ui::theme::{Palette, radius, space as s, typeface as t},
};

#[cfg(feature = "gpu-surface")]
type VideoSurfaceFrame = GpuSurfaceFrame;

fn title_for_width(title: &PlaybackTitle, full_width: Pixels, available: Pixels) -> &str {
    if full_width <= available {
        &title.full
    } else {
        &title.compact
    }
}

fn embedded_help() -> &'static str {
    if cfg!(feature = "gpu-surface") {
        "Embedded GPU playback unavailable. No CPU fallback or device-loss recovery. Close and reopen the window, or restart Kinema; you can also choose another player in Settings → Playback. Required SDR GPU interop capabilities may be missing. HDR deferred; Windows/macOS runtime qualification pending."
    } else {
        "Embedded GPU backend not compiled. Rebuild with gpu-surface enabled, or choose external MPV, VLC, or a custom player in Settings → Playback."
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailurePhase {
    Config,
    Initialize,
    Load,
    Runtime,
    Command,
    #[cfg(feature = "gpu-surface")]
    Surface,
}

impl FailurePhase {
    fn code(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Initialize => "initialize",
            Self::Load => "load",
            Self::Runtime => "runtime",
            Self::Command => "command",
            #[cfg(feature = "gpu-surface")]
            Self::Surface => "surface",
        }
    }
}

// The string stays private; only reviewed categories may be presented or logged.
type StartupError = (String, PlaybackFailure, FailurePhase);

fn startup_error(error: kinema_mpv::Error, phase: FailurePhase) -> StartupError {
    let reason = error.playback_failure();
    (error.to_string(), reason, phase)
}

fn failure_phase(state: &PlaybackState, private: Option<FailurePhase>) -> FailurePhase {
    if state.failure_reason.is_some() {
        FailurePhase::Runtime
    } else {
        private.unwrap_or(FailurePhase::Runtime)
    }
}

/// Only reviewed, static backend text crosses the presentation boundary.
fn failure_label(reason: PlaybackFailure) -> String {
    format!("{}: {}", reason.code(), reason.message())
}

fn failure_reason(state: &PlaybackState, private: Option<PlaybackFailure>) -> PlaybackFailure {
    state
        .failure_reason
        .or(private)
        .unwrap_or(PlaybackFailure::Unknown)
}

/// A failed window reports exactly once, even when sampled at display cadence.
fn failure_transition(failed: bool, reported: &mut bool) -> bool {
    if !failed || *reported {
        return false;
    }
    *reported = true;
    true
}

#[cfg(feature = "gpu-surface")]
fn required_gpu_context<T>(context: Option<T>) -> Result<T, StartupError> {
    context.ok_or_else(|| {
        (
            embedded_help().into(),
            PlaybackFailure::GpuContextUnavailable,
            FailurePhase::Config,
        )
    })
}

fn player_config() -> Result<Config, StartupError> {
    #[cfg(not(feature = "gpu-surface"))]
    {
        Err((
            embedded_help().into(),
            PlaybackFailure::BackendNotCompiled,
            FailurePhase::Config,
        ))
    }
    #[cfg(feature = "gpu-surface")]
    {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let context = {
            let context = required_gpu_context(gpui_wgpu::experimental_gpu_context())?;
            let gate = context.experimental_gpu_queue_gate();
            kinema_mpv::GpuSurfaceContext::new(context.device.clone(), context.queue.clone(), gate)
        };
        #[cfg(target_os = "macos")]
        let context = kinema_mpv::GpuSurfaceContext::new_metal(required_gpu_context(
            gpui_apple::experimental_gpu_registry_id(),
        )?);
        Ok(Config {
            resolution: ResolutionPolicy::Source,
            gpu_surface: Some(context),
            ..Default::default()
        })
    }
}

#[cfg(feature = "gpu-surface")]
fn video_surface(frame: kinema_mpv::GpuSurfaceFrame) -> Result<Arc<VideoSurfaceFrame>, String> {
    let frame = Arc::new(frame);
    let dimensions = frame.size();
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let surface = GpuSurfaceFrame::new(
        frame
            .texture()
            .create_view(&wgpu::TextureViewDescriptor::default()),
        dimensions,
        frame,
    );
    #[cfg(target_os = "macos")]
    let surface = GpuSurfaceFrame::new(frame.pixel_buffer(), dimensions, frame);
    surface
        .map(Arc::new)
        .map_err(|_| "Invalid embedded GPU surface; refusing CPU fallback".into())
}

use crate::player_shortcuts::{self as shortcuts, ShortcutId as Sid, *};

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(shortcuts::key_bindings());
}

// Focus/tab order follows the visual order of the dock.
const CLOSE: usize = 0;
const TIMELINE: usize = 1;
const PLAY: usize = 2;
const BACK: usize = 3;
const FORWARD: usize = 4;
const CHAPTERS: usize = 5;
const SUBTITLES: usize = 6;
const AUDIO: usize = 7;
const SPEED: usize = 8;
const FULLSCREEN: usize = 9;
const MUTE: usize = 10;
const VOLUME: usize = 11;
const HELP: usize = 12;
const CONTROL_COUNT: usize = 13;
/// Chrome auto-hides after this much inactivity while playing (approved design).
const CHROME_IDLE: Duration = Duration::from_millis(2500);
/// Dock height assumed until its first layout is measured.
/// Approved Cinema Dock content insets (design `.dock{left/right:28px;bottom:23px}`).
const DOCK_SIDE_INSET: Pixels = px(28.);
const DOCK_BOTTOM_INSET: Pixels = px(23.);
/// Height of the transparent-to-scrim fade above the dock backing (decoration only).
const DOCK_FADE: Pixels = px(72.);
const TOP_FADE: Pixels = px(48.);
/// Shortcut help column width (label + keycaps).
const HELP_COLUMN: Pixels = px(300.);
/// Volume panel chrome around the rail: padding, label, gaps, mute, border.
const VOLUME_PANEL_EXTRA: Pixels = px(84.);
/// Top title row height (padding + 32 px control) the volume panel clears.
const TOP_BAR_HEIGHT: Pixels = px(48.);
const VOLUME_TRACK: Pixels = px(12.);
const VOLUME_THUMB: Pixels = px(18.);
const DOCK_ESTIMATE: Pixels = px(148.);
const SPEEDS: [f64; 7] = [0.5, 0.75, 1., 1.25, 1.5, 1.75, 2.];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuKind {
    Chapters,
    Subtitles,
    Audio,
    Speed,
}
type MenuRun = Rc<dyn Fn(&mut PlayerWindow, &mut Context<PlayerWindow>)>;
struct OpenMenu {
    kind: MenuKind,
    menu: Entity<Menu>,
    _dismissed: Subscription,
}

#[derive(Default)]
struct PlayerWindows {
    next: u64,
    entries: HashMap<String, LeaseWindow>,
    focus_events: VecDeque<PlayerTarget>,
}
impl Global for PlayerWindows {}
struct LeaseWindow {
    generation: u64,
    handle: WindowHandle<PlayerWindow>,
    coordinator: Entity<AppCoordinator>,
    mode: PlayerChoice,
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

fn desktop_target(generation: u64) -> PlayerTarget {
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

fn desktop_snapshot(
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
fn desktop_can_seek(state: &PlaybackState) -> bool {
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
fn bounded_seek(position: f64, duration: Option<f64>) -> f64 {
    let position = position.max(0.0);
    duration
        .filter(|v| v.is_finite() && *v > 0.0)
        .map_or(position, |duration| position.min(duration))
}
fn media_time(seconds: f64) -> Option<Duration> {
    Duration::try_from_secs_f64(seconds).ok()
}
fn desktop_handle(target: PlayerTarget, cx: &App) -> Option<WindowHandle<PlayerWindow>> {
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
fn record_player_focus(generation: u64, cx: &mut App) {
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
            size(px(1100.), px(680.)),
            cx,
        ))),
        window_min_size: Some(size(px(720.), px(440.))),
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

fn player_window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    image::load_from_memory(include_bytes!(
        "../../../assets/brand/png/256/dev.tlmtech.kinema-player.png"
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

fn is_current(current: Option<u64>, closing: u64) -> bool {
    current == Some(closing)
}
fn release_window(session_id: &str, generation: u64, failed: bool, cx: &mut App) {
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
fn update_progress_window(
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
struct BackgroundPlayer {
    player: Option<Player>,
    executor: BackgroundExecutor,
    stopped: Arc<std::sync::atomic::AtomicBool>,
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

fn diagnostics_enabled(value: Option<&str>) -> bool {
    value == Some("1")
}

fn window_rate(count: u64, seconds: f64) -> f64 {
    if seconds.is_finite() && seconds > 0. {
        count as f64 / seconds
    } else {
        0.
    }
}

/// Furthest end of a valid observed range containing the playhead, in O(n).
/// Adjacent/overlapping ranges are not merged; never a download estimate.
fn cache_ahead(state: &PlaybackState) -> Option<f64> {
    let ranges = state.buffered_ranges.as_ref()?;
    let position = state.position;
    if !position.is_finite() {
        return None;
    }
    let end = ranges
        .iter()
        .filter(|range| {
            range.start.is_finite()
                && range.end.is_finite()
                && range.start <= position
                && range.end >= position
        })
        .fold(position, |end, range| end.max(range.end));
    Some(end - position)
}

/// Valid buffered ranges as clamped `(start, end)` fractions of `duration`.
/// Invalid, empty or out-of-media ranges are dropped; widths are never widened.
fn buffered_fractions(
    ranges: Option<&[kinema_mpv::BufferedRange]>,
    duration: f64,
) -> Vec<(f32, f32)> {
    if !(duration.is_finite() && duration > 0.) {
        return Vec::new();
    }
    ranges
        .into_iter()
        .flatten()
        .filter(|range| range.start.is_finite() && range.end.is_finite())
        .filter_map(|range| {
            let start = (range.start / duration).clamp(0., 1.) as f32;
            let end = (range.end / duration).clamp(0., 1.) as f32;
            (end > start).then_some((start, end))
        })
        .collect()
}

/// Rolling window for the recent download speed.
const RATE_WINDOW: Duration = Duration::from_secs(5);
/// Shortest observed span that yields a speed rather than noise.
const RATE_MIN_SPAN: Duration = Duration::from_secs(1);

/// Recent download speed from monotonically observed byte counters. The first
/// observation is only a baseline: bytes that already existed are never counted
/// as newly downloaded. A different session or a regressing counter resets it.
#[derive(Default)]
struct TransferRate {
    session: Option<String>,
    samples: VecDeque<(Instant, u64)>,
    latest: Option<(u64, u64)>,
}

impl TransferRate {
    fn observe(&mut self, session: &str, progress: Option<(u64, u64)>, now: Instant) {
        if self.session.as_deref() != Some(session) {
            self.session = Some(session.to_owned());
            self.samples.clear();
        }
        self.latest = progress;
        let Some((downloaded, _)) = progress else {
            self.samples.clear();
            return;
        };
        if let Some(&(_, last)) = self.samples.back()
            && downloaded < last
        {
            self.samples.clear();
        }
        let unchanged = self.samples.back().is_some_and(|&(at, bytes)| {
            bytes == downloaded && now.saturating_duration_since(at) < Duration::from_millis(250)
        });
        if !unchanged {
            self.samples.push_back((now, downloaded));
        }
        // Keep one sample at/before the window start so the span is ~RATE_WINDOW.
        while self.samples.len() > 1
            && self
                .samples
                .get(1)
                .is_some_and(|&(at, _)| now.saturating_duration_since(at) >= RATE_WINDOW)
        {
            self.samples.pop_front();
        }
    }
    /// What the label shows: counters and the whole-KB/s speed.
    fn display_key(&self) -> (Option<(u64, u64)>, Option<u64>) {
        (
            self.latest,
            self.bytes_per_second().map(|rate| rate as u64 / 1000),
        )
    }
    fn clear(&mut self) {
        self.session = None;
        self.samples.clear();
        self.latest = None;
    }
    /// Bytes/s, or `None` while there is too little observation (not zero).
    fn bytes_per_second(&self) -> Option<f64> {
        let &(first_at, first) = self.samples.front()?;
        let &(last_at, last) = self.samples.back()?;
        let span = last_at.saturating_duration_since(first_at);
        (span >= RATE_MIN_SPAN).then(|| last.saturating_sub(first) as f64 / span.as_secs_f64())
    }
}

const TRANSFER_POLL: Duration = Duration::from_secs(1);
fn transfer_poll_due(last: Option<Instant>, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= TRANSFER_POLL)
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1000. && unit < UNITS.len() - 1 {
        value /= 1000.;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

/// Playable MPV cache ahead of the playhead; `—` when MPV reports no cache.
/// Playable time ahead of the playhead, e.g. `45s` or `6m 33s`.
fn ahead_duration(state: &PlaybackState) -> Option<String> {
    cache_ahead(state).map(|seconds| {
        // Round once, then split, so 179.9 s is 3m 00s rather than 2m 60s.
        let total = seconds.max(0.).round() as u64;
        if total < 120 {
            format!("{total}s")
        } else {
            format!("{}m {:02}s", total / 60, total % 60)
        }
    })
}

fn buffer_ahead_label(state: &PlaybackState) -> String {
    format!("Buffer {}", ahead_duration(state).as_deref().unwrap_or("—"))
}

fn download_percent(downloaded: u64, total: u64) -> Option<u64> {
    (total > 0).then(|| (u128::from(downloaded.min(total)) * 100 / u128::from(total)) as u64)
}

/// Downloaded file bytes (not MPV's playable cache), labelled as such.
fn download_label(transfer: &TransferRate) -> Option<String> {
    let (downloaded, total) = transfer.latest?;
    let speed = transfer.bytes_per_second().map_or_else(
        || "—".to_owned(),
        |rate| format!("{}/s", format_bytes(rate as u64)),
    );
    Some(match download_percent(downloaded, total) {
        Some(p) => format!(
            "Downloaded {} of {} ({p}%) · {speed}",
            format_bytes(downloaded),
            format_bytes(total)
        ),
        None => format!("Downloaded {} · {speed}", format_bytes(downloaded)),
    })
}

/// One compact dock status entry: optional leading icon plus short text.
#[derive(Clone, Debug, PartialEq)]
struct StatusItem {
    icon: Option<IconName>,
    text: String,
    warn: bool,
}

impl StatusItem {
    fn text(text: impl Into<String>) -> Self {
        Self {
            icon: None,
            text: text.into(),
            warn: false,
        }
    }
    fn icon(icon: IconName, text: impl Into<String>) -> Self {
        Self {
            icon: Some(icon),
            text: text.into(),
            warn: false,
        }
    }
    fn warning(text: impl Into<String>) -> Self {
        Self {
            icon: Some(IconName::Warning),
            text: text.into(),
            warn: true,
        }
    }
}

/// Compact buffer-ahead and in-progress download entries. Unknown buffer and
/// completed downloads are omitted; full wording lives in the tooltip/label.
fn transfer_items(state: &PlaybackState, transfer: &TransferRate) -> Vec<StatusItem> {
    if state.loading || state.ended {
        return Vec::new();
    }
    let mut items: Vec<StatusItem> = ahead_duration(state)
        .map(|ahead| StatusItem::icon(IconName::Timer, ahead))
        .into_iter()
        .collect();
    if let Some((downloaded, total)) = transfer.latest {
        let percent = download_percent(downloaded, total);
        if percent.is_none() || downloaded < total {
            let amount = percent.map_or_else(|| format_bytes(downloaded), |p| format!("{p}%"));
            let text = match transfer.bytes_per_second() {
                Some(rate) => format!("{amount} · {}/s", format_bytes(rate as u64)),
                None => amount,
            };
            items.push(StatusItem::icon(IconName::Download, text));
        }
    }
    items
}

// Emit only fixed categories, never driver/decoder strings or fallback errors.
fn decoder_category(value: Option<&str>) -> &'static str {
    match value {
        None => "unavailable",
        Some("no" | "") => "software",
        Some("vaapi" | "vaapi-copy") => "vaapi",
        Some("vdpau" | "vdpau-copy") => "vdpau",
        Some("nvdec" | "nvdec-copy" | "cuda" | "cuda-copy") => "nvdec",
        Some("d3d11va" | "d3d11va-copy" | "dxva2" | "dxva2-copy") => "windows_hw",
        Some("videotoolbox" | "videotoolbox-copy") => "videotoolbox",
        Some(_) => "other",
    }
}

fn renderer_category(value: Option<&str>) -> &'static str {
    let Some(value) = value else {
        return "unavailable";
    };
    let value = value.to_ascii_lowercase();
    if ["llvmpipe", "softpipe", "swrast", "software"]
        .iter()
        .any(|s| value.contains(s))
    {
        "software"
    } else if value.contains("nvidia") {
        "nvidia"
    } else if value.contains("amd") || value.contains("radeon") {
        "amd"
    } else if value.contains("intel") {
        "intel"
    } else if value.contains("apple") {
        "apple"
    } else {
        "other"
    }
}

/// What the video pipeline is expected to be doing; only `Playing` makes the
/// accepted-image rate meaningful. Everything else is a state, not a regression.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Activity {
    Playing,
    Paused,
    Buffering,
    Waiting,
    Stopped,
}

fn activity_of(state: &PlaybackState, has_player: bool) -> Activity {
    if !has_player || state.loading {
        Activity::Waiting
    } else if state.paused_for_cache {
        Activity::Buffering
    } else if state.ended || state.media_stopped {
        Activity::Stopped
    } else if state.paused {
        Activity::Paused
    } else {
        Activity::Playing
    }
}

const CADENCE_INTERVAL: Duration = Duration::from_secs(1);

struct CadenceReport {
    interval_seconds: f64,
    playing_seconds: f64,
    accepted: u64,
    /// Accepted images per playing second; `None` when nothing was playing.
    fps: Option<f64>,
    /// Longest accepted-image interarrival gap while playing, including a gap
    /// still open at the interval end (a UI stall is not hidden by the boundary).
    max_accept_gap_ms: Option<f64>,
    sample_max_gap_ms: f64,
    cache_paused_samples: u64,
    /// `Render::render` invocations: view renders, not GPU presents.
    view_render_calls: u64,
    max_view_render_gap_ms: Option<f64>,
    activity: Activity,
}
impl CadenceReport {
    /// A UI-side image-acceptance rate, not the physical display refresh rate.
    fn label(&self) -> String {
        match (self.activity, self.fps) {
            (Activity::Playing, Some(fps)) => match self.max_accept_gap_ms {
                Some(gap) => format!("Video updates: {fps:.1} fps · max gap {gap:.0} ms"),
                None => format!("Video updates: {fps:.1} fps"),
            },
            (Activity::Playing, None) => "Video updates: measuring…".into(),
            (Activity::Paused, _) => "Video updates: Paused".into(),
            (Activity::Buffering, _) => "Video updates: Buffering".into(),
            (Activity::Waiting, _) => "Video updates: Waiting".into(),
            (Activity::Stopped, _) => "Video updates: Stopped".into(),
        }
    }
}

/// Per-second accepted-image cadence. Pure and clock-injected. Only images that
/// were successfully turned into a render image while `Playing` count; any
/// non-playing sample drops the interarrival baseline, so the first image after
/// load/seek-pause/resume/replay never measures the idle gap.
struct CadenceMeter {
    interval_start: Instant,
    last_sample: Instant,
    last_accept: Option<Instant>,
    activity: Activity,
    playing: Duration,
    accepted: u64,
    max_accept_gap: Option<Duration>,
    sample_max_gap: Duration,
    cache_paused_samples: u64,
    render_calls: u64,
    last_render: Option<Instant>,
    max_render_gap: Option<Duration>,
}
impl CadenceMeter {
    fn new(now: Instant) -> Self {
        Self {
            interval_start: now,
            last_sample: now,
            last_accept: None,
            activity: Activity::Waiting,
            playing: Duration::ZERO,
            accepted: 0,
            max_accept_gap: None,
            sample_max_gap: Duration::ZERO,
            cache_paused_samples: 0,
            render_calls: 0,
            last_render: None,
            max_render_gap: None,
        }
    }
    /// Count a `Render::render` call (not a GPU present) and its gap.
    fn render_called(&mut self, now: Instant) {
        self.render_calls += 1;
        if let Some(previous) = self.last_render {
            let gap = now.saturating_duration_since(previous);
            self.max_render_gap = Some(self.max_render_gap.map_or(gap, |max| max.max(gap)));
        }
        self.last_render = Some(now);
    }
    fn sample(&mut self, now: Instant, activity: Activity, accepted: bool) {
        let dt = now.saturating_duration_since(self.last_sample);
        self.last_sample = now;
        self.sample_max_gap = self.sample_max_gap.max(dt);
        // The elapsed span belongs to what was observed at its start, so idle
        // time before the first Playing sample is never charged to playback,
        // while a long UI stall during Playing stays in the playing time.
        let previous = std::mem::replace(&mut self.activity, activity);
        if previous == Activity::Playing {
            self.playing += dt;
        }
        if activity == Activity::Buffering {
            self.cache_paused_samples += 1;
        }
        if activity != Activity::Playing {
            self.last_accept = None;
            self.last_render = None;
            return;
        }
        if accepted {
            self.accepted += 1;
            if let Some(previous) = self.last_accept {
                let gap = now.saturating_duration_since(previous);
                self.max_accept_gap = Some(self.max_accept_gap.map_or(gap, |max| max.max(gap)));
            }
            self.last_accept = Some(now);
        }
    }
    /// One report per elapsed interval; a long stall yields one honest report
    /// covering the whole stall, never a burst of catch-up reports.
    fn finish_if_due(&mut self, now: Instant) -> Option<CadenceReport> {
        let elapsed = now.saturating_duration_since(self.interval_start);
        if elapsed < CADENCE_INTERVAL {
            return None;
        }
        // Time since the last sample (a stall, or just the tail of the interval)
        // belongs to the last observed activity.
        let tail = now.saturating_duration_since(self.last_sample);
        self.last_sample = now;
        self.sample_max_gap = self.sample_max_gap.max(tail);
        if self.activity == Activity::Playing {
            self.playing += tail;
        }
        let open = (self.activity == Activity::Playing)
            .then_some(self.last_accept)
            .flatten()
            .map(|at| now.saturating_duration_since(at));
        let max_gap = match (self.max_accept_gap, open) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let open_render = (self.activity == Activity::Playing)
            .then_some(self.last_render)
            .flatten()
            .map(|at| now.saturating_duration_since(at));
        let render_gap = match (self.max_render_gap, open_render) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
        let playing = self.playing.as_secs_f64();
        let report = CadenceReport {
            interval_seconds: elapsed.as_secs_f64(),
            playing_seconds: playing,
            accepted: self.accepted,
            fps: (playing > 0.).then(|| self.accepted as f64 / playing),
            max_accept_gap_ms: max_gap.map(|gap| gap.as_secs_f64() * 1000.),
            sample_max_gap_ms: self.sample_max_gap.as_secs_f64() * 1000.,
            cache_paused_samples: self.cache_paused_samples,
            view_render_calls: self.render_calls,
            max_view_render_gap_ms: render_gap.map(|gap| gap.as_secs_f64() * 1000.),
            activity: self.activity,
        };
        self.interval_start = now;
        self.playing = Duration::ZERO;
        self.accepted = 0;
        self.max_accept_gap = None;
        self.sample_max_gap = Duration::ZERO;
        self.cache_paused_samples = 0;
        self.render_calls = 0;
        self.max_render_gap = None;
        Some(report)
    }
    fn log(report: &CadenceReport) {
        tracing::info!(
            target: "kinema_desktop::player_performance",
            interval_seconds = report.interval_seconds,
            playing_seconds = report.playing_seconds,
            accepted_images = report.accepted,
            accepted_images_fps = report.fps,
            max_accepted_gap_ms = report.max_accept_gap_ms,
            sample_max_gap_ms = report.sample_max_gap_ms,
            samples_paused_for_cache = report.cache_paused_samples,
            view_render_calls = report.view_render_calls,
            max_view_render_gap_ms = report.max_view_render_gap_ms,
            activity = ?report.activity,
            "Embedded player cadence"
        );
    }
}

struct PlayerPerformance {
    started: Instant,
    last_report: Instant,
    last_sample: Instant,
    max_sample_gap_ms: f64,
    samples: u64,
    received: u64,
    presented: u64,
    paused_for_cache_samples: u64,
    sample_ms: f64,
}
impl PlayerPerformance {
    fn new(now: Instant) -> Self {
        Self {
            started: now,
            last_report: now,
            last_sample: now,
            max_sample_gap_ms: 0.,
            samples: 0,
            received: 0,
            presented: 0,
            paused_for_cache_samples: 0,
            sample_ms: 0.,
        }
    }

    fn report(
        &mut self,
        now: Instant,
        backend: &kinema_mpv::RenderDiagnostics,
        state: &PlaybackState,
    ) {
        let seconds = now.duration_since(self.last_report).as_secs_f64();
        tracing::info!(
            target: "kinema_desktop::player_performance",
            elapsed_seconds = now.duration_since(self.started).as_secs_f64(),
            window_seconds = seconds,
            window_samples = self.samples,
            window_received_frames = self.received,
            window_accepted_images = self.presented,
            window_received_fps = window_rate(self.received, seconds),
            window_accepted_images_fps = window_rate(self.presented, seconds),
            window_samples_paused_for_cache = self.paused_for_cache_samples,
            window_sample_max_gap_ms = self.max_sample_gap_ms,
            window_sample_mean_ms = self.sample_ms / self.samples.max(1) as f64,
            backend_elapsed_seconds = backend.elapsed_seconds,
            backend_cumulative_rendered_frames = backend.rendered_frames,
            backend_cumulative_delivered_frames = backend.delivered_frames,
            backend_cumulative_coalesced_frames = backend.coalesced_frames,
            backend_cumulative_delivered_fps = backend.delivered_fps,
            backend_cumulative_readback_bytes = backend.readback_bytes,
            backend_cumulative_upload_bytes_estimate = backend.upload_bytes_estimate,
            backend_cumulative_transfer_bytes_per_second = backend.transfer_bytes_per_second,
            backend_cumulative_mean_render_call_ms = backend.render_call_ms,
            backend_cumulative_mean_readback_submit_ms = backend.readback_submit_ms,
            backend_cumulative_mean_readback_copy_ms = backend.readback_copy_ms,
            backend_cumulative_mean_fence_latency_ms = backend.fence_latency_ms,
            backend_cumulative_mean_frame_age_ms = backend.frame_age_ms,
            render_width = backend.render_size[0],
            render_height = backend.render_size[1],
            source_width = backend.source_size.map(|size| size[0]),
            source_height = backend.source_size.map(|size| size[1]),
            position_seconds = state.position,
            duration_seconds = state.duration,
            loading = state.loading,
            media_stopped = state.media_stopped,
            requested_backend = ?backend.requested_backend,
            active_backend = ?backend.active_backend,
            hardware_decoder_category = decoder_category(backend.hardware_decoder.as_deref()),
            gl_renderer_category = renderer_category(backend.gl_renderer.as_deref()),
            fallback_category = if backend.fallback_reason.is_some() { "backend_fallback" } else { "none" },
            paused = state.paused,
            paused_for_cache = state.paused_for_cache,
            cache_available = state.buffered_ranges.is_some(),
            cache_range_count = state.buffered_ranges.as_ref().map(Vec::len),
            cache_ahead_seconds = cache_ahead(state),
            "Embedded player performance"
        );
        self.last_report = now;
        self.samples = 0;
        self.received = 0;
        self.presented = 0;
        self.paused_for_cache_samples = 0;
        self.sample_ms = 0.;
        self.max_sample_gap_ms = 0.;
    }
}

pub struct PlayerWindow {
    title: PlaybackTitle,
    progress: Option<Progress>,
    progress_owner: Option<Entity<AppCoordinator>>,
    lease: Option<(String, u64)>,
    #[cfg(any(feature = "gpu-surface", feature = "ui-probe"))]
    frames_presented: u64,
    diagnostics: Option<PlayerPerformance>,
    cadence: Option<CadenceMeter>,
    cadence_text: Option<String>,
    player: Option<BackgroundPlayer>,
    state: PlaybackState,
    error: Option<String>,
    failure_reason: Option<PlaybackFailure>,
    failure_phase: Option<FailurePhase>,
    failure_reported: bool,
    control_notice: bool,
    source_url: Option<String>,
    mode: PlayerChoice,
    // Static offline artwork only, never populated from a backend frame.
    #[cfg(feature = "ui-probe")]
    fixture_image: Option<Arc<RenderImage>>,
    #[cfg(feature = "gpu-surface")]
    gpu_frame: Option<Arc<VideoSurfaceFrame>>,
    last_activity: Instant,
    dock_hover: bool,
    chrome_was_visible: bool,
    surface_focus: FocusHandle,
    controls: Vec<FocusHandle>,
    scrub: Option<Scrub>,
    hover_ratio: Option<f32>,
    timeline_bounds: Bounds<Pixels>,
    volume_bounds: Bounds<Pixels>,
    volume_dragging: bool,
    volume_requested: Option<(f64, Instant)>,
    menu: Option<OpenMenu>,
    help_focus: Option<FocusHandle>,
    help_dismiss_focus: FocusHandle,
    help_previous: Option<FocusHandle>,
    shortcut_held: bool,
    _shortcut_keystrokes: Subscription,
    seek_requested: Option<(f64, Instant)>,
    speed_requested: Option<(f64, Instant)>,
    delay_requested: Option<(f64, Instant)>,
    dock_height: Pixels,
    /// In-memory download observations of the leased remote session.
    transfer: TransferRate,
    last_transfer_poll: Option<Instant>,
    /// Last `sub-pos` sent for this load; None = unknown, resend on next sync.
    subtitle_position: Option<u8>,
    startup_cancel: Arc<std::sync::atomic::AtomicBool>,
    _initialize: Task<()>,
    _updates: Task<()>,
}
impl Drop for PlayerWindow {
    fn drop(&mut self) {
        self.startup_cancel
            .store(true, std::sync::atomic::Ordering::Release);
        #[cfg(feature = "gpu-surface")]
        {
            self.gpu_frame.take();
        }
        self.player.take();
    }
}

impl PlayerWindow {
    fn open_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.help_focus.is_some() {
            return;
        }
        self.help_previous = window.focused(cx);
        let focus = cx.focus_handle();
        focus.focus(window, cx);
        self.help_focus = Some(focus);
        self.activity(cx);
    }
    fn close_help(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.help_focus = None;
        self.help_previous
            .take()
            .unwrap_or_else(|| self.surface_focus.clone())
            .focus(window, cx);
        self.activity(cx);
    }
    fn shortcut_help(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::dark();
        let groups = shortcuts::reference_groups();
        let width = window.viewport_size().width;
        let columns = if width >= HELP_COLUMN * 3. + s::S24 * 2. + s::S48 * 2. {
            3
        } else if width >= HELP_COLUMN * 2. + s::S24 + s::S48 * 2. {
            2
        } else {
            1
        };
        // Title row + one row per entry, so column heights balance visually.
        let sizes: Vec<usize> = groups.iter().map(|g| g.entries.len() + 2).collect();
        let keycap = |text: &'static str| {
            div()
                .flex_none()
                .px(s::S6)
                .h(px(20.))
                .flex()
                .items_center()
                .rounded(radius::SM)
                .bg(p.bg_raised)
                .border_1()
                .border_color(p.line_control)
                .type_style(t::SMALL_MEDIUM)
                .text_color(p.text_primary)
                .whitespace_nowrap()
                .child(text)
        };
        let group_view = |group: &'static shortcuts::ReferenceGroup| {
            div()
                .flex()
                .flex_col()
                .gap(s::S6)
                .child(
                    div()
                        .pb(s::S2)
                        .type_style(t::CAPTION)
                        .text_color(p.text_muted)
                        .child(group.title.to_uppercase()),
                )
                .children(group.entries.iter().map(move |entry| {
                    let mouse = entry.bindings.is_empty();
                    div()
                        .flex()
                        .items_center()
                        .gap(s::S12)
                        .min_h(px(24.))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .type_style(t::SMALL)
                                .text_color(p.text_secondary)
                                .child(entry.label),
                        )
                        .child(if mouse {
                            div()
                                .flex_none()
                                .max_w(px(132.))
                                .text_right()
                                .type_style(t::SMALL_MEDIUM)
                                .text_color(p.text_primary)
                                .child(entry.keys)
                        } else {
                            div()
                                .flex_none()
                                .flex()
                                .items_center()
                                .gap(s::S4)
                                .children(entry.keys.split(" / ").map(keycap))
                        })
                }))
        };
        div()
            .id("player-shortcut-overlay")
            .absolute()
            .inset_0()
            .bg(p.dialog_scrim)
            .flex()
            .items_center()
            .justify_center()
            .p(s::S24)
            .track_focus(self.help_focus.as_ref().unwrap_or(&self.surface_focus))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .id("player-shortcut-card")
                    .flex()
                    .flex_col()
                    .w_full()
                    .max_w(
                        HELP_COLUMN * columns as f32 + s::S32 * (columns as f32 - 1.) + s::S24 * 2.,
                    )
                    .max_h_full()
                    .bg(p.bg_overlay)
                    .border_1()
                    .border_color(p.line_subtle)
                    .rounded(radius::LG)
                    .overflow_hidden()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s::S12)
                            .pl(s::S24)
                            .pr(s::S12)
                            .py(s::S12)
                            .border_b_1()
                            .border_color(p.line_subtle)
                            .child(
                                Icon::new(IconName::Keyboard)
                                    .size(px(18.))
                                    .color(p.text_secondary),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .type_style(t::HEADING)
                                    .child("Keyboard & mouse shortcuts"),
                            )
                            .child({
                                let mut close = Button::new("player-shortcut-dismiss", "")
                                    .variant(ButtonVariant::Ghost)
                                    .icon(IconName::Close)
                                    .focus_handle(self.help_dismiss_focus.clone())
                                    .on_click(cx.listener(|view, _, window, cx| {
                                        view.close_help(window, cx)
                                    }));
                                close.square = true;
                                close.tooltip = Some("Close (Escape)".into());
                                close
                            }),
                    )
                    .child(
                        div()
                            .id("player-shortcut-scroll")
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scroll()
                            .px(s::S24)
                            .py(s::S16)
                            .child(
                                div().flex().items_start().gap(s::S32).children(
                                    dock::balance_columns(&sizes, columns).into_iter().map(
                                        |range| {
                                            div()
                                                .flex_1()
                                                .min_w_0()
                                                .flex()
                                                .flex_col()
                                                .gap(s::S20)
                                                .children(groups[range].iter().map(group_view))
                                        },
                                    ),
                                ),
                            )
                            // Notes scroll with the list so short windows keep it readable.
                            .child(
                                div()
                                    .mt(s::S20)
                                    .pt(s::S12)
                                    .flex()
                                    .flex_col()
                                    .gap(s::S4)
                                    .border_t_1()
                                    .border_color(p.line_subtle)
                                    .type_style(t::SMALL)
                                    .text_color(p.text_muted)
                                    .child(shortcuts::FOCUS_NOTE)
                                    .child(shortcuts::MEDIA_NOTE),
                            ),
                    ),
            )
            .into_any_element()
    }
    fn shortcut(&mut self, id: Sid, window: &mut Window, cx: &mut Context<Self>) {
        // Only the surface and our own controls participate. A future text editor
        // nested in this window must retain native editing shortcuts.
        let owns_focus = self.surface_focus.is_focused(window)
            || self.controls.iter().any(|focus| focus.is_focused(window));
        if shortcuts::entry(id).repeat == RepeatPolicy::Suppress && self.shortcut_held {
            return;
        }
        if self.help_focus.is_some() && matches!(id, Sid::NextControl | Sid::PreviousControl) {
            self.help_dismiss_focus.focus(window, cx);
            self.activity(cx);
            return;
        }
        if id == Sid::Escape && self.help_focus.is_some() {
            self.close_help(window, cx);
            return;
        }
        if self.menu_open() || !owns_focus {
            cx.propagate();
            return;
        }
        match id {
            Sid::TogglePlayback => self.toggle_playback(cx),
            Sid::SeekBack => self.seek(-10., cx),
            Sid::SeekForward => self.seek(10., cx),
            Sid::SeekBackLong => self.seek(-60., cx),
            Sid::SeekForwardLong => self.seek(60., cx),
            Sid::SeekBackFine => self.seek(-1., cx),
            Sid::SeekForwardFine => self.seek(1., cx),
            Sid::ToggleMute => self.command(Player::toggle_mute, cx),
            Sid::VolumeUp => self.nudge_volume(VOLUME_STEP, cx),
            Sid::VolumeDown => self.nudge_volume(-VOLUME_STEP, cx),
            Sid::ToggleFullscreen => {
                window.toggle_fullscreen();
                self.activity(cx);
            }
            Sid::Escape => {
                if !self.cancel_drag(cx) && window.is_fullscreen() {
                    window.toggle_fullscreen();
                }
                self.activity(cx);
            }
            Sid::NextControl => self.focus_control(false, window, cx),
            Sid::PreviousControl => self.focus_control(true, window, cx),
            Sid::ShowHelp => self.open_help(window, cx),
            Sid::SpeedUp | Sid::SpeedDown | Sid::ResetSpeed => {
                if !self.backend_available() {
                    return self.unavailable(cx);
                }
                let speed = self
                    .speed_requested
                    .filter(|(_, since)| since.elapsed() < Duration::from_millis(1200))
                    .map_or(self.state.speed, |(value, _)| value);
                let target = if id == Sid::ResetSpeed {
                    Some(1.)
                } else {
                    dock::adjacent_speed(speed, &SPEEDS, id == Sid::SpeedUp)
                };
                if let Some(target) = target {
                    self.speed_requested = Some((target, Instant::now()));
                    self.command(|p| p.set_speed(target), cx);
                }
            }
            Sid::PreviousChapter | Sid::NextChapter => {
                if !self.state.chapters_available || !self.can_seek() {
                    return self.unavailable(cx);
                }
                if let Some(target) = dock::adjacent_chapter(
                    &self.state.chapters,
                    self.state.position,
                    id == Sid::NextChapter,
                ) {
                    self.seek_to(target, cx);
                }
            }
            Sid::NextSubtitle | Sid::PreviousSubtitle | Sid::NextAudio => {
                if !self.state.tracks_available || !self.backend_available() {
                    return self.unavailable(cx);
                }
                let kind = if id == Sid::NextAudio {
                    TrackKind::Audio
                } else {
                    TrackKind::Subtitle
                };
                if let Some(target) =
                    dock::cycle_track(&self.state.tracks, kind, id != Sid::PreviousSubtitle)
                {
                    self.command(
                        |p| {
                            if kind == TrackKind::Subtitle {
                                p.select_subtitle(target)
                            } else {
                                p.select_audio(target)
                            }
                        },
                        cx,
                    );
                } else {
                    self.unavailable(cx);
                }
            }
            Sid::ToggleSubtitles => self.command(Player::toggle_subtitle_visibility, cx),
            Sid::DelayBack | Sid::DelayForward => {
                let Some(actual) = self.state.subtitle_delay else {
                    return self.unavailable(cx);
                };
                let delay = self
                    .delay_requested
                    .filter(|(_, since)| since.elapsed() < Duration::from_millis(1200))
                    .map_or(actual, |(value, _)| value);
                let target = ((delay + if id == Sid::DelayForward { 0.1 } else { -0.1 }) * 10.)
                    .round()
                    .clamp(-600., 600.)
                    / 10.;
                self.delay_requested = Some((target, Instant::now()));
                self.command(|p| p.set_subtitle_delay(target), cx);
            }
            Sid::PreviousFrame | Sid::NextFrame => {
                if self.request_command(|p| p.step_frame(id == Sid::PreviousFrame), cx)
                    && let Some(progress) = self.progress.as_mut()
                {
                    progress.play = false;
                }
            }
            Sid::SurfaceFullscreen | Sid::SurfacePause | Sid::SurfaceFocus => {}
        }
    }
    fn unavailable(&mut self, cx: &mut Context<Self>) {
        self.control_notice = true;
        self.activity(cx);
    }

    fn new_mode(
        title: PlaybackTitle,
        url: Option<String>,
        mode: PlayerChoice,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Read once per view; never inspect the environment on the sample path.
        let diagnostics =
            diagnostics_enabled(std::env::var("KINEMA_PLAYER_DIAGNOSTICS").ok().as_deref())
                .then(|| PlayerPerformance::new(Instant::now()));
        let source_url = url.clone();
        let startup_cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let updates = cx.spawn_in(window, async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                if cx
                    .update(|window, cx| view.update(cx, |view, cx| view.sample(window, cx)))
                    .is_err()
                {
                    break;
                }
            }
        });
        let surface_focus = cx.focus_handle();
        surface_focus.focus(window, cx);
        let controls = (0..CONTROL_COUNT).map(|_| cx.focus_handle()).collect();
        cx.observe_window_activation(window, |view, window, cx| {
            if window.is_window_active()
                && let Some((_, generation)) = &view.lease
            {
                record_player_focus(*generation, cx);
            }
        })
        .detach();
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| {
            if let Some(view) = weak.upgrade() {
                view.update(cx, |view, cx| {
                    view.flush_progress(cx);
                    if let Some(p) = &mut view.progress {
                        p.authorized = false;
                    }
                });
            }
            true
        });
        cx.on_release(|view, cx| view.flush_progress(cx)).detach();
        let weak = cx.weak_entity();
        let player_window = window.window_handle();
        // Native repeats must be inspected before action dispatch. Key-up
        // symbols need not match keydown (X11 emits "?" down and "/" up).
        let shortcut_keystrokes = cx.intercept_keystrokes(move |event, window, cx| {
            if window.window_handle() == player_window {
                let _ = weak.update(cx, |view, _| view.shortcut_held = event.is_held);
            }
        });
        let view = Self {
            title,
            progress: None,
            progress_owner: None,
            lease: None,
            #[cfg(any(feature = "gpu-surface", feature = "ui-probe"))]
            frames_presented: 0,
            cadence: diagnostics
                .is_some()
                .then(|| CadenceMeter::new(Instant::now())),
            cadence_text: None,
            diagnostics,
            player: None,
            state: PlaybackState {
                loading: true,
                ..Default::default()
            },
            error: None,
            failure_reason: None,
            failure_phase: None,
            failure_reported: false,
            control_notice: false,
            source_url,
            mode,
            #[cfg(feature = "ui-probe")]
            fixture_image: None,
            #[cfg(feature = "gpu-surface")]
            gpu_frame: None,
            last_activity: Instant::now(),
            dock_hover: false,
            chrome_was_visible: true,
            surface_focus,
            controls,
            scrub: None,
            hover_ratio: None,
            timeline_bounds: Bounds::default(),
            volume_bounds: Bounds::default(),
            volume_dragging: false,
            volume_requested: None,
            menu: None,
            help_focus: None,
            help_dismiss_focus: cx.focus_handle(),
            help_previous: None,
            shortcut_held: false,
            _shortcut_keystrokes: shortcut_keystrokes,
            seek_requested: None,
            speed_requested: None,
            delay_requested: None,
            dock_height: Pixels::default(),
            transfer: TransferRate::default(),
            last_transfer_poll: None,
            subtitle_position: None,
            startup_cancel,
            _initialize: Task::ready(()),
            _updates: updates,
        };
        {
            tracing::info!(
                mode = "embedded-mpv",
                phase = "requested",
                "Embedded GPU startup requested"
            );
            // Capture the actual renderer context only after native window
            // registration, not after an assumed amount of wall-clock time.
            cx.defer_in(window, |view, _, cx| view.start_player(cx));
        }
        view
    }

    fn start_player(&mut self, cx: &mut Context<Self>) {
        if self
            .startup_cancel
            .load(std::sync::atomic::Ordering::Acquire)
        {
            return;
        }
        let config = player_config();
        let url = self.source_url.clone();
        let paused_load = self.progress.is_some();
        let executor = cx.background_executor().clone();
        let worker = executor.clone();
        let canceled = self.startup_cancel.clone();
        let initialize = executor.spawn(async move {
            let Some(url) = url else {
                return Ok(None);
            };
            if canceled.load(std::sync::atomic::Ordering::Acquire) {
                return Ok(None);
            }
            let player = Player::new(config?)
                .map_err(|error| startup_error(error, FailurePhase::Initialize))?;
            // Wrap before load so every failure/cancellation destroys off-thread.
            let owner = BackgroundPlayer {
                player: Some(player),
                executor: worker,
                stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            };
            if canceled.load(std::sync::atomic::Ordering::Acquire) {
                return Ok(None);
            }
            if let Some(player) = owner.player.as_ref() {
                if paused_load {
                    player
                        .set_paused(true)
                        .map_err(|error| startup_error(error, FailurePhase::Load))?;
                }
                player
                    .load(&url)
                    .map_err(|error| startup_error(error, FailurePhase::Load))?;
            }
            if canceled.load(std::sync::atomic::Ordering::Acquire) {
                return Ok(None);
            }
            Ok(Some(owner))
        });
        self._initialize = cx.spawn(async move |view, cx| {
            let result = initialize.await;
            let _ = view.update(cx, |view, cx| {
                // A late owner is dropped through BackgroundPlayer, never joined
                // on the UI executor and never installed after cancellation.
                if view
                    .startup_cancel
                    .load(std::sync::atomic::Ordering::Acquire)
                {
                    return;
                }
                match result {
                    Ok(player) => {
                        view.player = player;
                        if let Some(p) = &view.progress
                            && p.phase == ProgressPhase::Active
                            && !p.authorized
                        {
                            let play = p.play;
                            view.presentation_command(|player| player.set_paused(!play), cx);
                        }
                    }
                    Err((error, reason, phase)) => {
                        view.failure_phase = Some(phase);
                        view.failure_reason = Some(reason);
                        view.error = Some(error);
                    }
                }
                cx.notify();
            });
        });
    }

    fn embedded_video(&self) -> AnyElement {
        let element = div().absolute().size_full();
        #[cfg(feature = "ui-probe")]
        let element = element.when_some(self.fixture_image.clone(), |element, image| {
            element.child(img(image).size_full().object_fit(ObjectFit::Contain))
        });
        #[cfg(feature = "gpu-surface")]
        let element = element.when_some(self.gpu_frame.clone(), |element, frame| {
            element.child(
                gpu_surface(frame)
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
        });
        element.into_any_element()
    }

    fn deliver_checkpoint(&self, mut sample: PlayerCheckpoint, cx: &mut App) {
        if let (Some((session, generation)), Some(owner)) = (&self.lease, &self.progress_owner) {
            sample.generation = *generation;
            let owner = owner.clone();
            let session = session.clone();
            cx.defer(move |cx| {
                owner.update(cx, |owner, cx| {
                    owner.submit_player_checkpoint(session, sample, cx)
                })
            });
        }
    }
    fn flush_progress(&mut self, cx: &mut App) {
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) {
            self.state = player.state();
        }
        if let Some(sample) = self
            .progress
            .as_mut()
            .and_then(|p| p.sample(&self.state, Instant::now(), true))
        {
            self.deliver_checkpoint(sample, cx);
        }
    }
    fn sample_progress(&mut self, cx: &mut Context<Self>) {
        let Some(mut p) = self.progress.take() else {
            return;
        };
        let previous = p.phase;
        if p.observe_readiness(&self.state) {
            match p.phase {
                ProgressPhase::Loading if p.initial_position_ready(&self.state) => {
                    if p.launch.watched
                        && p.launch.intent == PlaybackIntent::Normal
                        && p.usable_checkpoint().is_some()
                    {
                        p.phase = ProgressPhase::Choice;
                    } else if p.launch.intent == PlaybackIntent::StartOver {
                        self.begin_start_over(&mut p);
                    } else if let Some(saved) = p.usable_checkpoint() {
                        if !p.duration_accepted && duration_mismatch(saved, self.state.duration) {
                            p.phase = ProgressPhase::Mismatch;
                        } else {
                            self.begin_resume(&mut p);
                        }
                    } else {
                        p.phase = ProgressPhase::Ready;
                    }
                }
                ProgressPhase::AwaitSeekability => {
                    if !p.duration_accepted
                        && p.usable_checkpoint()
                            .is_some_and(|saved| duration_mismatch(saved, self.state.duration))
                    {
                        p.phase = ProgressPhase::Mismatch;
                    } else {
                        self.begin_resume(&mut p);
                    }
                }
                _ => {}
            }
        }
        let timeout_phase = match p.phase {
            ProgressPhase::Loading => "loading",
            ProgressPhase::AwaitSeekability => "await_seekability",
            ProgressPhase::Seeking { .. } => "seeking",
            ProgressPhase::Starting { .. } => "starting",
            _ => "other",
        };
        if p.check_timeout(Instant::now()) {
            // Fixed fields only: never log source URLs, content, or positions.
            tracing::warn!(
                reason = "playback_readiness_timeout",
                phase = timeout_phase,
                has_checkpoint = p.usable_checkpoint().is_some(),
                file_loaded = self.state.file_loaded,
                loading = self.state.loading,
                position_observed = self.state.position_observed,
                seeking = self.state.seeking,
                seekable = ?self.state.seekable,
                generation_matches = p.generation == Some(self.state.media_generation),
                stopped = self.state.stopped,
                media_stopped = self.state.media_stopped,
                has_error = self.state.error.is_some() || self.state.failure_reason.is_some(),
                "Playback progress readiness timed out"
            );
        }
        if previous != p.phase && matches!(p.phase, ProgressPhase::Ready | ProgressPhase::Reset) {
            self.send_progress_ready(&p, cx);
        }
        if let Some(sample) = p.sample(&self.state, Instant::now(), false) {
            self.deliver_checkpoint(sample, cx);
        }
        if previous != p.phase {
            cx.notify();
        }
        self.progress = Some(p);
    }
    fn send_progress_ready(&self, p: &Progress, cx: &mut Context<Self>) {
        if let (Some((session, generation)), Some(owner)) = (&self.lease, &self.progress_owner) {
            let (session, generation, epoch, start_over, owner) = (
                session.clone(),
                *generation,
                p.launch.epoch,
                p.phase == ProgressPhase::Reset,
                owner.clone(),
            );
            cx.defer(move |cx| {
                owner.update(cx, |owner, cx| {
                    owner.player_progress_ready(session, generation, epoch, start_over, cx)
                })
            });
        }
    }
    fn begin_resume(&mut self, p: &mut Progress) {
        let target = p
            .usable_checkpoint()
            .map_or(0.0, |v| v.position_ms() as f64 / 1000.0);
        if self.state.seekable == Some(false)
            || self
                .state
                .duration
                .is_some_and(|duration| target >= duration)
        {
            p.phase = ProgressPhase::Unavailable;
        } else if self.state.seekable == Some(true) {
            let result = self
                .player
                .as_ref()
                .and_then(|owner| owner.player.as_ref())
                .map(|player| player.seek_absolute(target));
            p.phase = if matches!(result, Some(Ok(()))) {
                p.seek_started = Some(Instant::now());
                ProgressPhase::Seeking {
                    target,
                    position_revision: self.state.position_revision,
                    restart_revision: self.state.restart_revision,
                }
            } else {
                ProgressPhase::Unavailable
            };
        } else {
            p.await_seekability(Instant::now());
        }
    }
    fn retry_startup(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.progress {
            p.retry_startup(Instant::now());
        }
        cx.notify();
    }
    fn retry_progress_ready(&mut self, cx: &mut Context<Self>) {
        let Some(mut p) = self.progress.take() else {
            return;
        };
        p.notice = false;
        self.send_progress_ready(&p, cx);
        self.progress = Some(p);
        cx.notify();
    }
    fn play_without_saving(&mut self, cx: &mut Context<Self>) {
        let Some(p) = &mut self.progress else {
            return;
        };
        p.authorized = false;
        p.phase = ProgressPhase::Active;
        p.play = true;
        let play = p.play;
        self.presentation_command(|player| player.set_paused(!play), cx);
        cx.notify();
    }
    fn begin_start_over(&mut self, p: &mut Progress) {
        p.launch.intent = PlaybackIntent::StartOver;
        p.authorized = false;
        if !p.can_start_over(&self.state) {
            p.phase = ProgressPhase::Unavailable;
        } else if self.state.position > 1.0 {
            let result = self
                .player
                .as_ref()
                .and_then(|owner| owner.player.as_ref())
                .map(|player| player.seek_absolute(0.0));
            p.phase = if matches!(result, Some(Ok(()))) {
                p.seek_started = Some(Instant::now());
                ProgressPhase::Starting {
                    position_revision: self.state.position_revision,
                    restart_revision: self.state.restart_revision,
                }
            } else {
                ProgressPhase::Unavailable
            };
        } else {
            p.phase = ProgressPhase::Reset;
        }
    }
    fn choose_progress(&mut self, resume: bool, cx: &mut Context<Self>) {
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) {
            self.state = player.state();
        }
        let Some(mut p) = self.progress.take() else {
            return;
        };
        if resume {
            p.duration_accepted |= p.phase == ProgressPhase::Mismatch;
            p.launch.intent = PlaybackIntent::Resume;
            self.begin_resume(&mut p);
        } else {
            self.begin_start_over(&mut p);
        }
        if p.phase == ProgressPhase::Reset {
            self.send_progress_ready(&p, cx);
        }
        self.progress = Some(p);
        cx.notify();
    }

    fn sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sample_started = self.diagnostics.as_ref().map(|_| Instant::now());
        let mut state_changed = false;
        #[cfg(feature = "gpu-surface")]
        let mut frame_received = false;
        #[cfg(not(feature = "gpu-surface"))]
        let frame_received = false;
        #[cfg(feature = "gpu-surface")]
        if terminal_failure(self.error.is_some(), &self.state) {
            frame_received |= self.gpu_frame.take().is_some();
        }
        if terminal_failure(self.error.is_some(), &self.state) {
            self.flush_progress(cx);
        }
        if terminal_failure(self.error.is_some(), &self.state)
            && let Some((session, generation)) = self.lease.take()
        {
            // Fatal startup/runtime errors release the download/serve lease once,
            // while keeping actionable error chrome visible for the user.
            release_window(&session, generation, true, cx);
            self.player.take();
        }
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) {
            let previous = self.state.clone();
            self.state = player.state();
            #[cfg(feature = "gpu-surface")]
            if self.mode == PlayerChoice::Embedded
                && !terminal_failure(self.error.is_some(), &self.state)
                && let Some(frame) = player.take_gpu_frame()
            {
                match video_surface(frame) {
                    Ok(frame) => {
                        self.gpu_frame = Some(frame);
                        self.frames_presented += 1;
                        frame_received = true;
                    }
                    Err(error) => {
                        self.failure_reason = Some(PlaybackFailure::Unknown);
                        self.failure_phase = Some(FailurePhase::Surface);
                        self.error = Some(error);
                        state_changed = true;
                    }
                }
            }
            state_changed |= self.state != previous;
        }
        // A fresh state snapshot or surface-validation error can become fatal
        // within this sample. Probes have no lease but must also release frames.
        #[cfg(feature = "gpu-surface")]
        if terminal_failure(self.error.is_some(), &self.state) {
            frame_received |= self.gpu_frame.take().is_some();
        }
        if self.mode == PlayerChoice::Embedded
            && failure_transition(
                terminal_failure(self.error.is_some(), &self.state),
                &mut self.failure_reported,
            )
        {
            let reason = failure_reason(&self.state, self.failure_reason);
            tracing::warn!(
                mode = "embedded-mpv",
                phase = failure_phase(&self.state, self.failure_phase).code(),
                reason = reason.code(),
                "Embedded GPU playback unavailable"
            );
        }
        self.sample_progress(cx);
        self.sync_subtitle_position(window, cx);
        let now = Instant::now();
        // Bounded in-memory read (~1 Hz, by reference, no session clones). The
        // dock repaints on a change even when playback state/frames are static,
        // e.g. during a cache stall; hidden chrome redraws on reveal instead.
        let transfer_changed = if transfer_poll_due(self.last_transfer_poll, now) {
            self.last_transfer_poll = Some(now);
            self.observe_transfer(now, cx)
        } else {
            false
        };
        if scrub_settled(self.scrub, self.state.position, now)
            || (self.scrub.is_some() && !self.can_seek())
        {
            self.scrub = None;
            state_changed = true;
        }
        if self
            .volume_requested
            .is_some_and(|requested| volume_settled(Some(requested), self.state.volume, now))
        {
            self.volume_requested = None;
            state_changed = true;
        }
        for (requested, actual) in [
            (&mut self.seek_requested, Some(self.state.position)),
            (&mut self.speed_requested, Some(self.state.speed)),
            (&mut self.delay_requested, self.state.subtitle_delay),
        ] {
            if requested.is_some_and(|(target, since)| {
                since.elapsed() >= Duration::from_millis(1200)
                    || actual.is_some_and(|value| (value - target).abs() < 0.01)
            }) {
                *requested = None;
            }
        }
        // No animations: reduced motion and normal motion both settle instantly.
        // GPUI's is_visible is application hide/show, not a reliable cross-platform
        // minimized/occluded signal; do not pause or change lease state from it.
        if redraw_needed(
            state_changed || (transfer_changed && self.chrome_visible(window)),
            frame_received,
            self.chrome_was_visible,
            self.chrome_visible(window),
        ) {
            cx.notify();
        }
        if let Some(meter) = self.cadence.as_mut() {
            let activity = activity_of(&self.state, self.player.is_some());
            meter.sample(Instant::now(), activity, frame_received);
            if let Some(report) = meter.finish_if_due(Instant::now()) {
                CadenceMeter::log(&report);
                let text = report.label();
                if self.cadence_text.as_deref() != Some(text.as_str()) {
                    self.cadence_text = Some(text);
                    cx.notify();
                }
            }
        }
        if let (Some(metrics), Some(started)) = (&mut self.diagnostics, sample_started) {
            let now = Instant::now();
            metrics.max_sample_gap_ms = metrics
                .max_sample_gap_ms
                .max(started.duration_since(metrics.last_sample).as_secs_f64() * 1000.);
            metrics.last_sample = started;
            metrics.samples += 1;
            metrics.received += u64::from(frame_received);
            metrics.presented += u64::from(frame_received);
            metrics.paused_for_cache_samples += u64::from(self.state.paused_for_cache);
            metrics.sample_ms += now.duration_since(started).as_secs_f64() * 1000.;
            if now.duration_since(metrics.last_report) >= Duration::from_secs(5)
                && let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref())
            {
                metrics.report(now, &player.diagnostics(), &self.state);
            }
        }
    }
    fn activity(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        cx.notify();
    }
    fn command(
        &mut self,
        command: impl FnOnce(&Player) -> kinema_mpv::Result<()>,
        cx: &mut Context<Self>,
    ) {
        self.request_command(command, cx);
    }
    fn request_command(
        &mut self,
        command: impl FnOnce(&Player) -> kinema_mpv::Result<()>,
        cx: &mut Context<Self>,
    ) -> bool {
        self.activity(cx);
        let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) else {
            self.unavailable(cx);
            return false;
        };
        match command(player) {
            Ok(()) => {
                self.control_notice = false;
                true
            }
            Err(error) => {
                if matches!(error, kinema_mpv::Error::Disconnected) {
                    self.failure_reason = Some(error.playback_failure());
                    self.failure_phase = Some(FailurePhase::Command);
                    self.error = Some(error.to_string());
                } else {
                    self.control_notice = true;
                }
                false
            }
        }
    }
    fn desktop_transport(&mut self, action: Transport, cx: &mut Context<Self>) -> bool {
        if !self.backend_available() {
            return false;
        }
        // Commands use a fresh observed snapshot, never toolbar scrub targets.
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref()) {
            self.state = player.state();
        }
        let pending = self
            .progress
            .as_ref()
            .is_some_and(|p| p.phase != ProgressPhase::Active);
        match action {
            Transport::Play | Transport::Pause => {
                let play = action == Transport::Play;
                if let Some(p) = &mut self.progress {
                    p.play = play;
                }
                if !pending {
                    if play && replay_needed(&self.state) {
                        self.toggle_playback(cx);
                    } else {
                        self.presentation_command(|player| player.set_paused(!play), cx);
                    }
                }
            }
            Transport::Toggle => self.toggle_playback(cx),
            Transport::Stop => {
                self.flush_progress(cx);
                if let Some(p) = &mut self.progress {
                    p.play = false;
                }
                self.presentation_command(Player::stop, cx);
            }
            Transport::SeekRelative { forward, amount } => {
                if pending || !desktop_can_seek(&self.state) {
                    return false;
                }
                let delta = amount.as_secs_f64() * if forward { 1.0 } else { -1.0 };
                let target = bounded_seek(self.state.position + delta, self.state.duration);
                self.presentation_command(|player| player.seek_absolute(target), cx);
            }
            Transport::SeekAbsolute(position) => {
                if pending || !desktop_can_seek(&self.state) {
                    return false;
                }
                let target = bounded_seek(position.as_secs_f64(), self.state.duration);
                self.presentation_command(|player| player.seek_absolute(target), cx);
            }
            Transport::SetVolume(volume) => {
                let Some(volume) = normalized_volume(volume) else {
                    return false;
                };
                self.presentation_command(|player| player.set_volume(volume * 100.0), cx);
            }
            Transport::ChangeVolume(delta) => {
                if !delta.is_finite() {
                    return false;
                }
                let Some(volume) = normalized_volume(self.state.volume / 100.0 + delta) else {
                    return false;
                };
                self.presentation_command(|player| player.set_volume(volume * 100.0), cx);
            }
            Transport::ToggleMute => self.presentation_command(Player::toggle_mute, cx),
        }
        cx.notify();
        true
    }
    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.progress
            && p.phase != ProgressPhase::Active
        {
            p.play = !p.play;
            cx.notify();
            return;
        }
        if replay_needed(&self.state)
            && let Some(url) = self.source_url.clone()
        {
            self.flush_progress(cx);
            let tracked = self.progress.is_some();
            if let Some(p) = &mut self.progress {
                p.authorized = false;
                p.generation = None;
                p.retry_startup(Instant::now());
                p.launch.intent = PlaybackIntent::StartOver;
                p.play = true;
            }
            // A new load: discard the previous GPU generation before issuing
            // load. The backend rejects late frames from the obsolete source.
            #[cfg(feature = "gpu-surface")]
            {
                self.gpu_frame.take();
            }
            // Do not assume the previous sub-pos survives.
            self.subtitle_position = None;
            self.delay_requested = None;
            self.speed_requested = None;
            self.seek_requested = None;
            self.scrub = None;
            self.command(
                |player| {
                    player.set_paused(tracked)?;
                    player.load(&url)
                },
                cx,
            );
        } else {
            self.command(Player::toggle_pause, cx);
        }
    }
    fn seek(&mut self, seconds: f64, cx: &mut Context<Self>) {
        if self.can_seek() {
            let position = self
                .seek_requested
                .filter(|(_, since)| since.elapsed() < Duration::from_millis(1200))
                .map_or(self.state.position, |(target, _)| target);
            let target = bounded_seek(position + seconds, self.state.duration);
            self.seek_requested = Some((target, Instant::now()));
            self.scrub = Some(Scrub::Pending {
                target,
                since: Instant::now(),
            });
            self.command(|player| player.seek_absolute(target), cx);
        } else {
            self.unavailable(cx);
        }
    }
    /// Backend command for automatic presentation: unlike `command`, it never
    /// counts as user activity (that would re-show chrome just hidden) and only
    /// surfaces a notice on a real error.
    fn presentation_command(
        &mut self,
        command: impl FnOnce(&Player) -> kinema_mpv::Result<()>,
        cx: &mut Context<Self>,
    ) {
        if let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref())
            && let Err(error) = command(player)
        {
            if matches!(error, kinema_mpv::Error::Disconnected) {
                self.failure_reason = Some(error.playback_failure());
                self.failure_phase = Some(FailurePhase::Command);
                self.error = Some(error.to_string());
            } else {
                self.control_notice = true;
            }
            cx.notify();
        }
    }
    /// Keeps subtitles above the dock while chrome is visible and restores the
    /// default when hidden. Sends only when the target changes (visibility,
    /// viewport or dock height), never per frame, and again after a new load.
    fn sync_subtitle_position(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.backend_available() {
            return;
        }
        let dock = if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let desired = subtitle_position(
            self.chrome_visible(window),
            // Controls, bottom inset and the fade above them, so subtitles are not dimmed.
            f32::from(dock + DOCK_BOTTOM_INSET + DOCK_FADE + s::S8),
            f32::from(window.viewport_size().height),
        );
        if self.subtitle_position != Some(desired) {
            // Recorded even on failure so an unsupported backend is not retried
            // every tick; the failure itself is reported once.
            self.subtitle_position = Some(desired);
            self.presentation_command(|p| p.set_subtitle_position(desired), cx);
        }
    }
    fn seek_to(&mut self, target: f64, cx: &mut Context<Self>) {
        if self.can_seek() {
            self.seek_requested = Some((target, Instant::now()));
            self.command(|player| player.seek_absolute(target), cx);
        }
    }
    fn menu_open(&self) -> bool {
        self.menu.is_some() || self.help_focus.is_some()
    }
    fn availability(&self) -> Availability {
        let has = |kind| self.state.tracks.iter().any(|track| track.kind == kind);
        Availability {
            backend: self.backend_available(),
            seekable: self.can_seek(),
            chapters: !self.state.chapters.is_empty(),
            subtitles: has(TrackKind::Subtitle),
            audio: has(TrackKind::Audio),
            chapters_known: self.state.chapters_available,
            tracks_known: self.state.tracks_available,
        }
    }
    fn timeline_ratio(&self, x: Pixels) -> f64 {
        f64::from(ratio_in_span(
            f32::from(x),
            f32::from(self.timeline_bounds.left()),
            f32::from(self.timeline_bounds.size.width),
        ))
    }
    fn begin_scrub(&mut self, x: Pixels, window: &mut Window, cx: &mut Context<Self>) {
        if self.can_seek() {
            self.volume_dragging = false;
            self.controls[TIMELINE].focus(window, cx);
            self.scrub = Some(Scrub::Dragging(self.timeline_ratio(x)));
            self.activity(cx);
        }
    }
    fn set_volume_at(&mut self, y: Pixels, cx: &mut Context<Self>) {
        let volume = volume_at(
            f32::from(y),
            f32::from(self.volume_bounds.top()),
            f32::from(self.volume_bounds.size.height),
        );
        self.request_volume(volume, cx);
    }
    fn request_volume(&mut self, volume: f64, cx: &mut Context<Self>) {
        let shown = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        if (shown - volume).abs() >= 0.5 || self.volume_requested.is_none() {
            self.volume_requested = Some((volume, Instant::now()));
            self.command(|player| player.set_volume(volume), cx);
        } else {
            self.activity(cx);
        }
    }
    fn nudge_volume(&mut self, delta: f64, cx: &mut Context<Self>) {
        if self.menu_open() || !self.backend_available() {
            return;
        }
        let shown = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        self.request_volume(step_volume(shown, delta), cx);
    }
    /// Pointer moved anywhere in the window: drags continue outside their widgets,
    /// and a missed button-up (released elsewhere) commits instead of sticking.
    fn pointer_moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
        if self.scrub.is_some_and(|s| matches!(s, Scrub::Dragging(_))) || self.volume_dragging {
            if event.pressed_button != Some(MouseButton::Left) {
                self.finish_drag(cx);
                return;
            }
            if matches!(self.scrub, Some(Scrub::Dragging(_))) {
                self.scrub = Some(Scrub::Dragging(self.timeline_ratio(event.position.x)));
            }
            if self.volume_dragging {
                self.set_volume_at(event.position.y, cx);
            }
        }
        self.activity(cx);
    }
    /// Button released anywhere (including outside the window): commit.
    fn finish_drag(&mut self, cx: &mut Context<Self>) {
        self.volume_dragging = false;
        if let Some(Scrub::Dragging(ratio)) = self.scrub {
            let target = ratio * self.state.duration.unwrap_or(0.);
            self.scrub = Some(Scrub::Pending {
                target,
                since: Instant::now(),
            });
            self.seek_to(target, cx);
        }
        cx.notify();
    }
    /// Escape cancels an active drag without seeking; returns whether it did.
    fn cancel_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let active = matches!(self.scrub, Some(Scrub::Dragging(_))) || self.volume_dragging;
        if active {
            self.scrub = None;
            self.volume_dragging = false;
            cx.notify();
        }
        active
    }
    fn toggle_menu(
        &mut self,
        kind: MenuKind,
        pointer: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let same = self.menu.as_ref().is_some_and(|open| open.kind == kind);
        if let Some(open) = self.menu.take() {
            open.menu.update(cx, |menu, cx| menu.dismiss(window, cx));
        }
        if same {
            return;
        }
        let items = self.menu_items(kind, cx);
        if items.is_empty() {
            return;
        }
        let menu = cx.new(|cx| Menu::new(items, window, cx).opened_by_pointer(pointer));
        let dismissed = cx.subscribe(&menu, |this, _, _: &DismissEvent, cx| {
            this.menu = None;
            cx.notify();
        });
        self.menu = Some(OpenMenu {
            kind,
            menu,
            _dismissed: dismissed,
        });
        self.activity(cx);
    }
    fn menu_items(&self, kind: MenuKind, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let view = cx.weak_entity();
        let action = |label: String, shortcut: Option<String>, checked: bool, run: MenuRun| {
            let view = view.clone();
            MenuItem::Action {
                label: label.into(),
                shortcut: shortcut.map(Into::into),
                icon: None,
                disabled_reason: None,
                checked,
                handler: Rc::new(move |_, cx| {
                    let run = run.clone();
                    let _ = view.update(cx, |view, cx| run(view, cx));
                }),
            }
        };
        let position = self.state.position;
        match kind {
            MenuKind::Chapters => {
                let current = chapter_index(&self.state.chapters, position);
                self.state
                    .chapters
                    .iter()
                    .enumerate()
                    .map(|(i, chapter)| {
                        let time = chapter.time;
                        action(
                            chapter_label(i, chapter),
                            Some(format_clock(time)),
                            current == Some(i),
                            Rc::new(move |view, cx| view.seek_to(time, cx)),
                        )
                    })
                    .collect()
            }
            MenuKind::Subtitles | MenuKind::Audio => {
                let subtitles = kind == MenuKind::Subtitles;
                let wanted = if subtitles {
                    TrackKind::Subtitle
                } else {
                    TrackKind::Audio
                };
                let tracks = self.state.tracks.iter().filter(|t| t.kind == wanted);
                let mut items = Vec::new();
                if subtitles {
                    let none_selected = !self
                        .state
                        .tracks
                        .iter()
                        .any(|t| t.kind == wanted && t.selected);
                    items.push(action(
                        "Off".into(),
                        None,
                        none_selected,
                        Rc::new(|view, cx| view.command(|p| p.select_subtitle(None), cx)),
                    ));
                }
                for track in tracks {
                    let id = track.id;
                    items.push(action(
                        track_label(track),
                        None,
                        track.selected,
                        Rc::new(move |view, cx| {
                            if subtitles {
                                view.command(|p| p.select_subtitle(Some(id)), cx)
                            } else {
                                view.command(|p| p.select_audio(Some(id)), cx)
                            }
                        }),
                    ));
                }
                items
            }
            MenuKind::Speed => SPEEDS
                .iter()
                .map(|&speed| {
                    action(
                        speed_label(speed),
                        None,
                        (self.state.speed - speed).abs() < 0.01,
                        Rc::new(move |view, cx| view.command(|p| p.set_speed(speed), cx)),
                    )
                })
                .collect(),
        }
    }
    fn focus_control(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut order = vec![self.surface_focus.clone()];
        order.extend(
            self.controls
                .iter()
                .enumerate()
                .filter(|(i, _)| control_enabled(*i, self.availability()))
                .map(|(_, focus)| focus.clone()),
        );
        let current = order
            .iter()
            .position(|focus| focus.is_focused(window))
            .unwrap_or(0);
        let next = if reverse {
            (current + order.len() - 1) % order.len()
        } else {
            (current + 1) % order.len()
        };
        order[next].focus(window, cx);
        self.activity(cx);
    }
    fn backend_available(&self) -> bool {
        self.player.is_some() && !terminal_failure(self.error.is_some(), &self.state)
    }
    fn can_seek(&self) -> bool {
        self.backend_available() && desktop_can_seek(&self.state)
    }
    fn chrome_visible(&self, window: &Window) -> bool {
        controls_visible(
            self.state.paused || self.state.ended,
            self.state.loading || self.state.paused_for_cache || self.player.is_none(),
            self.error.is_some()
                || self.control_notice
                || self.state.error.is_some()
                || self.progress.as_ref().is_some_and(|p| p.notice),
            self.dock_hover
                || self.scrub.is_some()
                || self.volume_dragging
                || self.menu_open()
                || !self.surface_focus.is_focused(window),
            self.last_activity.elapsed(),
        )
    }
    fn control(
        &self,
        index: usize,
        id: &'static str,
        icon: IconName,
        label: impl Into<SharedString>,
    ) -> Button {
        let mut button = Button::new(id, "")
            .variant(ButtonVariant::Ghost)
            .icon(icon)
            .over_art(true)
            .focus_handle(self.controls[index].clone());
        button.square = true;
        let availability = self.availability();
        button.tooltip =
            Some(disabled_reason(index, availability).map_or_else(|| label.into(), Into::into));
        button.disabled(!control_enabled(index, availability))
    }
    fn text_control(
        &self,
        index: usize,
        id: &'static str,
        text: impl Into<SharedString>,
        label: &'static str,
        active: bool,
    ) -> Button {
        let mut button = Button::new(id, text)
            .variant(ButtonVariant::Ghost)
            .over_art(true)
            .focus_handle(self.controls[index].clone());
        let availability = self.availability();
        button.tooltip =
            Some(disabled_reason(index, availability).map_or_else(|| label.into(), Into::into));
        button.toggled = active;
        button.disabled(!control_enabled(index, availability))
    }
    fn volume_panel(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = Palette::dark();
        let viewport = window.viewport_size().height;
        let rail = px(volume_rail_height(f32::from(viewport)));
        let dock = if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let panel_top = px(dock::centered_top(
            f32::from(TOP_BAR_HEIGHT + s::S8),
            f32::from(viewport - dock - s::S8),
            f32::from(rail + VOLUME_PANEL_EXTRA),
        ));
        let entity = cx.entity();
        let enabled = self.backend_available();
        let volume = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        let fraction = volume_fraction(volume);
        let unity = volume_fraction(100.);
        let amplified = volume > 100.5 && !self.state.muted;
        let fill = if self.state.muted {
            p.text_muted
        } else {
            p.accent
        };
        let slider = div()
            .id("player-volume").occlude()
            .aria_label(format!(
                "Volume, {volume:.0} percent{}. Up and down arrows change volume by {VOLUME_STEP:.0}.",
                if self.state.muted { ", muted" } else { "" }
            ))
            .key_context("KinemaPlayerVolume")
            .track_focus(&self.controls[VOLUME])
            .tab_stop(enabled)
            .relative()
            .w(s::S32)
            .h(rail)
            .flex()
            .justify_center()
            .rounded(radius::SM)
            .cursor(if enabled {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .focus_visible(|style| style.border(s::FOCUS_RING).border_color(p.focus_ring))
            .child(
                div()
                    .relative()
                    .h_full()
                    .w(VOLUME_TRACK)
                    .rounded_full()
                    .bg(p.line_control.opacity(0.5))
                    .child(
                        div()
                            .absolute()
                            .bottom_0()
                            .w_full()
                            .h(relative(fraction))
                            .rounded_full()
                            .bg(fill),
                    )
                    // 100% detent: above it mpv amplifies (softvol).
                    .child(
                        div()
                            .absolute()
                            .top(relative(1. - unity))
                            .left(-s::S4)
                            .w(VOLUME_TRACK + s::S8)
                            .h(s::S2)
                            .mt(-s::S2 / 2.)
                            .rounded_full()
                            .bg(p.text_primary.opacity(0.6)),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(relative(1. - fraction))
                            .mt(-VOLUME_THUMB / 2.)
                            .ml((VOLUME_TRACK - VOLUME_THUMB) / 2.)
                            .size(VOLUME_THUMB)
                            .rounded_full()
                            .bg(p.text_primary)
                            .shadow_sm(),
                    ),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |view, _| view.volume_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(enabled, |slider| {
                slider.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|view, event: &MouseDownEvent, window, cx| {
                        view.controls[VOLUME].focus(window, cx);
                        view.scrub = None;
                        view.volume_dragging = true;
                        view.set_volume_at(event.position.y, cx);
                    }),
                )
            });
        div()
            .id("player-volume-panel")
            .occlude()
            .absolute()
            .right(s::S24)
            // Centred in the band between the title row and the measured dock.
            .top(panel_top)
            .flex()
            .flex_col()
            .items_center()
            .gap(s::S8)
            .px(s::S6)
            .pt(s::S12)
            .pb(s::S6)
            .rounded(radius::LG)
            .bg(p.scrim_max)
            .border_1()
            .border_color(p.line_subtle)
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                view.dock_hover = *hovered;
                view.activity(cx);
            }))
            .child(
                div()
                    .min_w(s::S32)
                    .whitespace_nowrap()
                    .text_center()
                    .type_style(t::SMALL_MEDIUM)
                    .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
                    .text_color(if amplified {
                        p.accent_text
                    } else {
                        p.text_secondary
                    })
                    .child(format!("{volume:.0}%")),
            )
            .child(slider)
            .child(
                self.control(
                    MUTE,
                    "player-mute",
                    if self.state.muted {
                        IconName::VolumeMuted
                    } else {
                        IconName::Volume
                    },
                    shortcuts::tooltip(Sid::ToggleMute),
                )
                .on_click(cx.listener(|view, _, _, cx| view.command(Player::toggle_mute, cx))),
            )
    }
    fn timeline(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = Palette::dark();
        let entity = cx.entity();
        let seekable = self.can_seek();
        let duration = self.state.duration.filter(|d| *d > 0.).unwrap_or(0.);
        let shown = displayed_position(self.scrub, self.state.position, duration, Instant::now());
        let fraction = if seekable {
            (shown / duration).clamp(0., 1.) as f32
        } else {
            0.
        };
        let dragging = matches!(self.scrub, Some(Scrub::Dragging(_)));
        let preview = if !seekable {
            None
        } else if dragging {
            Some(fraction)
        } else {
            self.hover_ratio.or_else(|| {
                (self.controls[TIMELINE].is_focused(window) && window.last_input_was_keyboard())
                    .then_some(fraction)
            })
        };
        let width = f32::from(self.timeline_bounds.size.width);
        let mut track = div()
            .relative()
            .w_full()
            .h(px(12.))
            .rounded_full()
            .overflow_hidden()
            .bg(p.line_control.opacity(0.5));
        if seekable {
            for (start, end) in buffered_fractions(self.state.buffered_ranges.as_deref(), duration)
            {
                track = track.child(
                    div()
                        .absolute()
                        .h_full()
                        .left(relative(start))
                        .w(relative(end - start))
                        .bg(p.text_secondary.opacity(0.6)),
                );
            }
        }
        track = track.child(
            div()
                .absolute()
                .h_full()
                .w(relative(fraction))
                .rounded_full()
                .bg(p.accent),
        );
        for tick in chapter_ticks(&self.state.chapters, duration) {
            track = track.child(
                div()
                    .absolute()
                    .h_full()
                    .left(relative(tick))
                    .w(s::S2)
                    .bg(p.scrim_max),
            );
        }
        let active = dragging || preview.is_some();
        div()
            .id("player-timeline")
            .aria_label(if seekable {
                format!(
                    "Playback position, {} of {}. {}. Left and right arrows seek 10 seconds.",
                    format_clock(shown),
                    format_clock(duration),
                    self.buffer_summary()
                )
            } else {
                "Playback position, seeking unavailable".to_owned()
            })
            .relative()
            .key_context("KinemaPlayerTimeline")
            .track_focus(&self.controls[TIMELINE])
            .tab_stop(seekable)
            .w_full()
            .h(s::S32)
            .flex()
            .items_center()
            .rounded(radius::SM)
            .cursor(if seekable {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .focus_visible(|style| style.border(s::FOCUS_RING).border_color(p.focus_ring))
            .child(track)
            .when(seekable, |el| {
                let size = if active { px(18.) } else { px(14.) };
                el.child(
                    div()
                        .absolute()
                        .left(relative(fraction))
                        .ml(-size / 2.)
                        .size(size)
                        .rounded_full()
                        .bg(p.text_primary)
                        .border_2()
                        .border_color(p.accent),
                )
            })
            .when_some(preview.filter(|_| duration > 0.), |el, ratio| {
                // Honest scrub preview: time and chapter only. Decoded thumbnails
                // need a separate extraction/cache pipeline and are not faked.
                let time = f64::from(ratio) * duration;
                let chapter = chapter_index(&self.state.chapters, time)
                    .map(|i| chapter_label(i, &self.state.chapters[i]));
                let bubble = 132.;
                el.child(
                    div()
                        .absolute()
                        // Clear the 32 px timeline and the time/chapter row above it.
                        .bottom(s::S32 + s::S24 + s::S4)
                        .left(px(dock::bubble_left(ratio, width, bubble)))
                        .w(px(bubble))
                        .py(s::S4)
                        .flex()
                        .flex_col()
                        .items_center()
                        .rounded(radius::SM)
                        .bg(p.scrim_max)
                        .border_1()
                        .border_color(p.line_subtle)
                        .child(
                            div()
                                .type_style(t::MONO)
                                .text_color(p.text_primary)
                                .child(format_clock(time)),
                        )
                        .when_some(chapter, |bubble, chapter| {
                            bubble.child(
                                div()
                                    .max_w_full()
                                    .px(s::S6)
                                    .truncate()
                                    .type_style(t::CAPTION)
                                    .text_color(p.text_secondary)
                                    .child(chapter),
                            )
                        }),
                )
            })
            .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                if !*hovered && view.hover_ratio.take().is_some() {
                    cx.notify();
                }
            }))
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| {
                if view.can_seek() {
                    view.hover_ratio = Some(view.timeline_ratio(event.position.x) as f32);
                }
                view.activity(cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.begin_scrub(event.position.x, window, cx)
                }),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        entity.update(cx, |view, _| view.timeline_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
    }
    /// Buffer-ahead (MPV playable cache) plus, for leased remote sessions,
    /// separately labelled downloaded bytes and recent speed.
    fn buffer_summary(&self) -> String {
        if self.state.loading || self.state.ended {
            return String::new();
        }
        let mut parts = vec![buffer_ahead_label(&self.state)];
        parts.extend(download_label(&self.transfer));
        parts.join(" · ")
    }
    /// Cheap in-memory coordinator read; never touches MediaService or disk.
    /// Returns whether the displayed download figures changed. Views without a
    /// remote lease (local/probe) keep no download state, never a fake 0 B/s.
    fn observe_transfer(&mut self, now: Instant, cx: &App) -> bool {
        let before = self.transfer.display_key();
        let found = self.lease.as_ref().and_then(|(id, _)| {
            let coordinator = &cx
                .try_global::<PlayerWindows>()?
                .entries
                .get(id)?
                .coordinator;
            let snapshot = coordinator.read(cx).sessions.iter().find(|snapshot| {
                snapshot.session.id == *id && snapshot.origin == crate::media::SessionOrigin::Remote
            })?;
            Some((
                id.clone(),
                snapshot
                    .progress
                    .as_ref()
                    .map(|p| (p.downloaded_bytes, p.total_bytes)),
            ))
        });
        match found {
            Some((id, progress)) => self.transfer.observe(&id, progress, now),
            None => self.transfer.clear(),
        }
        self.transfer.display_key() != before
    }
    /// Measure with the bundled typography, not a character-count breakpoint.
    /// Remove the optional episode name before applying normal end truncation.
    fn chrome_title(&self, window: &Window) -> String {
        let text_width = |text: &str, style: crate::ui::theme::TypeStyle| {
            let mut font = font(t::UI_FAMILY);
            font.weight = style.weight;
            let run = TextRun {
                len: text.len(),
                font,
                color: Rgba::default().into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            window
                .text_system()
                .shape_line(text.to_owned().into(), style.size, &[run], None)
                .width
        };
        let available = window.viewport_size().width - DOCK_SIDE_INSET * 2. - s::S12 - s::S32;
        title_for_width(
            &self.title,
            text_width(&self.title.full, t::BODY_STRONG),
            available,
        )
        .to_owned()
    }

    fn chrome(&self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::dark();
        let fullscreen = window.is_fullscreen();
        let seekable = self.can_seek();
        let duration = self.state.duration.filter(|d| *d > 0.);
        let shown = displayed_position(
            self.scrub,
            self.state.position,
            duration.unwrap_or(0.),
            Instant::now(),
        );
        let chapter = chapter_index(&self.state.chapters, shown).map(|i| {
            format!(
                "{:02}  {}",
                i + 1,
                chapter_label(i, &self.state.chapters[i])
            )
        });
        let subtitle_on = self
            .state
            .tracks
            .iter()
            .any(|t| t.kind == TrackKind::Subtitle && t.selected);
        // Definite geometry: bottom-right of the menu sits just above the dock,
        // aligned with its right edge (no measured-bounds round trip).
        let viewport = window.viewport_size();
        let dock_extent = if self.dock_height > Pixels::default() {
            self.dock_height
        } else {
            DOCK_ESTIMATE
        };
        let menu_anchor = point(
            viewport.width - DOCK_SIDE_INSET,
            viewport.height - DOCK_BOTTOM_INSET - dock_extent - s::S8,
        );
        let dock_entity = cx.entity();
        // (compact item, full wording for the tooltip)
        let notice = if self.progress.as_ref().is_some_and(|p| p.notice) {
            Some((
                StatusItem::warning("Not saved"),
                "Progress could not be saved",
            ))
        } else if self.state.paused_for_cache {
            Some((StatusItem::text("Buffering…"), "Buffering…"))
        } else if self.control_notice
            || (self.state.error.is_some() && !terminal_failure(false, &self.state))
        {
            Some((
                StatusItem::warning("Control failed"),
                "Control unavailable — try again",
            ))
        } else if self.state.ended {
            Some((
                StatusItem::text("Finished"),
                "Finished · Replay to watch again",
            ))
        } else if !seekable && !self.state.loading {
            Some((StatusItem::text("No seeking"), "Seeking unavailable"))
        } else {
            None
        };
        let requested = self
            .delay_requested
            .filter(|(_, since)| since.elapsed() < Duration::from_millis(1200))
            .map(|(delay, _)| format!("Subtitle delay {delay:+.1}s"))
            .or_else(|| {
                self.speed_requested
                    .filter(|(_, since)| since.elapsed() < Duration::from_millis(1200))
                    .map(|(speed, _)| format!("Speed {}", speed_label(speed)))
            });
        let summary = self.buffer_summary();
        let detail = [
            notice.as_ref().map_or("", |(_, full)| *full),
            summary.as_str(),
            requested.as_deref().unwrap_or(""),
        ]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
        let status_items: Vec<StatusItem> = notice
            .map(|(item, _)| item)
            .into_iter()
            .chain(transfer_items(&self.state, &self.transfer))
            .chain(requested.map(StatusItem::text))
            .collect();
        let status = div()
            .id("player-status")
            .flex_1()
            .w(px(0.))
            .min_w_0()
            .overflow_hidden()
            .flex()
            .items_center()
            .gap(s::S12)
            .px(s::S8)
            .type_style(t::SMALL)
            .font_features(FontFeatures(vec![("tnum".into(), 1)].into()))
            .text_color(p.text_secondary)
            .children(status_items.into_iter().map(|item| {
                let color = if item.warn {
                    p.status_warn
                } else {
                    p.text_secondary
                };
                div()
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(s::S4)
                    .text_color(color)
                    .when_some(item.icon, |el, icon| {
                        el.child(Icon::new(icon).size(px(14.)).color(color))
                    })
                    .child(item.text)
            }))
            .when(!detail.is_empty(), |el| {
                el.tooltip(crate::ui::primitives::tooltip::tooltip(detail))
            });
        let slot = |width: Pixels, button: Button| {
            div()
                .w(width)
                .flex()
                .flex_none()
                .justify_center()
                .child(button)
        };
        // Fixed-width slots: the cluster's intrinsic width collapses under the
        // shared Button's negative focus-ring margins (verified on a capture), so
        // every utility sits in a slot whose width does not depend on measurement.
        let utilities = div()
            .relative()
            .flex_none()
            .flex()
            .items_center()
            .gap(s::S4)
            .child(slot(
                s::S32,
                self.control(
                    HELP,
                    "player-shortcut-help",
                    IconName::Keyboard,
                    format!(
                        "Keyboard & mouse shortcuts ({})",
                        shortcuts::entry(Sid::ShowHelp).keys
                    ),
                )
                .on_click(cx.listener(|view, _, window, cx| view.open_help(window, cx))),
            ))
            .child(slot(
                s::S32,
                self.control(CHAPTERS, "player-chapters", IconName::Menu, "Chapters")
                    .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                        view.toggle_menu(MenuKind::Chapters, !event.is_keyboard(), window, cx)
                    })),
            ))
            .child(slot(
                s::S48,
                self.text_control(
                    SUBTITLES,
                    "player-subtitles",
                    "CC",
                    "Subtitles",
                    subtitle_on,
                )
                .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                    view.toggle_menu(MenuKind::Subtitles, !event.is_keyboard(), window, cx)
                })),
            ))
            .child(slot(
                s::S48 + s::S12,
                self.text_control(AUDIO, "player-audio", "Audio", "Audio track", false)
                    .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                        view.toggle_menu(MenuKind::Audio, !event.is_keyboard(), window, cx)
                    })),
            ))
            .child(slot(
                s::S48 + s::S4,
                self.text_control(
                    SPEED,
                    "player-speed",
                    speed_label(self.state.speed),
                    "Playback speed",
                    (self.state.speed - 1.).abs() > 0.01,
                )
                .on_click(cx.listener(|view, event: &ClickEvent, window, cx| {
                    view.toggle_menu(MenuKind::Speed, !event.is_keyboard(), window, cx)
                })),
            ))
            .child(slot(
                s::S32,
                self.control(
                    FULLSCREEN,
                    "player-fullscreen",
                    if fullscreen {
                        IconName::ExitFullscreen
                    } else {
                        IconName::Fullscreen
                    },
                    shortcuts::tooltip(Sid::ToggleFullscreen),
                )
                .on_click(|_, window, _| window.toggle_fullscreen()),
            ));
        div()
            .absolute()
            .inset_0()
            .flex()
            .flex_col()
            .justify_between()
            .child(
                // Mirrors the dock: contrast-safe backing under the title row,
                // then a fade down into the video instead of a hard band edge.
                div()
                    .relative()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .pl(DOCK_SIDE_INSET)
                    .pr(DOCK_SIDE_INSET - s::S8)
                    .pt(s::S12)
                    .pb(s::S4)
                    .occlude()
                    .bg(p.scrim_max)
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .bottom(-TOP_FADE)
                            .h(TOP_FADE)
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(p.scrim_max, 0.),
                                linear_color_stop(p.scrim_max.opacity(0.), 1.),
                            )),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .type_style(t::BODY_STRONG)
                            .truncate()
                            .child(self.chrome_title(window)),
                    )
                    .child(
                        self.control(CLOSE, "player-close", IconName::Close, "Close player")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                // Edge-attached, full-bleed scrim fading up into the video; the
                // approved `.dock` is only inset content, with no card or border.
                div()
                    .id("player-dock-scrim")
                    .occlude()
                    .w_full()
                    .flex_none()
                    .relative()
                    .pt(s::S8)
                    .pb(DOCK_BOTTOM_INSET)
                    .px(DOCK_SIDE_INSET)
                    // Opaque-enough backing keeps text/buttons contrast-safe; the
                    // fade above it ends at the same colour, so there is no edge.
                    .bg(p.scrim_max)
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .right_0()
                            .top(-DOCK_FADE)
                            .h(DOCK_FADE)
                            .bg(linear_gradient(
                                180.,
                                linear_color_stop(p.scrim_max.opacity(0.), 0.),
                                linear_color_stop(p.scrim_max, 1.),
                            )),
                    )
                    .on_hover(cx.listener(|view, hovered: &bool, _, cx| {
                        view.dock_hover = *hovered;
                        view.activity(cx);
                    }))
                    .child(
                        div()
                            .id("player-dock")
                            .relative()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s::S12)
                                    .type_style(t::MONO)
                                    .text_color(p.text_secondary)
                                    .child(div().w(s::S48 + s::S16).child(format_clock(shown)))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .truncate()
                                            .text_center()
                                            .type_style(t::BODY_STRONG)
                                            .text_color(p.text_primary)
                                            .children(chapter),
                                    )
                                    .child(div().w(s::S48 + s::S16).text_right().child(
                                        match duration {
                                            Some(d) if seekable => format_remaining(shown, d),
                                            _ => "—:—".into(),
                                        },
                                    )),
                            )
                            .child(self.timeline(window, cx))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(s::S8)
                                    .child(
                                        self.control(
                                            PLAY,
                                            "player-play",
                                            if self.state.paused || self.state.ended {
                                                IconName::Play
                                            } else {
                                                IconName::Pause
                                            },
                                            shortcuts::tooltip(Sid::TogglePlayback),
                                        )
                                        .on_click(
                                            cx.listener(|view, _, _, cx| view.toggle_playback(cx)),
                                        ),
                                    )
                                    .child(
                                        self.control(
                                            BACK,
                                            "player-back",
                                            IconName::SeekBack,
                                            shortcuts::tooltip(Sid::SeekBack),
                                        )
                                        .on_click(
                                            cx.listener(|view, _, _, cx| view.seek(-10., cx)),
                                        ),
                                    )
                                    .child(
                                        self.control(
                                            FORWARD,
                                            "player-forward",
                                            IconName::SeekForward,
                                            shortcuts::tooltip(Sid::SeekForward),
                                        )
                                        .on_click(cx.listener(|view, _, _, cx| view.seek(10., cx))),
                                    )
                                    .child(status)
                                    .child(utilities),
                            )
                            .child(
                                canvas(
                                    move |bounds, _, cx| {
                                        dock_entity.update(cx, |view, _| {
                                            view.dock_height = bounds.size.height
                                        })
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            ),
                    ),
            )
            // After the bottom scrim/fade so the floating volume paints and hit-tests above it.
            .child(self.volume_panel(window, cx))
            .when_some(self.menu.as_ref(), |el, open| {
                el.child(
                    deferred(
                        anchored()
                            .anchor(Anchor::BottomRight)
                            .position(menu_anchor)
                            .snap_to_window_with_margin(s::S8)
                            .child(open.menu.clone()),
                    )
                    .priority(10),
                )
            })
            .into_any_element()
    }
}

impl Render for PlayerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(meter) = self.cadence.as_mut() {
            meter.render_called(Instant::now());
        }
        let p = Palette::dark();
        let visible = self.chrome_visible(window);
        self.chrome_was_visible = visible;
        let failed = terminal_failure(self.error.is_some(), &self.state);
        let entity = cx.weak_entity();
        let has_video = false;
        #[cfg(feature = "ui-probe")]
        let has_video = has_video || self.fixture_image.is_some();
        #[cfg(feature = "gpu-surface")]
        let has_video = has_video || self.gpu_frame.is_some();
        div().id("kinema-player").key_context("KinemaPlayer").relative().size_full().overflow_hidden().bg(p.bg_canvas).text_color(p.text_primary).font_family(t::UI_FAMILY)
            .on_action(cx.listener(|view, _: &TogglePlayback, window, cx| view.shortcut(Sid::TogglePlayback, window, cx)))
            .on_action(cx.listener(|view, _: &SeekBack, window, cx| view.shortcut(Sid::SeekBack, window, cx)))
            .on_action(cx.listener(|view, _: &SeekForward, window, cx| view.shortcut(Sid::SeekForward, window, cx)))
            .on_action(cx.listener(|view, _: &SeekBackLong, window, cx| view.shortcut(Sid::SeekBackLong, window, cx)))
            .on_action(cx.listener(|view, _: &SeekForwardLong, window, cx| view.shortcut(Sid::SeekForwardLong, window, cx)))
            .on_action(cx.listener(|view, _: &SeekBackFine, window, cx| view.shortcut(Sid::SeekBackFine, window, cx)))
            .on_action(cx.listener(|view, _: &SeekForwardFine, window, cx| view.shortcut(Sid::SeekForwardFine, window, cx)))
            .on_action(cx.listener(|view, _: &ToggleMute, window, cx| view.shortcut(Sid::ToggleMute, window, cx)))
            .on_action(cx.listener(|view, _: &VolumeUp, window, cx| view.shortcut(Sid::VolumeUp, window, cx)))
            .on_action(cx.listener(|view, _: &VolumeDown, window, cx| view.shortcut(Sid::VolumeDown, window, cx)))
            .on_action(cx.listener(|view, _: &NextAudio, window, cx| view.shortcut(Sid::NextAudio, window, cx)))
            .on_action(cx.listener(|view, _: &NextSubtitle, window, cx| view.shortcut(Sid::NextSubtitle, window, cx)))
            .on_action(cx.listener(|view, _: &PreviousSubtitle, window, cx| view.shortcut(Sid::PreviousSubtitle, window, cx)))
            .on_action(cx.listener(|view, _: &ToggleSubtitles, window, cx| view.shortcut(Sid::ToggleSubtitles, window, cx)))
            .on_action(cx.listener(|view, _: &DelayBack, window, cx| view.shortcut(Sid::DelayBack, window, cx)))
            .on_action(cx.listener(|view, _: &DelayForward, window, cx| view.shortcut(Sid::DelayForward, window, cx)))
            .on_action(cx.listener(|view, _: &SpeedDown, window, cx| view.shortcut(Sid::SpeedDown, window, cx)))
            .on_action(cx.listener(|view, _: &SpeedUp, window, cx| view.shortcut(Sid::SpeedUp, window, cx)))
            .on_action(cx.listener(|view, _: &ResetSpeed, window, cx| view.shortcut(Sid::ResetSpeed, window, cx)))
            .on_action(cx.listener(|view, _: &PreviousFrame, window, cx| view.shortcut(Sid::PreviousFrame, window, cx)))
            .on_action(cx.listener(|view, _: &NextFrame, window, cx| view.shortcut(Sid::NextFrame, window, cx)))
            .on_action(cx.listener(|view, _: &PreviousChapter, window, cx| view.shortcut(Sid::PreviousChapter, window, cx)))
            .on_action(cx.listener(|view, _: &NextChapter, window, cx| view.shortcut(Sid::NextChapter, window, cx)))
            .on_action(cx.listener(|view, _: &ToggleFullscreen, window, cx| view.shortcut(Sid::ToggleFullscreen, window, cx)))
            .on_action(cx.listener(|view, _: &Escape, window, cx| view.shortcut(Sid::Escape, window, cx)))
            .on_action(cx.listener(|view, _: &ShowHelp, window, cx| view.shortcut(Sid::ShowHelp, window, cx)))
            .on_action(cx.listener(|view, _: &NextControl, window, cx| view.shortcut(Sid::NextControl, window, cx)))
            .on_action(cx.listener(|view, _: &PreviousControl, window, cx| view.shortcut(Sid::PreviousControl, window, cx)))
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| view.pointer_moved(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| view.finish_drag(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| view.finish_drag(cx)))
            .child(div().id("player-surface").key_context("KinemaPlayerSurface").track_focus(&self.surface_focus).absolute().size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(|view, event: &MouseDownEvent, window, cx| {
                    view.surface_focus.focus(window, cx);
                    if event.click_count == 2 { window.toggle_fullscreen(); }
                    view.activity(cx);
                    cx.stop_propagation();
                }))
                .on_mouse_down(MouseButton::Right, cx.listener(|view, _, window, cx| {
                    view.surface_focus.focus(window, cx);
                    view.toggle_playback(cx);
                    cx.stop_propagation();
                }))
                .when(self.mode == PlayerChoice::Embedded && !failed, |element| element.child(self.embedded_video())))
            .when(failed || !has_video, |element| element.child(
                div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(s::S12).when(failed, |panel| panel.bg(p.scrim_max))
                    .child(div().type_style(t::TITLE).child(if failed { "Playback unavailable" } else if self.state.ended { "Playback finished" } else if self.state.paused_for_cache { "Buffering…" } else if self.state.loading || self.player.is_none() { "Opening media…" } else { "Audio playback" }))
                    .child(div().max_w(s::SHEET_WIDTH).px(s::S24).type_style(t::BODY).text_color(p.text_secondary).child(if failed && self.mode == PlayerChoice::Embedded { embedded_help() } else if failed { "Playback stopped. Choose external MPV or VLC in Settings → Playback, then try again. If libmpv is missing, install its runtime first." } else if self.state.loading || self.player.is_none() { "Preparing the embedded player. You can close this window at any time." } else { "Use the controls below. Press K to play or pause." }))
                    .when(failed && self.mode == PlayerChoice::Embedded, |panel| panel.child(
                        div().max_w(s::SHEET_WIDTH).px(s::S24).type_style(t::BODY).text_color(p.text_secondary)
                            .child(failure_label(failure_reason(&self.state, self.failure_reason)))
                    ))
                    // Backend errors may contain media URLs; never display/log
                    // their raw contents (signed stream URLs can be credentials).
            ))
            .child(canvas(move |bounds, window, cx| {
                let scale = window.scale_factor();
                let width = (f32::from(bounds.size.width) * scale).round().max(0.) as u32;
                let height = (f32::from(bounds.size.height) * scale).round().max(0.) as u32;
                let _ = entity.update(cx, |view, _| {
                    if let Some(player) = view.player.as_ref().and_then(|owner| owner.player.as_ref()) {
                        let _ = player.resize(width, height);
                    }
                });
            }, |_, _, _, _| {}).absolute().size_full())
            .when(visible, |div| div.child(self.chrome(window, cx)))
            .when(self.progress.as_ref().is_some_and(|p| matches!(p.phase, ProgressPhase::Choice | ProgressPhase::Mismatch | ProgressPhase::Unavailable)), |element| {
                let unavailable = self.progress.as_ref().is_some_and(|p| p.phase == ProgressPhase::Unavailable);
                let first_play = self.progress.as_ref().is_some_and(|p| p.usable_checkpoint().is_none());
                let mismatch = self.progress.as_ref().is_some_and(|p| p.phase == ProgressPhase::Mismatch);
                element.child(div().absolute().inset_0().occlude().bg(p.scrim_max).flex().flex_col().items_center().justify_center().gap(s::S16)
                    .child(div().type_style(t::TITLE).child(if unavailable && first_play { "Playback could not start" } else if unavailable { "Saved position cannot be resumed" } else if mismatch { "This source has a different duration" } else { "Resume this replay?" }))
                    .child(div().type_style(t::BODY).child(if unavailable && first_play { "Playback readiness could not be confirmed. Retry, or try playing without saving progress." } else if unavailable { "Playback readiness or resume could not be confirmed. Your saved progress is unchanged." } else if mismatch { "Resume at the saved time, or start over. Your saved progress is unchanged until playback is ready." } else { "Resume your saved position, or play from the beginning." }))
                    .when(!unavailable, |panel| panel.child(Button::new("progress-resume", if mismatch { "Resume anyway" } else { "Resume" }).on_click(cx.listener(|view, _, _, cx| view.choose_progress(true, cx)))))
                    .when(unavailable, |panel| panel
                        .child(Button::new("progress-retry-startup", "Retry").on_click(cx.listener(|view, _, _, cx| view.retry_startup(cx))))
                        .child(Button::new("progress-play-unsaved", if first_play { "Try playing without saving" } else { "Play without saving" }).on_click(cx.listener(|view, _, _, cx| view.play_without_saving(cx)))))
                    .when(self.progress.as_ref().is_some_and(|progress| progress.can_start_over(&self.state)), |panel| panel.child(Button::new("progress-start-over", "Play from beginning").on_click(cx.listener(|view, _, _, cx| view.choose_progress(false, cx))))))
            })
            .when(self.progress.as_ref().is_some_and(|p| p.notice && matches!(p.phase, ProgressPhase::Ready | ProgressPhase::Reset)), |element| {
                element.child(div().absolute().inset_0().occlude().bg(p.scrim_max).flex().flex_col().items_center().justify_center().gap(s::S16)
                    .child(div().type_style(t::TITLE).child("Progress could not be saved"))
                    .child(div().type_style(t::BODY).child("Your saved progress is unchanged. Retry saving, or play without saving progress."))
                    .child(Button::new("progress-retry-saving", "Retry saving").on_click(cx.listener(|view, _, _, cx| view.retry_progress_ready(cx))))
                    .child(Button::new("progress-untracked", "Play without saving").on_click(cx.listener(|view, _, _, cx| view.play_without_saving(cx)))))
            })
            .when(self.help_focus.is_some(), |el| el.child(self.shortcut_help(window, cx)))
            // Opt-in diagnostics only: no handlers, id or focus, so it never takes input.
            .when_some(self.cadence_text.clone(), |el, text| {
                el.child(
                    div()
                        .absolute()
                        .top(px(72.))
                        .left(s::S16)
                        .max_w(px(360.))
                        .px(s::S8)
                        .py(s::S4)
                        .truncate()
                        .rounded(radius::SM)
                        .bg(p.scrim_max.opacity(0.7))
                        .type_style(t::MONO)
                        .text_color(p.text_secondary)
                        .child(text),
                )
            })
    }
}

#[cfg(test)]
fn progress(position: f64, duration: Option<f64>) -> Option<f64> {
    duration.filter(|d| d.is_finite() && *d > 0.).map(|d| {
        if position.is_finite() {
            (position / d).clamp(0., 1.)
        } else {
            0.
        }
    })
}
fn redraw_needed(
    state_changed: bool,
    frame_received: bool,
    was_visible: bool,
    visible: bool,
) -> bool {
    state_changed || frame_received || was_visible != visible
}
#[derive(Clone, Copy)]
struct Availability {
    backend: bool,
    seekable: bool,
    chapters: bool,
    subtitles: bool,
    audio: bool,
    /// Metadata was parsed successfully (an empty list is then a known empty).
    chapters_known: bool,
    tracks_known: bool,
}
fn control_enabled(index: usize, a: Availability) -> bool {
    match index {
        CLOSE | FULLSCREEN | HELP => true,
        TIMELINE | BACK | FORWARD => a.backend && a.seekable,
        CHAPTERS => a.backend && a.seekable && a.chapters,
        SUBTITLES => a.backend && a.subtitles,
        AUDIO => a.backend && a.audio,
        _ => a.backend,
    }
}
/// Why a control is unavailable, so a disabled button is never unexplained.
fn disabled_reason(index: usize, a: Availability) -> Option<&'static str> {
    if control_enabled(index, a) {
        return None;
    }
    Some(match index {
        _ if !a.backend => "Player is not ready",
        TIMELINE | BACK | FORWARD => "Seeking unavailable for this media",
        CHAPTERS if !a.seekable => "Seeking unavailable for this media",
        CHAPTERS if !a.chapters_known => "Chapter information unavailable",
        CHAPTERS => "No chapters in this media",
        SUBTITLES if !a.tracks_known => "Subtitle tracks unavailable",
        SUBTITLES => "No subtitle tracks in this media",
        AUDIO if !a.tracks_known => "Audio tracks unavailable",
        AUDIO => "No audio tracks in this media",
        _ => "Unavailable",
    })
}
fn terminal_failure(startup_error: bool, state: &PlaybackState) -> bool {
    startup_error
        || state.stopped
        || state.fatal_error.is_some()
        || (state.media_stopped && state.error.is_some())
}
fn replay_needed(state: &PlaybackState) -> bool {
    state.ended && !terminal_failure(false, state)
}
fn controls_visible(
    paused: bool,
    loading: bool,
    error: bool,
    interacting: bool,
    idle: Duration,
) -> bool {
    paused || loading || error || interacting || idle < CHROME_IDLE
}

#[cfg(feature = "ui-probe")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProbeCommand {
    Pause,
    Adjust,
    Resume,
    Restore,
}

/// Each transition requires a fresh observed libmpv snapshot, not command acceptance.
#[cfg(feature = "ui-probe")]
#[derive(Default)]
struct ProbeControls {
    phase: u8,
    target: f64,
    resumed_at: f64,
    failed: bool,
}
#[cfg(feature = "ui-probe")]
impl ProbeControls {
    fn observe(&mut self, state: &PlaybackState, frames: u64) -> Option<ProbeCommand> {
        if self.failed {
            return None;
        }
        match self.phase {
            0 if frames > 2
                && !state.loading
                && state.duration.is_some_and(|duration| duration >= 6.) =>
            {
                self.target = (state.duration.unwrap_or(8.) * 0.25).clamp(1., 3.);
                self.phase = 1;
                Some(ProbeCommand::Pause)
            }
            1 if state.paused => {
                self.phase = 2;
                Some(ProbeCommand::Adjust)
            }
            2 if state.paused
                && (state.position - self.target).abs() < 0.35
                && (state.speed - 1.25).abs() < 0.01
                && (state.volume - 37.).abs() < 0.5 =>
            {
                self.resumed_at = state.position;
                self.phase = 3;
                Some(ProbeCommand::Resume)
            }
            3 if !state.paused && state.position > self.resumed_at + 0.05 => {
                self.phase = 4;
                Some(ProbeCommand::Restore)
            }
            4 if !state.paused
                && (state.speed - 1.).abs() < 0.01
                && (state.volume - 100.).abs() < 0.5 =>
            {
                self.phase = 5;
                None
            }
            _ => None,
        }
    }
    fn step(&mut self, player: &Player, frames: u64) {
        let command = self.observe(&player.state(), frames);
        let result = match command {
            Some(ProbeCommand::Pause) => player.set_paused(true),
            Some(ProbeCommand::Adjust) => player
                .seek_absolute(self.target)
                .and_then(|()| player.set_speed(1.25))
                .and_then(|()| player.set_volume(37.)),
            Some(ProbeCommand::Resume) => player.set_paused(false),
            Some(ProbeCommand::Restore) => {
                player.set_speed(1.).and_then(|()| player.set_volume(100.))
            }
            None => return,
        };
        self.failed |= result.is_err();
    }
    fn verified(&self) -> bool {
        self.phase == 5 && !self.failed
    }
}

#[cfg(feature = "ui-probe")]
#[derive(Default)]
struct ProbeResize {
    requested: u32,
    observed: u32,
    previous: Option<[u32; 2]>,
}
#[cfg(feature = "ui-probe")]
impl ProbeResize {
    fn observe(&mut self, viewport: [u32; 2]) {
        if self.previous.is_some_and(|previous| previous != viewport) {
            self.observed += 1;
        }
        self.previous = Some(viewport);
    }
    fn verified(&self) -> bool {
        self.requested >= 2 && self.observed >= 2
    }
}

#[cfg(feature = "ui-probe")]
struct ProbeProxy {
    paints: u64,
}
#[cfg(feature = "ui-probe")]
impl Render for ProbeProxy {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.paints += 1;
        div()
            .size_full()
            .bg(rgb(0x16202a))
            .text_color(rgb(0xe0e0e0))
            .child("Offline auxiliary surface — no settings or services")
    }
}
#[cfg(feature = "ui-probe")]
fn open_probe_proxy(cx: &mut App, offset: f32) -> Option<WindowHandle<ProbeProxy>> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(offset), px(60.)),
                size: size(px(340.), px(240.)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("Kinema / OFFLINE SURFACE PROXY".into()),
                ..Default::default()
            }),
            ..Default::default()
        },
        |_, cx| cx.new(|_| ProbeProxy { paints: 0 }),
    )
    .ok()
}
#[cfg(feature = "ui-probe")]
fn probe_viewport(window: &Window) -> [u32; 2] {
    let viewport = window.viewport_size();
    [
        f32::from(viewport.width).round() as u32,
        f32::from(viewport.height).round() as u32,
    ]
}

#[cfg(feature = "ui-probe")]
static PROBE_GPU_ERRORS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "ui-probe")]
static PROBE_PANICS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "ui-probe")]
struct ProbeLogger;
#[cfg(feature = "ui-probe")]
impl log::Log for ProbeLogger {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        let target = record.target();
        if record.level() == log::Level::Error
            && ["wgpu", "naga", "gpui", "vulkan", "metal"]
                .iter()
                .any(|category| target.contains(category))
        {
            PROBE_GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            eprintln!("player probe: category=gpu-error-log");
        }
        // Never print arbitrary log bodies, paths, URLs, or panic payloads.
    }
    fn flush(&self) {}
}
#[cfg(feature = "ui-probe")]
static PROBE_LOGGER: ProbeLogger = ProbeLogger;

#[cfg(feature = "ui-probe")]
fn install_probe_gpu_guard() {
    #[cfg(all(
        feature = "gpu-surface",
        any(target_os = "linux", target_os = "windows")
    ))]
    if let Some(context) = gpui_wgpu::experimental_gpu_context() {
        context
            .device
            .on_uncaptured_error(Arc::new(|_: wgpu::Error| {
                PROBE_GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                eprintln!("player probe: category=uncaptured-wgpu-error");
            }));
    } else {
        PROBE_GPU_ERRORS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        eprintln!("player probe: category=gpu-guard-unavailable");
    }
}

/// Offline fixture/native entry point; native media uses Embedded GPU playback.
#[cfg(feature = "ui-probe")]
pub fn run_probe(
    media: Option<std::path::PathBuf>,
    seconds: u64,
    light: bool,
    dimensions: [f32; 2],
) -> bool {
    run_probe_mode(
        media,
        seconds,
        light,
        dimensions,
        PlayerChoice::Embedded,
        false,
    )
}

/// Offline production-window smoke. Never initializes the coordinator, settings,
/// providers, database, credentials, or the normal application entry point.
#[cfg(feature = "ui-probe")]
pub fn run_probe_mode(
    media: Option<std::path::PathBuf>,
    seconds: u64,
    light: bool,
    dimensions: [f32; 2],
    mode: PlayerChoice,
    stress_resize: bool,
) -> bool {
    use std::{cell::Cell, rc::Rc, sync::atomic::Ordering};
    let gpu_playback = media.is_some();
    let native = media.is_some();
    if !mode.is_managed()
        || (native && !media.as_ref().is_some_and(|path| path.is_file()))
        || (gpu_playback && !cfg!(feature = "gpu-surface"))
        || (stress_resize && !gpu_playback)
    {
        eprintln!("player probe: category=invalid-probe-capability");
        return false;
    }
    PROBE_GPU_ERRORS.store(0, Ordering::Relaxed);
    PROBE_PANICS.store(0, Ordering::Relaxed);
    let old_hook = gpu_playback.then(|| {
        let old = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {
            PROBE_PANICS.fetch_add(1, Ordering::Relaxed);
            eprintln!("player probe: category=panic");
        }));
        old
    });
    let logger_ready = !gpu_playback || log::set_logger(&PROBE_LOGGER).is_ok();
    if gpu_playback {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let result = Rc::new(Cell::new(false));
    let outcome = result.clone();
    gpui_platform::application().with_assets(crate::ui::primitives::assets::Assets).run(move |cx| {
        crate::ui::primitives::assets::init(cx);
        crate::ui::theme::set_mode(if light { crate::ui::theme::Mode::Light } else { crate::ui::theme::Mode::Dark }, cx);
        bind_keys(cx);
        let options = WindowOptions {
            app_id: Some("dev.tlmtech.kinema".into()),
            icon: player_window_icon(),
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.), px(0.)), size: size(px(dimensions[0]), px(dimensions[1])) })),
            titlebar: Some(TitlebarOptions { title: Some(if gpu_playback { "Kinema / EMBEDDED GPU SMOKE" } else { "Kinema Design Lab / Player — FIXTURE ONLY" }.into()), ..Default::default() }),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| cx.new(|cx| {
            let mut view = PlayerWindow::new_mode(if gpu_playback { playback_title("Offline fixture", None) } else { playback_title("The Outsider", Some((1, 4, Some("Que Viene el Coco")))) }, media.map(|path| path.to_string_lossy().into_owned()), mode, window, cx);
            if gpu_playback { cx.defer_in(window, |_, _, _| install_probe_gpu_guard()); }
            if !native {
                let chapter = |time, title: &str| kinema_mpv::Chapter { time, title: Some(title.into()) };
                let track = |id, kind, title: &str, lang: &str, selected| kinema_mpv::Track { id, kind, title: Some(title.into()), lang: Some(lang.into()), selected };
                view.state = PlaybackState {
                    position: 1837., duration: Some(6840.), paused: true, volume: 80.,
                    chapters_available: true, tracks_available: true,
                    chapters: vec![chapter(0., "Opening"), chapter(821., "The long road"), chapter(1586., "First light"), chapter(2899., "The other shore"), chapter(4868., "Homeward")],
                    tracks: vec![track(1, TrackKind::Audio, "Stereo", "en", true), track(2, TrackKind::Subtitle, "English", "en", true), track(3, TrackKind::Subtitle, "Français", "fr", false)],
                    speed: 1.25,
                    buffered_ranges: Some(vec![kinema_mpv::BufferedRange { start: 0., end: 2600. }]),
                    ..Default::default()
                };
                // A synthetic horizon, not copyrighted artwork or a provider.
                let mut pixels = Vec::with_capacity(960 * 540 * 4);
                for y in 0..540u32 { for x in 0..960u32 {
                    let glow = ((1. - ((x as f32 - 610.) / 600.).abs()).max(0.) * (1. - ((y as f32 - 260.) / 210.).abs()).max(0.) * 40.) as u8;
                    pixels.extend_from_slice(&[8 + glow / 2, 16 + glow, 23 + glow, 255]);
                }}
                view.fixture_image = image::RgbaImage::from_raw(960, 540, pixels)
                    .map(|buffer| Arc::new(RenderImage::new([image::Frame::new(buffer)])));
            }
            view
        }));
        let Ok(handle) = handle else { eprintln!("player probe: category=window-open-failed"); cx.quit(); return; };
        let mut auxiliaries = Vec::new();
        if stress_resize {
            for offset in [100., 500.] {
                if let Some(window) = open_probe_proxy(cx, offset) { auxiliaries.push(window); }
            }
            if auxiliaries.len() != 2 { eprintln!("player probe: category=auxiliary-open-failed"); cx.quit(); return; }
        }
        cx.activate(true);
        cx.spawn(async move |cx| {
            let started = Instant::now();
            let deadline = started + Duration::from_secs(seconds.clamp(1, 15));
            let playback_deadline = if gpu_playback { deadline - Duration::from_millis(1000) } else { deadline };
            let mut controls = ProbeControls::default();
            let mut resize = ProbeResize::default();
            let mut aux_initial = None;
            let mut aux_resized = false;
            let mut aux_closed_painted = false;
            let mut aux_reopened = false;
            let mut stress_phase = 0;
            let mut stress_at = Instant::now();
            let mut failed = !logger_ready;
            while gpu_playback && Instant::now() < playback_deadline {
                cx.background_executor().timer(Duration::from_millis(40)).await;
                let tick = handle.update(cx, |view, window, _| {
                    resize.observe(probe_viewport(window));
                    if let Some(player) = view.player.as_ref().and_then(|owner| owner.player.as_ref()) {
                        controls.step(player, view.frames_presented);
                    }
                    (view.frames_presented, terminal_failure(view.error.is_some(), &view.state))
                });
                let Ok((frames, terminal)) = tick else { failed = true; break; };
                if terminal || controls.failed || PROBE_GPU_ERRORS.load(Ordering::Relaxed) > 0 || PROBE_PANICS.load(Ordering::Relaxed) > 0 {
                    failed = true; break;
                }
                if stress_resize && frames > 2 {
                    if let Some(auxiliary) = auxiliaries.first() {
                        let viewport = auxiliary.update(cx, |_, window, _| probe_viewport(window)).ok();
                        if aux_initial.is_none() { aux_initial = viewport; }
                        aux_resized |= viewport.zip(aux_initial).is_some_and(|(current, previous)| current != previous);
                    }
                    if stress_phase == 0 {
                        if handle.update(cx, |_, window, _| {
                            let target = if probe_viewport(window) == [940, 620] { [1000., 640.] } else { [940., 620.] };
                            window.resize(size(px(target[0]), px(target[1])));
                        }).is_ok() { resize.requested += 1; }
                        if let Some(auxiliary) = auxiliaries.first() {
                            failed |= auxiliary.update(cx, |_, window, _| window.resize(size(px(430.), px(310.)))).is_err();
                        }
                        stress_phase = 1;
                        stress_at = Instant::now();
                    } else if stress_phase == 1 && resize.observed >= 1 && aux_resized && stress_at.elapsed() >= Duration::from_millis(400) {
                        if handle.update(cx, |_, window, _| window.resize(size(px(1260.), px(760.)))).is_ok() { resize.requested += 1; }
                        if let Some(auxiliary) = auxiliaries.get(1) {
                            aux_closed_painted = auxiliary.update(cx, |view, window, _| {
                                let painted = view.paints > 0;
                                window.remove_window();
                                painted
                            }).unwrap_or(false);
                        }
                        if let Some(reopened) = cx.update(|cx| open_probe_proxy(cx, 500.)) {
                            auxiliaries[1] = reopened;
                            aux_reopened = true;
                        } else { failed = true; }
                        stress_phase = 2;
                    }
                }
            }
            if !gpu_playback {
                cx.background_executor().timer(Duration::from_secs(seconds.clamp(1, 15))).await;
            }
            let auxiliaries_painted = auxiliaries.iter().all(|handle| handle.update(cx, |view, _, _| view.paints > 0).unwrap_or(false));
            let stress_verified = !stress_resize || (resize.verified() && aux_resized && aux_closed_painted && aux_reopened && auxiliaries_painted);
            let released = Rc::new(Cell::new(!gpu_playback));
            let release_signal = released.clone();
            let snapshot = handle.update(cx, |view, window, _| {
                let diagnostics = view.player.as_ref().and_then(|owner| owner.player.as_ref()).map(Player::diagnostics);
                let source_sized = diagnostics.as_ref().is_some_and(|stats| {
                    matches!(stats.source_size, Some([1920, 1080] | [3840, 2160])) && stats.source_size == Some(stats.render_size)
                });
                let gpu_clean = diagnostics.as_ref().is_some_and(|stats| stats.gpu_surface_frames > 2
                    && stats.readback_bytes == 0 && stats.upload_bytes_estimate == 0
                    && stats.active_backend == kinema_mpv::RenderBackend::Gpu && stats.fallback_reason.is_none());
                let successful = !failed && (!native || (view.frames_presented > 2 && !terminal_failure(view.error.is_some(), &view.state) && view.state.error.is_none()))
                    && (!gpu_playback || (controls.verified() && source_sized && gpu_clean && stress_verified));
                if let Some(stats) = diagnostics.as_ref() {
                    eprintln!("player probe: category=render-diagnostics source={:?} render={:?} gpu-surfaces={} readback-bytes={} upload-bytes={} controls-phase={} position={:.2} speed={:.2} volume={:.0} paused={}",
                        stats.source_size, stats.render_size, stats.gpu_surface_frames, stats.readback_bytes, stats.upload_bytes_estimate,
                        controls.phase, view.state.position, view.state.speed, view.state.volume, view.state.paused);
                }
                eprintln!("player probe: category=snapshot frames={} error={} controls={} source-sized={} gpu-clean={} resize-requested={} resize-observed={} auxiliary-resized={} auxiliary-closed-painted={} auxiliary-reopened={} auxiliary-painted={} successful={}",
                    view.frames_presented, terminal_failure(view.error.is_some(), &view.state), controls.verified(), source_sized, gpu_clean,
                    resize.requested, resize.observed, aux_resized, aux_closed_painted, aux_reopened, auxiliaries_painted, successful);
                // Stop sampling and reject any late initialization result before
                // releasing the GPUI presentation lease and joining the worker.
                view.startup_cancel.store(true, Ordering::Release);
                view._updates = Task::ready(());
                view._initialize = Task::ready(());
                #[cfg(feature = "gpu-surface")]
                { view.gpu_frame.take(); }
                if gpu_playback {
                    window.refresh();
                    window.on_next_frame(move |window, _| {
                        window.refresh();
                        window.on_next_frame(move |_, _| release_signal.set(true));
                    });
                }
                (successful, view.frames_presented)
            });
            let cleanup_deadline = if gpu_playback { deadline } else { Instant::now() + Duration::from_secs(1) };
            while !released.get() && Instant::now() < cleanup_deadline {
                cx.background_executor().timer(Duration::from_millis(10)).await;
            }
            let stopped = handle.update(cx, |view, _, _| {
                let stopped = view.player.as_ref().map(|owner| owner.stopped.clone());
                view.player.take();
                stopped
            }).ok().flatten();
            while stopped.as_ref().is_some_and(|signal| !signal.load(Ordering::Acquire)) && Instant::now() < cleanup_deadline {
                cx.background_executor().timer(Duration::from_millis(10)).await;
            }
            let shutdown = stopped.as_ref().is_some_and(|signal| signal.load(Ordering::Acquire)) || !native;
            let errors = PROBE_GPU_ERRORS.load(Ordering::Relaxed);
            let panics = PROBE_PANICS.load(Ordering::Relaxed);
            let no_late_frames = handle.update(cx, |view, _, _| {
                snapshot.as_ref().is_ok_and(|(_, frames)| *frames == view.frames_presented) && view.player.is_none()
            }).unwrap_or(false);
            eprintln!("player probe: category=teardown presentation-released={} shutdown={} no-late-frames={} gpu-errors={} panics={}", released.get(), shutdown, no_late_frames, errors, panics);
            outcome.set(snapshot.is_ok_and(|(successful, _)| successful) && released.get() && shutdown && no_late_frames && errors == 0 && panics == 0);
            for auxiliary in auxiliaries { let _ = auxiliary.update(cx, |_, window, _| window.remove_window()); }
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            cx.update(|cx| cx.quit());
        }).detach();
    });
    if let Some(old_hook) = old_hook {
        std::panic::set_hook(old_hook);
    }
    result.get()
        && PROBE_GPU_ERRORS.load(Ordering::Relaxed) == 0
        && PROBE_PANICS.load(Ordering::Relaxed) == 0
}

#[cfg(test)]
mod tests {
    #[test]
    fn deferred_close_checkpoint_is_an_owned_pre_teardown_observation()
    -> Result<(), Box<dyn std::error::Error>> {
        use super::{PlaybackIntent, PlaybackLaunch, Progress, ProgressPhase};
        use kinema_core::{library::TitleId, watch_progress::PlaybackProgressTarget};
        let captured = {
            let mut progress = Progress::new(PlaybackLaunch {
                subject: PlaybackProgressTarget::Movie(
                    TitleId::new(1).ok_or("expected valid checkpoint fixture")?,
                ),
                epoch: 3,
                run: 3,
                checkpoint: None,
                watched: false,
                intent: PlaybackIntent::Normal,
            });
            progress.phase = ProgressPhase::Active;
            progress.authorized = true;
            progress.generation = Some(1);
            let state = kinema_mpv::PlaybackState {
                position: 24.0,
                position_revision: 1,
                position_observed: true,
                file_loaded: true,
                media_stopped: false,
                media_generation: 1,
                ..Default::default()
            };
            progress
                .sample(&state, std::time::Instant::now(), true)
                .ok_or("expected valid checkpoint fixture")?
        }; // Delivery owns values, not a player/view borrow.
        assert_eq!(captured.epoch, 3);
        assert_eq!(captured.run, 3);
        assert_eq!(captured.observation.position_ms(), 24_000);
        assert!(captured.force);
        Ok(())
    }

    #[test]
    fn desktop_metadata_never_publishes_untracked_filenames_or_urls() {
        let title = kinema_core::media::playback_title("/private/SECRET.mkv?token=secret", None);
        let snapshot = super::desktop_snapshot(
            super::desktop_target(7),
            &title,
            false,
            &kinema_mpv::PlaybackState::default(),
        );
        assert_eq!(snapshot.title, "Local video");
        assert_eq!(snapshot.description, None);
        let public = kinema_core::media::playback_title("Public movie", None);
        assert_eq!(
            super::desktop_snapshot(
                super::desktop_target(7),
                &public,
                true,
                &kinema_mpv::PlaybackState::default()
            )
            .title,
            "Public movie"
        );
    }
    #[test]
    fn desktop_snapshot_uses_observed_state_and_valid_units() {
        use crate::desktop_integration::Playback;
        let mut state = kinema_mpv::PlaybackState {
            file_loaded: true,
            position_observed: true,
            media_stopped: false,
            position: 12.5,
            duration: Some(90.0),
            volume: 75.0,
            seekable: Some(true),
            ..Default::default()
        };
        let title = kinema_core::media::playback_title("Public movie", None);
        let snapshot = super::desktop_snapshot(super::desktop_target(1), &title, true, &state);
        assert_eq!(snapshot.playback, Playback::Playing);
        assert_eq!(snapshot.position, std::time::Duration::from_millis(12_500));
        assert_eq!(snapshot.duration, Some(std::time::Duration::from_secs(90)));
        assert_eq!(snapshot.volume, 0.75);
        assert!(snapshot.seekable);
        state.paused = true;
        assert_eq!(
            super::desktop_snapshot(super::desktop_target(1), &title, true, &state).playback,
            Playback::Paused
        );
        state.loading = true;
        state.position = f64::NAN;
        state.duration = Some(f64::INFINITY);
        let snapshot = super::desktop_snapshot(super::desktop_target(1), &title, true, &state);
        assert_eq!(snapshot.playback, Playback::Stopped);
        assert!(!snapshot.seekable);
        assert_eq!(snapshot.position, std::time::Duration::ZERO);
        assert_eq!(snapshot.duration, None);
    }
    #[test]
    fn desktop_seek_capability_and_bounds_are_not_speculative() {
        let mut state = kinema_mpv::PlaybackState {
            duration: Some(90.0),
            ..Default::default()
        };
        assert!(!super::desktop_can_seek(&state));
        state.file_loaded = true;
        state.position_observed = true;
        state.media_stopped = false;
        state.seekable = Some(false);
        assert!(!super::desktop_can_seek(&state));
        state.seekable = Some(true);
        assert!(super::desktop_can_seek(&state));
        assert_eq!(super::bounded_seek(-10.0, Some(90.0)), 0.0);
        assert_eq!(super::bounded_seek(120.0, Some(90.0)), 90.0);
        assert_eq!(super::bounded_seek(120.0, None), 120.0);
        assert_ne!(super::desktop_target(1), super::desktop_target(2));
    }

    #[test]
    fn player_title_drops_episode_name_before_episode_identity() {
        let title = kinema_core::media::playback_title(
            "The Outsider",
            Some((1, 4, Some("Que Viene el Coco"))),
        );
        assert_eq!(
            super::title_for_width(&title, gpui::px(500.), gpui::px(500.)),
            title.full
        );
        assert_eq!(
            super::title_for_width(&title, gpui::px(500.), gpui::px(499.)),
            title.compact
        );
        assert_eq!(
            super::title_for_width(&title, gpui::px(500.), gpui::px(0.)),
            title.compact
        );
    }

    #[test]
    fn embedded_player_icon_decodes_and_differs_from_application_icon()
    -> Result<(), Box<dyn std::error::Error>> {
        let player = super::player_window_icon().ok_or("invalid embedded player PNG")?;
        let application = image::load_from_memory(include_bytes!(
            "../../../assets/brand/png/256/dev.tlmtech.kinema.png"
        ))?
        .into_rgba8();
        assert_eq!(player.dimensions(), (256, 256));
        assert_ne!(player.as_ref(), &application);
        Ok(())
    }

    #[cfg(feature = "ui-probe")]
    #[test]
    fn probe_controls_require_observed_mpv_state_for_every_phase() {
        use super::{ProbeCommand, ProbeControls};
        let mut controls = ProbeControls::default();
        let mut state = kinema_mpv::PlaybackState {
            duration: Some(12.),
            ..Default::default()
        };
        assert_eq!(controls.observe(&state, 2), None);
        assert_eq!(controls.observe(&state, 3), Some(ProbeCommand::Pause));
        assert_eq!(controls.observe(&state, 3), None);
        state.paused = true;
        assert_eq!(controls.observe(&state, 3), Some(ProbeCommand::Adjust));
        assert_eq!(controls.observe(&state, 4), None);
        state.position = controls.target;
        state.speed = 1.25;
        assert_eq!(controls.observe(&state, 4), None);
        state.volume = 37.;
        assert_eq!(controls.observe(&state, 4), Some(ProbeCommand::Resume));
        assert!(!controls.verified());
        state.paused = false;
        assert_eq!(controls.observe(&state, 4), None);
        state.position += 0.1;
        assert_eq!(controls.observe(&state, 5), Some(ProbeCommand::Restore));
        assert!(!controls.verified());
        state.speed = 1.;
        state.volume = 100.;
        assert_eq!(controls.observe(&state, 5), None);
        assert!(controls.verified());
    }

    #[cfg(feature = "ui-probe")]
    #[test]
    fn probe_resize_requires_changes_not_requests_or_repeated_samples() {
        let mut resize = super::ProbeResize {
            requested: 2,
            ..Default::default()
        };
        resize.observe([1100, 680]);
        resize.observe([1100, 680]);
        assert!(!resize.verified());
        resize.observe([940, 620]);
        resize.observe([940, 620]);
        assert_eq!(resize.observed, 1);
        assert!(!resize.verified());
        resize.observe([1260, 760]);
        assert!(resize.verified());
    }

    #[test]
    fn failure_labels_never_use_private_error_text() {
        let secret =
            "https://secret.invalid/media?credential=SECRET_MARKER /private/media raw-driver-error";
        let state = PlaybackState {
            error: Some(secret.into()),
            fatal_error: Some(secret.into()),
            ..Default::default()
        };
        for reason in [
            PlaybackFailure::Unknown,
            PlaybackFailure::BackendNotCompiled,
            PlaybackFailure::GpuContextUnavailable,
            PlaybackFailure::HdrUnsupported,
            PlaybackFailure::ColorMetadataUnavailable,
            PlaybackFailure::ColorTransferUnqualified,
            PlaybackFailure::DecodeMetadataUnavailable,
            PlaybackFailure::DecoderNotDirect,
            PlaybackFailure::GpuDeviceUnsupported,
            PlaybackFailure::GpuInteropUnavailable,
            PlaybackFailure::GpuCompletionFailed,
            PlaybackFailure::LibmpvUnavailable,
            PlaybackFailure::LibmpvIncompatible,
            PlaybackFailure::RenderInitFailed,
            PlaybackFailure::MediaLoadFailed,
            PlaybackFailure::RenderSizeLimit,
        ] {
            let label = failure_label(failure_reason(&state, Some(reason)));
            assert_eq!(label, format!("{}: {}", reason.code(), reason.message()));
            for forbidden in [
                "https://",
                "SECRET_MARKER",
                "/private/media",
                "raw-driver-error",
            ] {
                assert!(!label.contains(forbidden));
            }
        }
        assert_eq!(failure_reason(&state, None), PlaybackFailure::Unknown);
        let state = PlaybackState {
            failure_reason: Some(PlaybackFailure::GpuContextUnavailable),
            ..state
        };
        assert_eq!(
            failure_reason(&state, Some(PlaybackFailure::Unknown)),
            PlaybackFailure::GpuContextUnavailable
        );
    }

    #[test]
    fn startup_failure_preserves_typed_reason_and_safe_phase() {
        for phase in [FailurePhase::Initialize, FailurePhase::Load] {
            let raw = "https://SECRET.invalid/private raw-driver-error";
            let error = kinema_mpv::Error::Library {
                attempts: raw.into(),
            };
            let expected = error.playback_failure();
            let (_, reason, actual_phase) = startup_error(error, phase);
            assert_eq!(reason, expected);
            assert_eq!(actual_phase, phase);
            assert!(!actual_phase.code().contains("SECRET"));
        }
        let state = PlaybackState::default();
        assert_eq!(failure_phase(&state, None), FailurePhase::Runtime);
        assert_eq!(
            failure_phase(&state, Some(FailurePhase::Load)),
            FailurePhase::Load
        );
        let state = PlaybackState {
            failure_reason: Some(PlaybackFailure::Unknown),
            ..state
        };
        assert_eq!(
            failure_phase(&state, Some(FailurePhase::Load)),
            FailurePhase::Runtime
        );
    }

    #[test]
    fn embedded_failure_reports_once_not_each_sample() {
        let mut reported = false;
        assert!(!failure_transition(false, &mut reported));
        assert!(failure_transition(true, &mut reported));
        for _ in 0..120 {
            assert!(!failure_transition(true, &mut reported));
        }
        assert!(!failure_transition(false, &mut reported));
        assert!(!failure_transition(true, &mut reported));
    }

    #[cfg(feature = "gpu-surface")]
    #[test]
    fn absent_renderer_context_has_distinct_startup_reason() {
        let Err((_, reason, phase)) = required_gpu_context::<()>(None) else {
            panic!("missing renderer context must fail closed");
        };
        assert_eq!(reason, PlaybackFailure::GpuContextUnavailable);
        assert_eq!(phase, FailurePhase::Config);
        assert_eq!(required_gpu_context(Some(42)), Ok(42));
        // Unit tests register no native window renderer. This only reads its
        // context registry; it must not create a device or silently fall back.
        let Err((_, reason, phase)) = player_config() else {
            panic!("embedded startup requires the actual window renderer context");
        };
        assert_eq!(reason, PlaybackFailure::GpuContextUnavailable);
        assert_eq!(phase, FailurePhase::Config);
    }

    #[cfg(not(feature = "gpu-surface"))]
    #[test]
    fn embedded_backend_is_explicitly_unavailable_without_feature() {
        let result = super::player_config();
        let Err((error, reason, phase)) = result else {
            panic!("feature-off backend must fail closed");
        };
        assert_eq!(reason, PlaybackFailure::BackendNotCompiled);
        assert_eq!(phase, FailurePhase::Config);
        assert!(error.contains("not compiled"));
        assert!(error.contains("gpu-surface"));
    }

    use super::*;
    use ::core::prelude::v1::test;
    #[test]
    fn performance_opt_in_is_exact_and_categories_are_closed() {
        for value in [
            None,
            Some(""),
            Some("0"),
            Some("true"),
            Some(" 1"),
            Some("1 "),
        ] {
            assert!(!diagnostics_enabled(value));
        }
        assert!(diagnostics_enabled(Some("1")));
        assert_eq!(decoder_category(Some("vaapi-copy")), "vaapi");
        assert_eq!(
            decoder_category(Some("https://secret.invalid/token")),
            "other"
        );
        assert_eq!(renderer_category(Some("llvmpipe (LLVM)")), "software");
        assert_eq!(renderer_category(Some("private driver string")), "other");
    }
    #[test]
    fn performance_rates_use_window_time_and_reject_invalid_time() {
        assert_eq!(window_rate(150, 5.), 30.);
        assert_eq!(window_rate(0, 5.), 0.);
        for seconds in [0., -1., f64::NAN, f64::INFINITY] {
            assert_eq!(window_rate(150, seconds), 0.);
        }
    }
    #[test]
    fn buffered_fractions_drop_invalid_and_clamp_without_widening() {
        use kinema_mpv::BufferedRange as R;
        let r = |start, end| R { start, end };
        let ranges = [
            r(10., 20.),
            r(20., 20.),
            r(30., 10.),
            r(f64::NAN, 5.),
            r(5., f64::INFINITY),
            r(-5., 10.),
            r(90., 200.),
            r(150., 200.),
        ];
        assert_eq!(
            buffered_fractions(Some(&ranges), 100.),
            vec![(0.1, 0.2), (0., 0.1), (0.9, 1.)]
        );
        assert!(buffered_fractions(None, 100.).is_empty());
        assert!(buffered_fractions(Some(&[]), 100.).is_empty());
        assert!(buffered_fractions(Some(&ranges), 0.).is_empty());
        assert!(buffered_fractions(Some(&ranges), f64::NAN).is_empty());
    }
    #[test]
    fn transfer_rate_is_unknown_until_observed_and_ignores_baseline() {
        let t0 = Instant::now();
        let secs = |n| t0 + Duration::from_secs(n);
        let mut rate = TransferRate::default();
        assert_eq!(rate.bytes_per_second(), None);
        rate.observe("a", Some((500_000_000, 1_000_000_000)), t0);
        assert_eq!(rate.bytes_per_second(), None);
        rate.observe("a", Some((500_000_000, 1_000_000_000)), secs(2));
        assert_eq!(rate.bytes_per_second(), Some(0.));
        rate.observe("a", Some((502_000_000, 1_000_000_000)), secs(4));
        assert_eq!(rate.bytes_per_second(), Some(500_000.));
        // Samples older than the window are pruned.
        rate.observe("a", Some((502_000_000, 1_000_000_000)), secs(10));
        assert_eq!(rate.bytes_per_second(), Some(0.));
    }
    #[test]
    fn transfer_rate_resets_on_session_regression_and_missing_progress() {
        let t0 = Instant::now();
        let mut rate = TransferRate::default();
        rate.observe("a", Some((100, 1000)), t0);
        rate.observe(
            "a",
            Some((3_000_100, 5_000_000)),
            t0 + Duration::from_secs(2),
        );
        assert!(rate.bytes_per_second().is_some());
        rate.observe("a", Some((50, 5_000_000)), t0 + Duration::from_secs(3));
        assert_eq!(rate.bytes_per_second(), None);
        rate.observe("a", Some((60, 5_000_000)), t0 + Duration::from_secs(5));
        assert!(rate.bytes_per_second().is_some());
        rate.observe("b", Some((60, 5_000_000)), t0 + Duration::from_secs(6));
        assert_eq!(rate.bytes_per_second(), None);
        rate.observe("b", None, t0 + Duration::from_secs(8));
        assert_eq!(rate.latest, None);
        assert_eq!(download_label(&rate), None);
    }
    #[test]
    fn cadence_counts_playing_rate_and_gap_and_resets_per_interval() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut meter = CadenceMeter::new(t0);
        // Initial loading: no rate, and the first accepted image is only a baseline.
        for n in (16..=300).step_by(16) {
            meter.sample(ms(n), Activity::Waiting, false);
        }
        meter.sample(ms(304), Activity::Playing, true);
        for n in [350, 400, 450, 500] {
            meter.sample(ms(n), Activity::Playing, true);
        }
        meter.sample(ms(900), Activity::Playing, false);
        let Some(report) = meter.finish_if_due(ms(1000)) else {
            unreachable!("interval due")
        };
        assert_eq!(report.accepted, 5);
        assert!(
            report
                .fps
                .is_some_and(|fps| (fps - 5. / 0.696).abs() < 0.01)
        );
        // The open gap since the last image (500 -> 1000) is included.
        assert_eq!(report.max_accept_gap_ms, Some(500.));
        assert_eq!(report.sample_max_gap_ms, 400.);
        assert!(meter.finish_if_due(ms(1500)).is_none());
        let Some(next) = meter.finish_if_due(ms(2000)) else {
            unreachable!("interval due")
        };
        assert_eq!(next.accepted, 0);
        assert_eq!(next.fps, Some(0.));
    }
    #[test]
    fn cadence_pause_buffering_and_reload_are_states_not_regressions() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut meter = CadenceMeter::new(t0);
        meter.sample(ms(100), Activity::Playing, true);
        meter.sample(ms(200), Activity::Paused, false);
        let Some(paused) = meter.finish_if_due(ms(1000)) else {
            unreachable!("interval due")
        };
        assert_eq!(paused.label(), "Video updates: Paused");
        assert_eq!(paused.max_accept_gap_ms, None);
        meter.sample(ms(1100), Activity::Buffering, false);
        meter.sample(ms(1200), Activity::Buffering, false);
        let Some(buffering) = meter.finish_if_due(ms(2000)) else {
            unreachable!("interval due")
        };
        assert_eq!(buffering.fps, None);
        assert_eq!(buffering.cache_paused_samples, 2);
        assert_eq!(buffering.label(), "Video updates: Buffering");
        // Resume: the first image after the idle period must not span the gap.
        meter.sample(ms(2500), Activity::Playing, true);
        meter.sample(ms(2540), Activity::Playing, true);
        let Some(resumed) = meter.finish_if_due(ms(3000)) else {
            unreachable!("interval due")
        };
        assert_eq!(resumed.accepted, 2);
        assert_eq!(resumed.max_accept_gap_ms, Some(460.));
        // A replay passes through Stopped/Waiting and re-baselines.
        meter.sample(ms(3100), Activity::Stopped, false);
        meter.sample(ms(3200), Activity::Waiting, false);
        meter.sample(ms(3300), Activity::Playing, true);
        let Some(replay) = meter.finish_if_due(ms(4000)) else {
            unreachable!("interval due")
        };
        assert!(replay.max_accept_gap_ms.is_some_and(|gap| gap <= 700.));
        assert!(replay.label().starts_with("Video updates: "));
    }
    #[test]
    fn cadence_long_stall_yields_one_honest_report() {
        let t0 = Instant::now();
        let mut meter = CadenceMeter::new(t0);
        meter.sample(t0 + Duration::from_millis(16), Activity::Playing, true);
        meter.sample(t0 + Duration::from_millis(3016), Activity::Playing, true);
        let Some(report) = meter.finish_if_due(t0 + Duration::from_millis(3020)) else {
            unreachable!("interval due")
        };
        assert_eq!(report.accepted, 2);
        assert_eq!(report.max_accept_gap_ms, Some(3000.));
        assert!(report.interval_seconds > 3.);
        assert!(report.fps.is_some_and(|fps| fps < 1.));
        assert!(
            meter
                .finish_if_due(t0 + Duration::from_millis(3100))
                .is_none()
        );
    }
    #[test]
    fn activity_distinguishes_states() {
        let mut state = PlaybackState {
            loading: false,
            paused: false,
            paused_for_cache: false,
            ended: false,
            media_stopped: false,
            ..Default::default()
        };
        assert_eq!(activity_of(&state, true), Activity::Playing);
        state.ended = true;
        assert_eq!(activity_of(&state, true), Activity::Stopped);
        state.ended = false;
        assert_eq!(activity_of(&state, false), Activity::Waiting);
        state.paused = true;
        assert_eq!(activity_of(&state, true), Activity::Paused);
        state.paused_for_cache = true;
        assert_eq!(activity_of(&state, true), Activity::Buffering);
        state.loading = true;
        assert_eq!(activity_of(&state, true), Activity::Waiting);
    }

    #[test]
    fn cadence_counts_view_renders_and_gaps_only_while_playing() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut meter = CadenceMeter::new(t0);
        meter.sample(ms(10), Activity::Playing, false);
        meter.render_called(ms(20));
        meter.render_called(ms(60));
        meter.sample(ms(70), Activity::Playing, false);
        let Some(report) = meter.finish_if_due(ms(1000)) else {
            unreachable!("interval due")
        };
        assert_eq!(report.view_render_calls, 2);
        // The open gap since the last render (60 -> 1000) is included.
        assert_eq!(report.max_view_render_gap_ms, Some(940.));
        meter.sample(ms(1100), Activity::Paused, false);
        meter.render_called(ms(1200));
        let Some(next) = meter.finish_if_due(ms(2000)) else {
            unreachable!("interval due")
        };
        assert_eq!(next.view_render_calls, 1);
        assert_eq!(next.max_view_render_gap_ms, None);
    }
    #[test]
    fn transfer_poll_is_bounded_and_display_key_tracks_changes() {
        let t0 = Instant::now();
        assert!(transfer_poll_due(None, t0));
        assert!(!transfer_poll_due(
            Some(t0),
            t0 + Duration::from_millis(900)
        ));
        assert!(transfer_poll_due(Some(t0), t0 + Duration::from_secs(1)));
        let mut rate = TransferRate::default();
        let empty = rate.display_key();
        rate.observe("a", Some((10, 100)), t0);
        assert_ne!(rate.display_key(), empty);
        rate.clear();
        assert_eq!(rate.display_key(), empty);
    }
    #[test]
    fn buffer_and_download_labels_are_distinct_and_honest() {
        let mut state = PlaybackState {
            position: 10.,
            ..Default::default()
        };
        assert_eq!(buffer_ahead_label(&state), "Buffer —");
        state.buffered_ranges = Some(vec![]);
        assert_eq!(buffer_ahead_label(&state), "Buffer 0s");
        state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange {
            start: 0.,
            end: 145.,
        }]);
        assert_eq!(buffer_ahead_label(&state), "Buffer 2m 15s");
        let mut transfer = TransferRate::default();
        let t0 = Instant::now();
        transfer.observe("a", Some((1_500_000, 4_500_000)), t0);
        assert_eq!(
            download_label(&transfer).as_deref(),
            Some("Downloaded 1.5 MB of 4.5 MB (33%) · —")
        );
        transfer.observe(
            "a",
            Some((2_500_000, 4_500_000)),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(
            download_label(&transfer).as_deref(),
            Some("Downloaded 2.5 MB of 4.5 MB (55%) · 500.0 KB/s")
        );
        assert_eq!(format_bytes(999), "999 B");
        // Position 10: ahead = end - 10, rounded once before splitting.
        for (end, label) in [
            (129.6, "Buffer 2m 00s"),
            (189.9, "Buffer 3m 00s"),
            (129.4, "Buffer 119s"),
            (10. + 180.4, "Buffer 3m 00s"),
        ] {
            state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange { start: 0., end }]);
            assert_eq!(buffer_ahead_label(&state), label);
        }
        let mut huge = TransferRate::default();
        huge.observe("a", Some((u64::MAX, u64::MAX)), t0);
        assert!(download_label(&huge).is_some_and(|label| label.contains("100%")));
    }
    #[test]
    fn compact_status_omits_unknown_buffer_and_finished_downloads() {
        let mut state = PlaybackState {
            position: 10.,
            ..Default::default()
        };
        let mut transfer = TransferRate::default();
        assert!(transfer_items(&state, &transfer).is_empty());
        state.buffered_ranges = Some(vec![kinema_mpv::BufferedRange {
            start: 0.,
            end: 403.,
        }]);
        let t0 = Instant::now();
        transfer.observe("a", Some((1_500_000, 4_500_000)), t0);
        transfer.observe(
            "a",
            Some((2_500_000, 4_500_000)),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(
            transfer_items(&state, &transfer),
            vec![
                StatusItem::icon(IconName::Timer, "6m 33s"),
                StatusItem::icon(IconName::Download, "55% · 500.0 KB/s"),
            ]
        );
        // Complete download: no "100% · 0 B/s" noise.
        transfer.observe(
            "a",
            Some((4_500_000, 4_500_000)),
            t0 + Duration::from_secs(4),
        );
        assert_eq!(
            transfer_items(&state, &transfer),
            vec![StatusItem::icon(IconName::Timer, "6m 33s")]
        );
        // Unknown total keeps byte count; no fabricated percentage.
        let mut unknown = TransferRate::default();
        unknown.observe("b", Some((1_500_000, 0)), t0);
        state.buffered_ranges = None;
        assert_eq!(
            transfer_items(&state, &unknown),
            vec![StatusItem::icon(IconName::Download, "1.5 MB")]
        );
        state.ended = true;
        assert!(transfer_items(&state, &unknown).is_empty());
    }
    #[test]
    fn performance_cache_preserves_unknown_empty_and_containing_ranges() {
        use kinema_mpv::BufferedRange;
        let mut state = PlaybackState {
            position: 10.,
            ..Default::default()
        };
        assert_eq!(cache_ahead(&state), None);
        state.buffered_ranges = Some(vec![]);
        assert_eq!(cache_ahead(&state), Some(0.));
        state.buffered_ranges = Some(vec![
            BufferedRange {
                start: 15.,
                end: 20.,
            },
            BufferedRange {
                start: 25.,
                end: 30.,
            },
            BufferedRange {
                start: 8.,
                end: 15.,
            },
            BufferedRange {
                start: f64::NAN,
                end: 99.,
            },
            BufferedRange {
                start: 9.,
                end: f64::INFINITY,
            },
        ]);
        // The adjacent 15..20 range is intentionally not merged.
        assert_eq!(cache_ahead(&state), Some(5.));
        state.position = 22.;
        assert_eq!(cache_ahead(&state), Some(0.));
        state.position = f64::NAN;
        assert_eq!(cache_ahead(&state), None);
    }
    #[test]
    fn time_is_safe_and_handles_hours() {
        assert_eq!(format_clock(f64::NAN), "0:00");
        assert_eq!(format_clock(-2.), "0:00");
        assert_eq!(format_clock(3661.9), "1:01:01");
    }
    #[test]
    fn timeline_rejects_unknown_and_invalid_duration() {
        for duration in [
            None,
            Some(0.),
            Some(-1.),
            Some(f64::NAN),
            Some(f64::INFINITY),
        ] {
            assert_eq!(progress(1., duration), None);
        }
        assert_eq!(progress(200., Some(100.)), Some(1.));
        assert_eq!(progress(-1., Some(100.)), Some(0.));
    }
    #[test]
    fn disabled_controls_explain_themselves() {
        let a = Availability {
            backend: true,
            seekable: true,
            chapters: false,
            subtitles: false,
            audio: false,
            chapters_known: true,
            tracks_known: true,
        };
        assert_eq!(disabled_reason(PLAY, a), None);
        assert_eq!(
            disabled_reason(SUBTITLES, a),
            Some("No subtitle tracks in this media")
        );
        assert_eq!(
            disabled_reason(CHAPTERS, a),
            Some("No chapters in this media")
        );
        let unknown = Availability {
            chapters_known: false,
            tracks_known: false,
            ..a
        };
        assert_eq!(
            disabled_reason(SUBTITLES, unknown),
            Some("Subtitle tracks unavailable")
        );
        assert_eq!(
            disabled_reason(AUDIO, unknown),
            Some("Audio tracks unavailable")
        );
        assert_eq!(
            disabled_reason(CHAPTERS, unknown),
            Some("Chapter information unavailable")
        );
        let off = Availability {
            backend: false,
            ..a
        };
        assert_eq!(disabled_reason(AUDIO, off), Some("Player is not ready"));
        assert_eq!(disabled_reason(CLOSE, off), None);
    }
    #[test]
    fn hides_after_approved_idle_delay() {
        let idle = |ms| controls_visible(false, false, false, false, Duration::from_millis(ms));
        assert!(idle(2400));
        assert!(!idle(2500));
    }
    #[test]
    fn hides_only_while_playing_and_idle() {
        let idle = Duration::from_secs(4);
        assert!(!controls_visible(false, false, false, false, idle));
        assert!(controls_visible(true, false, false, false, idle));
        assert!(controls_visible(false, true, false, false, idle));
        assert!(controls_visible(false, false, true, false, idle));
        assert!(controls_visible(false, false, false, true, idle));
    }
    #[test]
    fn idle_and_paused_ticks_do_not_redraw() {
        assert!(!redraw_needed(false, false, true, true));
        assert!(!redraw_needed(false, false, false, false));
        assert!(redraw_needed(false, false, true, false));
        assert!(redraw_needed(true, false, true, true));
        assert!(redraw_needed(false, true, false, false));
    }
    #[test]
    fn disabled_controls_are_skipped() {
        let enabled = |availability| {
            (0..CONTROL_COUNT)
                .filter(|i| control_enabled(*i, availability))
                .collect::<Vec<_>>()
        };
        let none = Availability {
            backend: false,
            seekable: false,
            chapters: true,
            subtitles: true,
            audio: true,
            chapters_known: true,
            tracks_known: true,
        };
        assert_eq!(enabled(none), vec![CLOSE, FULLSCREEN, HELP]);
        let live = Availability {
            backend: true,
            ..none
        };
        assert_eq!(
            enabled(live),
            vec![
                CLOSE, PLAY, SUBTITLES, AUDIO, SPEED, FULLSCREEN, MUTE, VOLUME, HELP
            ]
        );
        let seekable = Availability {
            seekable: true,
            ..live
        };
        assert_eq!(enabled(seekable), (0..CONTROL_COUNT).collect::<Vec<_>>());
        let bare = Availability {
            chapters: false,
            subtitles: false,
            audio: false,
            chapters_known: true,
            tracks_known: true,
            ..seekable
        };
        assert!(
            !control_enabled(CHAPTERS, bare)
                && !control_enabled(SUBTITLES, bare)
                && !control_enabled(AUDIO, bare)
        );
    }
    #[test]
    fn recoverable_control_error_keeps_player_and_lease() {
        let mut state = PlaybackState {
            media_stopped: false,
            error: Some("queue error".into()),
            ..Default::default()
        };
        assert!(!terminal_failure(false, &state));
        state.media_stopped = true;
        assert!(terminal_failure(false, &state));
        state.error = None;
        state.ended = true;
        assert!(!terminal_failure(false, &state));
        assert!(replay_needed(&state));
        state.stopped = true;
        assert!(terminal_failure(false, &state));
        assert!(!replay_needed(&state));
    }
}

#[cfg(all(test, feature = "ui-probe-test"))]
mod input_tests {
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
}

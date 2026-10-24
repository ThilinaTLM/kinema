//! Separate, lease-owned Cinema player. libmpv initialization and destruction
//! (including worker joins) never run on GPUI's foreground executor.
use std::{
    collections::{HashMap, VecDeque},
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use crate::player::PlayerChoice;
use gpui::{prelude::*, *};
use kinema_mpv::{
    Config, Frame, PlaybackFailure, PlaybackState, Player, ResolutionPolicy, TrackKind,
};

use super::{
    coordinator::AppCoordinator,
    player_controls::{
        self as dock, Scrub, VOLUME_STEP, chapter_index, chapter_label, chapter_ticks,
        displayed_position, displayed_volume, format_clock, format_remaining, ratio_in_span,
        scrub_settled, speed_label, step_volume, subtitle_position, track_label, volume_at,
        volume_settled,
    },
};
use crate::widgets::{
    button::{Button, ButtonVariant},
    icon::IconName,
    menu::{Menu, MenuItem},
    text::TypeExt,
    tokens::{Palette, radius, space as s, typeface as t},
};

#[cfg(feature = "experimental-gpu")]
type ExperimentalSurfaceFrame = GpuSurfaceFrame;

fn experimental_help() -> &'static str {
    if cfg!(feature = "experimental-gpu") {
        "Experimental GPU playback unavailable. No CPU fallback or device-loss recovery. Close and reopen the window, or restart Kinema; you can also choose another player in Settings → Playback. Required SDR GPU interop capabilities may be missing. HDR deferred; Windows/macOS runtime qualification pending."
    } else {
        "Experimental GPU backend not compiled. Use the isolated experimental app build, or choose Embedded MPV, MPV, or VLC in Settings → Playback."
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FailurePhase {
    Config,
    Initialize,
    Load,
    Runtime,
    Command,
    #[cfg(feature = "experimental-gpu")]
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
            #[cfg(feature = "experimental-gpu")]
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

#[cfg(feature = "experimental-gpu")]
fn required_gpu_context<T>(context: Option<T>) -> Result<T, StartupError> {
    context.ok_or_else(|| {
        (
            experimental_help().into(),
            PlaybackFailure::GpuContextUnavailable,
            FailurePhase::Config,
        )
    })
}

fn player_config(mode: &PlayerChoice) -> Result<Config, StartupError> {
    if *mode != PlayerChoice::ExperimentalEmbedded {
        return Ok(Config {
            resolution: ResolutionPolicy::Viewport,
            ..Default::default()
        });
    }
    #[cfg(not(feature = "experimental-gpu"))]
    {
        Err((
            experimental_help().into(),
            PlaybackFailure::BackendNotCompiled,
            FailurePhase::Config,
        ))
    }
    #[cfg(feature = "experimental-gpu")]
    {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let context = {
            let context = required_gpu_context(gpui_wgpu::experimental_gpu_context())?;
            let gate = context.experimental_gpu_queue_gate();
            kinema_mpv::ExperimentalGpuContext::new(
                context.device.clone(),
                context.queue.clone(),
                gate,
            )
        };
        #[cfg(target_os = "macos")]
        let context = kinema_mpv::ExperimentalGpuContext::new_metal(required_gpu_context(
            gpui_apple::experimental_gpu_registry_id(),
        )?);
        Ok(Config {
            resolution: ResolutionPolicy::Source,
            experimental_gpu: Some(context),
            ..Default::default()
        })
    }
}

#[cfg(feature = "experimental-gpu")]
fn experimental_surface(
    frame: kinema_mpv::ExperimentalFrame,
) -> Result<Arc<ExperimentalSurfaceFrame>, String> {
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
        .map_err(|_| "Invalid experimental GPU surface; refusing CPU fallback".into())
}

actions!(
    player_window,
    [
        TogglePlayback,
        SeekBack,
        SeekForward,
        ToggleMute,
        ToggleFullscreen,
        Escape,
        VolumeUp,
        VolumeDown,
        NextControl,
        PreviousControl
    ]
);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        // Space belongs to focused buttons when navigating the dock; K is the
        // unambiguous playback shortcut everywhere in the player window.
        KeyBinding::new("space", TogglePlayback, Some("KinemaPlayerSurface")),
        KeyBinding::new("k", TogglePlayback, Some("KinemaPlayer")),
        KeyBinding::new("left", SeekBack, Some("KinemaPlayer")),
        KeyBinding::new("right", SeekForward, Some("KinemaPlayer")),
        KeyBinding::new("m", ToggleMute, Some("KinemaPlayer")),
        KeyBinding::new("f", ToggleFullscreen, Some("KinemaPlayer")),
        KeyBinding::new("escape", Escape, Some("KinemaPlayer")),
        KeyBinding::new("up", VolumeUp, Some("KinemaPlayer")),
        KeyBinding::new("down", VolumeDown, Some("KinemaPlayer")),
        KeyBinding::new("tab", NextControl, Some("KinemaPlayer")),
        KeyBinding::new("shift-tab", PreviousControl, Some("KinemaPlayer")),
    ]);
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
const CONTROL_COUNT: usize = 12;
/// Chrome auto-hides after this much inactivity while playing (approved design).
const CHROME_IDLE: Duration = Duration::from_millis(2500);
/// Dock height assumed until its first layout is measured.
/// Approved Cinema Dock content insets (design `.dock{left/right:28px;bottom:23px}`).
const DOCK_SIDE_INSET: Pixels = px(28.);
const DOCK_BOTTOM_INSET: Pixels = px(23.);
/// Height of the transparent-to-scrim fade above the dock backing (decoration only).
const DOCK_FADE: Pixels = px(72.);
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
}
impl Global for PlayerWindows {}
struct LeaseWindow {
    generation: u64,
    handle: WindowHandle<PlayerWindow>,
    coordinator: Entity<AppCoordinator>,
    mode: PlayerChoice,
}

/// Replaying a lease activates its existing window without reloading its URL.
pub fn open(
    session_id: String,
    mode: PlayerChoice,
    url: String,
    title: String,
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
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(1100.), px(680.)),
            cx,
        ))),
        window_min_size: Some(size(px(720.), px(440.))),
        titlebar: Some(TitlebarOptions {
            title: Some(format!("{title} — Kinema").into()),
            ..Default::default()
        }),
        ..Default::default()
    };
    let handle = cx
        .open_window(options, |window, cx| {
            cx.new(|cx| PlayerWindow::new_mode(title, Some(url), mode.clone(), window, cx))
        })
        .map_err(|error| format!("Could not open embedded player window: {error}"))?;
    let _ = handle.update(cx, |view, _, _| {
        view.lease = Some((session_id.clone(), generation))
    });
    cx.global_mut::<PlayerWindows>().entries.insert(
        session_id,
        LeaseWindow {
            generation,
            handle,
            coordinator,
            mode,
        },
    );
    Ok(())
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
        entry.coordinator.update(cx, |coordinator, cx| {
            if failed {
                coordinator.embedded_player_failed(session_id.to_owned(), cx);
            } else {
                coordinator.embedded_player_closed(session_id.to_owned(), cx);
            }
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
        let _ = handle.update(cx, |_, window, _| window.remove_window());
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
fn buffer_ahead_label(state: &PlaybackState) -> String {
    match cache_ahead(state) {
        None => "Buffer —".into(),
        Some(seconds) => {
            // Round once, then split, so 179.9 s is 3m 00s rather than 2m 60s.
            let total = seconds.max(0.).round() as u64;
            if total < 120 {
                format!("Buffer {total}s")
            } else {
                format!("Buffer {}m {:02}s", total / 60, total % 60)
            }
        }
    }
}

/// Downloaded file bytes (not MPV's playable cache), labelled as such.
fn download_label(transfer: &TransferRate, compact: bool) -> Option<String> {
    let (downloaded, total) = transfer.latest?;
    let speed = transfer.bytes_per_second().map_or_else(
        || "—".to_owned(),
        |rate| format!("{}/s", format_bytes(rate as u64)),
    );
    let percent =
        (total > 0).then(|| (u128::from(downloaded.min(total)) * 100 / u128::from(total)) as u64);
    Some(match (compact, percent) {
        (true, Some(p)) => format!("Download {speed} · {p}%"),
        (true, None) => format!("Download {speed} · {}", format_bytes(downloaded)),
        (false, Some(p)) => format!(
            "Downloaded {} of {} ({p}%) · {speed}",
            format_bytes(downloaded),
            format_bytes(total)
        ),
        (false, None) => format!("Downloaded {} · {speed}", format_bytes(downloaded)),
    })
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
    retirement_skips: u64,
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
    retirement_skips: u64,
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
            retirement_skips: 0,
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
    fn sample(&mut self, now: Instant, activity: Activity, accepted: bool, skipped: bool) {
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
        self.retirement_skips += u64::from(skipped);
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
            retirement_skips: self.retirement_skips,
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
        self.retirement_skips = 0;
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
            samples_skipped_retiring = report.retirement_skips,
            samples_paused_for_cache = report.cache_paused_samples,
            view_render_calls = report.view_render_calls,
            max_view_render_gap_ms = report.max_view_render_gap_ms,
            activity = ?report.activity,
            "Embedded player cadence"
        );
    }
}

/// Old images allowed to wait for their two GPUI frame boundaries at once, in
/// addition to the one currently displayed.
const RETIRE_CAPACITY: usize = 2;

#[derive(Clone, Copy, Default)]
struct Aggregate {
    count: u64,
    sum_ms: f64,
    max_ms: f64,
}
impl Aggregate {
    fn record(&mut self, elapsed: Duration) {
        let ms = elapsed.as_secs_f64() * 1000.;
        self.count += 1;
        self.sum_ms += ms;
        self.max_ms = self.max_ms.max(ms);
    }
    fn mean_ms(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum_ms / self.count as f64)
    }
}

#[derive(Clone, Copy)]
struct PendingRetirement {
    ticket: u64,
    started: Instant,
    first: Option<Instant>,
}

/// Per-window retirement statistics (`first` = start -> first callback,
/// `complete` = start -> `drop_image`). Callback timing, not GPU fences.
struct RetirementWindow {
    high_water: usize,
    first: Aggregate,
    complete: Aggregate,
}

/// Bounded ticket accounting for images waiting on the nested two-frame
/// retirement. Each completion removes only its own ticket, so a stale or
/// duplicate completion can never release another image's slot. The images
/// themselves stay owned by their callback closures.
struct RetirementQueue {
    next: u64,
    pending: Vec<PendingRetirement>,
    high_water: usize,
    first: Aggregate,
    complete: Aggregate,
}
impl RetirementQueue {
    fn new() -> Self {
        Self {
            next: 0,
            pending: Vec::with_capacity(RETIRE_CAPACITY),
            high_water: 0,
            first: Aggregate::default(),
            complete: Aggregate::default(),
        }
    }
    fn has_room(&self) -> bool {
        // Ticket exhaustion also closes the queue: tickets are never reused.
        self.pending.len() < RETIRE_CAPACITY && self.next < u64::MAX
    }
    fn inflight(&self) -> usize {
        self.pending.len()
    }
    /// A fresh non-zero ticket, or `None` when the queue is full.
    fn admit(&mut self, now: Instant) -> Option<u64> {
        if !self.has_room() {
            return None;
        }
        let ticket = self.next.checked_add(1)?;
        self.next = ticket;
        self.pending.push(PendingRetirement {
            ticket,
            started: now,
            first: None,
        });
        self.high_water = self.high_water.max(self.pending.len());
        Some(ticket)
    }
    /// Record the first boundary for a still-pending ticket (once).
    fn first_callback(&mut self, ticket: u64, now: Instant) -> bool {
        let Some(entry) = self
            .pending
            .iter_mut()
            .find(|entry| entry.ticket == ticket && entry.first.is_none())
        else {
            return false;
        };
        entry.first = Some(now);
        self.first
            .record(now.saturating_duration_since(entry.started));
        true
    }
    /// Release exactly `ticket`; unknown/duplicate tickets change nothing.
    fn complete(&mut self, ticket: u64, now: Instant) -> bool {
        let Some(index) = self.pending.iter().position(|entry| entry.ticket == ticket) else {
            return false;
        };
        let entry = self.pending.remove(index);
        self.complete
            .record(now.saturating_duration_since(entry.started));
        true
    }
    fn take_window(&mut self) -> RetirementWindow {
        let window = RetirementWindow {
            high_water: self.high_water,
            first: self.first,
            complete: self.complete,
        };
        self.high_water = self.pending.len();
        self.first = Aggregate::default();
        self.complete = Aggregate::default();
        window
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
    retirement_skips: u64,
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
            retirement_skips: 0,
            paused_for_cache_samples: 0,
            sample_ms: 0.,
        }
    }

    fn report(
        &mut self,
        now: Instant,
        backend: &kinema_mpv::RenderDiagnostics,
        state: &PlaybackState,
        retirement: &mut RetirementQueue,
    ) {
        let inflight = retirement.inflight();
        let retired = retirement.take_window();
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
            window_samples_skipped_retiring = self.retirement_skips,
            window_samples_paused_for_cache = self.paused_for_cache_samples,
            window_sample_max_gap_ms = self.max_sample_gap_ms,
            window_sample_mean_ms = self.sample_ms / self.samples.max(1) as f64,
            retiring = inflight > 0,
            retirement_capacity = RETIRE_CAPACITY,
            retirement_inflight = inflight,
            retirement_inflight_high_water = retired.high_water,
            retirement_first_callback_count = retired.first.count,
            retirement_first_callback_mean_ms = retired.first.mean_ms(),
            retirement_first_callback_max_ms = retired.first.max_ms,
            retirement_complete_count = retired.complete.count,
            retirement_complete_mean_ms = retired.complete.mean_ms(),
            retirement_complete_max_ms = retired.complete.max_ms,
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
        self.retirement_skips = 0;
        self.paused_for_cache_samples = 0;
        self.sample_ms = 0.;
        self.max_sample_gap_ms = 0.;
    }
}

pub struct PlayerWindow {
    title: String,
    lease: Option<(String, u64)>,
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
    image: Option<Arc<RenderImage>>,
    mode: PlayerChoice,
    #[cfg(feature = "experimental-gpu")]
    gpu_frame: Option<Arc<ExperimentalSurfaceFrame>>,
    retirement: RetirementQueue,
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
        #[cfg(feature = "experimental-gpu")]
        {
            self.gpu_frame.take();
        }
        self.player.take();
    }
}

impl PlayerWindow {
    fn new_mode(
        title: String,
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
        let experimental = mode == PlayerChoice::ExperimentalEmbedded;
        let mut view = Self {
            title,
            lease: None,
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
            image: None,
            mode,
            #[cfg(feature = "experimental-gpu")]
            gpu_frame: None,
            retirement: RetirementQueue::new(),
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
            dock_height: Pixels::default(),
            transfer: TransferRate::default(),
            last_transfer_poll: None,
            subtitle_position: None,
            startup_cancel,
            _initialize: Task::ready(()),
            _updates: updates,
        };
        if experimental {
            tracing::info!(
                mode = "experimental-embedded",
                phase = "requested",
                "Experimental GPU startup requested"
            );
            // Capture the actual renderer context only after native window
            // registration, not after an assumed amount of wall-clock time.
            cx.defer_in(window, |view, _, cx| view.start_player(cx));
        } else {
            view.start_player(cx);
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
        let config = player_config(&self.mode);
        let url = self.source_url.clone();
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
                    Ok(player) => view.player = player,
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

    fn experimental_video(&self) -> AnyElement {
        let element = div().absolute().size_full();
        #[cfg(feature = "experimental-gpu")]
        let element = element.when_some(self.gpu_frame.clone(), |element, frame| {
            element.child(
                gpu_surface(frame)
                    .size_full()
                    .object_fit(ObjectFit::Contain),
            )
        });
        element.into_any_element()
    }

    fn sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sample_started = self.diagnostics.as_ref().map(|_| Instant::now());
        let mut state_changed = false;
        let mut frame_received = false;
        let mut received = false;
        let mut retirement_skipped = false;
        #[cfg(feature = "experimental-gpu")]
        if terminal_failure(self.error.is_some(), &self.state) {
            frame_received |= self.gpu_frame.take().is_some();
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
            // Queue full (not merely "one pending") skips acquisition; no extra
            // image is allocated and the backend keeps its latest-frame mailbox.
            #[cfg(feature = "experimental-gpu")]
            if self.mode == PlayerChoice::ExperimentalEmbedded
                && !terminal_failure(self.error.is_some(), &self.state)
                && let Some(frame) = player.take_gpu_frame()
            {
                match experimental_surface(frame) {
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
            let frame = if self.mode == PlayerChoice::Embedded && self.retirement.has_room() {
                player.take_frame()
            } else {
                retirement_skipped = self.mode == PlayerChoice::Embedded;
                None
            };
            received = frame.is_some();
            if let Some(frame) = frame.and_then(render_image) {
                // Admit BEFORE displacing the current image. An old image is
                // only ever replaced with a tracked ticket; if admission fails
                // (unreachable after the room check) keep the current image and
                // drop the new, not-yet-uploaded one. Fail closed.
                let admitted = self
                    .image
                    .is_some()
                    .then(|| self.retirement.admit(Instant::now()));
                if admitted == Some(None) {
                    retirement_skipped = true;
                } else {
                    self.frames_presented += 1;
                    let old = self.image.replace(frame);
                    if let (Some(old), Some(Some(ticket))) = (old, admitted) {
                        retire_image(
                            old,
                            cx.weak_entity(),
                            window,
                            ticket,
                            self.diagnostics.is_some(),
                        );
                    }
                    frame_received = true;
                }
            }
            state_changed |= self.state != previous;
        }
        // A fresh state snapshot or surface-validation error can become fatal
        // within this sample. Probes have no lease but must also release frames.
        #[cfg(feature = "experimental-gpu")]
        if terminal_failure(self.error.is_some(), &self.state) {
            frame_received |= self.gpu_frame.take().is_some();
        }
        if self.mode == PlayerChoice::ExperimentalEmbedded
            && failure_transition(
                terminal_failure(self.error.is_some(), &self.state),
                &mut self.failure_reported,
            )
        {
            let reason = failure_reason(&self.state, self.failure_reason);
            tracing::warn!(
                mode = "experimental-embedded",
                phase = failure_phase(&self.state, self.failure_phase).code(),
                reason = reason.code(),
                "Experimental GPU playback unavailable"
            );
        }
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
            meter.sample(Instant::now(), activity, frame_received, retirement_skipped);
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
            metrics.received += u64::from(received);
            metrics.presented += u64::from(frame_received);
            metrics.retirement_skips += u64::from(retirement_skipped);
            metrics.paused_for_cache_samples += u64::from(self.state.paused_for_cache);
            metrics.sample_ms += now.duration_since(started).as_secs_f64() * 1000.;
            if now.duration_since(metrics.last_report) >= Duration::from_secs(5)
                && let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref())
            {
                metrics.report(
                    now,
                    &player.diagnostics(),
                    &self.state,
                    &mut self.retirement,
                );
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
        self.activity(cx);
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
        } else {
            self.control_notice = false;
        }
    }
    fn toggle_playback(&mut self, cx: &mut Context<Self>) {
        if replay_needed(&self.state)
            && let Some(url) = self.source_url.clone()
        {
            // A new load: discard the previous GPU generation before issuing
            // load. The backend rejects late frames from the obsolete source.
            #[cfg(feature = "experimental-gpu")]
            {
                self.gpu_frame.take();
            }
            // Do not assume the previous sub-pos survives.
            self.subtitle_position = None;
            self.command(
                |player| {
                    player.load(&url)?;
                    player.set_paused(false)
                },
                cx,
            );
        } else {
            self.command(Player::toggle_pause, cx);
        }
    }
    fn seek(&mut self, seconds: f64, cx: &mut Context<Self>) {
        if progress(self.state.position, self.state.duration).is_some() {
            self.command(|player| player.seek(seconds), cx);
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
            self.command(|player| player.seek_absolute(target), cx);
        }
    }
    fn menu_open(&self) -> bool {
        self.menu.is_some()
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
    fn toggle_menu(&mut self, kind: MenuKind, window: &mut Window, cx: &mut Context<Self>) {
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
        let menu = cx.new(|cx| Menu::new(items, window, cx));
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
        self.backend_available() && progress(self.state.position, self.state.duration).is_some()
    }
    fn chrome_visible(&self, window: &Window) -> bool {
        controls_visible(
            self.state.paused || self.state.ended,
            self.state.loading || self.state.paused_for_cache || self.player.is_none(),
            self.error.is_some() || self.control_notice || self.state.error.is_some(),
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
    fn volume_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = Palette::dark();
        let entity = cx.entity();
        let enabled = self.backend_available();
        let volume = displayed_volume(self.volume_requested, self.state.volume, Instant::now());
        let fraction = (volume / 100.).clamp(0., 1.) as f32;
        let fill = if self.state.muted {
            p.text_muted
        } else {
            p.accent
        };
        let slider = div()
            .id("player-volume")
            .aria_label(format!(
                "Volume, {volume:.0} percent{}. Up and down arrows change volume by {VOLUME_STEP:.0}.",
                if self.state.muted { ", muted" } else { "" }
            ))
            .key_context("KinemaPlayerVolume")
            .track_focus(&self.controls[VOLUME])
            .tab_stop(enabled)
            .relative()
            .w(s::S32)
            .h(px(100.))
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
                    .w(s::S8)
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
                    .child(
                        div()
                            .absolute()
                            .top(relative(1. - fraction))
                            .mt(px(-7.))
                            .ml(px(-3.))
                            .size(px(14.))
                            .rounded_full()
                            .bg(p.text_primary),
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
            .absolute()
            .right(s::S20)
            .top(relative(0.5))
            .mt(px(-98.))
            .flex()
            .flex_col()
            .items_center()
            .gap(s::S6)
            .p(s::S8)
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
                    .type_style(t::MONO)
                    .text_color(p.text_secondary)
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
                    if self.state.muted {
                        "Unmute (M)"
                    } else {
                        "Mute (M)"
                    },
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
                    self.buffer_summary(false)
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
    fn buffer_summary(&self, compact: bool) -> String {
        if self.state.loading || self.state.ended {
            return String::new();
        }
        let mut parts = vec![buffer_ahead_label(&self.state)];
        parts.extend(download_label(&self.transfer, compact));
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
        let notice = {
            if self.state.paused_for_cache {
                "Buffering…"
            } else if self.control_notice
                || (self.state.error.is_some() && !terminal_failure(false, &self.state))
            {
                "Control unavailable — try again"
            } else if self.state.ended {
                "Finished · Replay to watch again"
            } else if !seekable && !self.state.loading {
                "Seeking unavailable"
            } else {
                ""
            }
        };
        let buffer = self.buffer_summary(f32::from(viewport.width) < 960.);
        let status = [notice, buffer.as_str()]
            .into_iter()
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" · ");
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
        let utilities =
            div()
                .relative()
                .flex_none()
                .flex()
                .items_center()
                .gap(s::S4)
                .child(slot(
                    s::S32,
                    self.control(CHAPTERS, "player-chapters", IconName::Menu, "Chapters")
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.toggle_menu(MenuKind::Chapters, window, cx)
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
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_menu(MenuKind::Subtitles, window, cx)
                    })),
                ))
                .child(slot(
                    s::S48 + s::S12,
                    self.text_control(AUDIO, "player-audio", "Audio", "Audio track", false)
                        .on_click(cx.listener(|view, _, window, cx| {
                            view.toggle_menu(MenuKind::Audio, window, cx)
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
                    .on_click(cx.listener(|view, _, window, cx| {
                        view.toggle_menu(MenuKind::Speed, window, cx)
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
                        "Toggle fullscreen (F)",
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
                div()
                    .flex()
                    .items_center()
                    .gap(s::S12)
                    .p(s::S16)
                    .bg(p.scrim_max)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .type_style(t::BODY_STRONG)
                            .truncate()
                            .child(self.title.clone()),
                    )
                    .child(
                        div()
                            .type_style(t::CAPTION)
                            .text_color(p.text_muted)
                            .child("KINEMA / PLAYER"),
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
                                            if self.state.ended {
                                                "Replay (K)"
                                            } else if self.state.paused {
                                                "Play (K)"
                                            } else {
                                                "Pause (K)"
                                            },
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
                                            "Back 10 seconds (←)",
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
                                            "Forward 10 seconds (→)",
                                        )
                                        .on_click(cx.listener(|view, _, _, cx| view.seek(10., cx))),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .w(px(0.))
                                            .min_w_0()
                                            .truncate()
                                            .px(s::S8)
                                            .type_style(t::SMALL)
                                            .text_color(p.text_secondary)
                                            .child(status),
                                    )
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
            .child(self.volume_panel(cx))
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
        let has_video = self.image.is_some();
        #[cfg(feature = "experimental-gpu")]
        let has_video = has_video || self.gpu_frame.is_some();
        div().id("kinema-player").key_context("KinemaPlayer").relative().size_full().overflow_hidden().bg(p.bg_canvas).text_color(p.text_primary).font_family(t::UI_FAMILY)
            // While a menu is open every player shortcut is passed through
            // (`propagate`) so the menu's own key handling sees the keystroke.
            .on_action(cx.listener(|view, _: &TogglePlayback, _, cx| if view.menu_open() { cx.propagate() } else { view.toggle_playback(cx) }))
            .on_action(cx.listener(|view, _: &SeekBack, _, cx| if view.menu_open() { cx.propagate() } else { view.seek(-10., cx) }))
            .on_action(cx.listener(|view, _: &SeekForward, _, cx| if view.menu_open() { cx.propagate() } else { view.seek(10., cx) }))
            .on_action(cx.listener(|view, _: &ToggleMute, _, cx| if view.menu_open() { cx.propagate() } else { view.command(Player::toggle_mute, cx) }))
            .on_action(cx.listener(|view, _: &VolumeUp, _, cx| if view.menu_open() { cx.propagate() } else { view.nudge_volume(VOLUME_STEP, cx) }))
            .on_action(cx.listener(|view, _: &VolumeDown, _, cx| if view.menu_open() { cx.propagate() } else { view.nudge_volume(-VOLUME_STEP, cx) }))
            .on_action(cx.listener(|view, _: &ToggleFullscreen, window, cx| if view.menu_open() { cx.propagate() } else { window.toggle_fullscreen(); view.activity(cx); }))
            // Escape: an open menu dismisses itself first; otherwise it cancels an
            // active drag, and only then leaves fullscreen.
            .on_action(cx.listener(|view, _: &Escape, window, cx| { if view.menu_open() { cx.propagate(); } else { if !view.cancel_drag(cx) && window.is_fullscreen() { window.toggle_fullscreen(); } view.activity(cx); } }))
            .on_action(cx.listener(|view, _: &NextControl, window, cx| if view.menu_open() { cx.propagate() } else { view.focus_control(false, window, cx) }))
            .on_action(cx.listener(|view, _: &PreviousControl, window, cx| if view.menu_open() { cx.propagate() } else { view.focus_control(true, window, cx) }))
            .on_mouse_move(cx.listener(|view, event: &MouseMoveEvent, _, cx| view.pointer_moved(event, cx)))
            .on_mouse_up(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| view.finish_drag(cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|view, _: &MouseUpEvent, _, cx| view.finish_drag(cx)))
            .child(div().id("player-surface").key_context("KinemaPlayerSurface").track_focus(&self.surface_focus).absolute().size_full()
                .on_mouse_down(MouseButton::Left, cx.listener(|view, _, window, cx| { view.surface_focus.focus(window, cx); view.activity(cx); }))
                .when_some(self.image.clone(), |div, image| div.child(img(image).absolute().size_full().object_fit(ObjectFit::Contain))))
            .when(self.mode == PlayerChoice::ExperimentalEmbedded && !failed, |element| element.child(self.experimental_video()))
            .when(failed || !has_video, |element| element.child(
                div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(s::S12).when(failed, |panel| panel.bg(p.scrim_max))
                    .child(div().type_style(t::TITLE).child(if failed { "Playback unavailable" } else if self.state.ended { "Playback finished" } else if self.state.paused_for_cache { "Buffering…" } else if self.state.loading || self.player.is_none() { "Opening media…" } else { "Audio playback" }))
                    .child(div().max_w(s::SHEET_WIDTH).px(s::S24).type_style(t::BODY).text_color(p.text_secondary).child(if failed && self.mode == PlayerChoice::ExperimentalEmbedded { experimental_help() } else if failed { "Playback stopped. Choose external MPV or VLC in Settings → Playback, then try again. If libmpv is missing, install its runtime first." } else if self.state.loading || self.player.is_none() { "Preparing the embedded player. You can close this window at any time." } else { "Use the controls below. Press K to play or pause." }))
                    .when(failed && self.mode == PlayerChoice::ExperimentalEmbedded, |panel| panel.child(
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
            .when(self.mode == PlayerChoice::ExperimentalEmbedded, |element| element.child(
                div().absolute().top(s::S16).left(s::S16).px(s::S8).py(s::S4)
                    .rounded(radius::SM).bg(p.scrim_max.opacity(0.7)).type_style(t::MONO)
                    .text_color(p.text_secondary)
                    .child(format!("Experimental GPU · SDR · {}×{} · no CPU fallback", self.state.render_size[0], self.state.render_size[1]))
            ))
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

/// next_frame_callbacks run BEFORE should_draw in gpui-pre 0.3.9. WGPU
/// atlas removal deallocates/reuses tiles immediately. Each old image (at most
/// `RETIRE_CAPACITY` at once) is owned by its closure through two nested frame
/// boundaries: the first requests a draw without it
/// (GPUI callbacks do not prove `should_draw` ran), the second removes it. This
/// is a conservative bounded policy, not a GPU-fence proof for every OS.
fn retire_image(
    image: Arc<RenderImage>,
    view: WeakEntity<PlayerWindow>,
    window: &mut Window,
    ticket: u64,
    timed: bool,
) {
    window.on_next_frame(move |window, cx| {
        window.refresh();
        if timed {
            let _ = view.update(cx, |view, _| {
                view.retirement.first_callback(ticket, Instant::now());
            });
        }
        window.on_next_frame(move |window, cx| {
            let _ = window.drop_image(image);
            let _ = view.update(cx, |view, cx| {
                view.retirement.complete(ticket, Instant::now());
                cx.notify();
            });
        });
    });
}
fn render_image(frame: Frame) -> Option<Arc<RenderImage>> {
    // RenderImage is explicitly BGRA in gpui-pre 0.3.9 assets.rs. Conversion
    // belongs to the optimized backend package, not an unoptimized UI loop.
    let (width, height) = (frame.width, frame.height);
    let buffer = image::RgbaImage::from_raw(width, height, frame.into_bgra_pixels())?;
    Some(Arc::new(RenderImage::new([image::Frame::new(buffer)])))
}
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
        CLOSE | FULLSCREEN => true,
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
        feature = "experimental-gpu",
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

/// Original offline probe entry point, unchanged for existing callers.
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
    let experimental = mode == PlayerChoice::ExperimentalEmbedded;
    let native = media.is_some();
    if !mode.is_managed()
        || (native && !media.as_ref().is_some_and(|path| path.is_file()))
        || (experimental && (!native || !cfg!(feature = "experimental-gpu")))
        || (stress_resize && !experimental)
    {
        eprintln!("player probe: category=invalid-probe-capability");
        return false;
    }
    PROBE_GPU_ERRORS.store(0, Ordering::Relaxed);
    PROBE_PANICS.store(0, Ordering::Relaxed);
    let old_hook = experimental.then(|| {
        let old = std::panic::take_hook();
        std::panic::set_hook(Box::new(|_| {
            PROBE_PANICS.fetch_add(1, Ordering::Relaxed);
            eprintln!("player probe: category=panic");
        }));
        old
    });
    let logger_ready = !experimental || log::set_logger(&PROBE_LOGGER).is_ok();
    if experimental {
        log::set_max_level(log::LevelFilter::Warn);
    }
    let result = Rc::new(Cell::new(false));
    let outcome = result.clone();
    gpui_platform::application().with_assets(crate::widgets::assets::Assets).run(move |cx| {
        crate::widgets::assets::init(cx);
        crate::widgets::tokens::set_mode(if light { crate::widgets::tokens::Mode::Light } else { crate::widgets::tokens::Mode::Dark }, cx);
        bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.), px(0.)), size: size(px(dimensions[0]), px(dimensions[1])) })),
            titlebar: Some(TitlebarOptions { title: Some(if experimental { "Kinema / EXPERIMENTAL GPU SMOKE" } else if native { "Kinema / Player — NATIVE SMOKE" } else { "Kinema Design Lab / Player — FIXTURE ONLY" }.into()), ..Default::default() }),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| cx.new(|cx| {
            let mut view = PlayerWindow::new_mode(if experimental { "Offline fixture" } else { "The Last Light" }.into(), media.map(|path| path.to_string_lossy().into_owned()), mode, window, cx);
            if experimental { cx.defer_in(window, |_, _, _| install_probe_gpu_guard()); }
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
                view.image = render_image(Frame { width: 960, height: 540, pixels });
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
            let playback_deadline = if experimental { deadline - Duration::from_millis(1000) } else { deadline };
            let mut controls = ProbeControls::default();
            let mut resize = ProbeResize::default();
            let mut aux_initial = None;
            let mut aux_resized = false;
            let mut aux_closed_painted = false;
            let mut aux_reopened = false;
            let mut stress_phase = 0;
            let mut stress_at = Instant::now();
            let mut failed = !logger_ready;
            while experimental && Instant::now() < playback_deadline {
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
            if !experimental {
                cx.background_executor().timer(Duration::from_secs(seconds.clamp(1, 15))).await;
            }
            let auxiliaries_painted = auxiliaries.iter().all(|handle| handle.update(cx, |view, _, _| view.paints > 0).unwrap_or(false));
            let stress_verified = !stress_resize || (resize.verified() && aux_resized && aux_closed_painted && aux_reopened && auxiliaries_painted);
            let released = Rc::new(Cell::new(!experimental));
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
                    && (!experimental || (controls.verified() && source_sized && gpu_clean && stress_verified));
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
                #[cfg(feature = "experimental-gpu")]
                { view.gpu_frame.take(); }
                if experimental {
                    window.refresh();
                    window.on_next_frame(move |window, _| {
                        window.refresh();
                        window.on_next_frame(move |_, _| release_signal.set(true));
                    });
                }
                (successful, view.frames_presented)
            });
            let cleanup_deadline = if experimental { deadline } else { Instant::now() + Duration::from_secs(1) };
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
    fn experimental_failure_reports_once_not_each_sample() {
        let mut reported = false;
        assert!(!failure_transition(false, &mut reported));
        assert!(failure_transition(true, &mut reported));
        for _ in 0..120 {
            assert!(!failure_transition(true, &mut reported));
        }
        assert!(!failure_transition(false, &mut reported));
        assert!(!failure_transition(true, &mut reported));
    }

    #[cfg(feature = "experimental-gpu")]
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
        let Err((_, reason, phase)) =
            player_config(&crate::player::PlayerChoice::ExperimentalEmbedded)
        else {
            panic!("experimental startup requires the actual window renderer context");
        };
        assert_eq!(reason, PlaybackFailure::GpuContextUnavailable);
        assert_eq!(phase, FailurePhase::Config);
    }

    #[test]
    fn default_player_config_keeps_viewport_policy() {
        let Ok(config) = player_config(&crate::player::PlayerChoice::Embedded) else {
            panic!("default embedded configuration must remain available");
        };
        assert_eq!(config.resolution, ResolutionPolicy::Viewport);
    }

    #[cfg(not(feature = "experimental-gpu"))]
    #[test]
    fn experimental_backend_is_explicitly_unavailable_without_feature() {
        let result = super::player_config(&crate::player::PlayerChoice::ExperimentalEmbedded);
        let Err((error, reason, phase)) = result else {
            panic!("feature-off backend must fail closed");
        };
        assert_eq!(reason, PlaybackFailure::BackendNotCompiled);
        assert_eq!(phase, FailurePhase::Config);
        assert!(error.contains("not compiled"));
        assert!(error.contains("isolated experimental app build"));
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
        assert_eq!(download_label(&rate, false), None);
    }
    #[test]
    fn cadence_counts_playing_rate_and_gap_and_resets_per_interval() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut meter = CadenceMeter::new(t0);
        // Initial loading: no rate, and the first accepted image is only a baseline.
        for n in (16..=300).step_by(16) {
            meter.sample(ms(n), Activity::Waiting, false, false);
        }
        meter.sample(ms(304), Activity::Playing, true, false);
        for n in [350, 400, 450, 500] {
            meter.sample(ms(n), Activity::Playing, true, false);
        }
        meter.sample(ms(900), Activity::Playing, false, true);
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
        assert_eq!(report.retirement_skips, 1);
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
        meter.sample(ms(100), Activity::Playing, true, false);
        meter.sample(ms(200), Activity::Paused, false, false);
        let Some(paused) = meter.finish_if_due(ms(1000)) else {
            unreachable!("interval due")
        };
        assert_eq!(paused.label(), "Video updates: Paused");
        assert_eq!(paused.max_accept_gap_ms, None);
        meter.sample(ms(1100), Activity::Buffering, false, false);
        meter.sample(ms(1200), Activity::Buffering, false, false);
        let Some(buffering) = meter.finish_if_due(ms(2000)) else {
            unreachable!("interval due")
        };
        assert_eq!(buffering.fps, None);
        assert_eq!(buffering.cache_paused_samples, 2);
        assert_eq!(buffering.label(), "Video updates: Buffering");
        // Resume: the first image after the idle period must not span the gap.
        meter.sample(ms(2500), Activity::Playing, true, false);
        meter.sample(ms(2540), Activity::Playing, true, false);
        let Some(resumed) = meter.finish_if_due(ms(3000)) else {
            unreachable!("interval due")
        };
        assert_eq!(resumed.accepted, 2);
        assert_eq!(resumed.max_accept_gap_ms, Some(460.));
        // A replay passes through Stopped/Waiting and re-baselines.
        meter.sample(ms(3100), Activity::Stopped, false, false);
        meter.sample(ms(3200), Activity::Waiting, false, false);
        meter.sample(ms(3300), Activity::Playing, true, false);
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
        meter.sample(
            t0 + Duration::from_millis(16),
            Activity::Playing,
            true,
            false,
        );
        meter.sample(
            t0 + Duration::from_millis(3016),
            Activity::Playing,
            true,
            false,
        );
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
    fn retirement_queue_admits_two_and_rejects_third_until_completion() {
        let t0 = Instant::now();
        let mut queue = RetirementQueue::new();
        assert_eq!(RETIRE_CAPACITY, 2);
        assert!(queue.has_room());
        let (a, b) = (queue.admit(t0), queue.admit(t0));
        assert_eq!((a, b), (Some(1), Some(2)));
        assert!(!queue.has_room());
        assert_eq!(queue.admit(t0), None);
        assert_eq!(queue.inflight(), 2);
        assert!(queue.complete(1, t0 + Duration::from_millis(40)));
        assert!(queue.has_room());
        assert_eq!(queue.admit(t0), Some(3));
    }
    #[test]
    fn retirement_ticket_exhaustion_fails_closed_without_reuse() {
        let t0 = Instant::now();
        let mut queue = RetirementQueue::new();
        queue.next = u64::MAX - 1;
        assert_eq!(queue.admit(t0), Some(u64::MAX));
        assert!(queue.complete(u64::MAX, t0));
        assert!(!queue.has_room());
        assert_eq!(queue.admit(t0), None);
        assert_eq!(queue.inflight(), 0);
    }
    #[test]
    fn retirement_completion_releases_only_its_own_ticket() {
        let t0 = Instant::now();
        let mut queue = RetirementQueue::new();
        queue.admit(t0);
        queue.admit(t0);
        // Out-of-order, duplicate, unknown and untracked (0) completions.
        assert!(queue.complete(2, t0));
        assert!(!queue.complete(2, t0));
        assert!(!queue.complete(99, t0));
        assert!(!queue.complete(0, t0));
        assert_eq!(queue.inflight(), 1);
        assert!(queue.complete(1, t0));
        assert_eq!(queue.inflight(), 0);
        assert!(!queue.complete(1, t0));
    }
    #[test]
    fn retirement_stats_aggregate_timings_and_reset_per_window() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut queue = RetirementQueue::new();
        let a = queue.admit(t0).unwrap_or(0);
        let b = queue.admit(ms(10)).unwrap_or(0);
        assert!(queue.first_callback(a, ms(10)));
        assert!(
            !queue.first_callback(a, ms(11)),
            "first boundary counts once"
        );
        assert!(queue.first_callback(b, ms(30)));
        assert!(!queue.first_callback(99, ms(30)));
        queue.complete(a, ms(30));
        queue.complete(b, ms(70));
        let window = queue.take_window();
        assert_eq!(window.high_water, 2);
        assert_eq!(window.first.count, 2);
        assert_eq!(window.first.mean_ms(), Some(15.));
        assert_eq!(window.first.max_ms, 20.);
        assert_eq!(window.complete.count, 2);
        assert_eq!(window.complete.mean_ms(), Some(45.));
        assert_eq!(window.complete.max_ms, 60.);
        let empty = queue.take_window();
        assert_eq!(empty.high_water, 0);
        assert_eq!(empty.first.mean_ms(), None);
    }
    #[test]
    fn cadence_counts_view_renders_and_gaps_only_while_playing() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        let mut meter = CadenceMeter::new(t0);
        meter.sample(ms(10), Activity::Playing, false, false);
        meter.render_called(ms(20));
        meter.render_called(ms(60));
        meter.sample(ms(70), Activity::Playing, false, false);
        let Some(report) = meter.finish_if_due(ms(1000)) else {
            unreachable!("interval due")
        };
        assert_eq!(report.view_render_calls, 2);
        // The open gap since the last render (60 -> 1000) is included.
        assert_eq!(report.max_view_render_gap_ms, Some(940.));
        meter.sample(ms(1100), Activity::Paused, false, false);
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
            download_label(&transfer, false).as_deref(),
            Some("Downloaded 1.5 MB of 4.5 MB (33%) · —")
        );
        transfer.observe(
            "a",
            Some((2_500_000, 4_500_000)),
            t0 + Duration::from_secs(2),
        );
        assert_eq!(
            download_label(&transfer, true).as_deref(),
            Some("Download 500.0 KB/s · 55%")
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
        assert!(download_label(&huge, true).is_some_and(|label| label.contains("100%")));
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
        assert_eq!(enabled(none), vec![CLOSE, FULLSCREEN]);
        let live = Availability {
            backend: true,
            ..none
        };
        assert_eq!(
            enabled(live),
            vec![
                CLOSE, PLAY, SUBTITLES, AUDIO, SPEED, FULLSCREEN, MUTE, VOLUME
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
    #[test]
    fn stale_close_cannot_remove_replacement() {
        assert!(is_current(Some(4), 4));
        assert!(!is_current(Some(5), 4));
        assert!(!is_current(None, 4));
    }
    #[test]
    fn converts_bgra_and_rejects_malformed_frame() {
        let Some(image) = render_image(Frame {
            width: 1,
            height: 1,
            pixels: vec![255, 0, 0, 255],
        }) else {
            panic!("valid one-pixel frame");
        };
        assert_eq!(image.as_bytes(0), Some([0, 0, 255, 255].as_slice()));
        assert!(
            render_image(Frame {
                width: 2,
                height: 2,
                pixels: vec![]
            })
            .is_none()
        );
    }
}

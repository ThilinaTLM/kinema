//! Separate, lease-owned Cinema player. libmpv initialization and destruction
//! (including worker joins) never run on GPUI's foreground executor.
use crate::ui::theme::layout::player;
use crate::ui::theme::layout::player::{
    DOCK_BOTTOM_INSET, DOCK_ESTIMATE, DOCK_FADE, DOCK_SIDE_INSET, HELP_COLUMN, TOP_BAR_HEIGHT,
    TOP_FADE, VOLUME_PANEL_EXTRA, VOLUME_THUMB, VOLUME_TRACK,
};
use crate::ui::theme::motion::timing;
use crate::ui::theme::opacity;
use crate::ui::theme::transparent;
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

mod chrome;
mod controls;
mod diagnostics;
mod dock;
mod help;
#[cfg(all(test, feature = "ui-probe-test"))]
mod input_tests;
mod overlays;
#[cfg(feature = "ui-probe")]
mod probe;
mod registry;
mod startup;
#[cfg(test)]
mod tests;
mod volume;
mod window;

use diagnostics::*;
#[cfg(feature = "ui-probe")]
#[allow(unused_imports)]
use probe::*;
#[cfg(feature = "ui-probe")]
pub use probe::{run_probe, run_probe_mode};
use registry::*;
pub(crate) use registry::{
    authorize_progress, checkpoint_all, close_session, desktop_snapshots, dispatch_transport,
    invalidate_progress, open, progress_notice, show_player, take_focus_events,
};
use startup::*;

#[cfg(feature = "gpu-surface")]
use kinema_mpv::ResolutionPolicy;

use crate::controllers::coordinator::AppCoordinator;
use crate::{
    ui::primitives::button::{Button, ButtonVariant},
    ui::primitives::icon::{Icon, IconName},
    ui::primitives::menu::{Menu, MenuItem},
    ui::theme::typography::{self, TextSize, TypeExt, Weight},
    ui::theme::{Palette, radius, size, space as s},
};
use controls::{
    Scrub, VOLUME_STEP, chapter_index, chapter_label, chapter_ticks, displayed_position,
    displayed_volume, format_clock, format_remaining, ratio_in_span, scrub_settled, speed_label,
    step_volume, subtitle_position, track_label, volume_at, volume_fraction, volume_rail_height,
    volume_settled,
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
    paused || loading || error || interacting || idle < timing::CHROME_IDLE
}

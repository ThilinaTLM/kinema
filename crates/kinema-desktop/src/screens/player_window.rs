//! Separate, lease-owned Cinema player. libmpv initialization and destruction
//! (including worker joins) never run on GPUI's foreground executor.
use std::{
    collections::HashMap,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{prelude::*, *};
use kinema_mpv::{Config, Frame, PlaybackState, Player, ResolutionPolicy, TrackKind};

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
}

/// Replaying a lease activates its existing window without reloading its URL.
pub fn open(
    session_id: String,
    url: String,
    title: String,
    coordinator: Entity<AppCoordinator>,
    cx: &mut App,
) -> Result<(), String> {
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
            cx.new(|cx| PlayerWindow::new(title, Some(url), window, cx))
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

pub struct PlayerWindow {
    title: String,
    lease: Option<(String, u64)>,
    frames_presented: u64,
    player: Option<BackgroundPlayer>,
    state: PlaybackState,
    error: Option<String>,
    control_notice: bool,
    source_url: Option<String>,
    image: Option<Arc<RenderImage>>,
    retiring: bool,
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
    /// Last `sub-pos` sent for this load; None = unknown, resend on next sync.
    subtitle_position: Option<u8>,
    _initialize: Task<()>,
    _updates: Task<()>,
}
impl PlayerWindow {
    fn new(
        title: String,
        url: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let source_url = url.clone();
        let executor = cx.background_executor().clone();
        let worker = executor.clone();
        let initialize = cx.background_executor().spawn(async move {
            let Some(url) = url else {
                return Ok(None);
            };
            let result = Player::new(Config {
                resolution: ResolutionPolicy::Viewport,
                ..Default::default()
            });
            match result {
                Ok(player) => {
                    player.load(&url).map_err(|error| error.to_string())?;
                    Ok(Some(BackgroundPlayer {
                        player: Some(player),
                        executor: worker,
                        stopped: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                    }))
                }
                Err(error) => Err(error.to_string()),
            }
        });
        let initialize = cx.spawn(async move |view, cx| {
            let result = initialize.await;
            let _ = view.update(cx, |view, cx| {
                match result {
                    Ok(player) => view.player = player,
                    Err(error) => view.error = Some(error),
                }
                cx.notify();
            });
        });
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
        Self {
            title,
            lease: None,
            frames_presented: 0,
            player: None,
            state: PlaybackState {
                loading: true,
                ..Default::default()
            },
            error: None,
            control_notice: false,
            source_url,
            image: None,
            retiring: false,
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
            subtitle_position: None,
            _initialize: initialize,
            _updates: updates,
        }
    }

    fn sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let mut state_changed = false;
        let mut frame_received = false;
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
            let frame = if !self.retiring {
                player.take_frame()
            } else {
                None
            };
            if let Some(frame) = frame.and_then(render_image) {
                self.frames_presented += 1;
                if let Some(old) = self.image.replace(frame) {
                    self.retiring = true;
                    retire_image(old, cx.weak_entity(), window);
                }
                frame_received = true;
            }
            state_changed = self.state != previous;
        }
        self.sync_subtitle_position(window, cx);
        let now = Instant::now();
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
            state_changed,
            frame_received,
            self.chrome_was_visible,
            self.chrome_visible(window),
        ) {
            cx.notify();
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
            // A new load: do not assume the previous sub-pos survives.
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
            f32::from(dock + s::S20 + s::S8),
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
            for range in self.state.buffered_ranges.iter().flatten() {
                let start = (range.start / duration).clamp(0., 1.) as f32;
                let end = (range.end / duration).clamp(0., 1.) as f32;
                if end > start {
                    track = track.child(
                        div()
                            .absolute()
                            .h_full()
                            .left(relative(start))
                            .w(relative(end - start))
                            .bg(p.text_muted.opacity(0.45)),
                    );
                }
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
                    "Playback position, {} of {}. Left and right arrows seek 10 seconds.",
                    format_clock(shown),
                    format_clock(duration)
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
            viewport.width - s::S20,
            viewport.height - s::S20 - dock_extent - s::S8,
        );
        let dock_entity = cx.entity();
        let status: &'static str = {
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
            .child(self.volume_panel(cx))
            .child(
                div()
                    .id("player-dock")
                    .relative()
                    .mx(s::S20)
                    .mb(s::S20)
                    .p(s::S16)
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
                            .child(div().w(s::S48 + s::S16).text_right().child(match duration {
                                Some(d) if seekable => format_remaining(shown, d),
                                _ => "—:—".into(),
                            })),
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
                                .on_click(cx.listener(|view, _, _, cx| view.toggle_playback(cx))),
                            )
                            .child(
                                self.control(
                                    BACK,
                                    "player-back",
                                    IconName::SeekBack,
                                    "Back 10 seconds (←)",
                                )
                                .on_click(cx.listener(|view, _, _, cx| view.seek(-10., cx))),
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
                                dock_entity
                                    .update(cx, |view, _| view.dock_height = bounds.size.height)
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .size_full(),
                    ),
            )
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
        let p = Palette::dark();
        let visible = self.chrome_visible(window);
        self.chrome_was_visible = visible;
        let failed = terminal_failure(self.error.is_some(), &self.state);
        let entity = cx.weak_entity();
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
            .when(failed || self.image.is_none(), |element| element.child(
                div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(s::S12).when(failed, |panel| panel.bg(p.scrim_max))
                    .child(div().type_style(t::TITLE).child(if failed { "Playback unavailable" } else if self.state.ended { "Playback finished" } else if self.state.paused_for_cache { "Buffering…" } else if self.state.loading || self.player.is_none() { "Opening media…" } else { "Audio playback" }))
                    .child(div().max_w(s::SHEET_WIDTH).px(s::S24).type_style(t::BODY).text_color(p.text_secondary).child(if failed { "Playback stopped. Choose external MPV or VLC in Settings → Playback, then try again. If libmpv is missing, install its runtime first." } else if self.state.loading || self.player.is_none() { "Preparing the embedded player. You can close this window at any time." } else { "Use the controls below. Press K to play or pause." }))
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
    }
}

/// next_frame_callbacks run BEFORE should_draw in gpui-pre 0.3.9. WGPU
/// atlas removal deallocates/reuses tiles immediately. Keep one retiring image,
/// force a draw without it, then remove it on the following boundary. This is a
/// conservative bounded retirement policy, not a GPU-fence proof for every OS.
fn retire_image(image: Arc<RenderImage>, view: WeakEntity<PlayerWindow>, window: &mut Window) {
    window.on_next_frame(move |window, _| {
        window.refresh();
        window.on_next_frame(move |window, cx| {
            let _ = window.drop_image(image);
            let _ = view.update(cx, |view, cx| {
                view.retiring = false;
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

/// Opt-in offline probe: production view, without coordinator/services, user
/// settings, database or credentials. Native playback accepts only a local file.
/// No screenshot is taken here; synthetic chrome can be captured under Xvfb.
#[cfg(feature = "ui-probe")]
pub fn run_probe(
    media: Option<std::path::PathBuf>,
    seconds: u64,
    light: bool,
    dimensions: [f32; 2],
) -> bool {
    use std::{cell::Cell, rc::Rc, sync::atomic::Ordering};
    let native = media.is_some();
    let result = Rc::new(Cell::new(false));
    let outcome = result.clone();
    gpui_platform::application().with_assets(crate::widgets::assets::Assets).run(move |cx| {
        crate::widgets::assets::init(cx);
        crate::widgets::tokens::set_mode(if light { crate::widgets::tokens::Mode::Light } else { crate::widgets::tokens::Mode::Dark }, cx);
        bind_keys(cx);
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds { origin: point(px(0.), px(0.)), size: size(px(dimensions[0]), px(dimensions[1])) })),
            titlebar: Some(TitlebarOptions { title: Some(if native { "Kinema / Player — NATIVE SMOKE" } else { "Kinema Design Lab / Player — FIXTURE ONLY" }.into()), ..Default::default() }),
            ..Default::default()
        };
        let handle = cx.open_window(options, |window, cx| cx.new(|cx| {
            let mut view = PlayerWindow::new("The Last Light".into(), media.map(|path| path.to_string_lossy().into_owned()), window, cx);
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
        let Ok(handle) = handle else { eprintln!("player probe: could not open window"); cx.quit(); return; };
        cx.activate(true);
        cx.spawn(async move |cx| {
            cx.background_executor().timer(Duration::from_secs(seconds.clamp(1, 8))).await;
            let snapshot = handle.update(cx, |view, _, _| {
                let successful = !native || (view.frames_presented > 2 && view.error.is_none() && view.state.error.is_none());
                eprintln!("player probe: frames={} backend={:?} source={:?} output={:?} position={:.2} paused={} muted={} error={} successful={}", view.frames_presented, view.state.active_backend, view.state.source_size, view.state.render_size, view.state.position, view.state.paused, view.state.muted, terminal_failure(view.error.is_some(), &view.state), successful);
                let stopped = view.player.as_ref().map(|owner| owner.stopped.clone());
                // Keep the native window alive until the background join is
                // observed; some platforms exit the event loop on last close.
                view.player.take();
                (successful, stopped)
            });
            if let Ok((successful, stopped)) = snapshot {
                let deadline = Instant::now() + Duration::from_secs(1);
                while stopped.as_ref().is_some_and(|signal| !signal.load(Ordering::Acquire)) && Instant::now() < deadline {
                    cx.background_executor().timer(Duration::from_millis(20)).await;
                }
                let shutdown = stopped.is_none_or(|signal| signal.load(Ordering::Acquire));
                eprintln!("player probe: background shutdown completed={shutdown}");
                outcome.set(successful && shutdown);
            }
            let _ = handle.update(cx, |_, window, _| window.remove_window());
            cx.update(|cx| cx.quit());
        }).detach();
    });
    result.get()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ::core::prelude::v1::test;
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

//! Separate, lease-owned Cinema player. libmpv initialization and destruction
//! (including worker joins) never run on GPUI's foreground executor.
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use gpui::{prelude::*, *};
use kinema_mpv::{Config, Frame, PlaybackState, Player, ResolutionPolicy};

use super::coordinator::AppCoordinator;
use crate::widgets::{
    button::{Button, ButtonVariant},
    icon::IconName,
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
        KeyBinding::new("tab", NextControl, Some("KinemaPlayer")),
        KeyBinding::new("shift-tab", PreviousControl, Some("KinemaPlayer")),
    ]);
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
        let controls = (0..9).map(|_| cx.focus_handle()).collect();
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
    fn focus_control(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        let mut order = vec![self.surface_focus.clone()];
        order.extend(
            self.controls
                .iter()
                .enumerate()
                .filter(|(i, _)| control_enabled(*i, self.backend_available(), self.can_seek()))
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
            self.dock_hover || !self.surface_focus.is_focused(window),
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
        button.tooltip = Some(label.into());
        button.disabled(!control_enabled(
            index,
            self.backend_available(),
            self.can_seek(),
        ))
    }
    fn chrome(&self, fullscreen: bool, cx: &mut Context<Self>) -> AnyElement {
        let p = Palette::dark();
        let entity = cx.entity();
        let time = format!(
            "{} / {}",
            format_time(self.state.position),
            self.state
                .duration
                .filter(|d| d.is_finite() && *d > 0.)
                .map(format_time)
                .unwrap_or_else(|| "—:—".into())
        );
        let seekable = self.can_seek();
        let fraction = progress(self.state.position, self.state.duration).unwrap_or(0.);
        let timeline = div()
            .id("player-timeline")
            .relative()
            .key_context("KinemaPlayerTimeline")
            .track_focus(&self.controls[4])
            .tab_stop(seekable)
            .w_full()
            .h(s::S24)
            .flex()
            .items_center()
            .cursor(if seekable {
                CursorStyle::PointingHand
            } else {
                CursorStyle::Arrow
            })
            .focus_visible(|style| style.border(s::FOCUS_RING).border_color(p.focus_ring))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(s::S4)
                    .rounded(radius::SM)
                    .bg(p.line_control)
                    .child(
                        div()
                            .absolute()
                            .h_full()
                            .w(relative(fraction as f32))
                            .rounded(radius::SM)
                            .bg(p.accent),
                    ),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    if seekable {
                        view.controls[4].focus(window, cx);
                        // Bounds are read from the timeline's canvas below; mouse
                        // targeting is performed there, not from window width.
                        view.activity(cx);
                        let _ = event;
                    }
                }),
            );
        // A canvas installs a hit-tested listener using actual timeline bounds.
        let timeline = timeline.child(
            canvas(
                move |bounds, _, _| bounds,
                move |bounds, _, window, _| {
                    let entity = entity.clone();
                    window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && event.button == MouseButton::Left
                            && bounds.contains(&event.position)
                            && seekable
                        {
                            entity.update(cx, |view, cx| {
                                let ratio = ((event.position.x - bounds.left())
                                    / bounds.size.width)
                                    .clamp(0., 1.);
                                let target = view.state.duration.unwrap_or(0.) * ratio as f64;
                                view.seek(target - view.state.position, cx);
                                view.controls[4].focus(window, cx);
                            });
                        }
                    });
                },
            )
            .absolute()
            .size_full(),
        );
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
                        self.control(0, "player-close", IconName::Close, "Close player")
                            .on_click(|_, window, _| window.remove_window()),
                    ),
            )
            .child(
                div()
                    .id("player-dock")
                    .mx(s::S20)
                    .mb(s::S20)
                    .p(s::S16)
                    .rounded(radius::LG)
                    .bg(p.scrim_max)
                    .border_1()
                    .border_color(p.line_subtle)
                    .on_hover(cx.listener(|view, hovered, _, cx| {
                        view.dock_hover = *hovered;
                        view.activity(cx);
                    }))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .type_style(t::MONO)
                            .text_color(p.text_secondary)
                            .child(time)
                            .child(if seekable {
                                "← / →  Seek 10s"
                            } else {
                                "Seeking unavailable"
                            }),
                    )
                    .child(timeline)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(s::S8)
                            .child(
                                self.control(
                                    1,
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
                                .disabled(!self.backend_available())
                                .on_click(cx.listener(
                                    |view, _, _, cx| {
                                        view.toggle_playback(cx);
                                    },
                                )),
                            )
                            .child(
                                self.control(
                                    2,
                                    "player-back",
                                    IconName::SeekBack,
                                    "Back 10 seconds (←)",
                                )
                                .disabled(!seekable)
                                .on_click(cx.listener(|view, _, _, cx| view.seek(-10., cx))),
                            )
                            .child(
                                self.control(
                                    3,
                                    "player-forward",
                                    IconName::SeekForward,
                                    "Forward 10 seconds (→)",
                                )
                                .disabled(!seekable)
                                .on_click(cx.listener(|view, _, _, cx| view.seek(10., cx))),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .type_style(t::SMALL)
                                    .text_color(p.text_secondary)
                                    .child(if self.state.paused_for_cache {
                                        "Buffering…"
                                    } else if self.control_notice
                                        || (self.state.error.is_some()
                                            && !terminal_failure(false, &self.state))
                                    {
                                        "Control unavailable — try again"
                                    } else if self.state.ended {
                                        "Finished · Replay to watch again"
                                    } else {
                                        ""
                                    }),
                            )
                            .child(
                                self.control(
                                    5,
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
                                .on_click(cx.listener(
                                    |view, _, _, cx| view.command(Player::toggle_mute, cx),
                                )),
                            )
                            .child(
                                self.control(
                                    6,
                                    "player-quieter",
                                    IconName::Minus,
                                    "Decrease volume",
                                )
                                .on_click(cx.listener(
                                    |view, _, _, cx| {
                                        let volume = (view.state.volume - 10.).max(0.);
                                        view.command(|player| player.set_volume(volume), cx);
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .type_style(t::MONO)
                                    .text_color(p.text_secondary)
                                    .child(format!("{:.0}%", self.state.volume)),
                            )
                            .child(
                                self.control(7, "player-louder", IconName::Plus, "Increase volume")
                                    .on_click(cx.listener(|view, _, _, cx| {
                                        let volume = (view.state.volume + 10.).min(100.);
                                        view.command(|player| player.set_volume(volume), cx);
                                    })),
                            )
                            .child(
                                self.control(
                                    8,
                                    "player-fullscreen",
                                    if fullscreen {
                                        IconName::ExitFullscreen
                                    } else {
                                        IconName::Fullscreen
                                    },
                                    "Toggle fullscreen (F)",
                                )
                                .on_click(|_, window, _| window.toggle_fullscreen()),
                            ),
                    ),
            )
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
            .on_action(cx.listener(|view, _: &TogglePlayback, _, cx| view.toggle_playback(cx)))
            .on_action(cx.listener(|view, _: &SeekBack, _, cx| view.seek(-10., cx)))
            .on_action(cx.listener(|view, _: &SeekForward, _, cx| view.seek(10., cx)))
            .on_action(cx.listener(|view, _: &ToggleMute, _, cx| view.command(Player::toggle_mute, cx)))
            .on_action(cx.listener(|view, _: &ToggleFullscreen, window, cx| { window.toggle_fullscreen(); view.activity(cx); }))
            .on_action(cx.listener(|view, _: &Escape, window, cx| { if window.is_fullscreen() { window.toggle_fullscreen(); } view.activity(cx); }))
            .on_action(cx.listener(|view, _: &NextControl, window, cx| view.focus_control(false, window, cx)))
            .on_action(cx.listener(|view, _: &PreviousControl, window, cx| view.focus_control(true, window, cx)))
            .on_mouse_move(cx.listener(|view, _, _, cx| view.activity(cx)))
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
            .when(visible, |div| div.child(self.chrome(window.is_fullscreen(), cx)))
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
fn format_time(seconds: f64) -> String {
    let seconds = if seconds.is_finite() {
        seconds.max(0.).floor() as u64
    } else {
        0
    };
    if seconds >= 3600 {
        format!(
            "{}:{:02}:{:02}",
            seconds / 3600,
            seconds / 60 % 60,
            seconds % 60
        )
    } else {
        format!("{}:{:02}", seconds / 60, seconds % 60)
    }
}
fn redraw_needed(
    state_changed: bool,
    frame_received: bool,
    was_visible: bool,
    visible: bool,
) -> bool {
    state_changed || frame_received || was_visible != visible
}
fn control_enabled(index: usize, backend_available: bool, seekable: bool) -> bool {
    match index {
        0 | 8 => true,
        2..=4 => backend_available && seekable,
        _ => backend_available,
    }
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
    paused || loading || error || interacting || idle < Duration::from_secs(3)
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
                view.state = PlaybackState { position: 1837., duration: Some(6840.), paused: true, volume: 80., ..Default::default() };
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
        assert_eq!(format_time(f64::NAN), "0:00");
        assert_eq!(format_time(-2.), "0:00");
        assert_eq!(format_time(3661.9), "1:01:01");
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
        let enabled = |backend, seekable| {
            (0..9)
                .filter(|i| control_enabled(*i, backend, seekable))
                .collect::<Vec<_>>()
        };
        assert_eq!(enabled(false, false), vec![0, 8]);
        assert_eq!(enabled(true, false), vec![0, 1, 5, 6, 7, 8]);
        assert_eq!(enabled(true, true), (0..9).collect::<Vec<_>>());
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

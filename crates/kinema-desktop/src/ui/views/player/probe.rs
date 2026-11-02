//! Native UI probe harness.
use super::*;

#[cfg(feature = "ui-probe")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProbeCommand {
    Pause,
    Adjust,
    Resume,
    Restore,
}

/// Each transition requires a fresh observed libmpv snapshot, not command acceptance.
#[cfg(feature = "ui-probe")]
#[derive(Default)]
pub(super) struct ProbeControls {
    pub(super) phase: u8,
    pub(super) target: f64,
    pub(super) resumed_at: f64,
    pub(super) failed: bool,
}
#[cfg(feature = "ui-probe")]
impl ProbeControls {
    pub(super) fn observe(&mut self, state: &PlaybackState, frames: u64) -> Option<ProbeCommand> {
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
    pub(super) fn step(&mut self, player: &Player, frames: u64) {
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
    pub(super) fn verified(&self) -> bool {
        self.phase == 5 && !self.failed
    }
}

#[cfg(feature = "ui-probe")]
#[derive(Default)]
pub(super) struct ProbeResize {
    pub(super) requested: u32,
    pub(super) observed: u32,
    pub(super) previous: Option<[u32; 2]>,
}
#[cfg(feature = "ui-probe")]
impl ProbeResize {
    pub(super) fn observe(&mut self, viewport: [u32; 2]) {
        if self.previous.is_some_and(|previous| previous != viewport) {
            self.observed += 1;
        }
        self.previous = Some(viewport);
    }
    pub(super) fn verified(&self) -> bool {
        self.requested >= 2 && self.observed >= 2
    }
}

#[cfg(feature = "ui-probe")]
pub(super) struct ProbeProxy {
    pub(super) paints: u64,
}
#[cfg(feature = "ui-probe")]
impl Render for ProbeProxy {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.paints += 1;
        let p = Palette::cinema();
        div()
            .size_full()
            .bg(p.surface)
            .text_color(p.foreground)
            .child("Offline auxiliary surface — no settings or services")
    }
}
#[cfg(feature = "ui-probe")]
pub(super) fn open_probe_proxy(cx: &mut App, offset: f32) -> Option<WindowHandle<ProbeProxy>> {
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
pub(super) fn probe_viewport(window: &Window) -> [u32; 2] {
    let viewport = window.viewport_size();
    [
        f32::from(viewport.width).round() as u32,
        f32::from(viewport.height).round() as u32,
    ]
}

#[cfg(feature = "ui-probe")]
pub(super) static PROBE_GPU_ERRORS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "ui-probe")]
pub(super) static PROBE_PANICS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(feature = "ui-probe")]
pub(super) struct ProbeLogger;
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
pub(super) static PROBE_LOGGER: ProbeLogger = ProbeLogger;

#[cfg(feature = "ui-probe")]
pub(super) fn install_probe_gpu_guard() {
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

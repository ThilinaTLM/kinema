//! Player window core: lifecycle, progress, transport commands and render.
use super::*;

impl PlayerWindow {
    pub(super) fn shortcut(&mut self, id: Sid, window: &mut Window, cx: &mut Context<Self>) {
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
                    .filter(|(_, since)| since.elapsed() < timing::REQUEST_PENDING)
                    .map_or(self.state.speed, |(value, _)| value);
                let target = if id == Sid::ResetSpeed {
                    Some(1.)
                } else {
                    controls::adjacent_speed(speed, &SPEEDS, id == Sid::SpeedUp)
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
                if let Some(target) = controls::adjacent_chapter(
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
                    controls::cycle_track(&self.state.tracks, kind, id != Sid::PreviousSubtitle)
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
                    .filter(|(_, since)| since.elapsed() < timing::REQUEST_PENDING)
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
    pub(super) fn unavailable(&mut self, cx: &mut Context<Self>) {
        self.control_notice = true;
        self.activity(cx);
    }

    pub(super) fn new_mode(
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
                cx.background_executor().timer(timing::FRAME_TICK).await;
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

    pub(super) fn start_player(&mut self, cx: &mut Context<Self>) {
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

    pub(super) fn embedded_video(&self) -> AnyElement {
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

    pub(super) fn deliver_checkpoint(&self, mut sample: PlayerCheckpoint, cx: &mut App) {
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
    pub(super) fn flush_progress(&mut self, cx: &mut App) {
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
    pub(super) fn sample_progress(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn send_progress_ready(&self, p: &Progress, cx: &mut Context<Self>) {
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
    pub(super) fn begin_resume(&mut self, p: &mut Progress) {
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
    pub(super) fn retry_startup(&mut self, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.progress {
            p.retry_startup(Instant::now());
        }
        cx.notify();
    }
    pub(super) fn retry_progress_ready(&mut self, cx: &mut Context<Self>) {
        let Some(mut p) = self.progress.take() else {
            return;
        };
        p.notice = false;
        self.send_progress_ready(&p, cx);
        self.progress = Some(p);
        cx.notify();
    }
    pub(super) fn play_without_saving(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn begin_start_over(&mut self, p: &mut Progress) {
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
    pub(super) fn choose_progress(&mut self, resume: bool, cx: &mut Context<Self>) {
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

    pub(super) fn sample(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
                since.elapsed() >= timing::REQUEST_PENDING
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
            if now.duration_since(metrics.last_report) >= timing::METRICS_REPORT
                && let Some(player) = self.player.as_ref().and_then(|owner| owner.player.as_ref())
            {
                metrics.report(now, &player.diagnostics(), &self.state);
            }
        }
    }
    pub(super) fn activity(&mut self, cx: &mut Context<Self>) {
        self.last_activity = Instant::now();
        cx.notify();
    }
    pub(super) fn command(
        &mut self,
        command: impl FnOnce(&Player) -> kinema_mpv::Result<()>,
        cx: &mut Context<Self>,
    ) {
        self.request_command(command, cx);
    }
    pub(super) fn request_command(
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
    pub(super) fn desktop_transport(&mut self, action: Transport, cx: &mut Context<Self>) -> bool {
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
    pub(super) fn toggle_playback(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn seek(&mut self, seconds: f64, cx: &mut Context<Self>) {
        if self.can_seek() {
            let position = self
                .seek_requested
                .filter(|(_, since)| since.elapsed() < timing::REQUEST_PENDING)
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
    pub(super) fn presentation_command(
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
    pub(super) fn sync_subtitle_position(&mut self, window: &Window, cx: &mut Context<Self>) {
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
    pub(super) fn seek_to(&mut self, target: f64, cx: &mut Context<Self>) {
        if self.can_seek() {
            self.seek_requested = Some((target, Instant::now()));
            self.command(|player| player.seek_absolute(target), cx);
        }
    }
    pub(super) fn availability(&self) -> Availability {
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
    /// Pointer moved anywhere in the window: drags continue outside their widgets,
    /// and a missed button-up (released elsewhere) commits instead of sticking.
    pub(super) fn pointer_moved(&mut self, event: &MouseMoveEvent, cx: &mut Context<Self>) {
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
    pub(super) fn finish_drag(&mut self, cx: &mut Context<Self>) {
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
    pub(super) fn cancel_drag(&mut self, cx: &mut Context<Self>) -> bool {
        let active = matches!(self.scrub, Some(Scrub::Dragging(_))) || self.volume_dragging;
        if active {
            self.scrub = None;
            self.volume_dragging = false;
            cx.notify();
        }
        active
    }
    pub(super) fn focus_control(
        &mut self,
        reverse: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
    pub(super) fn backend_available(&self) -> bool {
        self.player.is_some() && !terminal_failure(self.error.is_some(), &self.state)
    }
    pub(super) fn can_seek(&self) -> bool {
        self.backend_available() && desktop_can_seek(&self.state)
    }
    pub(super) fn chrome_visible(&self, window: &Window) -> bool {
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
    /// Cheap in-memory coordinator read; never touches MediaService or disk.
    /// Returns whether the displayed download figures changed. Views without a
    /// remote lease (local/probe) keep no download state, never a fake 0 B/s.
    pub(super) fn observe_transfer(&mut self, now: Instant, cx: &App) -> bool {
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
}

impl Render for PlayerWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(meter) = self.cadence.as_mut() {
            meter.render_called(Instant::now());
        }
        let p = Palette::cinema();
        let visible = self.chrome_visible(window);
        self.chrome_was_visible = visible;
        let failed = terminal_failure(self.error.is_some(), &self.state);
        let entity = cx.weak_entity();
        let has_video = false;
        #[cfg(feature = "ui-probe")]
        let has_video = has_video || self.fixture_image.is_some();
        #[cfg(feature = "gpu-surface")]
        let has_video = has_video || self.gpu_frame.is_some();
        div().id("kinema-player").key_context("KinemaPlayer").relative().size_full().overflow_hidden().bg(p.background).text_color(p.foreground).font_family(typography::UI_FAMILY)
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
                div().absolute().inset_0().flex().flex_col().items_center().justify_center().gap(s::S12).when(failed, |panel| panel.bg(p.scrim))
                    .child(div().text(TextSize::Xl).weight(Weight::Semibold).child(if failed { "Playback unavailable" } else if self.state.ended { "Playback finished" } else if self.state.paused_for_cache { "Buffering…" } else if self.state.loading || self.player.is_none() { "Opening media…" } else { "Audio playback" }))
                    .child(div().max_w(size::SHEET_WIDTH).px(s::S24).text(TextSize::Base).text_color(p.muted_foreground).child(if failed && self.mode == PlayerChoice::Embedded { embedded_help() } else if failed { "Playback stopped. Choose external MPV or VLC in Settings → Playback, then try again. If libmpv is missing, install its runtime first." } else if self.state.loading || self.player.is_none() { "Preparing the embedded player. You can close this window at any time." } else { "Use the controls below. Press K to play or pause." }))
                    .when(failed && self.mode == PlayerChoice::Embedded, |panel| panel.child(
                        div().max_w(size::SHEET_WIDTH).px(s::S24).text(TextSize::Base).text_color(p.muted_foreground)
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
                element.child(div().absolute().inset_0().occlude().bg(p.scrim).flex().flex_col().items_center().justify_center().gap(s::S16)
                    .child(div().text(TextSize::Xl).weight(Weight::Semibold).child(if unavailable && first_play { "Playback could not start" } else if unavailable { "Saved position cannot be resumed" } else if mismatch { "This source has a different duration" } else { "Resume this replay?" }))
                    .child(div().text(TextSize::Base).child(if unavailable && first_play { "Playback readiness could not be confirmed. Retry, or try playing without saving progress." } else if unavailable { "Playback readiness or resume could not be confirmed. Your saved progress is unchanged." } else if mismatch { "Resume at the saved time, or start over. Your saved progress is unchanged until playback is ready." } else { "Resume your saved position, or play from the beginning." }))
                    .when(!unavailable, |panel| panel.child(Button::new("progress-resume", if mismatch { "Resume anyway" } else { "Resume" }).on_click(cx.listener(|view, _, _, cx| view.choose_progress(true, cx)))))
                    .when(unavailable, |panel| panel
                        .child(Button::new("progress-retry-startup", "Retry").on_click(cx.listener(|view, _, _, cx| view.retry_startup(cx))))
                        .child(Button::new("progress-play-unsaved", if first_play { "Try playing without saving" } else { "Play without saving" }).on_click(cx.listener(|view, _, _, cx| view.play_without_saving(cx)))))
                    .when(self.progress.as_ref().is_some_and(|progress| progress.can_start_over(&self.state)), |panel| panel.child(Button::new("progress-start-over", "Play from beginning").on_click(cx.listener(|view, _, _, cx| view.choose_progress(false, cx))))))
            })
            .when(self.progress.as_ref().is_some_and(|p| p.notice && matches!(p.phase, ProgressPhase::Ready | ProgressPhase::Reset)), |element| {
                element.child(div().absolute().inset_0().occlude().bg(p.scrim).flex().flex_col().items_center().justify_center().gap(s::S16)
                    .child(div().text(TextSize::Xl).weight(Weight::Semibold).child("Progress could not be saved"))
                    .child(div().text(TextSize::Base).child("Your saved progress is unchanged. Retry saving, or play without saving progress."))
                    .child(Button::new("progress-retry-saving", "Retry saving").on_click(cx.listener(|view, _, _, cx| view.retry_progress_ready(cx))))
                    .child(Button::new("progress-untracked", "Play without saving").on_click(cx.listener(|view, _, _, cx| view.play_without_saving(cx)))))
            })
            .when(self.help_focus.is_some(), |el| el.child(self.shortcut_help(window, cx)))
            // Opt-in diagnostics only: no handlers, id or focus, so it never takes input.
            .when_some(self.cadence_text.clone(), |el, text| {
                el.child(
                    div()
                        .absolute()
                        .top(player::DIAGNOSTICS_TOP)
                        .left(s::S16)
                        .max_w(player::DIAGNOSTICS_MAX_WIDTH)
                        .px(s::S8)
                        .py(s::S4)
                        .truncate()
                        .rounded(radius::MD)
                        .bg(p.scrim.opacity(opacity::PLAYER_SCRIM))
                        .text(TextSize::Sm).mono()
                        .text_color(p.muted_foreground)
                        .child(text),
                )
            })
    }
}

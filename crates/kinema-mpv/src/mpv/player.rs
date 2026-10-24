use super::{
    diagnostics::{Diagnostics, Evidence, FailureDetails, Lifecycle, NativeMessage},
    ffi::{EndFile, LogMessage, Property},
    library::{Client, Loaded},
    metadata,
    render::{self, Pipeline},
    software::Frame,
};
use crate::{
    Error, HardwareDecoding, PlaybackFailure, PlaybackState, RenderBackend, RenderDiagnostics,
    ResolutionPolicy, Result,
};
use std::{
    ffi::{CStr, CString, c_int},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Debug)]
pub struct Config {
    /// Presentation-device context for default GPU-resident SDR playback with
    /// no CPU/copy-back fallback. Requires Auto/Gpu rendering and Auto decoding.
    /// With `gpu-surface` enabled, default Auto fails before loading libmpv if
    /// this is absent. Explicit Cpu/Gpu without a context selects low-level
    /// readback instead; desktop presentation must always supply the context.
    #[cfg(feature = "gpu-surface")]
    pub gpu_surface: Option<super::gpu_surface::GpuSurfaceContext>,
    pub library_path: Option<PathBuf>,
    pub initial_volume: f64,
    pub max_render_size: Option<[u32; 2]>,
    pub render_backend: RenderBackend,
    pub hardware_decoding: HardwareDecoding,
    pub resolution: ResolutionPolicy,
    pub max_frame_bytes: usize,
    pub egl_library_path: Option<PathBuf>,
    pub gles_library_path: Option<PathBuf>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            #[cfg(feature = "gpu-surface")]
            gpu_surface: None,
            library_path: None,
            initial_volume: 100.0,
            max_render_size: None,
            render_backend: RenderBackend::Auto,
            hardware_decoding: HardwareDecoding::Auto,
            resolution: ResolutionPolicy::Source,
            max_frame_bytes: 64 * 1024 * 1024,
            egl_library_path: None,
            gles_library_path: None,
        }
    }
}

pub(super) struct Shared {
    pub diagnostics: Arc<Diagnostics>,
    pub stopped: AtomicBool,
    pub pipeline: Mutex<Pipeline>,
    pub state: Mutex<PlaybackState>,
}
impl Shared {
    /// Only nonfatal control/media errors may be replaced by later events.
    #[cfg(test)]
    pub fn set_error(&self, error: String) {
        if let Some(mut state) = self.control_state() {
            state.failure_reason = Some(PlaybackFailure::from_gpu_message(&error));
            state.error = Some(error);
        }
    }
    pub fn set_error_typed(&self, error: Error) {
        if let Some(mut state) = self.control_state() {
            state.failure_reason = Some(error.playback_failure());
            self.diagnostics.failure_typed(&error, Some(&state));
            state.error = Some(error.to_string());
        }
    }
    /// Fatal render failures require Player recreation, never backend recovery.
    pub fn fail_render(&self, error: String) {
        let reason = PlaybackFailure::from_gpu_message(&error);
        self.fail_render_with_reason(error, reason);
    }
    pub fn fail_render_typed(&self, error: Error) {
        self.fail_render_ref(&error);
    }
    pub fn fail_render_ref(&self, error: &Error) {
        self.fail_render_details(
            error.to_string(),
            error.playback_failure(),
            FailureDetails::from_error(error),
        );
    }
    fn fail_render_with_reason(&self, error: String, reason: PlaybackFailure) {
        self.fail_render_details(error, reason, FailureDetails::default());
    }
    fn fail_render_details(&self, error: String, reason: PlaybackFailure, details: FailureDetails) {
        // Publish shutdown first. An event already holding state completes before
        // this final error is written; later events cannot obtain a mutation guard.
        self.stopped.store(true, Ordering::Release);
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.stopped = true;
        state.loading = false;
        state.paused_for_cache = false;
        if state.fatal_error.is_none() {
            self.diagnostics
                .failure_details(reason, Some(&state), details);
            state.failure_reason = Some(reason);
            state.fatal_error = Some(error.clone());
            state.error = Some(error);
        }
        #[cfg(feature = "gpu-surface")]
        self.pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel_gpu();
    }
    fn control_state(&self) -> Option<std::sync::MutexGuard<'_, PlaybackState>> {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        if self.stopped.load(Ordering::Acquire) {
            None
        } else {
            Some(state)
        }
    }
}

/// Marks unexpected worker exits (including unwinding) as terminal failures.
pub(super) struct WorkerExit(pub Arc<Shared>);
impl Drop for WorkerExit {
    fn drop(&mut self) {
        if !self.0.stopped.load(Ordering::Acquire) {
            self.0
                .fail_render("playback worker exited unexpectedly; recreate Player".into());
        }
    }
}

/// A bounded asynchronous control queue and a latest-frame mailbox.
/// Dropping joins both workers; rendering is freed before the libmpv client.
/// Runtime renderer errors permanently stop this Player; inspect state/diagnostics
/// and recreate the Player to recover. Media/command errors remain recoverable.
pub struct Player {
    shared: Arc<Shared>,
    commands: SyncSender<Vec<CString>>,
    control: Option<JoinHandle<()>>,
    render: Option<JoinHandle<()>>,
}
impl Player {
    pub fn new(config: Config) -> Result<Self> {
        let diagnostics = Arc::new(Diagnostics::new(&config));
        let result = Self::initialize(config, diagnostics.clone());
        if let Err(error) = &result {
            diagnostics.event(Lifecycle::StartupFailed);
            diagnostics.failure_typed(error, None);
        }
        result
    }
    fn initialize(config: Config, diagnostics: Arc<Diagnostics>) -> Result<Self> {
        validate_volume(config.initial_volume)?;
        render::validate_config(&config)?;
        #[cfg(feature = "gpu-surface")]
        let config = {
            let mut config = config;
            if config.gpu_surface.is_some() {
                config.render_backend = RenderBackend::Gpu;
            }
            config
        };
        let effective_hwdec = {
            #[cfg(feature = "gpu-surface")]
            if config.gpu_surface.is_some() {
                super::gpu_surface::hwdec()
            } else {
                render::hwdec(config.hardware_decoding, config.render_backend)
            }
            #[cfg(not(feature = "gpu-surface"))]
            render::hwdec(config.hardware_decoding, config.render_backend)
        };
        diagnostics.effective_hwdec_request(effective_hwdec);
        let loaded = Loaded::load(config.library_path.as_deref())?;
        let handle = unsafe { (loaded.api.create)() };
        if handle.is_null() {
            return Err(Error::ClientCreation);
        }
        let client = Arc::new(Client { loaded, handle });
        let warning_capture = unsafe {
            super::ffi::enable_warning_capture(client.loaded.api.request_log_messages, handle)
        };
        diagnostics.warning_capability(warning_capture);
        for (name, value) in [
            ("config", "no".into()),
            ("vo", "libmpv".into()),
            ("idle", "yes".into()),
            ("terminal", "no".into()),
            ("input-default-bindings", "no".into()),
            ("osd-level", "0".into()),
            ("volume", config.initial_volume.to_string()),
            ("hwdec", effective_hwdec.into()),
        ] {
            let name = CString::new(name).map_err(|_| Error::InvalidInput("option name"))?;
            let value = CString::new(value).map_err(|_| Error::InvalidInput("option value"))?;
            client.loaded.check(
                unsafe { (client.loaded.api.option)(handle, name.as_ptr(), value.as_ptr()) },
                "configure libmpv",
            )?;
        }
        #[cfg(feature = "gpu-surface")]
        if config.gpu_surface.is_some() {
            client.loaded.check(
                unsafe {
                    (client.loaded.api.option)(
                        handle,
                        c"hwdec-software-fallback".as_ptr(),
                        c"no".as_ptr(),
                    )
                },
                "require direct hardware decode",
            )?;
        }
        // These options are compiled out when their scripting engines are absent.
        // Missing capability is safe; any other failure to disable it is an error.
        for name in [c"load-scripts", c"osc"] {
            let code = unsafe { (client.loaded.api.option)(handle, name.as_ptr(), c"no".as_ptr()) };
            check_scripting_option(&client.loaded, code)?;
        }
        let mut option_warnings = Vec::new();
        if config.render_backend != RenderBackend::Cpu {
            // Best-effort stock-mpv quality settings and explicit SDR output.
            // Configure on the creator/control side, never on the renderer worker.
            for (name, value) in [
                (c"scale", c"bicubic"),
                (c"dscale", c"bicubic"),
                (c"cscale", c"bilinear"),
                (c"target-prim", c"bt.709"),
                (c"target-trc", c"bt.1886"),
                (c"tone-mapping", c"bt.2390"),
            ] {
                let code =
                    unsafe { (client.loaded.api.option)(handle, name.as_ptr(), value.as_ptr()) };
                if let Err(e) = client
                    .loaded
                    .check(code, "configure optional GPU quality/SDR option")
                {
                    option_warnings.push(format!("{}: {e}", name.to_string_lossy()));
                }
            }
        }
        client.loaded.check(
            unsafe { (client.loaded.api.initialize)(handle) },
            "initialize libmpv",
        )?;
        for (id, name, format) in [
            (1, c"time-pos", 5),
            (2, c"duration", 5),
            (3, c"pause", 3),
            (4, c"volume", 5),
            (5, c"mute", 3),
            (6, c"video-out-params/w", 4),
            (7, c"video-out-params/h", 4),
            (8, c"video-out-params/dw", 4),
            (9, c"video-out-params/dh", 4),
            (10, c"video-out-params/rotate", 4),
            (11, c"hwdec-current", 1),
            (12, c"paused-for-cache", 3),
            (13, c"chapter-list", 6),
            (14, c"track-list", 6),
            (15, c"speed", 5),
            (16, c"demuxer-cache-state", 6),
        ] {
            client.loaded.check(
                unsafe { (client.loaded.api.observe)(handle, id, name.as_ptr(), format) },
                "observe property",
            )?;
        }
        for (id, name) in [
            (17, c"video-dec-params/pixelformat"),
            (18, c"video-out-params/pixelformat"),
            (19, c"video-dec-params/hw-pixelformat"),
            (20, c"video-dec-params/gamma"),
            (21, c"video-params/gamma"),
        ] {
            let result = client.loaded.check(
                unsafe { (client.loaded.api.observe)(handle, id, name.as_ptr(), 1) },
                "observe direct decode evidence",
            );
            // Preserve the GPU surface requirement. Explicit readback treats
            // optional evidence observation as diagnostic-only.
            #[cfg(feature = "gpu-surface")]
            if config.gpu_surface.is_some() {
                result?;
                continue;
            }
            if result.is_err() {
                tracing::info!(
                    player_id = diagnostics.id,
                    available = false,
                    "mpv decode evidence observation"
                );
            }
        }
        tracing::info!(
            player_id = diagnostics.id,
            unsupported_optional_options = option_warnings.len(),
            "mpv optional renderer configuration"
        );
        let shared = Arc::new(Shared {
            diagnostics,
            stopped: AtomicBool::new(false),
            pipeline: Mutex::new(Pipeline::new(config.clone())),
            state: Mutex::new(PlaybackState {
                volume: config.initial_volume,
                requested_backend: config.render_backend,
                ..PlaybackState::default()
            }),
        });
        shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .option_warnings(option_warnings);
        let (commands, receiver) = mpsc::sync_channel(64);
        let (decoder_tx, decoder_rx) = mpsc::sync_channel(1);
        let control_shared = shared.clone();
        let control_client = client.clone();
        let control = thread::Builder::new()
            .name("mpv-control".into())
            .spawn(move || control_worker(control_client, control_shared, receiver, decoder_rx))?;
        let (ready_sender, ready_receiver) = mpsc::sync_channel(1);
        let render_shared = shared.clone();
        let render = match thread::Builder::new()
            .name("mpv-render".into())
            .spawn(move || render::run(client, render_shared, config, decoder_tx, ready_sender))
        {
            Ok(worker) => worker,
            Err(e) => {
                shared.stopped.store(true, Ordering::Release);
                control.thread().unpark();
                let _ = control.join();
                return Err(e.into());
            }
        };
        if let Err(e) = ready_receiver.recv().unwrap_or(Err(Error::Disconnected)) {
            shared.stopped.store(true, Ordering::Release);
            control.thread().unpark();
            let _ = render.join();
            let _ = control.join();
            return Err(e);
        }
        Ok(Self {
            shared,
            commands,
            control: Some(control),
            render: Some(render),
        })
    }
    fn command(&self, arguments: &[&str]) -> Result<()> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(Error::Disconnected);
        }
        let arguments = arguments
            .iter()
            .map(|s| {
                CString::new(*s).map_err(|_| Error::InvalidInput("command contains a NUL byte"))
            })
            .collect::<Result<Vec<_>>>()?;
        self.commands.try_send(arguments).map_err(|e| match e {
            TrySendError::Full(_) => Error::QueueFull,
            TrySendError::Disconnected(_) => Error::Disconnected,
        })?;
        if let Some(worker) = &self.control {
            worker.thread().unpark();
        }
        Ok(())
    }
    pub fn load(&self, source: &str) -> Result<()> {
        if source.is_empty() {
            return Err(Error::InvalidInput("media path or URL cannot be empty"));
        }
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut pipeline = self
            .shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        self.command(&["loadfile", source, "replace"])?;
        self.shared.diagnostics.new_media();
        self.shared.diagnostics.event(Lifecycle::LoadRequested);
        pipeline.clear(&mut state);
        clear_media_metadata(&mut state);
        Ok(())
    }
    pub fn set_paused(&self, paused: bool) -> Result<()> {
        self.command(&["set", "pause", if paused { "yes" } else { "no" }])
    }
    pub fn toggle_pause(&self) -> Result<()> {
        self.command(&["cycle", "pause"])
    }
    pub fn stop(&self) -> Result<()> {
        self.command(&["stop"])
    }
    /// Seek by a finite relative number of seconds.
    pub fn seek(&self, seconds: f64) -> Result<()> {
        if !seconds.is_finite() {
            return Err(Error::InvalidInput("seek seconds must be finite"));
        }
        self.command(&["seek", &seconds.to_string(), "relative+exact"])
    }
    /// Seek to a finite, nonnegative media time in seconds.
    pub fn seek_absolute(&self, seconds: f64) -> Result<()> {
        if !seconds.is_finite() || seconds < 0.0 {
            return Err(Error::InvalidInput(
                "absolute seek must be finite and nonnegative",
            ));
        }
        self.command(&["seek", &seconds.to_string(), "absolute+exact"])
    }
    /// Set playback rate in libmpv's supported range, 0.01..=100.
    pub fn set_speed(&self, speed: f64) -> Result<()> {
        if !speed.is_finite() || !(0.01..=100.0).contains(&speed) {
            return Err(Error::InvalidInput(
                "speed must be finite and between 0.01 and 100",
            ));
        }
        self.command(&["set", "speed", &speed.to_string()])
    }
    /// Select an observed audio track ID; None disables audio.
    pub fn select_audio(&self, id: Option<i64>) -> Result<()> {
        self.select_track("aid", id)
    }
    /// Select an observed subtitle track ID; None disables subtitles.
    pub fn select_subtitle(&self, id: Option<i64>) -> Result<()> {
        self.select_track("sid", id)
    }
    /// Position subtitles vertically: 0 is the top, 100 the default bottom.
    /// Accepts 0..=100 and persists across media loads on this Player.
    /// Presentation-only: acceptance does not update the observed playback state.
    /// Placement remains subject to subtitle format/styling and libmpv behavior.
    pub fn set_subtitle_position(&self, percent: u8) -> Result<()> {
        if percent > 100 {
            return Err(Error::InvalidInput(
                "subtitle position must be between 0 and 100",
            ));
        }
        self.command(&["set", "sub-pos", &percent.to_string()])
    }
    fn select_track(&self, property: &str, id: Option<i64>) -> Result<()> {
        if id.is_some_and(|id| id < 0) {
            return Err(Error::InvalidInput("track ID must be nonnegative"));
        }
        let value = id.map_or_else(|| "no".to_owned(), |id| id.to_string());
        self.command(&["set", property, &value])
    }
    /// Set a finite volume percentage in 0..=100.
    pub fn set_volume(&self, volume: f64) -> Result<()> {
        validate_volume(volume)?;
        self.command(&["set", "volume", &volume.to_string()])
    }
    pub fn set_muted(&self, muted: bool) -> Result<()> {
        self.command(&["set", "mute", if muted { "yes" } else { "no" }])
    }
    pub fn toggle_mute(&self) -> Result<()> {
        self.command(&["cycle", "mute"])
    }
    /// Coalesced physical viewport resize. Zero dimensions suspend rendering.
    pub fn resize(&self, width: u32, height: u32) -> Result<()> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return Err(Error::Disconnected);
        }
        {
            let mut state = self.shared.control_state().ok_or(Error::Disconnected)?;
            let mut pipeline = self
                .shared
                .pipeline
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            pipeline.viewport = [width, height];
            pipeline.recompute(&mut state)?;
        }
        if let Some(worker) = &self.render {
            worker.thread().unpark();
        }
        Ok(())
    }
    #[cfg(feature = "gpu-surface")]
    pub fn take_gpu_frame(&self) -> Option<super::gpu_surface::GpuSurfaceFrame> {
        if self.shared.stopped.load(Ordering::Acquire) {
            return None;
        }
        self.shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take_gpu()
    }
    pub fn state(&self) -> PlaybackState {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        state.stopped |= self.shared.stopped.load(Ordering::Acquire);
        state
    }
    /// Whether workers have permanently stopped, not whether media reached EOF.
    pub fn is_stopped(&self) -> bool {
        self.shared.stopped.load(Ordering::Acquire)
    }
    pub fn diagnostics(&self) -> RenderDiagnostics {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        self.shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot(&state)
    }
    pub fn take_frame(&self) -> Option<Frame> {
        self.shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
}
impl Drop for Player {
    fn drop(&mut self) {
        self.shared.diagnostics.event(Lifecycle::ShutdownRequested);
        self.shared.stopped.store(true, Ordering::Release);
        #[cfg(feature = "gpu-surface")]
        self.shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .cancel_gpu();
        if let Some(worker) = &self.control {
            worker.thread().unpark();
        }
        if let Some(worker) = self.render.take() {
            worker.thread().unpark();
            let _ = worker.join();
        }
        if let Some(worker) = self.control.take() {
            let _ = worker.join();
        }
        self.shared.diagnostics.event(Lifecycle::WorkersStopped);
    }
}
// SAFETY: property payload must be a live libmpv event; copied values alone escape.
unsafe fn observe_media_metadata(state: &mut PlaybackState, id: u64, property: &Property) {
    match id {
        13 => {
            let chapters = unsafe { metadata::chapters(property) };
            state.chapters_available = chapters.is_some();
            state.chapters = chapters.unwrap_or_default();
        }
        14 => {
            let tracks = unsafe { metadata::tracks(property) };
            state.tracks_available = tracks.is_some();
            state.tracks = tracks.unwrap_or_default();
        }
        16 => state.buffered_ranges = unsafe { metadata::buffered_ranges(property) },
        _ => {}
    }
}

fn clear_media_metadata(state: &mut PlaybackState) {
    state.chapters.clear();
    state.chapters_available = false;
    state.tracks.clear();
    state.tracks_available = false;
    state.buffered_ranges = None;
}

fn check_scripting_option(loaded: &Loaded, code: i32) -> Result<()> {
    if scripting_option_absent(code) {
        Ok(())
    } else {
        loaded.check(code, "disable scripting/OSC")
    }
}
fn scripting_option_absent(code: i32) -> bool {
    // MPV_ERROR_OPTION_NOT_FOUND: options/options.c conditionally registers
    // load-scripts for scripting builds and osc for Lua builds only.
    code == -5
}
fn validate_volume(volume: f64) -> Result<()> {
    if !volume.is_finite() || !(0.0..=100.0).contains(&volume) {
        Err(Error::InvalidInput(
            "volume must be finite and between 0 and 100",
        ))
    } else {
        Ok(())
    }
}
// SAFETY: non-null payloads must have libmpv's documented string-property layout.
unsafe fn copied_string(property: &Property) -> Option<String> {
    if property.format != 1 || property.data.is_null() {
        return None;
    }
    let ptr = unsafe { *property.data.cast::<*const std::ffi::c_char>() };
    if ptr.is_null() {
        None
    } else {
        Some(
            unsafe { CStr::from_ptr(ptr) }
                .to_string_lossy()
                .into_owned(),
        )
    }
}

/// Runs on the existing control worker before its client reference is dropped,
/// including on unwind. Never invoke from the GL/render thread: quarantined
/// clients must remain alive, but should not continue playback intentionally.
struct ControlPauseOnExit<'a> {
    client: &'a Client,
    shared: &'a Shared,
}
impl Drop for ControlPauseOnExit<'_> {
    fn drop(&mut self) {
        pause_on_control_exit(self.shared, |arguments| {
            let pointers = [
                arguments[0].as_ptr(),
                arguments[1].as_ptr(),
                arguments[2].as_ptr(),
                std::ptr::null(),
            ];
            // mpv_command_async copies its arguments; do not await a reply or
            // request stop/reload/GL cleanup while resources are quarantined.
            unsafe { (self.client.loaded.api.command)(self.client.handle, 0, pointers.as_ptr()) }
        });
    }
}

/// Cooperative best effort, not a guarantee that a hung core/driver will pause.
/// Bypasses Player::command's stopped check on purpose. Only a fixed diagnostic
/// category is recorded on enqueue failure; fatal/observed state is untouched.
fn pause_on_control_exit(shared: &Shared, enqueue: impl FnOnce(&[&CStr; 3]) -> c_int) {
    let code = enqueue(&[c"set", c"pause", c"yes"]);
    shared.diagnostics.pause_result(code);
    shared.diagnostics.event(Lifecycle::ControlStopped);
    if code < 0 {
        shared
            .pipeline
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .record_shutdown_pause_failure();
    }
}

fn control_worker(
    client: Arc<Client>,
    shared: Arc<Shared>,
    receiver: Receiver<Vec<CString>>,
    decoder: Receiver<SyncSender<Result<()>>>,
) {
    shared.diagnostics.event(Lifecycle::ControlStarted);
    let _exit = WorkerExit(shared.clone());
    let _pause_on_exit = ControlPauseOnExit {
        client: &client,
        shared: &shared,
    };
    while !shared.stopped.load(Ordering::Acquire) {
        for reply in decoder.try_iter() {
            if shared.stopped.load(Ordering::Acquire) {
                let _ = reply.send(Err(Error::Disconnected));
                break;
            }
            let result = client.loaded.check(
                unsafe {
                    (client.loaded.api.option)(
                        client.handle,
                        c"hwdec".as_ptr(),
                        c"auto-copy".as_ptr(),
                    )
                },
                "set fallback hardware decoding",
            );
            if result.is_ok() {
                shared.diagnostics.effective_hwdec_request("auto-copy");
            }
            let _ = reply.send(result);
        }
        // Bounded batches prevent command floods starving events or shutdown.
        for arguments in receiver.try_iter().take(64) {
            if shared.stopped.load(Ordering::Acquire) {
                break;
            }
            let mut pointers: Vec<_> = arguments.iter().map(|s| s.as_ptr()).collect();
            pointers.push(std::ptr::null());
            let code = unsafe { (client.loaded.api.command)(client.handle, 0, pointers.as_ptr()) };
            if let Err(e) = client.loaded.check(code, "queue command") {
                shared.set_error_typed(e);
            }
        }
        for _ in 0..256 {
            if shared.stopped.load(Ordering::Acquire) {
                break;
            }
            // Event payloads are consumed completely before the next wait_event call.
            let event = unsafe { &*(client.loaded.api.wait)(client.handle, 0.0) };
            if event.id == 0 {
                break;
            }
            // Recheck under the mutation lock: an already-read START_FILE must
            // not erase a fatal renderer error after shutdown was published.
            let Some(mut state) = shared.control_state() else {
                break;
            };
            match event.id {
                2 if !event.data.is_null() => {
                    // Consume borrowed ABI strings now. Only the classified
                    // static tokens survive the next wait_event call.
                    let message = unsafe { &*event.data.cast::<LogMessage>() };
                    let copied = unsafe { classify_native_message(message) };
                    shared.diagnostics.native_message(copied);
                }
                1 => {
                    shared.stopped.store(true, Ordering::Release);
                    state.stopped = true;
                    state.loading = false;
                    state.paused_for_cache = false;
                    break;
                }
                5 if event.error < 0 => {
                    let error = client.loaded.check(event.error, "command").err();
                    state.failure_reason = error.as_ref().map(Error::playback_failure);
                    if let Some(error) = &error {
                        shared.diagnostics.failure_typed(error, Some(&state));
                    }
                    state.error = error.map(|e| e.to_string());
                    state.loading = false;
                }
                6 => {
                    shared.diagnostics.new_media();
                    shared.diagnostics.event(Lifecycle::StartFile);
                    state.loading = true;
                    state.media_stopped = false;
                    state.paused_for_cache = false;
                    state.ended = false;
                    state.clear_media_failure();
                    state.position = 0.0;
                    state.duration = None;
                    clear_media_metadata(&mut state);
                    shared
                        .pipeline
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clear(&mut state);
                }
                7 => {
                    shared
                        .pipeline
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .clear(&mut state);
                    state.loading = false;
                    state.media_stopped = true;
                    state.paused_for_cache = false;
                    clear_media_metadata(&mut state);
                    if !event.data.is_null() {
                        let end = unsafe { &*event.data.cast::<EndFile>() };
                        shared.diagnostics.end_file(end.reason, end.error);
                        state.ended = end.reason == 0;
                        if end.reason == 4 {
                            state.failure_reason = Some(PlaybackFailure::MediaLoadFailed);
                            shared
                                .diagnostics
                                .failure(PlaybackFailure::MediaLoadFailed, Some(&state));
                            state.error = client
                                .loaded
                                .check(end.error, "playback")
                                .err()
                                .map(|e| e.to_string())
                                .or(Some("playback failed".into()));
                        }
                    } else {
                        shared.diagnostics.end_file(-1, event.error);
                    }
                }
                8 => {
                    shared.diagnostics.event(Lifecycle::FileLoaded);
                    state.loading = false;
                }
                22 if !event.data.is_null() => {
                    let property = unsafe { &*event.data.cast::<Property>() };
                    match (event.userdata, property.format, property.data.is_null()) {
                        (13 | 14 | 16, _, _) if !state.media_stopped => unsafe {
                            observe_media_metadata(&mut state, event.userdata, property);
                        },
                        (15, 5, false) => {
                            let speed = unsafe { *property.data.cast::<f64>() };
                            if speed.is_finite() && speed > 0.0 {
                                state.speed = speed;
                            }
                        }
                        (1 | 2 | 4, 5, false) => {
                            let value = unsafe { *property.data.cast::<f64>() };
                            if value.is_finite() {
                                match event.userdata {
                                    1 => state.position = value.max(0.0),
                                    2 => state.duration = Some(value.max(0.0)),
                                    4 => state.volume = value,
                                    _ => {}
                                }
                            }
                        }
                        (3 | 5 | 12, 3, false) => {
                            let value = unsafe { *property.data.cast::<c_int>() } != 0;
                            if event.userdata == 3 {
                                state.paused = value;
                            } else if event.userdata == 5 {
                                state.muted = value;
                            } else {
                                state.paused_for_cache = value;
                            }
                        }
                        (2, 0, _) => state.duration = None,
                        (6..=10, _, _) => {
                            let value = if property.format == 4 && !property.data.is_null() {
                                Some(unsafe { *property.data.cast::<i64>() })
                            } else {
                                None
                            };
                            let mut pipeline =
                                shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                            pipeline.metadata[(event.userdata - 6) as usize] = value;
                            if event.userdata == 6 {
                                pipeline.video_absent = value.is_none_or(|v| v <= 0);
                            }
                            if let Err(e) = pipeline.recompute(&mut state) {
                                state.failure_reason = Some(e.playback_failure());
                                shared.diagnostics.failure_typed(&e, Some(&state));
                                state.error = Some(e.to_string());
                            }
                        }
                        (11, _, _) => {
                            // The string and its pointer are borrowed only until wait_event.
                            state.hardware_decoder = unsafe { copied_string(property) };
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
            if !state.loading
                && !state.media_stopped
                && (event.id == 8 || (event.id == 22 && matches!(event.userdata, 6..=11 | 17..=21)))
            {
                // Never hold UI-visible state/pipeline locks during client calls.
                drop(state);
                refresh_decode_evidence(&client, &shared);
            }
        }
        thread::park_timeout(Duration::from_millis(10));
    }
}

// SAFETY: message and each non-null string must belong to the current event.
// Classify before wait_event invalidates them; no borrowed/raw text escapes.
unsafe fn classify_native_message(message: &LogMessage) -> NativeMessage {
    unsafe fn text<'a>(ptr: *const std::ffi::c_char) -> &'a str {
        if ptr.is_null() {
            ""
        } else {
            unsafe { CStr::from_ptr(ptr) }.to_str().unwrap_or("")
        }
    }
    NativeMessage::classify(
        unsafe { text(message.prefix) },
        unsafe { text(message.level) },
        unsafe { text(message.text) },
        message.log_level,
    )
}

fn refresh_decode_evidence(client: &Client, shared: &Shared) {
    // Client property calls belong exclusively on the existing control worker.
    // Query all fields together after FILE_LOADED/property changes, rather than
    // accepting queued string values from a previous source. video-params is
    // effective filter INPUT (including inference/overrides), not rendered VO
    // output. Keep decoder gamma separately so explicit HDR cannot be hidden.
    let generation = {
        let pipeline = shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
        pipeline.generation
    };
    let evidence = [
        c"hwdec-current",
        c"video-dec-params/pixelformat",
        c"video-out-params/pixelformat",
        c"video-dec-params/hw-pixelformat",
        c"video-dec-params/gamma",
        c"current-tracks/video/codec",
        c"video-params/gamma",
    ]
    .map(|name| {
        let ptr = unsafe { (client.loaded.api.get_property_string)(client.handle, name.as_ptr()) };
        if ptr.is_null() {
            return None;
        }
        let value = unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned();
        unsafe {
            (client.loaded.api.free)(ptr.cast());
        }
        Some(value)
    });
    let Some(state) = shared.control_state() else {
        return;
    };
    let mut pipeline = shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
    if pipeline.generation != generation {
        return;
    }
    shared.diagnostics.evidence(
        Evidence::new(
            std::array::from_fn(|index| evidence[index].as_deref()),
            state.source_size.unwrap_or([0; 2]),
            state.render_size,
        )
        .with_effective_transfer(evidence[6].as_deref()),
    );
    #[cfg(feature = "gpu-surface")]
    if pipeline.gpu_surface_enabled() {
        pipeline.update_decode_evidence(
            generation,
            std::array::from_fn(|i| {
                evidence[if i == 5 { 6 } else { i }]
                    .clone()
                    .unwrap_or_default()
            }),
        );
    }
    #[cfg(not(feature = "gpu-surface"))]
    let _ = &mut pipeline;
}

#[cfg(test)]
mod tests {
    use super::*;
    fn mock_player() -> (Player, Receiver<Vec<CString>>) {
        let (commands, receiver) = mpsc::sync_channel(1);
        (
            Player {
                shared: Arc::new(Shared {
                    diagnostics: Arc::new(Diagnostics::new(&Config::default())),
                    stopped: AtomicBool::new(false),
                    pipeline: Mutex::new(Pipeline::new(Config::default())),
                    state: Mutex::new(PlaybackState::default()),
                }),
                commands,
                control: None,
                render: None,
            },
            receiver,
        )
    }
    #[test]
    fn borrowed_native_warning_strings_are_consumed_before_event_lifetime_ends() {
        let copied = {
            let module = CString::new("stream/curl").unwrap();
            let level = CString::new("warn").unwrap();
            let text = CString::new("Failed to connect https://user:PASSWORD_MARKER@example.invalid/private?signature=SIGNED_SECRET 日本語").unwrap();
            unsafe {
                classify_native_message(&LogMessage {
                    prefix: module.as_ptr(),
                    level: level.as_ptr(),
                    text: text.as_ptr(),
                    log_level: 30,
                })
            }
        };
        // The source CStrings are gone; the record owns only static reviewed tokens.
        let output = format!("{copied:?}");
        assert!(output.contains("NetworkFailed"));
        for forbidden in [
            "PASSWORD_MARKER",
            "SIGNED_SECRET",
            "https://",
            "日本語",
            "private",
        ] {
            assert!(!output.contains(forbidden));
        }
        let empty = unsafe {
            classify_native_message(&LogMessage {
                prefix: std::ptr::null(),
                level: std::ptr::null(),
                text: std::ptr::null(),
                log_level: 30,
            })
        };
        assert!(format!("{empty:?}").contains("Unclassified"));
    }
    #[test]
    fn control_exit_enqueues_one_async_pause_even_after_fatal_stop() {
        let (player, _) = mock_player();
        player
            .shared
            .fail_render("fatal GPU completion uncertainty".into());
        let before = player.state();
        let mut calls = 0;
        pause_on_control_exit(&player.shared, |arguments| {
            calls += 1;
            assert_eq!(arguments, &[c"set", c"pause", c"yes"]);
            0
        });
        assert_eq!(calls, 1);
        assert_eq!(player.state(), before);
        assert!(!player.diagnostics().shutdown_pause_enqueue_failed);
    }
    #[test]
    fn control_exit_pause_enqueue_failure_preserves_fatal_state_and_records_category() {
        let (player, _) = mock_player();
        player.shared.fail_render("original fatal failure".into());
        let before = player.state();
        let mut calls = 0;
        pause_on_control_exit(&player.shared, |_| {
            calls += 1;
            -1
        });
        assert_eq!(calls, 1);
        assert_eq!(player.state(), before);
        assert!(player.diagnostics().shutdown_pause_enqueue_failed);
    }
    #[test]
    fn dock_commands_enqueue_exact_arguments_without_optimistic_state() {
        let (player, receiver) = mock_player();
        let initial = player.state();
        type CommandCase<'a> = (&'a dyn Fn() -> Result<()>, &'a [&'a str]);
        let cases: &[CommandCase<'_>] = &[
            (
                &|| player.seek_absolute(12.5),
                &["seek", "12.5", "absolute+exact"],
            ),
            (&|| player.set_speed(1.5), &["set", "speed", "1.5"]),
            (&|| player.select_audio(Some(7)), &["set", "aid", "7"]),
            (&|| player.select_audio(None), &["set", "aid", "no"]),
            (&|| player.select_subtitle(Some(4)), &["set", "sid", "4"]),
            (&|| player.select_subtitle(None), &["set", "sid", "no"]),
        ];
        for (command, expected) in cases {
            command().unwrap();
            let arguments = receiver.try_recv().unwrap();
            let actual: Vec<_> = arguments.iter().map(|a| a.to_str().unwrap()).collect();
            assert_eq!(actual, *expected);
            assert_eq!(player.state(), initial);
        }
    }
    #[test]
    fn subtitle_position_validation_enqueue_and_lifecycle() {
        let (player, receiver) = mock_player();
        let initial = player.state();
        for percent in [101, 150, u8::MAX] {
            assert!(matches!(
                player.set_subtitle_position(percent),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(receiver.try_recv().is_err());
        for (percent, expected) in [(0, "0"), (85, "85"), (100, "100")] {
            player.set_subtitle_position(percent).unwrap();
            let arguments = receiver.try_recv().unwrap();
            let actual: Vec<_> = arguments.iter().map(|a| a.to_str().unwrap()).collect();
            assert_eq!(actual, ["set", "sub-pos", expected]);
            assert_eq!(player.state(), initial);
        }
        player.set_subtitle_position(85).unwrap();
        assert!(matches!(
            player.set_subtitle_position(100),
            Err(Error::QueueFull)
        ));
        receiver.try_recv().unwrap();
        player.shared.stopped.store(true, Ordering::Release);
        assert!(matches!(
            player.set_subtitle_position(100),
            Err(Error::Disconnected)
        ));
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn dock_commands_reject_invalid_values_and_respect_queue_lifecycle() {
        let (player, receiver) = mock_player();
        for seconds in [-1.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                player.seek_absolute(seconds),
                Err(Error::InvalidInput(_))
            ));
        }
        for speed in [0.0, -1.0, 0.009, 101.0, f64::NAN, f64::INFINITY] {
            assert!(matches!(
                player.set_speed(speed),
                Err(Error::InvalidInput(_))
            ));
        }
        assert!(matches!(
            player.select_audio(Some(-1)),
            Err(Error::InvalidInput(_))
        ));
        assert!(matches!(
            player.select_subtitle(Some(-1)),
            Err(Error::InvalidInput(_))
        ));
        assert!(receiver.try_recv().is_err());
        for speed in [0.01, 100.0] {
            player.set_speed(speed).unwrap();
            receiver.try_recv().unwrap();
        }
        player.seek_absolute(0.0).unwrap();
        assert!(matches!(
            player.select_subtitle(None),
            Err(Error::QueueFull)
        ));
        receiver.try_recv().unwrap();
        player.shared.stopped.store(true, Ordering::Release);
        assert!(matches!(
            player.seek_absolute(0.0),
            Err(Error::Disconnected)
        ));
        assert!(matches!(player.set_speed(1.0), Err(Error::Disconnected)));
        assert!(matches!(
            player.select_audio(None),
            Err(Error::Disconnected)
        ));
        assert!(matches!(
            player.select_subtitle(None),
            Err(Error::Disconnected)
        ));
        assert!(receiver.try_recv().is_err());
    }
    #[test]
    fn metadata_availability_distinguishes_empty_unavailable_invalid_and_reset() {
        use super::super::ffi::{Node, NodeList, NodeValue};
        let mut state = PlaybackState::default();
        assert!(!state.chapters_available && !state.tracks_available);
        let list = NodeList {
            num: 0,
            values: std::ptr::null(),
            keys: std::ptr::null(),
        };
        let mut node = Node {
            format: 7,
            value: NodeValue { list: &list },
        };
        let empty = Property {
            name: std::ptr::null(),
            format: 6,
            data: (&mut node as *mut Node).cast(),
        };
        let unavailable = Property {
            name: std::ptr::null(),
            format: 0,
            data: std::ptr::null_mut(),
        };
        let mut invalid_node = Node {
            format: 5,
            value: NodeValue { double: 1.0 },
        };
        let invalid = Property {
            name: std::ptr::null(),
            format: 6,
            data: (&mut invalid_node as *mut Node).cast(),
        };
        for failure in [&unavailable, &invalid] {
            for id in [13, 14] {
                unsafe { observe_media_metadata(&mut state, id, &empty) };
            }
            assert!(state.chapters_available && state.tracks_available);
            assert!(state.chapters.is_empty() && state.tracks.is_empty());
            for id in [13, 14] {
                unsafe { observe_media_metadata(&mut state, id, failure) };
            }
            assert!(!state.chapters_available && !state.tracks_available);
            assert!(state.chapters.is_empty() && state.tracks.is_empty());
        }
        for id in [13, 14] {
            unsafe { observe_media_metadata(&mut state, id, &empty) };
        }
        clear_media_metadata(&mut state);
        assert!(!state.chapters_available && !state.tracks_available);
        assert!(state.chapters.is_empty() && state.tracks.is_empty());
        // Accepted load also resets availability immediately, before worker events.
        let (player, _receiver) = mock_player();
        {
            let mut observed = player.shared.state.lock().unwrap();
            observed.chapters_available = true;
            observed.tracks_available = true;
        }
        player.load("new.mkv").unwrap();
        let observed = player.state();
        assert!(!observed.chapters_available && !observed.tracks_available);
    }
    #[test]
    fn clearing_media_metadata_does_not_reset_session_speed() {
        let mut state = PlaybackState {
            speed: 1.5,
            chapters: vec![crate::Chapter {
                time: 1.0,
                title: None,
            }],
            tracks: vec![crate::Track {
                id: 1,
                kind: crate::TrackKind::Audio,
                title: None,
                lang: None,
                selected: true,
            }],
            buffered_ranges: Some(vec![crate::BufferedRange {
                start: 0.0,
                end: 2.0,
            }]),
            ..PlaybackState::default()
        };
        let unavailable = Property {
            name: std::ptr::null(),
            format: 0,
            data: std::ptr::null_mut(),
        };
        for id in [13, 14, 16] {
            unsafe { observe_media_metadata(&mut state, id, &unavailable) };
        }
        assert!(state.chapters.is_empty());
        assert!(state.tracks.is_empty());
        assert!(state.buffered_ranges.is_none());
        clear_media_metadata(&mut state);
        assert!(state.chapters.is_empty());
        assert!(state.tracks.is_empty());
        assert_eq!(state.buffered_ranges, None);
        assert_eq!(state.speed, 1.5);
    }
    #[test]
    fn safe_failure_reasons_survive_fatal_events_and_clear_only_for_new_media() {
        let (player, _receiver) = mock_player();
        assert_eq!(player.state().failure_reason, None);
        player.shared.set_error_typed(Error::Mpv {
            operation: "playback",
            code: -13,
            message: "SECRET https://user:password@example.invalid/private".into(),
        });
        assert_eq!(
            player.state().failure_reason,
            Some(PlaybackFailure::MediaLoadFailed)
        );
        player.shared.control_state().unwrap().clear_media_failure();
        assert_eq!(player.state().failure_reason, None);
        player.shared.fail_render_typed(Error::Gpu(
            "completion GL wait incomplete: SECRET /private/path".into(),
        ));
        assert_eq!(
            player.state().failure_reason,
            Some(PlaybackFailure::GpuCompletionFailed)
        );
        player.shared.set_error_typed(Error::Allocation);
        player.shared.fail_render_typed(Error::Library {
            attempts: "SECRET".into(),
        });
        player.shared.state.lock().unwrap().clear_media_failure();
        assert_eq!(
            player.state().failure_reason,
            Some(PlaybackFailure::GpuCompletionFailed)
        );
        assert!(
            !player
                .state()
                .failure_reason
                .unwrap()
                .message()
                .contains("SECRET")
        );
    }
    #[test]
    fn fatal_render_failure_rejects_control_and_preserves_last_diagnostics() {
        let (player, receiver) = mock_player();
        player.shared.state.lock().unwrap().render_size = [640, 480];
        player
            .shared
            .fail_render("lost GPU context; recreate Player".into());
        assert!(player.shared.stopped.load(Ordering::Acquire));
        assert!(player.is_stopped());
        assert!(player.state().stopped);
        assert_eq!(
            player.state().fatal_error.as_deref(),
            Some("lost GPU context; recreate Player")
        );
        assert!(matches!(player.load("later.mkv"), Err(Error::Disconnected)));
        assert!(matches!(player.stop(), Err(Error::Disconnected)));
        assert!(matches!(player.resize(320, 240), Err(Error::Disconnected)));
        assert!(receiver.try_recv().is_err());
        // Simulate a START_FILE already read by the control worker: every event
        // must acquire this exact guard before clearing/mutating observed state.
        assert!(player.shared.control_state().is_none());
        player
            .shared
            .set_error("late nonfatal command error".into());
        assert_eq!(
            player.state().error.as_deref(),
            Some("lost GPU context; recreate Player")
        );
        assert_eq!(player.diagnostics().render_size, [640, 480]);
    }
    #[test]
    fn in_flight_event_mutation_cannot_outlive_fatal_error_recording() {
        let (player, _receiver) = mock_player();
        let mut already_read_event = player.shared.control_state().unwrap();
        let shared = player.shared.clone();
        let failing_renderer =
            thread::spawn(move || shared.fail_render("fatal renderer failure".into()));
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !player.shared.stopped.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
        // An event which acquired the guard before failure may finish, but the
        // fatal writer waits for this guard and has the final state mutation.
        already_read_event.error = None;
        drop(already_read_event);
        failing_renderer.join().unwrap();
        assert_eq!(
            player.state().error.as_deref(),
            Some("fatal renderer failure")
        );
        assert!(player.shared.control_state().is_none());
    }
    #[test]
    fn nonfatal_media_error_keeps_control_events_recoverable() {
        let (player, receiver) = mock_player();
        player.shared.set_error("invalid media".into());
        assert!(!player.shared.stopped.load(Ordering::Acquire));
        let mut state = player
            .shared
            .control_state()
            .expect("nonfatal error permits START_FILE");
        state.error = None;
        drop(state);
        player.load("valid.mkv").unwrap();
        assert!(receiver.try_recv().is_ok());
        assert!(player.state().error.is_none());
    }
    #[test]
    fn classified_startup_failure_is_not_replaced_by_unexpected_exit() {
        let (player, _) = mock_player();
        let error = Error::Gpu("this proof currently supports Intel Mesa only".into());
        let reason = error.playback_failure();
        player
            .shared
            .fail_render_with_reason(error.to_string(), reason);
        drop(WorkerExit(player.shared.clone()));
        assert_eq!(
            player.state().failure_reason,
            Some(PlaybackFailure::GpuDeviceUnsupported)
        );
        assert!(player.state().stopped);
    }
    #[test]
    fn unexpected_worker_exit_is_terminal_and_keeps_original_failure() {
        let (player, _receiver) = mock_player();
        drop(WorkerExit(player.shared.clone()));
        let state = player.state();
        assert!(state.stopped);
        assert!(state.fatal_error.is_some());
        assert_eq!(state.error, state.fatal_error);
        player.shared.set_error("late error".into());
        player.shared.fail_render("second worker failure".into());
        assert_eq!(player.state().fatal_error, state.fatal_error);
        assert_eq!(player.state().error, state.error);
    }
    #[test]
    fn dropping_player_joins_both_workers() {
        let (mut player, _receiver) = mock_player();
        let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        for render in [true, false] {
            let shared = player.shared.clone();
            let completed = completed.clone();
            let worker = thread::spawn(move || {
                while !shared.stopped.load(Ordering::Acquire) {
                    thread::park_timeout(Duration::from_millis(10));
                }
                completed.fetch_add(1, Ordering::AcqRel);
            });
            if render {
                player.render = Some(worker);
            } else {
                player.control = Some(worker);
            }
        }
        drop(player);
        assert_eq!(completed.load(Ordering::Acquire), 2);
    }
    #[test]
    fn string_properties_are_copied_and_tolerate_missing_values() {
        let text = CString::new(vec![b'a', 0xff]).unwrap();
        let mut ptr = text.as_ptr();
        let mut property = Property {
            name: std::ptr::null(),
            format: 1,
            data: (&mut ptr as *mut *const std::ffi::c_char).cast(),
        };
        assert_eq!(unsafe { copied_string(&property) }.unwrap(), "a�");
        property.format = 0;
        assert!(unsafe { copied_string(&property) }.is_none());
        property.format = 1;
        property.data = std::ptr::null_mut();
        assert!(unsafe { copied_string(&property) }.is_none());
        let mut ptr: *const std::ffi::c_char = std::ptr::null();
        property.data = (&mut ptr as *mut *const std::ffi::c_char).cast();
        assert!(unsafe { copied_string(&property) }.is_none());
    }
    #[test]
    #[ignore = "requires installed libmpv and ffmpeg; native 4K CPU rendering"]
    fn real_source_native_4k() {
        let path = std::env::temp_dir().join(format!("mpv-gpui-4k-{}.mkv", std::process::id()));
        assert!(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=red:s=3840x2160:r=1",
                    "-t",
                    "2",
                    "-c:v",
                    "mpeg4"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let player = Player::new(Config {
            render_backend: RenderBackend::Cpu,
            hardware_decoding: HardwareDecoding::Disabled,
            ..Config::default()
        })
        .unwrap();
        player.resize(640, 360).unwrap();
        player.load(path.to_str().unwrap()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(frame) = player.take_frame()
                && [frame.width, frame.height] == [3840, 2160]
            {
                assert_eq!(frame.pixels.len(), 3840 * 2160 * 4);
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no native 4K frame: {:?}",
                player.state()
            );
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(player.state().source_size, Some([3840, 2160]));
        drop(player);
        let _ = std::fs::remove_file(path);
    }
    #[test]
    fn input_errors_do_not_enqueue() {
        let (player, receiver) = mock_player();
        assert!(player.load("").is_err());
        assert!(player.load("invalid\0path").is_err());
        assert!(player.seek(f64::NAN).is_err());
        assert!(player.set_volume(f64::INFINITY).is_err());
        assert!(receiver.try_recv().is_err());
        player.load("a file with spaces.mp4").unwrap();
        let command = receiver.try_recv().unwrap();
        assert_eq!(command[1].to_str().unwrap(), "a file with spaces.mp4");
    }
    #[test]
    fn command_backpressure_and_disconnect() {
        let (player, receiver) = mock_player();
        player.stop().unwrap();
        assert!(matches!(player.stop(), Err(Error::QueueFull)));
        drop(receiver);
        assert!(matches!(player.stop(), Err(Error::Disconnected)));
    }
    #[test]
    fn shutdown_rejects_commands_and_resize() {
        let (player, _receiver) = mock_player();
        player.shared.stopped.store(true, Ordering::Release);
        assert!(matches!(player.stop(), Err(Error::Disconnected)));
        assert!(matches!(player.resize(10, 10), Err(Error::Disconnected)));
    }
    #[test]
    fn absent_scripting_options_are_optional_but_other_errors_are_not() {
        assert!(scripting_option_absent(-5));
        for code in [-3, -4, -6, -7, -12, 0] {
            assert!(!scripting_option_absent(code));
        }
    }
    #[test]
    fn volume_validation() {
        for invalid in [f64::NAN, f64::INFINITY, -1.0, 101.0] {
            assert!(validate_volume(invalid).is_err());
        }
        assert!(validate_volume(0.0).is_ok());
        assert!(validate_volume(100.0).is_ok());
    }
    #[test]
    fn mailbox_replaces_and_takes() {
        let frame = Mutex::new(None);
        for width in [1, 2, 3] {
            *frame.lock().unwrap() = Some(Frame {
                width,
                height: 1,
                pixels: vec![],
            });
        }
        assert_eq!(frame.lock().unwrap().take().unwrap().width, 3);
        assert!(frame.lock().unwrap().take().is_none());
    }
    #[test]
    fn missing_library_is_actionable() {
        let result = Player::new(Config {
            library_path: Some("/definitely/not/a/libmpv-library".into()),
            render_backend: RenderBackend::Cpu,
            ..Config::default()
        });
        assert!(matches!(result, Err(Error::Library { .. })));
    }
    #[test]
    #[ignore = "requires installed libmpv and ffmpeg; exercises real software rendering"]
    fn real_mpv_render_and_teardown() {
        let path = std::env::temp_dir().join(format!("mpv-gpui-test-{}.mkv", std::process::id()));
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=c=red:s=64x48:r=10",
                "-t",
                "2",
                "-c:v",
                "mpeg4",
            ])
            .arg(&path)
            .status()
            .expect("ffmpeg is required");
        assert!(status.success());
        let player = Player::new(Config {
            render_backend: RenderBackend::Cpu,
            resolution: ResolutionPolicy::Viewport,
            ..Config::default()
        })
        .unwrap();
        player.resize(64, 48).unwrap();
        player.load(path.to_str().unwrap()).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        let mut saw_red = false;
        while std::time::Instant::now() < deadline {
            if let Some(frame) = player.take_frame()
                && frame
                    .pixels
                    .chunks_exact(4)
                    .any(|p| p[0] > 200 && p[1] < 40 && p[2] < 40 && p[3] == 255)
            {
                saw_red = true;
                break;
            }
            thread::sleep(Duration::from_millis(20));
        }
        assert!(saw_red, "no red RGBA frame; state: {:?}", player.state());
        assert!(player.seek(f64::NAN).is_err());
        assert!(player.load("bad\0path").is_err());
        player.set_paused(true).unwrap();
        wait_for(&player, |s| s.paused && s.duration.is_some());
        player.seek(0.2).unwrap();
        wait_for(&player, |s| s.position >= 0.15);
        player.resize(32, 24).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(frame) = player.take_frame()
                && frame.width == 32
            {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "paused resize did not redraw"
            );
            thread::sleep(Duration::from_millis(10));
        }
        player.set_paused(false).unwrap();
        wait_for(&player, |s| s.ended);
        player.load("/definitely/not/a/video-file").unwrap();
        wait_for(&player, |s| s.error.is_some() && !s.loading);
        player.stop().unwrap();
        drop(player);
        let _ = std::fs::remove_file(path);
        // Exercise context initialization and destruction repeatedly without media.
        for _ in 0..3 {
            drop(
                Player::new(Config {
                    render_backend: RenderBackend::Cpu,
                    resolution: ResolutionPolicy::Viewport,
                    ..Config::default()
                })
                .unwrap(),
            );
        }
    }
    struct TemporaryMedia(std::path::PathBuf);
    impl TemporaryMedia {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "mpv-gpui-lifecycle-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            std::fs::create_dir(&path).unwrap();
            Self(path)
        }
        fn clip(&self, name: &str, filter: &str, audio: bool) -> std::path::PathBuf {
            let path = self.0.join(name);
            let mut command = std::process::Command::new("ffmpeg");
            command.args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                filter,
                "-t",
                if audio { "2" } else { "4" },
            ]);
            command.args(if audio {
                ["-c:a", "pcm_s16le"]
            } else {
                ["-c:v", "mpeg4"]
            });
            assert!(
                command
                    .arg(&path)
                    .status()
                    .expect("ffmpeg required for opt-in lifecycle tests")
                    .success()
            );
            path
        }
    }
    impl Drop for TemporaryMedia {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn wait_frame(player: &Player, size: [u32; 2], color: [u8; 3]) -> Frame {
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        loop {
            if let Some(frame) = player.take_frame()
                && [frame.width, frame.height] == size
                && frame.pixels.chunks_exact(4).any(|p| {
                    p[3] == 255
                        && p[..3]
                            .iter()
                            .zip(color)
                            .all(|(&actual, expected)| actual.abs_diff(expected) < 40)
                })
            {
                return frame;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no expected {size:?}/{color:?} frame; state={:?}, diagnostics={:?}",
                player.state(),
                player.diagnostics()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
    fn player_media_lifecycle(backend: RenderBackend) {
        let media = TemporaryMedia::new();
        let red = media.clip("red.mkv", "color=c=red:s=64x48:r=10", false);
        let blue = media.clip("blue.mkv", "color=c=blue:s=96x64:r=10", false);
        let audio = media.clip("audio.wav", "sine=frequency=440:sample_rate=8000", true);
        let config = Config {
            render_backend: backend,
            hardware_decoding: HardwareDecoding::Auto,
            ..Config::default()
        };
        let player = Player::new(config.clone())
            .unwrap_or_else(|e| panic!("requested {backend:?} capability unavailable: {e}"));
        player.resize(160, 120).unwrap();
        player.load(red.to_str().unwrap()).unwrap();
        wait_frame(&player, [64, 48], [255, 0, 0]);
        assert_eq!(player.state().active_backend, backend);
        player.set_paused(true).unwrap();
        wait_for(&player, |s| s.paused);
        player.seek(0.5).unwrap();
        wait_for(&player, |s| s.position >= 0.4);
        // Source is native, not a disguised viewport cap, even while paused.
        player.resize(32, 24).unwrap();
        assert_eq!(player.state().render_size, [64, 48]);
        assert_eq!(player.state().source_display_size, Some([64, 48]));
        player.load(blue.to_str().unwrap()).unwrap();
        wait_frame(&player, [96, 64], [0, 0, 255]);
        assert_eq!(player.state().source_size, Some([96, 64]));
        assert!(
            player.state().paused,
            "source replacement unexpectedly unpaused"
        );
        player.resize(320, 240).unwrap();
        assert_eq!(player.state().render_size, [96, 64]);
        // Existing/late GPU readbacks must not leak across a no-video load.
        player.load(audio.to_str().unwrap()).unwrap();
        wait_for(&player, |s| {
            !s.loading && s.duration.is_some_and(|d| d > 1.8 && d < 2.2)
        });
        let deadline = std::time::Instant::now() + Duration::from_millis(250);
        while std::time::Instant::now() < deadline {
            assert!(
                player.take_frame().is_none(),
                "audio-only media delivered a stale video frame"
            );
            let state = player.state();
            assert_eq!(state.source_size, None);
            assert_eq!(state.source_display_size, None);
            assert_eq!(state.render_size, [0, 0]);
            thread::sleep(Duration::from_millis(5));
        }
        player
            .load(media.0.join("missing.mkv").to_str().unwrap())
            .unwrap();
        wait_for(&player, |s| s.error.is_some() && !s.loading);
        assert!(player.take_frame().is_none());
        player.load(red.to_str().unwrap()).unwrap();
        wait_frame(&player, [64, 48], [255, 0, 0]);
        player.set_paused(false).unwrap();
        player.seek(3.7).unwrap();
        wait_for(&player, |s| s.ended);
        assert!(player.take_frame().is_none());
        let diagnostics = player.diagnostics();
        assert!(diagnostics.delivered_frames > 0);
        if backend == RenderBackend::Gpu {
            assert!(diagnostics.readback_bytes > 0);
        }
        // Decoder reporting is observed; hardware activation is driver/codec dependent.
        eprintln!("Player lifecycle {backend:?}: {diagnostics:?}");
        drop(player);
        let viewport = Player::new(Config {
            resolution: ResolutionPolicy::Viewport,
            ..config.clone()
        })
        .unwrap();
        viewport.resize(80, 60).unwrap();
        viewport.set_paused(true).unwrap();
        viewport.load(red.to_str().unwrap()).unwrap();
        wait_frame(&viewport, [80, 60], [255, 0, 0]);
        wait_for(&viewport, |s| s.paused);
        viewport.resize(32, 24).unwrap();
        wait_frame(&viewport, [32, 24], [255, 0, 0]);
        assert_eq!(viewport.state().source_size, Some([64, 48]));
        drop(viewport);
        for _ in 0..3 {
            drop(Player::new(config.clone()).unwrap());
        }
    }
    #[test]
    #[ignore = "requires libmpv and ffmpeg; real Cinema Dock metadata and commands"]
    fn real_cinema_dock_metadata() {
        let media = TemporaryMedia::new();
        let metadata = media.0.join("chapters.txt");
        let subtitles = media.0.join("captions.srt");
        let path = media.0.join("cinema.mkv");
        std::fs::write(&metadata, ";FFMETADATA1\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=0\nEND=2000\ntitle=Opening\n[CHAPTER]\nTIMEBASE=1/1000\nSTART=2000\nEND=4000\ntitle=Closing\n").unwrap();
        std::fs::write(
            &subtitles,
            "1\n00:00:00,000 --> 00:00:04,000\nCinema metadata test\n",
        )
        .unwrap();
        assert!(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=c=red:s=640x360:r=10",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:sample_rate=8000",
                    "-i"
                ])
                .arg(&subtitles)
                .args(["-i"])
                .arg(&metadata)
                .args([
                    "-map",
                    "0:v",
                    "-map",
                    "1:a",
                    "-map",
                    "2:s",
                    "-map_metadata",
                    "3",
                    "-map_chapters",
                    "3",
                    "-metadata:s:a:0",
                    "language=eng",
                    "-metadata:s:s:0",
                    "language=eng",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "pcm_s16le",
                    "-c:s",
                    "srt",
                    "-t",
                    "4"
                ])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let player = Player::new(Config {
            render_backend: RenderBackend::Cpu,
            ..Config::default()
        })
        .unwrap();
        player.set_paused(true).unwrap();
        player.resize(640, 360).unwrap();
        // Avoid requiring a real audio device in this headless native test.
        player.select_audio(None).unwrap();
        player.load(path.to_str().unwrap()).unwrap();
        wait_for(&player, |s| {
            s.paused && s.chapters.len() == 2 && s.tracks.len() == 3
        });
        let state = player.state();
        assert!(state.chapters_available && state.tracks_available);
        assert_eq!(state.chapters[0].title.as_deref(), Some("Opening"));
        assert_eq!(state.chapters[1].time, 2.0);
        assert!(
            state
                .tracks
                .iter()
                .any(|t| t.kind == crate::TrackKind::Audio && t.lang.as_deref() == Some("eng"))
        );
        let subtitle = state
            .tracks
            .iter()
            .find(|t| t.kind == crate::TrackKind::Subtitle)
            .unwrap()
            .id;
        player.select_subtitle(Some(subtitle)).unwrap();
        wait_for(&player, |s| {
            s.tracks
                .iter()
                .any(|t| t.id == subtitle && t.kind == crate::TrackKind::Subtitle && t.selected)
        });
        // Verify the real sub-pos effect in rendered pixels, not just acceptance.
        fn wait_subtitle_band(player: &Player, raised: bool) {
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            loop {
                if let Some(frame) = player.take_frame() {
                    let top = frame
                        .pixels
                        .chunks_exact(4)
                        .position(|p| p[0] > 180 && p[1] > 180 && p[2] > 180)
                        .map(|i| i / frame.width as usize);
                    if top.is_some_and(|y| {
                        if raised {
                            y < frame.height as usize * 3 / 5
                        } else {
                            y > frame.height as usize * 3 / 4
                        }
                    }) {
                        return;
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "subtitle band not rendered; raised={raised}, state={:?}",
                    player.state()
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
        player.seek_absolute(1.0).unwrap();
        wait_subtitle_band(&player, false);
        player.set_subtitle_position(50).unwrap();
        wait_subtitle_band(&player, true);
        // The UI deduplicates updates: a new load must retain the position.
        player.stop().unwrap();
        wait_for(&player, |s| s.media_stopped);
        // Allow the unavailable video-out-params observations to drain before
        // reloading identical media (whose dimensions otherwise never change).
        thread::sleep(Duration::from_millis(100));
        player.load(path.to_str().unwrap()).unwrap();
        wait_for(&player, |s| s.chapters.len() == 2 && !s.loading);
        player.select_subtitle(Some(subtitle)).unwrap();
        player.seek_absolute(1.0).unwrap();
        wait_subtitle_band(&player, true);
        player.set_subtitle_position(100).unwrap();
        wait_subtitle_band(&player, false);
        player.select_subtitle(None).unwrap();
        wait_for(&player, |s| {
            s.tracks
                .iter()
                .all(|t| t.kind != crate::TrackKind::Subtitle || !t.selected)
        });
        player.set_speed(1.5).unwrap();
        wait_for(&player, |s| s.speed == 1.5);
        player.seek_absolute(2.5).unwrap();
        wait_for(&player, |s| (s.position - 2.5).abs() < 0.2);
        player.stop().unwrap();
        wait_for(&player, |s| s.media_stopped);
        let state = player.state();
        assert!(state.chapters.is_empty());
        assert!(state.tracks.is_empty());
        assert!(state.buffered_ranges.is_none());
        assert!(!state.chapters_available && !state.tracks_available);
        player.load("/definitely/not/a/cinema-file").unwrap();
        wait_for(&player, |s| s.error.is_some() && !s.loading);
        assert!(player.state().chapters.is_empty());
        assert!(player.state().tracks.is_empty());
    }
    #[test]
    #[ignore = "requires libmpv and ffmpeg; real source/viewport/audio-only control lifecycle"]
    fn real_cpu_player_media_lifecycle() {
        player_media_lifecycle(RenderBackend::Cpu);
    }
    #[cfg(feature = "gpu-render")]
    #[test]
    #[ignore = "requires native hardware GL, libmpv and ffmpeg; full Player control/readback lifecycle"]
    fn real_gpu_player_media_lifecycle() {
        player_media_lifecycle(RenderBackend::Gpu);
    }
    fn wait_for(player: &Player, predicate: impl Fn(&PlaybackState) -> bool) {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !predicate(&player.state()) {
            assert!(
                std::time::Instant::now() < deadline,
                "state timed out: {:?}",
                player.state()
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

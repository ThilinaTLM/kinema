use super::{
    library::Client,
    player::{Config, Shared},
    software::{CpuRenderer, Frame},
};
use crate::{
    Error, HardwareDecoding, PlaybackState, RenderBackend, RenderDiagnostics, ResolutionPolicy,
    Result,
};
use std::{
    sync::{
        Arc,
        atomic::Ordering,
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

pub(super) struct RenderedFrame {
    pub frame: Frame,
    pub generation: u64,
    pub created_at: Instant,
}

pub(super) fn hwdec(policy: HardwareDecoding, backend: RenderBackend) -> &'static str {
    match (policy, backend) {
        (HardwareDecoding::Disabled, _) => "no",
        (HardwareDecoding::CopyBack, _) | (HardwareDecoding::Auto, RenderBackend::Cpu) => {
            "auto-copy"
        }
        _ => "auto",
    }
}
fn validate_size(size: [u32; 2], limit: usize) -> Result<usize> {
    if size.contains(&0) || size.iter().any(|&v| v > i32::MAX as u32) {
        return Err(Error::InvalidInput(
            "render dimensions must be positive signed 32-bit integers",
        ));
    }
    let bytes = (size[0] as usize)
        .checked_mul(size[1] as usize)
        .and_then(|n| n.checked_mul(4))
        .ok_or(Error::Allocation)?;
    if bytes > limit {
        return Err(Error::InvalidInput(
            "render frame exceeds max_frame_bytes; select a cap or increase the guard",
        ));
    }
    Ok(bytes)
}
/// Presence is an opt-in, not permission to override contradictory policies.
#[cfg(any(feature = "experimental-gpu", test))]
fn validate_experimental_policy(
    enabled: bool,
    backend: RenderBackend,
    decoding: HardwareDecoding,
) -> Result<()> {
    if !enabled {
        return Ok(());
    }
    if backend == RenderBackend::Cpu {
        return Err(Error::InvalidInput(
            "experimental_gpu requires Auto or Gpu rendering; Cpu is incompatible",
        ));
    }
    if decoding != HardwareDecoding::Auto {
        return Err(Error::InvalidInput(
            "experimental_gpu requires Auto direct hardware decoding; Disabled and CopyBack are incompatible",
        ));
    }
    Ok(())
}

pub(super) fn validate_config(config: &Config) -> Result<()> {
    #[cfg(feature = "experimental-gpu")]
    validate_experimental_policy(
        config.experimental_gpu.is_some(),
        config.render_backend,
        config.hardware_decoding,
    )?;
    if let Some(size) = config.max_render_size {
        validate_size(size, usize::MAX)?;
    }
    if let ResolutionPolicy::Fixed(size) = config.resolution {
        validate_size(size, config.max_frame_bytes)?;
    }
    if config.max_frame_bytes == 0 {
        return Err(Error::InvalidInput("max_frame_bytes must be positive"));
    }
    Ok(())
}
fn resolution(config: &Config, viewport: [u32; 2], source: Option<[u32; 2]>) -> Result<[u32; 2]> {
    if viewport.contains(&0) {
        return Ok([0, 0]);
    }
    let mut size = match config.resolution {
        ResolutionPolicy::Source => source.unwrap_or(viewport),
        ResolutionPolicy::Viewport => viewport,
        ResolutionPolicy::Fixed(size) => size,
    };
    if let Some(cap) = config.max_render_size {
        size = super::software::capped_size(size[0], size[1], cap);
    }
    validate_size(size, config.max_frame_bytes)?;
    Ok(size)
}
fn dimensions(w: Option<i64>, h: Option<i64>) -> Option<[u32; 2]> {
    let w = u32::try_from(w?).ok()?;
    let h = u32::try_from(h?).ok()?;
    (w > 0 && h > 0).then_some([w, h])
}

pub(super) struct Pipeline {
    config: Config,
    pub viewport: [u32; 2],
    #[cfg(feature = "experimental-gpu")]
    decode_evidence: [String; 6],
    #[cfg(feature = "experimental-gpu")]
    decode_evidence_observed: bool,
    pub metadata: [Option<i64>; 5],
    size: [u32; 2],
    pub video_absent: bool,
    readbacks: u64,
    gpu_submissions: u64,
    pub(super) generation: u64,
    frame: Option<RenderedFrame>,
    #[cfg(feature = "experimental-gpu")]
    gpu_frame: Option<(super::experimental::ExperimentalFrame, u64, Instant)>,
    stats: RenderDiagnostics,
    started: Instant,
    age_total: f64,
}
impl Pipeline {
    pub fn new(config: Config) -> Self {
        Self {
            stats: RenderDiagnostics {
                requested_backend: config.render_backend,
                ..Default::default()
            },
            config,
            viewport: [0, 0],
            #[cfg(feature = "experimental-gpu")]
            decode_evidence: Default::default(),
            #[cfg(feature = "experimental-gpu")]
            decode_evidence_observed: false,
            metadata: [None; 5],
            size: [0, 0],
            video_absent: false,
            readbacks: 0,
            gpu_submissions: 0,
            generation: 0,
            frame: None,
            #[cfg(feature = "experimental-gpu")]
            gpu_frame: None,
            started: Instant::now(),
            age_total: 0.0,
        }
    }
    fn invalidate(&mut self) {
        #[cfg(feature = "experimental-gpu")]
        if self.gpu_frame.take().is_some() {
            self.stats.coalesced_frames += 1;
        }
        self.generation = self.generation.wrapping_add(1);
        if self.frame.take().is_some() {
            self.stats.coalesced_frames += 1;
        }
    }
    /// Accept a control-worker snapshot only for the generation queried. A
    /// changed snapshot invalidates the mailbox and forces a render even when
    /// paused libmpv has no further frame-update notification to send.
    #[cfg(feature = "experimental-gpu")]
    pub fn update_decode_evidence(&mut self, generation: u64, evidence: [String; 6]) -> bool {
        if self.generation != generation
            || (self.decode_evidence_observed && self.decode_evidence == evidence)
        {
            return false;
        }
        // An observed unavailable snapshot (including FORMAT_NONE properties)
        // must start the bounded metadata wait, unlike an idle unloaded player.
        self.decode_evidence_observed = true;
        self.decode_evidence = evidence;
        self.invalidate();
        true
    }
    #[cfg(feature = "experimental-gpu")]
    pub fn experimental_enabled(&self) -> bool {
        self.config.experimental_gpu.is_some()
    }
    #[cfg(feature = "experimental-gpu")]
    pub fn cancel_gpu(&mut self) {
        if self.experimental_enabled() {
            self.invalidate();
        }
    }
    pub fn record_shutdown_pause_failure(&mut self) {
        self.stats.shutdown_pause_enqueue_failed = true;
    }
    pub fn option_warnings(&mut self, warnings: Vec<String>) {
        self.stats.option_warnings = warnings;
    }
    pub fn clear(&mut self, state: &mut PlaybackState) {
        self.metadata = [None; 5];
        #[cfg(feature = "experimental-gpu")]
        {
            self.decode_evidence = Default::default();
            self.decode_evidence_observed = false;
        }
        self.video_absent = true;
        self.invalidate();
        state.source_size = None;
        state.source_display_size = None;
        let _ = self.recompute(state);
    }
    pub fn recompute(&mut self, state: &mut PlaybackState) -> Result<()> {
        let source = dimensions(self.metadata[0], self.metadata[1]);
        // libmpv video-out-params dw/dh include sample aspect, but not rotate.
        let mut display = source
            .and_then(|source| dimensions(self.metadata[2], self.metadata[3]).or(Some(source)));
        if self.metadata[4].unwrap_or(0).rem_euclid(180) == 90
            && let Some(size) = display.as_mut()
        {
            size.swap(0, 1);
        }
        if state.source_size != source || state.source_display_size != display {
            self.invalidate();
        }
        state.source_size = source;
        state.source_display_size = display;
        if source.is_none() && self.metadata[0].is_none() {
            self.frame = None;
        }
        let viewport = if self.video_absent {
            [0, 0]
        } else {
            self.viewport
        };
        let size = match resolution(&self.config, viewport, display) {
            Ok(size) => size,
            Err(e) => {
                self.size = [0, 0];
                state.render_size = [0, 0];
                self.invalidate();
                return Err(e);
            }
        };
        if size != self.size {
            self.size = size;
            self.invalidate();
        }
        state.render_size = size;
        Ok(())
    }
    fn deliver(&mut self, frame: RenderedFrame) {
        if frame.generation != self.generation || self.size.contains(&0) {
            self.stats.coalesced_frames += 1;
            return;
        }
        if self.frame.replace(frame).is_some() {
            self.stats.coalesced_frames += 1;
        }
    }
    pub fn take(&mut self) -> Option<Frame> {
        let frame = self.frame.take()?;
        if frame.generation != self.generation {
            self.stats.coalesced_frames += 1;
            return None;
        }
        self.stats.delivered_frames += 1;
        self.stats.upload_bytes_estimate += frame.frame.pixels.len() as u64;
        self.age_total += frame.created_at.elapsed().as_secs_f64() * 1000.0;
        Some(frame.frame)
    }
    #[cfg(feature = "experimental-gpu")]
    pub fn take_gpu(&mut self) -> Option<super::experimental::ExperimentalFrame> {
        let (frame, generation, created) = self.gpu_frame.take()?;
        if generation != self.generation || self.size.contains(&0) {
            self.stats.coalesced_frames += 1;
            return None;
        }
        self.stats.delivered_frames += 1;
        self.age_total += created.elapsed().as_secs_f64() * 1000.0;
        Some(frame)
    }
    pub fn snapshot(&self, state: &PlaybackState) -> RenderDiagnostics {
        let mut stats = self.stats.clone();
        stats.requested_backend = state.requested_backend;
        stats.active_backend = state.active_backend;
        stats.hardware_decoder = state.hardware_decoder.clone();
        stats.fallback_reason = state.fallback_reason.clone();
        stats.render_size = state.render_size;
        stats.source_size = state.source_size;
        stats.elapsed_seconds = self.started.elapsed().as_secs_f64();
        stats.delivered_fps =
            stats.delivered_frames as f64 / stats.elapsed_seconds.max(f64::EPSILON);
        stats.transfer_bytes_per_second = (stats.readback_bytes + stats.upload_bytes_estimate)
            as f64
            / stats.elapsed_seconds.max(f64::EPSILON);
        stats.frame_age_ms = self.age_total / stats.delivered_frames.max(1) as f64;
        stats.render_call_ms /= stats.rendered_frames.max(1) as f64;
        stats.readback_submit_ms /= self.gpu_submissions.max(1) as f64;
        stats.readback_copy_ms /= self.readbacks.max(1) as f64;
        stats.fence_latency_ms /= self.readbacks.max(1) as f64;
        stats
    }
}

enum Renderer {
    #[cfg(feature = "experimental-gpu")]
    Experimental(super::experimental::Renderer),
    Cpu(CpuRenderer),
    #[cfg(feature = "gpu-render")]
    Gpu(super::gpu::GpuRenderer),
}
impl Renderer {
    fn update(&mut self) -> u64 {
        match self {
            Self::Cpu(r) => r.update(),
            #[cfg(feature = "experimental-gpu")]
            Self::Experimental(r) => r.update(),
            #[cfg(feature = "gpu-render")]
            Self::Gpu(r) => r.update(),
        }
    }
    fn pending(&self) -> bool {
        match self {
            Self::Cpu(_) => false,
            #[cfg(feature = "experimental-gpu")]
            Self::Experimental(_) => false,
            #[cfg(feature = "gpu-render")]
            Self::Gpu(r) => r.pending(),
        }
    }
}
fn gpu(client: Arc<Client>, config: &Config) -> Result<Renderer> {
    #[cfg(feature = "gpu-render")]
    {
        super::gpu::GpuRenderer::new(client, config).map(Renderer::Gpu)
    }
    #[cfg(not(feature = "gpu-render"))]
    {
        let _ = (client, config);
        Err(Error::Gpu(
            "gpu-render feature is disabled; enable it or request Cpu".into(),
        ))
    }
}
fn select<T>(
    backend: RenderBackend,
    gpu: impl FnOnce() -> Result<T>,
    cpu: impl FnOnce(bool) -> Result<T>,
) -> Result<(T, Option<String>)> {
    if backend == RenderBackend::Cpu {
        return cpu(false).map(|r| (r, None));
    }
    match gpu() {
        Ok(renderer) => Ok((renderer, None)),
        Err(e) if backend == RenderBackend::Auto => cpu(true).map(|r| (r, Some(e.to_string()))),
        Err(e) => Err(e),
    }
}
fn initialize(
    client: Arc<Client>,
    config: &Config,
    decoder: &SyncSender<SyncSender<Result<()>>>,
    diagnostics: &super::diagnostics::Diagnostics,
) -> Result<(Renderer, Option<String>)> {
    #[cfg(feature = "experimental-gpu")]
    if let Some(context) = &config.experimental_gpu {
        return super::experimental::Renderer::new(client, context, diagnostics.id)
            .map(|r| (Renderer::Experimental(r), None));
    }
    select(
        config.render_backend,
        || gpu(client.clone(), config).inspect_err(|error| diagnostics.capability_error(error)),
        |fallback| {
            if fallback && config.hardware_decoding == HardwareDecoding::Auto {
                let (tx, rx) = mpsc::sync_channel(1);
                decoder.send(tx).map_err(|_| Error::Disconnected)?;
                rx.recv().map_err(|_| Error::Disconnected)??;
            }
            CpuRenderer::new(client.clone(), config.max_frame_bytes).map(Renderer::Cpu)
        },
    )
}
/// Coalesce libmpv update notifications without losing them while waiting for
/// older readbacks. Generations are acknowledged only after render/skip consumption.
#[cfg(any(feature = "gpu-render", feature = "experimental-gpu", test))]
struct GpuSchedule {
    frame_requested: bool,
    consumed_generation: Option<u64>,
}
#[cfg(any(feature = "gpu-render", feature = "experimental-gpu", test))]
impl GpuSchedule {
    fn new() -> Self {
        Self {
            frame_requested: false,
            consumed_generation: None,
        }
    }
    fn observe(&mut self, updates: u64) {
        self.frame_requested |= updates & 1 != 0;
    }
    fn ready(&self, pending: bool, generation: u64) -> bool {
        !pending && (self.frame_requested || self.consumed_generation != Some(generation))
    }
    fn consumed(&mut self, generation: u64) {
        self.frame_requested = false;
        self.consumed_generation = Some(generation);
    }
}

pub(super) fn run(
    client: Arc<Client>,
    shared: Arc<Shared>,
    config: Config,
    decoder: SyncSender<SyncSender<Result<()>>>,
    ready: SyncSender<Result<()>>,
) {
    // Declared before renderer so native render resources are released before
    // publishing an unexpected exit and allowing the client worker to terminate.
    let _exit = super::player::WorkerExit(shared.clone());
    shared
        .diagnostics
        .event(super::diagnostics::Lifecycle::RendererStarting);
    let (mut renderer, fallback) = match initialize(client, &config, &decoder, &shared.diagnostics)
    {
        Ok(r) => r,
        Err(e) => {
            // This is an expected terminal startup failure, not an unexplained
            // worker exit. Preserve its classification before WorkerExit runs.
            shared.fail_render_ref(&e);
            let _ = ready.send(Err(e));
            return;
        }
    };
    {
        let mut state = shared.state.lock().unwrap_or_else(|e| e.into_inner());
        state.active_backend = match &renderer {
            Renderer::Cpu(_) => RenderBackend::Cpu,
            #[cfg(feature = "experimental-gpu")]
            Renderer::Experimental(_) => RenderBackend::Gpu,
            #[cfg(feature = "gpu-render")]
            Renderer::Gpu(_) => RenderBackend::Gpu,
        };
        shared.diagnostics.render_ready(state.active_backend);
        state.fallback_reason = fallback;
        #[cfg(feature = "gpu-render")]
        if let Renderer::Gpu(gpu) = &renderer {
            let stats = gpu.stats();
            let mut pipeline = shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
            pipeline.stats.gl_vendor = Some(stats.vendor);
            pipeline.stats.gl_renderer = Some(stats.renderer);
            pipeline.stats.gl_version = Some(stats.version);
        }
    }
    let _ = ready.send(Ok(()));
    let mut last_generation = u64::MAX;
    let mut last_diagnostic_size = None;
    #[cfg(any(feature = "gpu-render", feature = "experimental-gpu"))]
    let mut gpu_schedule = GpuSchedule::new();
    while !shared.stopped.load(Ordering::Acquire) {
        let (size, generation) = {
            let pipeline = shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
            (pipeline.size, pipeline.generation)
        };
        if last_diagnostic_size != Some(size) {
            shared.diagnostics.render_size(size);
            last_diagnostic_size = Some(size);
        }
        let updates = renderer.update();
        let result = (|| -> Result<()> {
            match &mut renderer {
                Renderer::Cpu(cpu) => {
                    // Preserve the CPU pump's existing timed-render behavior.
                    if updates & 1 != 0 || generation != last_generation {
                        if size.contains(&0) {
                            if updates & 1 != 0 {
                                cpu.skip()?;
                            }
                        } else {
                            let created_at = Instant::now();
                            let frame = cpu.render(size)?;
                            let mut pipeline =
                                shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                            pipeline.stats.rendered_frames += 1;
                            pipeline.stats.render_call_ms += cpu.render_call_ms;
                            pipeline.stats.readback_copy_ms += cpu.copy_ms;
                            pipeline.readbacks += 1;
                            pipeline.deliver(RenderedFrame {
                                frame,
                                generation,
                                created_at,
                            });
                        }
                        last_generation = generation;
                    }
                }
                #[cfg(feature = "experimental-gpu")]
                Renderer::Experimental(gpu) => {
                    gpu_schedule.observe(updates);
                    let (evidence, observed, video_ready) = {
                        let pipeline = shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                        (
                            pipeline.decode_evidence.clone(),
                            pipeline.decode_evidence_observed,
                            dimensions(pipeline.metadata[0], pipeline.metadata[1]).is_some(),
                        )
                    };
                    // Missing video dimensions must not short-circuit the decode
                    // metadata timeout after an active-media observation.
                    let qualified = observed && gpu.evidence_ready(generation, &evidence)?;
                    let valid = video_ready && qualified;
                    if !gpu_schedule.ready(false, generation) {
                        return Ok(());
                    }
                    if size.contains(&0) {
                        if gpu_schedule.frame_requested {
                            gpu.skip()?;
                        }
                    } else {
                        let created = Instant::now();
                        let frame = gpu.render(size)?;
                        let frame = frame.filter(|_| valid);
                        let mut pipeline =
                            shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                        if let Some(frame) = frame {
                            pipeline.stats.rendered_frames += 1;
                            pipeline.gpu_submissions += 1;
                            pipeline.stats.gpu_surface_frames += 1;
                            pipeline.stats.render_call_ms +=
                                created.elapsed().as_secs_f64() * 1000.0;
                            if generation == pipeline.generation && !pipeline.size.contains(&0) {
                                if pipeline
                                    .gpu_frame
                                    .replace((frame, generation, created))
                                    .is_some()
                                {
                                    pipeline.stats.coalesced_frames += 1;
                                }
                            } else {
                                pipeline.stats.coalesced_frames += 1;
                            }
                        } else {
                            pipeline.stats.coalesced_frames += 1;
                        }
                    }
                    gpu_schedule.consumed(generation);
                }
                #[cfg(feature = "gpu-render")]
                Renderer::Gpu(gpu) => {
                    gpu_schedule.observe(updates);
                    // Deliver older frames BEFORE mpv's next audio-timed render
                    // can block. A pending fence defers even a generation redraw;
                    // do not resize/retire its resources or lose consumed updates.
                    while let Some(frame) = gpu.poll()? {
                        let stats = gpu.stats();
                        let mut pipeline =
                            shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                        pipeline.readbacks += 1;
                        pipeline.stats.readback_copy_ms += stats.readback_copy_ms;
                        pipeline.stats.fence_latency_ms += stats.fence_latency_ms;
                        pipeline.deliver(frame);
                    }
                    let stats = gpu.stats();
                    {
                        let mut pipeline =
                            shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                        pipeline.stats.gl_vendor = Some(stats.vendor);
                        pipeline.stats.gl_renderer = Some(stats.renderer);
                        pipeline.stats.gl_version = Some(stats.version);
                        pipeline.stats.readback_bytes = stats.readback_bytes;
                    }
                    if !gpu_schedule.ready(gpu.pending(), generation) {
                        return Ok(());
                    }
                    if size.contains(&0) {
                        if gpu_schedule.frame_requested {
                            gpu.skip()?;
                        }
                    } else {
                        let rendered = gpu.render(size, generation)?;
                        let stats = gpu.stats();
                        let mut pipeline =
                            shared.pipeline.lock().unwrap_or_else(|e| e.into_inner());
                        if rendered {
                            pipeline.stats.rendered_frames += 1;
                            pipeline.gpu_submissions += 1;
                            pipeline.stats.render_call_ms += stats.render_call_ms;
                            pipeline.stats.readback_submit_ms += stats.readback_submit_ms;
                        } else {
                            // GpuRenderer's full-ring result consumes/skips the
                            // mpv frame, so acknowledge this request as well.
                            pipeline.stats.coalesced_frames += 1;
                        }
                    }
                    gpu_schedule.consumed(generation);
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            shared.fail_render_typed(e);
            break;
        }
        thread::park_timeout(Duration::from_millis(if renderer.pending() {
            2
        } else {
            10
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn experimental_policy_allows_only_auto_or_gpu_with_auto_decoding() {
        for backend in [RenderBackend::Auto, RenderBackend::Gpu] {
            assert!(validate_experimental_policy(true, backend, HardwareDecoding::Auto).is_ok());
        }
    }
    #[test]
    fn experimental_policy_rejects_explicit_cpu_and_non_direct_decoding() {
        for backend in [RenderBackend::Auto, RenderBackend::Cpu, RenderBackend::Gpu] {
            for decoding in [
                HardwareDecoding::Auto,
                HardwareDecoding::Disabled,
                HardwareDecoding::CopyBack,
            ] {
                if backend != RenderBackend::Cpu && decoding == HardwareDecoding::Auto {
                    continue;
                }
                let error = validate_experimental_policy(true, backend, decoding).unwrap_err();
                assert!(matches!(error, Error::InvalidInput(_)));
                assert!(error.to_string().contains("experimental_gpu requires"));
            }
        }
    }
    #[test]
    fn absent_experimental_context_preserves_all_existing_policy_combinations() {
        for backend in [RenderBackend::Auto, RenderBackend::Cpu, RenderBackend::Gpu] {
            for decoding in [
                HardwareDecoding::Auto,
                HardwareDecoding::Disabled,
                HardwareDecoding::CopyBack,
            ] {
                assert!(validate_experimental_policy(false, backend, decoding).is_ok());
            }
        }
    }

    #[cfg(feature = "experimental-gpu")]
    #[test]
    fn paused_metadata_ready_forces_redraw_after_consumed_generation() {
        let mut pipeline = Pipeline::new(Config::default());
        let mut state = PlaybackState {
            paused: true,
            ..Default::default()
        };
        pipeline.viewport = [1920, 1080];
        pipeline.metadata = [Some(1920), Some(1080), None, None, None];
        pipeline.recompute(&mut state).unwrap();
        let mut schedule = GpuSchedule::new();
        schedule.consumed(pipeline.generation);
        schedule.observe(0); // Paused video supplies no more mpv updates.
        assert!(!schedule.ready(false, pipeline.generation));
        let evidence = ["vaapi", "vaapi", "vaapi", "nv12", "auto", "bt.1886"].map(str::to_owned);
        assert!(pipeline.update_decode_evidence(pipeline.generation, evidence.clone()));
        assert!(schedule.ready(false, pipeline.generation));
        schedule.consumed(pipeline.generation);
        assert!(!pipeline.update_decode_evidence(pipeline.generation, evidence));
        assert!(!schedule.ready(false, pipeline.generation));
        // Only video-params/gamma changes; no mpv frame notification follows.
        let mut resolved = pipeline.decode_evidence.clone();
        resolved[5] = "bt.709".into();
        assert!(pipeline.update_decode_evidence(pipeline.generation, resolved));
        assert!(schedule.ready(false, pipeline.generation));
        assert!(state.paused);
    }
    #[cfg(feature = "experimental-gpu")]
    #[test]
    fn stale_evidence_cannot_override_source_invalidation() {
        let mut pipeline = Pipeline::new(Config::default());
        let queried_generation = pipeline.generation;
        pipeline.clear(&mut PlaybackState::default());
        let generation = pipeline.generation;
        assert!(
            !pipeline
                .update_decode_evidence(queried_generation, std::array::from_fn(|_| "old".into()))
        );
        assert_eq!(pipeline.generation, generation);
        assert_eq!(
            pipeline.decode_evidence,
            std::array::from_fn(|_| String::new())
        );
    }
    #[cfg(feature = "experimental-gpu")]
    #[test]
    fn unknown_evidence_forces_recheck_but_never_produces_cpu_pixels() {
        let mut pipeline = Pipeline::new(Config::default());
        let evidence = std::array::from_fn(|_| "unavailable".into());
        assert!(pipeline.update_decode_evidence(pipeline.generation, evidence));
        assert!(pipeline.take().is_none());
        assert!(pipeline.take_gpu().is_none());
        let stats = pipeline.snapshot(&PlaybackState::default());
        assert_eq!(stats.rendered_frames, 0);
        assert_eq!(stats.readback_bytes, 0);
        assert_eq!(stats.upload_bytes_estimate, 0);
    }

    #[test]
    fn gpu_pending_deferral_retains_and_coalesces_updates() {
        let mut schedule = GpuSchedule::new();
        schedule.consumed(1);
        schedule.observe(1);
        assert!(!schedule.ready(true, 1));
        schedule.observe(0); // update() has consumed the prior notification.
        assert!(!schedule.ready(true, 1));
        schedule.observe(1); // further notifications remain one bounded request.
        assert!(schedule.ready(false, 1));
        schedule.consumed(1);
        assert!(!schedule.ready(false, 1));
    }
    #[test]
    fn gpu_pending_deferral_retains_latest_generation_redraw() {
        let mut schedule = GpuSchedule::new();
        assert!(schedule.ready(false, 0));
        schedule.consumed(0);
        assert!(!schedule.ready(true, 1));
        assert!(!schedule.ready(true, 2));
        schedule.observe(0); // paused redraw has no new video notification.
        assert!(schedule.ready(false, 2));
        schedule.consumed(2);
        assert!(!schedule.ready(false, 2));
        schedule.observe(1);
        assert!(!schedule.ready(true, 3)); // hidden viewport skip also defers.
        assert!(schedule.frame_requested);
        schedule.consumed(3);
        assert!(!schedule.frame_requested);
        assert!(!schedule.ready(false, 3));
    }
    #[test]
    fn readback_submission_mean_uses_successful_gpu_frames() {
        let mut pipeline = Pipeline::new(Config::default());
        let state = PlaybackState::default();
        assert_eq!(pipeline.snapshot(&state).readback_submit_ms, 0.0);
        pipeline.stats.rendered_frames = 10;
        pipeline.gpu_submissions = 2;
        pipeline.stats.readback_submit_ms = 6.0;
        let snapshot = pipeline.snapshot(&state);
        assert_eq!(snapshot.readback_submit_ms, 3.0);
        assert_eq!(snapshot.readback_copy_ms, 0.0);
        assert_eq!(snapshot.fence_latency_ms, 0.0);
    }
    #[test]
    fn backend_selection() {
        let selected = select(
            RenderBackend::Auto,
            || Err(Error::Gpu("mock unavailable".into())),
            |fallback| {
                assert!(fallback);
                Ok(RenderBackend::Cpu)
            },
        )
        .unwrap();
        assert_eq!(selected.0, RenderBackend::Cpu);
        assert!(selected.1.unwrap().contains("mock unavailable"));
        assert!(
            select(
                RenderBackend::Gpu,
                || Err::<(), _>(Error::Gpu("mock unavailable".into())),
                |_| panic!("forced GPU must not fall back")
            )
            .is_err()
        );
        let selected = select(
            RenderBackend::Cpu,
            || panic!("CPU must not initialize GPU"),
            |fallback| {
                assert!(!fallback);
                Ok(())
            },
        )
        .unwrap();
        assert!(selected.1.is_none());
        assert_eq!(
            select(
                RenderBackend::Auto,
                || Ok(RenderBackend::Gpu),
                |_| panic!("successful GPU must not fall back")
            )
            .unwrap()
            .0,
            RenderBackend::Gpu
        );
    }
    #[test]
    fn decoding_policy() {
        assert_eq!(
            hwdec(HardwareDecoding::Auto, RenderBackend::Cpu),
            "auto-copy"
        );
        assert_eq!(hwdec(HardwareDecoding::Auto, RenderBackend::Gpu), "auto");
        assert_eq!(hwdec(HardwareDecoding::Disabled, RenderBackend::Gpu), "no");
        assert_eq!(
            hwdec(HardwareDecoding::CopyBack, RenderBackend::Gpu),
            "auto-copy"
        );
    }
    #[test]
    fn resolution_policies() {
        let mut config = Config::default();
        for source in [[1280, 720], [1920, 1080], [3840, 2160], [1080, 1920]] {
            assert_eq!(
                resolution(&config, [800, 600], Some(source)).unwrap(),
                source
            );
        }
        assert_eq!(
            resolution(&config, [0, 600], Some([3840, 2160])).unwrap(),
            [0, 0]
        );
        assert_eq!(resolution(&config, [800, 600], None).unwrap(), [800, 600]);
        config.resolution = ResolutionPolicy::Viewport;
        assert_eq!(
            resolution(&config, [1600, 1200], Some([3840, 2160])).unwrap(),
            [1600, 1200]
        );
        config.resolution = ResolutionPolicy::Fixed([1920, 1080]);
        config.max_render_size = Some([1280, 720]);
        assert_eq!(resolution(&config, [800, 600], None).unwrap(), [1280, 720]);
        assert!(validate_size([u32::MAX, 2], usize::MAX).is_err());
        assert!(validate_size([8192, 8192], 64 * 1024 * 1024).is_err());
    }
    #[cfg(feature = "experimental-gpu")]
    #[test]
    fn unavailable_decode_observation_is_tracked_and_reset_without_a_native_context() {
        let mut pipeline = Pipeline::new(Config::default());
        assert!(!pipeline.decode_evidence_observed);
        let generation = pipeline.generation;
        assert!(pipeline.update_decode_evidence(generation, Default::default()));
        assert!(pipeline.decode_evidence_observed);
        assert!(!pipeline.update_decode_evidence(pipeline.generation, Default::default()));
        pipeline.clear(&mut PlaybackState::default());
        assert!(!pipeline.decode_evidence_observed);
        assert!(!pipeline.update_decode_evidence(generation, Default::default()));
    }
    #[test]
    fn metadata_and_generations() {
        let mut pipeline = Pipeline::new(Config::default());
        let mut state = PlaybackState::default();
        pipeline.viewport = [640, 480];
        pipeline.metadata = [Some(720), Some(576), Some(1024), Some(576), Some(90)];
        pipeline.recompute(&mut state).unwrap();
        assert_eq!(state.source_display_size, Some([576, 1024]));
        let old = pipeline.generation;
        pipeline.clear(&mut state);
        pipeline.deliver(RenderedFrame {
            frame: Frame {
                width: 1,
                height: 1,
                pixels: vec![0; 4],
            },
            generation: old,
            created_at: Instant::now(),
        });
        assert!(pipeline.take().is_none());
        assert_eq!(state.source_size, None);
    }
}

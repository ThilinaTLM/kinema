#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RenderBackend {
    #[default]
    Auto,
    Cpu,
    Gpu,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HardwareDecoding {
    #[default]
    Auto,
    Disabled,
    CopyBack,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResolutionPolicy {
    #[default]
    Source,
    Viewport,
    Fixed([u32; 2]),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RenderDiagnostics {
    /// Unsupported optional quality/SDR options; initialization remains usable.
    pub option_warnings: Vec<String>,
    pub requested_backend: RenderBackend,
    pub active_backend: RenderBackend,
    pub hardware_decoder: Option<String>,
    pub fallback_reason: Option<String>,
    pub gl_vendor: Option<String>,
    pub gl_renderer: Option<String>,
    pub gl_version: Option<String>,
    pub render_size: [u32; 2],
    pub source_size: Option<[u32; 2]>,
    pub rendered_frames: u64,
    pub delivered_frames: u64,
    pub coalesced_frames: u64,
    pub readback_bytes: u64,
    pub upload_bytes_estimate: u64,
    /// Cumulative per-frame averages of CPU wall times, not GPU execution times.
    pub render_call_ms: f64,
    /// Cumulative mean CPU wall time issuing GPU readback/PBO, fence, and flush,
    /// per successfully submitted GPU frame; zero for CPU rendering.
    pub readback_submit_ms: f64,
    pub readback_copy_ms: f64,
    pub fence_latency_ms: f64,
    pub frame_age_ms: f64,
    pub elapsed_seconds: f64,
    pub delivered_fps: f64,
    pub transfer_bytes_per_second: f64,
}

/// Latest observed libmpv state. Commands are asynchronous; snapshots are not optimistic.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackState {
    pub position: f64,
    pub duration: Option<f64>,
    pub paused: bool,
    /// libmpv has paused playback to refill its cache (distinct from user pause).
    pub paused_for_cache: bool,
    /// No media is active (initial state, explicit stop, EOF, or media failure).
    pub media_stopped: bool,
    /// The worker lifecycle has terminated; this Player cannot accept commands.
    pub stopped: bool,
    /// Fatal backend failure, preserved until this Player is destroyed.
    pub fatal_error: Option<String>,
    pub volume: f64,
    pub muted: bool,
    pub loading: bool,
    pub ended: bool,
    pub error: Option<String>,
    pub source_size: Option<[u32; 2]>,
    pub source_display_size: Option<[u32; 2]>,
    pub active_backend: RenderBackend,
    pub requested_backend: RenderBackend,
    pub hardware_decoder: Option<String>,
    pub render_size: [u32; 2],
    pub fallback_reason: Option<String>,
}

impl Default for PlaybackState {
    fn default() -> Self {
        Self {
            position: 0.0,
            duration: None,
            paused: false,
            paused_for_cache: false,
            media_stopped: true,
            stopped: false,
            fatal_error: None,
            volume: 100.0,
            muted: false,
            loading: false,
            ended: false,
            error: None,
            source_size: None,
            source_display_size: None,
            active_backend: RenderBackend::Cpu,
            requested_backend: RenderBackend::Auto,
            hardware_decoder: None,
            render_size: [0, 0],
            fallback_reason: None,
        }
    }
}

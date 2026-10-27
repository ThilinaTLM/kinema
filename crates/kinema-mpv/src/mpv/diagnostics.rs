//! Privacy boundary for backend telemetry. Only fixed vocabulary and numeric
//! observations leave this module; native strings are never tracing fields.
use super::player::Config;
use crate::{Error, HardwareDecoding, PlaybackFailure, PlaybackState, RenderBackend};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::Instant;

static NEXT_PLAYER: AtomicU64 = AtomicU64::new(1);

// Every string expression below belongs to Evidence's fixed vocabulary. Keep
// the event fields explicit so text and JSON subscribers expose the same data.
macro_rules! evidence_event {
    ($level:expr, $id:expr, $e:expr, $failure:expr, $details:expr, $terminal:expr, $message:literal) => {
        tracing::event!(
            $level,
            player_id = $id,
            player_terminal = $terminal,
            failure_code = $failure,
            error_kind = $details.kind,
            operation = $details.operation,
            mpv_error = $details.mpv_error,
            actual_decoder = $e.actual_decoder,
            decoded_pixfmt = $e.decoded,
            output_pixfmt = $e.output,
            storage_fmt = $e.storage,
            color_transfer = $e.transfer,
            decoded_transfer = $e.transfer,
            effective_transfer = $e.effective_transfer,
            codec = $e.codec,
            decoder_unavailable = $e.unavailable[0],
            decoded_unavailable = $e.unavailable[1],
            output_unavailable = $e.unavailable[2],
            storage_unavailable = $e.unavailable[3],
            transfer_unavailable = $e.unavailable[4],
            codec_unavailable = $e.unavailable[5],
            source_width = $e.source[0],
            source_height = $e.source[1],
            render_width = $e.render[0],
            render_height = $e.render[1],
            $message
        );
    };
}

/// Fixed metadata only, even if a caller constructs Error::Mpv with a
/// malicious supposedly-static operation string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct FailureDetails {
    kind: &'static str,
    operation: &'static str,
    mpv_error: Option<i32>,
}
impl Default for FailureDetails {
    fn default() -> Self {
        Self {
            kind: "unknown",
            operation: "other",
            mpv_error: None,
        }
    }
}
impl FailureDetails {
    fn media_end(code: i32) -> Self {
        Self {
            kind: "mpv",
            operation: "load",
            mpv_error: Some(code),
        }
    }
    pub fn from_error(error: &Error) -> Self {
        let kind = match error {
            Error::Mpv { .. } => "mpv",
            Error::Library { .. } => "library",
            Error::Symbol { .. } => "symbol",
            Error::UnsupportedApi(_) => "api-version",
            Error::Gpu(_) => "gpu",
            Error::ClientCreation => "client-create",
            Error::InvalidInput(_) => "input",
            Error::Allocation => "allocation",
            Error::QueueFull => "queue-full",
            Error::Disconnected => "disconnected",
            Error::Worker(_) => "worker",
        };
        let (operation, mpv_error) = if let Error::Mpv {
            operation, code, ..
        } = error
        {
            let operation = match *operation {
                "configure libmpv"
                | "require direct hardware decode"
                | "disable scripting/OSC"
                | "configure optional GPU quality/SDR option"
                | "set fallback hardware decoding" => "configure",
                "initialize libmpv" => "initialize",
                "create software render context"
                | "create OpenGL render context"
                | "create GPU surface render context" => "render-create",
                "software render"
                | "OpenGL render"
                | "render GPU surface frame"
                | "skip hidden frame"
                | "skip GPU frame"
                | "skip GPU surface frame" => "render",
                "queue command" | "command" => "command",
                "observe property" | "observe direct decode evidence" => "observe",
                "playback" | "loadfile" | "load" => "load",
                _ => "other",
            };
            (operation, Some(*code))
        } else {
            ("none", None)
        };
        Self {
            kind,
            operation,
            mpv_error,
        }
    }
}

#[cfg(feature = "gpu-surface")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeCleanup {
    Complete,
    Quarantined,
}
#[cfg(feature = "gpu-surface")]
impl NativeCleanup {
    fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Quarantined => "quarantined",
        }
    }
}
#[cfg(feature = "gpu-surface")]
pub(super) fn native_cleanup(player_id: u64, outcome: NativeCleanup) {
    tracing::info!(
        player_id,
        native_cleanup = outcome.name(),
        "mpv GPU surface native cleanup"
    );
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lifecycle {
    Startup,
    StartupFailed,
    ControlStarted,
    RendererStarting,
    RendererReady,
    LoadRequested,
    StartFile,
    FileLoaded,
    EndFile,
    ShutdownRequested,
    ControlStopped,
    WorkersStopped,
}
impl Lifecycle {
    fn name(self) -> &'static str {
        match self {
            Self::Startup => "startup",
            Self::StartupFailed => "startup-failed",
            Self::ControlStarted => "control-started",
            Self::RendererStarting => "renderer-starting",
            Self::RendererReady => "renderer-ready",
            Self::LoadRequested => "load-requested",
            Self::StartFile => "start-file",
            Self::FileLoaded => "file-loaded",
            Self::EndFile => "end-file",
            Self::ShutdownRequested => "shutdown-requested",
            Self::ControlStopped => "control-stopped",
            Self::WorkersStopped => "workers-stopped",
        }
    }
}

pub(super) fn backend(value: RenderBackend) -> &'static str {
    match value {
        RenderBackend::Auto => "auto",
        RenderBackend::Cpu => "cpu",
        RenderBackend::Gpu => "gpu",
    }
}
fn decoding(value: HardwareDecoding) -> &'static str {
    match value {
        HardwareDecoding::Auto => "auto",
        HardwareDecoding::Disabled => "disabled",
        HardwareDecoding::CopyBack => "copy-back",
    }
}

/// Never return the input, even if printable ASCII: unknown filenames, Unicode,
/// credential-bearing strings, and URLs all become the same static token.
fn vocabulary(value: Option<&str>, allowed: &'static [&'static str]) -> &'static str {
    match value {
        None | Some("" | "unavailable") => "unavailable",
        Some(value) => allowed
            .iter()
            .copied()
            .find(|&known| known == value)
            .unwrap_or("other"),
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Evidence {
    pub actual_decoder: &'static str,
    pub decoded: &'static str,
    pub output: &'static str,
    pub storage: &'static str,
    pub transfer: &'static str,
    pub effective_transfer: &'static str,
    pub codec: &'static str,
    pub unavailable: [bool; 6],
    pub source: [u32; 2],
    pub render: [u32; 2],
}
impl Evidence {
    pub fn new(values: [Option<&str>; 6], source: [u32; 2], render: [u32; 2]) -> Self {
        let [actual, decoded, output, storage, transfer, codec] = values;
        const PIXELS: &[&str] = &[
            "vaapi",
            "d3d11",
            "d3d11va_vld",
            "videotoolbox_vld",
            "nv12",
            "p010",
            "p010le",
            "p016",
            "p016le",
            "yuv420p",
            "yuv420p10le",
            "yuv420p12le",
            "yuv422p",
            "yuv422p10le",
            "yuv444p",
            "yuv444p10le",
            "yuv444p12le",
            "rgba",
            "bgra",
            "rgb0",
            "bgr0",
            "rgb24",
            "gbrp",
            "gbrp10le",
            "cuda",
            "drm_prime",
            "vulkan",
        ];
        Self {
            effective_transfer: "unavailable",
            actual_decoder: vocabulary(
                actual,
                &[
                    "no",
                    "auto",
                    "vaapi",
                    "vaapi-copy",
                    "d3d11va",
                    "d3d11va-copy",
                    "videotoolbox",
                    "videotoolbox-copy",
                    "nvdec",
                    "nvdec-copy",
                    "vulkan",
                    "vulkan-copy",
                    "cuda",
                ],
            ),
            decoded: vocabulary(decoded, PIXELS),
            output: vocabulary(output, PIXELS),
            storage: vocabulary(storage, PIXELS),
            transfer: vocabulary(
                transfer,
                &[
                    "bt.1886",
                    "bt.709",
                    "srgb",
                    "gamma2.2",
                    "gamma2.8",
                    "pq",
                    "hlg",
                    "st2084",
                    "arib-std-b67",
                    "auto",
                    "unknown",
                    "linear",
                ],
            ),
            codec: vocabulary(
                codec,
                &[
                    "h264",
                    "hevc",
                    "av1",
                    "vp9",
                    "vp8",
                    "mpeg2video",
                    "mpeg4",
                    "vc1",
                    "mjpeg",
                    "prores",
                    "theora",
                ],
            ),
            unavailable: values.map(|v| v.is_none_or(|s| s.is_empty() || s == "unavailable")),
            source,
            render,
        }
    }
    pub fn with_effective_transfer(mut self, value: Option<&str>) -> Self {
        self.effective_transfer = vocabulary(
            value,
            &[
                "bt.1886",
                "bt.709",
                "srgb",
                "gamma2.2",
                "gamma2.8",
                "pq",
                "hlg",
                "st2084",
                "arib-std-b67",
                "auto",
                "unknown",
                "linear",
            ],
        );
        self
    }
    /// Shared by telemetry and privacy tests: no native text is formatted here.
    #[cfg(test)]
    pub fn summary(self) -> String {
        format!(
            "decoder={} decoded={} output={} storage={} transfer={} effective={} codec={} unavailable={:?} source={}x{} render={}x{}",
            self.actual_decoder,
            self.decoded,
            self.output,
            self.storage,
            self.transfer,
            self.effective_transfer,
            self.codec,
            self.unavailable,
            self.source[0],
            self.source[1],
            self.render[0],
            self.render[1]
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum NativeDiagnostic {
    HwdecFailed,
    DecoderOpenFailed,
    OpenGlContextFailed,
    NetworkFailed,
    ReadFailed,
    DemuxFailed,
    DecodeFailed,
    TruncatedStream,
    CancelledIo,
    MetadataUnseekable,
    Unclassified,
}
impl NativeDiagnostic {
    fn code(self) -> &'static str {
        match self {
            Self::HwdecFailed => "hwdec-failed",
            Self::DecoderOpenFailed => "decoder-open-failed",
            Self::OpenGlContextFailed => "opengl-context-failed",
            Self::NetworkFailed => "network-failed",
            Self::ReadFailed => "read-failed",
            Self::DemuxFailed => "demux-failed",
            Self::DecodeFailed => "decode-failed",
            Self::TruncatedStream => "truncated-stream",
            Self::CancelledIo => "cancelled-io",
            Self::MetadataUnseekable => "metadata-unseekable",
            Self::Unclassified => "unclassified",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeModule {
    Vd,
    VdFfmpeg,
    Vo,
    VoLibmpv,
    VoGpu,
    VoGpuOpenGl,
    Ffmpeg,
    FfmpegVideo,
    FfmpegDemuxer,
    Demux,
    DemuxLavf,
    Stream,
    StreamCurl,
    Cplayer,
    Mkv,
    Other,
}
impl NativeModule {
    fn from_prefix(prefix: &str) -> Self {
        match prefix {
            "vd" => Self::Vd,
            "vd/ffmpeg" => Self::VdFfmpeg,
            "vo" => Self::Vo,
            "vo/libmpv" => Self::VoLibmpv,
            "vo/gpu" => Self::VoGpu,
            "vo/gpu/opengl" => Self::VoGpuOpenGl,
            "ffmpeg" => Self::Ffmpeg,
            "ffmpeg/video" | "ffmpeg/video/h264" | "ffmpeg/video/hevc" | "ffmpeg/video/av1" => {
                Self::FfmpegVideo
            }
            "ffmpeg/demuxer"
            | "ffmpeg/demuxer/mov,mp4,m4a,3gp,3g2,mj2"
            | "ffmpeg/demuxer/matroska,webm" => Self::FfmpegDemuxer,
            "demux" => Self::Demux,
            "demux/lavf" => Self::DemuxLavf,
            "stream" => Self::Stream,
            "stream/curl" => Self::StreamCurl,
            "cplayer" => Self::Cplayer,
            "mkv" => Self::Mkv,
            _ => Self::Other,
        }
    }
    fn name(self) -> &'static str {
        match self {
            Self::Vd => "vd",
            Self::VdFfmpeg => "vd/ffmpeg",
            Self::Vo => "vo",
            Self::VoLibmpv => "vo/libmpv",
            Self::VoGpu => "vo/gpu",
            Self::VoGpuOpenGl => "vo/gpu/opengl",
            Self::Ffmpeg => "ffmpeg",
            Self::FfmpegVideo => "ffmpeg/video",
            Self::FfmpegDemuxer => "ffmpeg/demuxer",
            Self::Demux => "demux",
            Self::DemuxLavf => "demux/lavf",
            Self::Stream => "stream",
            Self::StreamCurl => "stream/curl",
            Self::Cplayer => "cplayer",
            Self::Mkv => "mkv",
            Self::Other => "other",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NativeSeverity {
    Fatal,
    Error,
    Warn,
    Info,
    Other,
}
impl NativeSeverity {
    fn name(self) -> &'static str {
        match self {
            Self::Fatal => "fatal",
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Other => "other",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct NativeMessage {
    code: NativeDiagnostic,
    module: NativeModule,
    severity: NativeSeverity,
}
impl NativeMessage {
    pub fn classify(prefix: &str, level: &str, text: &str, log_level: i32) -> Self {
        use NativeModule::*;
        let module = NativeModule::from_prefix(prefix);
        // client.h: NONE=0, FATAL=10, ERROR=20, WARN=30, INFO=40.
        let severity = match (log_level, level) {
            (0, _) => NativeSeverity::Other,
            (10, _) => NativeSeverity::Fatal,
            (20, _) => NativeSeverity::Error,
            (30, _) => NativeSeverity::Warn,
            (40, _) => NativeSeverity::Info,
            (_, "fatal") => NativeSeverity::Fatal,
            (_, "error") => NativeSeverity::Error,
            (_, "warn") => NativeSeverity::Warn,
            (_, "info") => NativeSeverity::Info,
            _ => NativeSeverity::Other,
        };
        let begins = |prefixes: &[&str]| prefixes.iter().any(|p| text.starts_with(p));
        let code = match module {
            Mkv if text
                == "Stream is not seekable or unknown size, not reading mkv metadata at end of file.\n" =>
            {
                NativeDiagnostic::MetadataUnseekable
            }
            // Cancellation text is evidence only, not proof of user intent. In
            // particular, do not suppress read errors near Player shutdown.
            Stream | StreamCurl | Ffmpeg | FfmpegDemuxer
                if matches!(text.trim_end_matches('\n'), "Immediate exit requested") =>
            {
                NativeDiagnostic::CancelledIo
            }
            Ffmpeg | FfmpegDemuxer
                if begins(&[
                    "Stream ends prematurely at ",
                    "http: Stream ends prematurely at ",
                ]) =>
            {
                NativeDiagnostic::TruncatedStream
            }
            DemuxLavf | FfmpegDemuxer
                if begins(&["Packet corrupt", "partial file", "File ended prematurely"]) =>
            {
                NativeDiagnostic::TruncatedStream
            }
            Vd | VdFfmpeg | FfmpegVideo
                if begins(&[
                    "Error while decoding",
                    "Error decoding",
                    "Invalid NAL unit size",
                    "Invalid data found when processing input",
                ]) =>
            {
                NativeDiagnostic::DecodeFailed
            }
            Vd | VdFfmpeg | FfmpegVideo
                if begins(&[
                    "Failed to initialize a hardware decoder",
                    "Could not initialize hardware decoding",
                    "Hardware decoding failed",
                    "Failed setup for format",
                    "No device available for decoder",
                ]) =>
            {
                NativeDiagnostic::HwdecFailed
            }
            Vd | VdFfmpeg | Ffmpeg | FfmpegVideo
                if begins(&[
                    "Could not open codec",
                    "Failed to open codec",
                    "Could not open decoder",
                    "Failed to initialize decoder",
                ]) =>
            {
                NativeDiagnostic::DecoderOpenFailed
            }
            Vo | VoLibmpv | VoGpu | VoGpuOpenGl
                if begins(&[
                    "Failed initializing OpenGL context",
                    "Failed to create OpenGL context",
                    "Failed to initialize OpenGL",
                    "OpenGL context creation failed",
                ]) =>
            {
                NativeDiagnostic::OpenGlContextFailed
            }
            Stream | StreamCurl | FfmpegDemuxer
                if begins(&[
                    "HTTP error",
                    "Failed to connect",
                    "Could not resolve host",
                    "Connection timed out",
                    "TLS handshake failed",
                ]) =>
            {
                NativeDiagnostic::NetworkFailed
            }
            Stream | StreamCurl | FfmpegDemuxer
                if begins(&["Read error", "Error reading", "Failed to read"]) =>
            {
                NativeDiagnostic::ReadFailed
            }
            Demux | DemuxLavf | FfmpegDemuxer
                if begins(&[
                    "Failed to open",
                    "No streams found",
                    "Error opening",
                    "Invalid data found",
                ]) =>
            {
                NativeDiagnostic::DemuxFailed
            }
            _ => NativeDiagnostic::Unclassified,
        };
        Self {
            code,
            module,
            severity,
        }
    }
    #[cfg(test)]
    fn summary(self) -> String {
        format!(
            "code={} module={} severity={}",
            self.code.code(),
            self.module.name(),
            self.severity.name()
        )
    }
}

#[derive(Default)]
struct WarningBudget {
    seen: Vec<NativeMessage>,
    emitted: u8,
    suppressed: u64,
}
impl WarningBudget {
    fn accept(&mut self, message: NativeMessage) -> bool {
        if self.seen.contains(&message) || self.emitted >= 8 {
            self.suppressed = self.suppressed.saturating_add(1);
            return false;
        }
        self.seen.push(message);
        self.emitted += 1;
        true
    }
}
#[derive(Default)]
struct Observations {
    evidence: Option<Evidence>,
    changes: u32,
    suppressed: u64,
    warnings: WarningBudget,
    failures: Vec<PlaybackFailure>,
    phase: Option<Lifecycle>,
    start_file: Option<Instant>,
    media_sequence: u64,
    qualified: bool,
    published: bool,
}
pub(super) struct Diagnostics {
    pub id: u64,
    sequence: AtomicU64,
    started: Instant,
    observations: Mutex<Observations>,
}
impl Diagnostics {
    pub fn new(config: &Config) -> Self {
        let this = Self {
            id: NEXT_PLAYER.fetch_add(1, Ordering::Relaxed),
            sequence: AtomicU64::new(0),
            started: Instant::now(),
            observations: Mutex::new(Observations::default()),
        };
        this.event(Lifecycle::Startup);
        #[cfg(feature = "gpu-surface")]
        let gpu_surface = config.gpu_surface.is_some();
        #[cfg(not(feature = "gpu-surface"))]
        let gpu_surface = false;
        tracing::info!(
            player_id = this.id,
            requested_backend = backend(config.render_backend),
            hardware_policy = decoding(config.hardware_decoding),
            gpu_surface,
            platform = std::env::consts::OS,
            "mpv configuration"
        );
        this
    }
    pub fn event(&self, event: Lifecycle) -> u64 {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
        let (media_sequence, start_file_elapsed_ms, open_duration_ms) = {
            let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
            observed.phase = Some(event);
            if event == Lifecycle::StartFile {
                observed.media_sequence = observed.media_sequence.wrapping_add(1);
                observed.start_file = Some(Instant::now());
            }
            let elapsed = observed
                .start_file
                .map(|start| start.elapsed().as_secs_f64() * 1000.0);
            // This is START_FILE -> FILE_LOADED, not load intent -> readiness.
            // No duration is attributed to an unobserved or already closed file.
            let duration = if event == Lifecycle::FileLoaded {
                elapsed
            } else {
                None
            };
            if event == Lifecycle::EndFile {
                observed.start_file = None;
            }
            (observed.media_sequence, elapsed, duration)
        };
        let scope = if event == Lifecycle::WorkersStopped {
            "workers"
        } else {
            "player"
        };
        tracing::info!(
            player_id = self.id,
            sequence,
            scope,
            event = event.name(),
            elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0,
            media_sequence,
            start_file_elapsed_ms,
            open_duration_ms,
            "mpv lifecycle"
        );
        sequence
    }
    pub fn effective_hwdec_request(&self, value: &str) {
        let effective_hwdec_request = vocabulary(
            Some(value),
            &[
                "vaapi",
                "d3d11va",
                "videotoolbox",
                "no",
                "auto",
                "auto-copy",
            ],
        );
        tracing::info!(
            player_id = self.id,
            effective_hwdec_request,
            "mpv hardware decoder request"
        );
    }
    pub fn capability_error(&self, error: &Error) {
        let details = FailureDetails::from_error(error);
        tracing::info!(
            player_id = self.id,
            failure_code = error.playback_failure().code(),
            error_kind = details.kind,
            operation = details.operation,
            mpv_error = details.mpv_error,
            capability = "gpu-renderer",
            "mpv capability unavailable"
        );
    }
    pub fn warning_capability(&self, available: bool) {
        tracing::info!(player_id = self.id, available, "mpv native warning capture");
    }
    pub fn native_message(&self, message: NativeMessage) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        if !observed.warnings.accept(message) {
            return;
        }
        let phase = observed.phase.map(Lifecycle::name).unwrap_or("unknown");
        // Native FATAL is a diagnostic severity, not a terminal Player event.
        // Only END_FILE / typed worker failures decide playback state.
        macro_rules! emit {
            ($level:expr) => {
                tracing::event!(
                    $level,
                    player_id = self.id,
                    diagnostic_code = message.code.code(),
                    module = message.module.name(),
                    severity = message.severity.name(),
                    phase,
                    media_sequence = observed.media_sequence,
                    player_terminal = false,
                    elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0,
                    "mpv native diagnostic"
                )
            };
        }
        match message.severity {
            NativeSeverity::Fatal | NativeSeverity::Error => emit!(tracing::Level::ERROR),
            NativeSeverity::Warn => emit!(tracing::Level::WARN),
            NativeSeverity::Info | NativeSeverity::Other => emit!(tracing::Level::INFO),
        }
    }
    /// First qualification/backend-mailbox publication per media only. Neither
    /// milestone measures UI acceptance or physical presentation; no per-frame logs.
    #[cfg(feature = "gpu-surface")]
    pub fn gpu_milestone(&self, generation: u64, publication: bool) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        let recorded = if publication {
            &mut observed.published
        } else {
            &mut observed.qualified
        };
        if *recorded {
            return;
        }
        *recorded = true;
        tracing::info!(
            player_id = self.id,
            generation,
            milestone = if publication {
                "first-mailbox-publication"
            } else {
                "first-qualification"
            },
            elapsed_ms = self.started.elapsed().as_secs_f64() * 1000.0,
            media_sequence = observed.media_sequence,
            start_file_elapsed_ms = observed
                .start_file
                .map(|start| start.elapsed().as_secs_f64() * 1000.0),
            "mpv GPU surface startup timing"
        );
    }
    #[cfg(test)]
    pub fn native_diagnostic_codes(&self) -> Vec<&'static str> {
        self.observations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .warnings
            .seen
            .iter()
            .map(|message| message.code.code())
            .collect()
    }
    pub fn new_media(&self) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        observed.evidence = None;
        observed.failures.clear();
        observed.qualified = false;
        observed.published = false;
    }
    pub fn render_size(&self, size: [u32; 2]) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut evidence) = observed.evidence {
            evidence.render = size;
            self.record_evidence(&mut observed, evidence);
        }
    }
    pub fn evidence(&self, evidence: Evidence) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        self.record_evidence(&mut observed, evidence);
    }
    fn record_evidence(&self, observed: &mut Observations, evidence: Evidence) {
        if observed.evidence == Some(evidence) {
            return;
        }
        observed.evidence = Some(evidence);
        // Malformed/churning providers cannot flood logs even with changing dimensions.
        if observed.changes < 32 {
            observed.changes += 1;
            evidence_event!(
                tracing::Level::INFO,
                self.id,
                evidence,
                "none",
                FailureDetails {
                    kind: "none",
                    operation: "none",
                    mpv_error: None
                },
                false,
                "mpv evidence changed"
            );
        } else {
            observed.suppressed = observed.suppressed.saturating_add(1);
        }
    }
    #[cfg(test)]
    pub fn failure(&self, reason: PlaybackFailure, state: Option<&PlaybackState>) {
        self.failure_details(reason, state, FailureDetails::default());
    }
    pub fn media_failure(&self, code: i32, state: &PlaybackState) {
        self.failure_details(
            PlaybackFailure::MediaLoadFailed,
            Some(state),
            FailureDetails::media_end(code),
        );
    }
    pub fn failure_typed(&self, error: &Error, state: Option<&PlaybackState>) {
        self.failure_details(
            error.playback_failure(),
            state,
            FailureDetails::from_error(error),
        );
    }
    pub fn failure_details(
        &self,
        reason: PlaybackFailure,
        state: Option<&PlaybackState>,
        details: FailureDetails,
    ) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        // One warning per reviewed reason per media lifecycle. A prior command
        // error must not hide a later, differently classified fatal render error.
        if observed.failures.contains(&reason) {
            return;
        }
        observed.failures.push(reason);
        let mut evidence = observed
            .evidence
            .unwrap_or_else(|| Evidence::new([None; 6], [0; 2], [0; 2]));
        if let Some(state) = state {
            if let Some(source) = state.source_size {
                evidence.source = source;
            }
            if !state.render_size.contains(&0) {
                evidence.render = state.render_size;
            }
        }
        evidence_event!(
            tracing::Level::WARN,
            self.id,
            evidence,
            reason.code(),
            details,
            state.is_some_and(|state| state.stopped),
            "mpv playback failure"
        );
    }
    pub fn end_file(&self, reason: i32, error: i32) {
        let reason = match reason {
            0 => "eof",
            2 => "stop",
            3 => "quit",
            4 => "error",
            5 => "redirect",
            _ => "other",
        };
        self.event(Lifecycle::EndFile);
        tracing::info!(
            player_id = self.id,
            reason,
            mpv_error = error,
            "mpv media ended"
        );
    }
    pub fn pause_result(&self, code: i32) {
        tracing::info!(
            player_id = self.id,
            enqueued = code >= 0,
            request_only = true,
            mpv_error = code,
            "mpv shutdown pause request"
        );
    }
    pub fn render_ready(&self, active: RenderBackend) {
        self.event(Lifecycle::RendererReady);
        tracing::info!(
            player_id = self.id,
            active_backend = backend(active),
            "mpv renderer ready"
        );
    }
}
impl Drop for Diagnostics {
    fn drop(&mut self) {
        let observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        tracing::info!(
            player_id = self.id,
            native_emitted = observed.warnings.emitted,
            native_suppressed = observed.warnings.suppressed,
            evidence_suppressed = observed.suppressed,
            "mpv diagnostics summary"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn media_end_failure_preserves_numeric_context_without_native_text() {
        assert_eq!(
            FailureDetails::media_end(-17),
            FailureDetails {
                kind: "mpv",
                operation: "load",
                mpv_error: Some(-17),
            }
        );
    }
    #[test]
    fn typed_failure_details_keep_numeric_codes_but_never_native_text_or_operations() {
        const SECRET: &str =
            "https://user:PASSWORD_MARKER@example.invalid/private?signature=SIGNED_SECRET 日本語";
        for (operation, expected) in [
            ("configure libmpv", "configure"),
            ("initialize libmpv", "initialize"),
            ("create GPU surface render context", "render-create"),
            ("render GPU surface frame", "render"),
            ("queue command", "command"),
            ("observe direct decode evidence", "observe"),
            ("playback", "load"),
            (SECRET, "other"),
            ("/private/FILENAME_MARKER", "other"),
        ] {
            let details = FailureDetails::from_error(&Error::Mpv {
                operation,
                code: -42,
                message: SECRET.into(),
            });
            assert_eq!(details.kind, "mpv");
            assert_eq!(details.operation, expected);
            assert_eq!(details.mpv_error, Some(-42));
            for forbidden in [
                "PASSWORD_MARKER",
                "SIGNED_SECRET",
                "FILENAME_MARKER",
                "https://",
                "日本語",
                "private",
            ] {
                assert!(!format!("{details:?}").contains(forbidden));
            }
        }
        for error in [
            Error::Library {
                attempts: SECRET.into(),
            },
            Error::Symbol {
                path: SECRET.into(),
                symbol: SECRET,
                detail: SECRET.into(),
            },
            Error::Gpu(SECRET.into()),
            Error::InvalidInput(SECRET),
            Error::Worker(std::io::Error::other(SECRET)),
        ] {
            let details = FailureDetails::from_error(&error);
            assert_eq!(details.mpv_error, None);
            assert_eq!(details.operation, "none");
            assert!(!format!("{details:?}").contains("MARKER"));
            assert!(!format!("{details:?}").contains("https://"));
        }
    }
    #[test]
    fn raw_inputs_never_survive_evidence_or_native_formatting() {
        for secret in [
            "https://user:PASSWORD_MARKER@example.invalid/movie?signature=SIGNED_SECRET",
            "/private/FILENAME_MARKER.mkv",
            "C:\\private\\PASSWORD_MARKER.mkv",
            "日本語SECRET🔑",
            "password=PASSWORD_MARKER",
        ] {
            let evidence = Evidence::new([Some(secret); 6], [1920, 1080], [1280, 720])
                .with_effective_transfer(Some(secret));
            let output = evidence.summary();
            assert_eq!(evidence.actual_decoder, "other");
            for text in [
                secret.to_owned(),
                format!("Failed to connect {secret}"),
                format!("Could not open codec {secret}"),
            ] {
                for module in [secret, "vd/ffmpeg", "stream/curl"] {
                    let output = format!(
                        "{} {}",
                        output,
                        NativeMessage::classify(module, secret, &text, 30).summary()
                    );
                    for forbidden in [
                        "PASSWORD_MARKER",
                        "SIGNED_SECRET",
                        "FILENAME_MARKER",
                        "SECRET",
                        "https://",
                        "日本語",
                        "private",
                        "example.invalid",
                    ] {
                        assert!(!output.contains(forbidden), "{output}");
                    }
                }
            }
        }
    }
    #[test]
    fn native_categories_are_module_scoped_and_deduplicated_with_a_global_bound() {
        let cases = [
            (
                "vd",
                "Hardware decoding failed",
                NativeDiagnostic::HwdecFailed,
            ),
            (
                "vd/ffmpeg",
                "Could not open codec",
                NativeDiagnostic::DecoderOpenFailed,
            ),
            (
                "vo/gpu",
                "Failed to create OpenGL context",
                NativeDiagnostic::OpenGlContextFailed,
            ),
            (
                "stream/curl",
                "Failed to connect",
                NativeDiagnostic::NetworkFailed,
            ),
            ("stream", "Read error", NativeDiagnostic::ReadFailed),
            (
                "demux/lavf",
                "No streams found",
                NativeDiagnostic::DemuxFailed,
            ),
            ("other", "unknown", NativeDiagnostic::Unclassified),
        ];
        let mut budget = WarningBudget::default();
        for (module, text, code) in cases {
            let message = NativeMessage::classify(module, "warn", text, 30);
            assert_eq!(message.code, code);
            assert!(budget.accept(message));
            for _ in 0..100 {
                assert!(!budget.accept(message));
            }
        }
        assert_eq!(budget.emitted, 7);
        assert_eq!(budget.suppressed, 700);
        // A later unclassified fatal must not be swallowed by an earlier warn.
        assert!(budget.accept(NativeMessage::classify("other", "fatal", "unknown", 10)));
        assert_eq!(budget.emitted, 8);
        assert!(!budget.accept(NativeMessage::classify(
            "ffmpeg/video",
            "error",
            "different",
            20
        )));
        assert_eq!(budget.seen.len(), 8);
        assert_eq!(budget.suppressed, 701);
        assert_eq!(
            NativeMessage::classify("other", "warn", "Hardware decoding failed", 30).code,
            NativeDiagnostic::Unclassified
        );
    }
    #[test]
    fn native_failure_fixtures_do_not_infer_cancellation_or_terminal_state() {
        let fixtures = [
            (
                "ffmpeg",
                "http: Stream ends prematurely at 1024, should be 4096\n",
                NativeDiagnostic::TruncatedStream,
            ),
            (
                "ffmpeg/demuxer",
                "Packet corrupt (stream = 0, dts = 1).",
                NativeDiagnostic::TruncatedStream,
            ),
            (
                "ffmpeg/video/h264",
                "Invalid NAL unit size (1024 > 2).",
                NativeDiagnostic::DecodeFailed,
            ),
            (
                "stream",
                "Immediate exit requested\n",
                NativeDiagnostic::CancelledIo,
            ),
            (
                "stream",
                "Read error: Immediate exit requested",
                NativeDiagnostic::ReadFailed,
            ),
            (
                "other",
                "Immediate exit requested",
                NativeDiagnostic::Unclassified,
            ),
        ];
        for (module, text, expected) in fixtures {
            let message = NativeMessage::classify(module, "error", text, 20);
            assert_eq!(message.code, expected);
            assert_eq!(message.severity, NativeSeverity::Error);
        }
        for secret in [
            "https://name:token@host/private?token=SECRET",
            "/private/SECRET",
            "Bearer SECRET",
            "秘密\nSECRET",
        ] {
            for module in ["ffmpeg", "ffmpeg/video/h264", "stream", secret] {
                // A phrase embedded in a URL/path/error must not become a cause.
                let text =
                    format!("{secret}: Immediate exit requested; Stream ends prematurely at 1");
                let message = NativeMessage::classify(module, secret, &text, 20);
                assert_eq!(message.code, NativeDiagnostic::Unclassified);
                assert!(!message.summary().contains("SECRET"));
                assert!(!message.summary().contains(secret));
            }
        }
    }
    #[cfg(feature = "gpu-surface")]
    #[test]
    fn startup_milestones_are_once_per_media_and_reset_without_resetting_log_budget() {
        let diagnostics = Diagnostics::new(&Config::default());
        diagnostics.new_media();
        diagnostics.gpu_milestone(1, false);
        diagnostics.gpu_milestone(1, true);
        diagnostics.gpu_milestone(2, false);
        diagnostics.gpu_milestone(2, true);
        {
            let observations = diagnostics.observations.lock().unwrap();
            assert!(observations.qualified && observations.published);
            assert!(observations.start_file.is_none());
        }
        diagnostics.new_media();
        let observations = diagnostics.observations.lock().unwrap();
        assert!(!observations.qualified && !observations.published);
    }
    #[test]
    fn ffmpeg_aliases_and_severity_use_fixed_enums() {
        for (module, text, expected) in [
            (
                "ffmpeg/video",
                "Could not open codec SECRET",
                NativeDiagnostic::DecoderOpenFailed,
            ),
            (
                "ffmpeg/video",
                "Failed setup for format SECRET",
                NativeDiagnostic::HwdecFailed,
            ),
            (
                "ffmpeg/demuxer",
                "Invalid data found SECRET",
                NativeDiagnostic::DemuxFailed,
            ),
            (
                "ffmpeg/demuxer",
                "Read error SECRET",
                NativeDiagnostic::ReadFailed,
            ),
            (
                "ffmpeg/demuxer",
                "Failed to connect SECRET",
                NativeDiagnostic::NetworkFailed,
            ),
        ] {
            assert_eq!(
                NativeMessage::classify(module, "error", text, 20).code,
                expected
            );
        }
        assert_eq!(
            NativeMessage::classify("SECRET", "fatal", "SECRET", 0).severity,
            NativeSeverity::Other
        );
        assert_eq!(
            NativeMessage::classify("SECRET", "SECRET", "SECRET", 20).severity,
            NativeSeverity::Error
        );
        assert_eq!(
            NativeMessage::classify("SECRET", "SECRET", "SECRET", -1).severity,
            NativeSeverity::Other
        );
        let mut budget = WarningBudget::default();
        assert!(budget.accept(NativeMessage::classify("vd", "warn", "unclassified", 30)));
        assert!(budget.accept(NativeMessage::classify("vo", "warn", "unclassified", 30)));
    }
    #[test]
    fn evidence_transition_and_failure_diagnostics_are_bounded_and_reset_per_media() {
        let diagnostics = Diagnostics::new(&Config::default());
        let evidence = Evidence::new([None; 6], [1920, 1080], [1280, 720]);
        diagnostics.evidence(evidence);
        diagnostics.evidence(evidence);
        assert_eq!(diagnostics.observations.lock().unwrap().changes, 1);
        for width in 1..100 {
            diagnostics.render_size([width, 720]);
        }
        assert_eq!(diagnostics.observations.lock().unwrap().changes, 32);
        assert_eq!(diagnostics.observations.lock().unwrap().suppressed, 68);
        diagnostics.failure(PlaybackFailure::Unknown, None);
        diagnostics.failure(PlaybackFailure::Unknown, None);
        diagnostics.failure(PlaybackFailure::GpuCompletionFailed, None);
        assert_eq!(diagnostics.observations.lock().unwrap().failures.len(), 2);
        diagnostics.new_media();
        let observations = diagnostics.observations.lock().unwrap();
        assert!(observations.evidence.is_none());
        assert!(observations.failures.is_empty());
        assert_eq!(observations.changes, 32);
    }
    #[test]
    fn open_timing_is_scoped_to_observed_start_file_and_closed_at_end_file() {
        let diagnostics = Diagnostics::new(&Config::default());
        diagnostics.new_media();
        diagnostics.event(Lifecycle::LoadRequested);
        assert!(
            diagnostics
                .observations
                .lock()
                .unwrap()
                .start_file
                .is_none()
        );
        diagnostics.event(Lifecycle::StartFile);
        diagnostics.event(Lifecycle::FileLoaded);
        {
            let observed = diagnostics.observations.lock().unwrap();
            assert_eq!(observed.media_sequence, 1);
            assert!(observed.start_file.is_some());
        }
        diagnostics.event(Lifecycle::EndFile);
        assert!(
            diagnostics
                .observations
                .lock()
                .unwrap()
                .start_file
                .is_none()
        );
        diagnostics.event(Lifecycle::FileLoaded); // A late event cannot reopen timing.
        assert!(
            diagnostics
                .observations
                .lock()
                .unwrap()
                .start_file
                .is_none()
        );
        diagnostics.event(Lifecycle::StartFile);
        assert_eq!(diagnostics.observations.lock().unwrap().media_sequence, 2);
    }
    #[test]
    fn missing_observations_are_distinct_from_unrecognized_metadata() {
        let evidence = Evidence::new(
            [
                None,
                Some(""),
                Some("unavailable"),
                Some("SECRET"),
                Some("unknown"),
                Some("h264"),
            ],
            [0; 2],
            [0; 2],
        );
        assert_eq!(
            evidence.unavailable,
            [true, true, true, false, false, false]
        );
        assert_eq!(evidence.transfer, "unknown");
        assert_eq!(evidence.storage, "other");
    }
    #[cfg(feature = "gpu-surface")]
    #[test]
    fn worker_shutdown_and_native_quarantine_have_distinct_fixed_diagnostics() {
        assert_eq!(Lifecycle::WorkersStopped.name(), "workers-stopped");
        assert_eq!(NativeCleanup::Quarantined.name(), "quarantined");
        assert_eq!(NativeCleanup::Complete.name(), "complete");
        assert_ne!(
            NativeCleanup::Quarantined.name(),
            NativeCleanup::Complete.name()
        );
    }
    #[test]
    fn lifecycle_ids_and_order_are_local_and_monotonic() {
        let first = Diagnostics::new(&Config::default());
        let second = Diagnostics::new(&Config::default());
        assert!(second.id > first.id);
        let mut previous = 0;
        for event in [
            Lifecycle::RendererStarting,
            Lifecycle::RendererReady,
            Lifecycle::LoadRequested,
            Lifecycle::StartFile,
            Lifecycle::FileLoaded,
            Lifecycle::EndFile,
            Lifecycle::ShutdownRequested,
            Lifecycle::ControlStopped,
            Lifecycle::WorkersStopped,
        ] {
            let next = first.event(event);
            assert!(next > previous);
            previous = next;
        }
    }
}

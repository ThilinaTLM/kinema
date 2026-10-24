//! Privacy boundary for backend telemetry. Only fixed vocabulary and numeric
//! observations leave this module; native strings are never tracing fields.
use super::player::Config;
use crate::{Error, HardwareDecoding, PlaybackFailure, PlaybackState, RenderBackend};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

static NEXT_PLAYER: AtomicU64 = AtomicU64::new(1);

// Every string expression below belongs to Evidence's fixed vocabulary. Keep
// the event fields explicit so text and JSON subscribers expose the same data.
macro_rules! evidence_event {
    ($level:expr, $id:expr, $e:expr, $failure:expr, $details:expr, $message:literal) => {
        tracing::event!(
            $level,
            player_id = $id,
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
                | "create experimental GPU render context" => "render-create",
                "software render"
                | "OpenGL render"
                | "render experimental GPU frame"
                | "skip hidden frame"
                | "skip GPU frame"
                | "skip experimental frame" => "render",
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

#[cfg(feature = "experimental-gpu")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum NativeCleanup {
    Complete,
    Quarantined,
}
#[cfg(feature = "experimental-gpu")]
impl NativeCleanup {
    fn name(self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Quarantined => "quarantined",
        }
    }
}
#[cfg(feature = "experimental-gpu")]
pub(super) fn native_cleanup(player_id: u64, outcome: NativeCleanup) {
    tracing::info!(
        player_id,
        native_cleanup = outcome.name(),
        "mpv experimental native cleanup"
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
            "ffmpeg/video" => Self::FfmpegVideo,
            "ffmpeg/demuxer" => Self::FfmpegDemuxer,
            "demux" => Self::Demux,
            "demux/lavf" => Self::DemuxLavf,
            "stream" => Self::Stream,
            "stream/curl" => Self::StreamCurl,
            "cplayer" => Self::Cplayer,
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
}
pub(super) struct Diagnostics {
    pub id: u64,
    sequence: AtomicU64,
    observations: Mutex<Observations>,
}
impl Diagnostics {
    pub fn new(config: &Config) -> Self {
        let this = Self {
            id: NEXT_PLAYER.fetch_add(1, Ordering::Relaxed),
            sequence: AtomicU64::new(0),
            observations: Mutex::new(Observations::default()),
        };
        this.event(Lifecycle::Startup);
        #[cfg(feature = "experimental-gpu")]
        let experimental = config.experimental_gpu.is_some();
        #[cfg(not(feature = "experimental-gpu"))]
        let experimental = false;
        tracing::info!(
            player_id = this.id,
            requested_backend = backend(config.render_backend),
            hardware_policy = decoding(config.hardware_decoding),
            experimental,
            platform = std::env::consts::OS,
            "mpv configuration"
        );
        this
    }
    pub fn event(&self, event: Lifecycle) -> u64 {
        let sequence = self.sequence.fetch_add(1, Ordering::Relaxed);
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
        if self
            .observations
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .warnings
            .accept(message)
        {
            tracing::warn!(
                player_id = self.id,
                diagnostic_code = message.code.code(),
                module = message.module.name(),
                severity = message.severity.name(),
                "mpv native diagnostic"
            );
        }
    }
    pub fn new_media(&self) {
        let mut observed = self.observations.lock().unwrap_or_else(|e| e.into_inner());
        observed.evidence = None;
        observed.failures.clear();
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
                "mpv evidence changed"
            );
        } else {
            observed.suppressed = observed.suppressed.saturating_add(1);
        }
    }
    pub fn failure(&self, reason: PlaybackFailure, state: Option<&PlaybackState>) {
        self.failure_details(reason, state, FailureDetails::default());
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
    fn typed_failure_details_keep_numeric_codes_but_never_native_text_or_operations() {
        const SECRET: &str =
            "https://user:PASSWORD_MARKER@example.invalid/private?signature=SIGNED_SECRET 日本語";
        for (operation, expected) in [
            ("configure libmpv", "configure"),
            ("initialize libmpv", "initialize"),
            ("create experimental GPU render context", "render-create"),
            ("render experimental GPU frame", "render"),
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
    #[cfg(feature = "experimental-gpu")]
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

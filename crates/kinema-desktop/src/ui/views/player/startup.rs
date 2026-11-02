//! Startup configuration and failure classification.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum FailurePhase {
    Config,
    Initialize,
    Load,
    Runtime,
    Command,
    #[cfg(feature = "gpu-surface")]
    Surface,
}

impl FailurePhase {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::Config => "config",
            Self::Initialize => "initialize",
            Self::Load => "load",
            Self::Runtime => "runtime",
            Self::Command => "command",
            #[cfg(feature = "gpu-surface")]
            Self::Surface => "surface",
        }
    }
}

// The string stays private; only reviewed categories may be presented or logged.
pub(super) type StartupError = (String, PlaybackFailure, FailurePhase);

pub(super) fn startup_error(error: kinema_mpv::Error, phase: FailurePhase) -> StartupError {
    let reason = error.playback_failure();
    (error.to_string(), reason, phase)
}

pub(super) fn failure_phase(state: &PlaybackState, private: Option<FailurePhase>) -> FailurePhase {
    if state.failure_reason.is_some() {
        FailurePhase::Runtime
    } else {
        private.unwrap_or(FailurePhase::Runtime)
    }
}

/// Only reviewed, static backend text crosses the presentation boundary.
pub(super) fn failure_label(reason: PlaybackFailure) -> String {
    format!("{}: {}", reason.code(), reason.message())
}

pub(super) fn failure_reason(
    state: &PlaybackState,
    private: Option<PlaybackFailure>,
) -> PlaybackFailure {
    state
        .failure_reason
        .or(private)
        .unwrap_or(PlaybackFailure::Unknown)
}

/// A failed window reports exactly once, even when sampled at display cadence.
pub(super) fn failure_transition(failed: bool, reported: &mut bool) -> bool {
    if !failed || *reported {
        return false;
    }
    *reported = true;
    true
}

#[cfg(feature = "gpu-surface")]
pub(super) fn required_gpu_context<T>(context: Option<T>) -> Result<T, StartupError> {
    context.ok_or_else(|| {
        (
            embedded_help().into(),
            PlaybackFailure::GpuContextUnavailable,
            FailurePhase::Config,
        )
    })
}

pub(super) fn player_config() -> Result<Config, StartupError> {
    #[cfg(not(feature = "gpu-surface"))]
    {
        Err((
            embedded_help().into(),
            PlaybackFailure::BackendNotCompiled,
            FailurePhase::Config,
        ))
    }
    #[cfg(feature = "gpu-surface")]
    {
        #[cfg(any(target_os = "linux", target_os = "windows"))]
        let context = {
            let context = required_gpu_context(gpui_wgpu::experimental_gpu_context())?;
            let gate = context.experimental_gpu_queue_gate();
            kinema_mpv::GpuSurfaceContext::new(context.device.clone(), context.queue.clone(), gate)
        };
        #[cfg(target_os = "macos")]
        let context = kinema_mpv::GpuSurfaceContext::new_metal(required_gpu_context(
            gpui_apple::experimental_gpu_registry_id(),
        )?);
        Ok(Config {
            resolution: ResolutionPolicy::Source,
            gpu_surface: Some(context),
            ..Default::default()
        })
    }
}

#[cfg(feature = "gpu-surface")]
pub(super) fn video_surface(
    frame: kinema_mpv::GpuSurfaceFrame,
) -> Result<Arc<VideoSurfaceFrame>, String> {
    let frame = Arc::new(frame);
    let dimensions = frame.size();
    #[cfg(any(target_os = "linux", target_os = "windows"))]
    let surface = GpuSurfaceFrame::new(
        frame
            .texture()
            .create_view(&wgpu::TextureViewDescriptor::default()),
        dimensions,
        frame,
    );
    #[cfg(target_os = "macos")]
    let surface = GpuSurfaceFrame::new(frame.pixel_buffer(), dimensions, frame);
    surface
        .map(Arc::new)
        .map_err(|_| "Invalid embedded GPU surface; refusing CPU fallback".into())
}

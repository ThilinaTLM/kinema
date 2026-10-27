use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Bounded reviewed diagnostic. Never display/log `Display` as a substitute.
    pub fn playback_failure(&self) -> crate::PlaybackFailure {
        use crate::PlaybackFailure as F;
        match self {
            Self::Library { .. } => F::LibmpvUnavailable,
            Self::ClientCreation => F::RenderInitFailed,
            Self::Symbol { .. } | Self::UnsupportedApi(_) => F::LibmpvIncompatible,
            Self::Gpu(message) => F::from_gpu_message(message),
            Self::Mpv { operation, .. } => match *operation {
                "create software render context"
                | "create OpenGL render context"
                | "create GPU surface render context"
                | "configure libmpv"
                | "initialize libmpv"
                | "require direct hardware decode"
                | "disable scripting/OSC"
                | "observe property"
                | "observe direct decode evidence" => F::RenderInitFailed,
                "playback" => F::MediaLoadFailed,
                _ => F::Unknown,
            },
            Self::Allocation => F::RenderSizeLimit,
            Self::InvalidInput(
                "render dimensions must be positive signed 32-bit integers"
                | "render frame exceeds max_frame_bytes; select a cap or increase the guard"
                | "CPU render surface including 64-byte row padding exceeds max_frame_bytes; select smaller dimensions or increase the guard"
                | "max_frame_bytes must be positive",
            ) => F::RenderSizeLimit,
            _ => F::Unknown,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PlaybackFailure as F;

    #[test]
    fn typed_boundaries_do_not_classify_or_echo_error_payloads() {
        let secret = "https://user:SECRET@example.invalid/private?token=SECRET";
        let cases = [
            (
                Error::Library {
                    attempts: secret.into(),
                },
                F::LibmpvUnavailable,
            ),
            (
                Error::Symbol {
                    path: secret.into(),
                    symbol: "SECRET",
                    detail: secret.into(),
                },
                F::LibmpvIncompatible,
            ),
            (Error::UnsupportedApi(0), F::LibmpvIncompatible),
            (
                Error::Mpv {
                    operation: "create GPU surface render context",
                    code: -1,
                    message: secret.into(),
                },
                F::RenderInitFailed,
            ),
            (
                Error::Mpv {
                    operation: "playback",
                    code: -1,
                    message: secret.into(),
                },
                F::MediaLoadFailed,
            ),
            (
                Error::Mpv {
                    operation: "SECRET",
                    code: -1,
                    message: "this proof currently supports Intel Mesa only".into(),
                },
                F::Unknown,
            ),
            (Error::Gpu(secret.into()), F::Unknown),
            (Error::ClientCreation, F::RenderInitFailed),
            (Error::Allocation, F::RenderSizeLimit),
        ];
        for (error, expected) in cases {
            let reason = error.playback_failure();
            assert_eq!(reason, expected);
            assert!(!reason.code().contains("SECRET"));
            assert!(!reason.message().contains("SECRET"));
            assert!(!reason.message().contains("example.invalid"));
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("GPU renderer: {0}")]
    Gpu(String),
    #[error("could not load libmpv ({attempts}); install libmpv or set MPV_LIBRARY_PATH")]
    Library { attempts: String },
    #[error("libmpv at {path:?} is missing {symbol}: {detail}")]
    Symbol {
        path: PathBuf,
        symbol: &'static str,
        detail: String,
    },
    #[error("unsupported libmpv client API {0:#x}; need major 2")]
    UnsupportedApi(u64),
    #[error("{operation}: {message} ({code})")]
    Mpv {
        operation: &'static str,
        code: i32,
        message: String,
    },
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    #[error("could not create libmpv client")]
    ClientCreation,
    #[error("frame allocation failed or dimensions overflow")]
    Allocation,
    #[error("player command queue is full")]
    QueueFull,
    #[error("player worker has stopped")]
    Disconnected,
    #[error("could not start worker: {0}")]
    Worker(#[from] std::io::Error),
}

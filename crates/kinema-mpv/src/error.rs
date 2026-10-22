use std::path::PathBuf;

pub type Result<T> = std::result::Result<T, Error>;

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
    #[error("frame allocation failed or dimensions overflow")]
    Allocation,
    #[error("player command queue is full")]
    QueueFull,
    #[error("player worker has stopped")]
    Disconnected,
    #[error("could not start worker: {0}")]
    Worker(#[from] std::io::Error),
}

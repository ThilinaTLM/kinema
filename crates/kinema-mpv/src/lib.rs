//! Runtime-loaded libmpv playback with a latest-frame mailbox; no UI dependency.
//!
//! Create and drop [`Player`] outside the UI thread: initialization and worker
//! joins block. Native FFI is confined to this crate. See README.md for runtime
//! prerequisites and PROVENANCE.md for the Apache-2.0 experiment attribution.
mod error;
pub mod mpv;
mod state;

pub use error::{Error, Result};
pub use mpv::{Config, Frame, Player};
pub use state::{
    BufferedRange, Chapter, HardwareDecoding, PlaybackState, RenderBackend, RenderDiagnostics,
    ResolutionPolicy, Track, TrackKind,
};

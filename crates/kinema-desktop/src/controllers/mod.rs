//! Orchestration controllers: app coordination, lifecycle, play sessions and
//! enrichment. No rendering; views reach them through entities and events.
pub mod coordinator;
pub(crate) mod enrichment;
pub mod lifecycle;
pub(crate) mod play_session;
pub(crate) mod playback_progress;

//! Durable local FIFO envelope around the catalog owner's authoritative protocol.
pub(crate) use crate::catalog::library::{LibraryOperation, LibraryResult};

pub(crate) fn is_mutation(operation: &LibraryOperation) -> bool {
    matches!(
        operation,
        LibraryOperation::PlaybackProgress(operation) if !matches!(operation, crate::catalog::playback_progress::PlaybackProgressOperation::Read(_))
    ) || matches!(
        operation,
        LibraryOperation::SetMembership { .. }
            | LibraryOperation::SetMovieWatched { .. }
            | LibraryOperation::SetEpisodeWatched { .. }
            | LibraryOperation::SetSeasonWatched { .. }
    )
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LocalRequest {
    pub request_id: u64,
    pub generation: u64,
    pub operation: LibraryOperation,
}
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LocalEvent {
    pub request: LocalRequest,
    pub revision: u64,
    pub result: Result<LibraryResult, String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalSubmit {
    Queued,
    Full,
    Closed,
}
/// Accepted but not yet acknowledged local work, including unread events.
pub(crate) const LOCAL_CAPACITY: usize = 64;

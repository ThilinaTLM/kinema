// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QString>

namespace kinema::playback::session {

enum class TransferState {
    None,
    Queued,
    Resolving,
    Streaming, ///< OnDemand: bytes flowing only as the player asks
    Downloading, ///< Full mode: eager fetch
    IdleCached, ///< OnDemand: nothing flowing, file partially cached
    Paused,
    Complete,
    Failed,
    Cancelled,
    Removed,
};

enum class TransferInput {
    OpenSession,
    OnDemandReady,
    FullReady,
    SaveOffline,
    PlayerAttached,
    PlayerDetached,
    Pause,
    Resume,
    AllBytesCached,
    Error,
    Retry,
    Cancel,
    Remove,
};

QString transferStateName(TransferState s);
QString transferInputName(TransferInput i);

/// Pure state machine that mirrors `core::DownloadStore` semantics
/// without owning persistence. Adapters/projections translate
/// `TransferState` into the existing `domain::DownloadState` +
/// `DownloadMode` + `CacheDisposition` triple on persistence.
class TransferStateMachine
{
public:
    TransferStateMachine() = default;

    TransferState state() const noexcept { return m_state; }
    void reset() noexcept { m_state = TransferState::None; }

    /// Drive the machine. Returns the new state; equals
    /// `state()` when the input was ignored.
    TransferState handle(TransferInput input) noexcept;

    /// Convenience projection: map current `(state)` plus
    /// `playerAttached` hint into a `domain::DownloadState`
    /// suitable for persistence in `download_items`.
    domain::DownloadState toDownloadState(bool playerAttached) const noexcept;

private:
    TransferState m_state = TransferState::None;
};

} // namespace kinema::playback::session

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/TransferStateMachine.h"

namespace kinema::playback::session {

QString transferStateName(TransferState s)
{
    switch (s) {
    case TransferState::None: return QStringLiteral("None");
    case TransferState::Queued: return QStringLiteral("Queued");
    case TransferState::Resolving: return QStringLiteral("Resolving");
    case TransferState::Streaming: return QStringLiteral("Streaming");
    case TransferState::Downloading: return QStringLiteral("Downloading");
    case TransferState::IdleCached: return QStringLiteral("IdleCached");
    case TransferState::Paused: return QStringLiteral("Paused");
    case TransferState::Complete: return QStringLiteral("Complete");
    case TransferState::Failed: return QStringLiteral("Failed");
    case TransferState::Cancelled: return QStringLiteral("Cancelled");
    case TransferState::Removed: return QStringLiteral("Removed");
    }
    return QStringLiteral("Unknown");
}

QString transferInputName(TransferInput i)
{
    switch (i) {
    case TransferInput::OpenSession: return QStringLiteral("OpenSession");
    case TransferInput::OnDemandReady: return QStringLiteral("OnDemandReady");
    case TransferInput::FullReady: return QStringLiteral("FullReady");
    case TransferInput::SaveOffline: return QStringLiteral("SaveOffline");
    case TransferInput::PlayerAttached: return QStringLiteral("PlayerAttached");
    case TransferInput::PlayerDetached: return QStringLiteral("PlayerDetached");
    case TransferInput::Pause: return QStringLiteral("Pause");
    case TransferInput::Resume: return QStringLiteral("Resume");
    case TransferInput::AllBytesCached: return QStringLiteral("AllBytesCached");
    case TransferInput::Error: return QStringLiteral("Error");
    case TransferInput::Retry: return QStringLiteral("Retry");
    case TransferInput::Cancel: return QStringLiteral("Cancel");
    case TransferInput::Remove: return QStringLiteral("Remove");
    }
    return QStringLiteral("Unknown");
}

TransferState TransferStateMachine::handle(TransferInput input) noexcept
{
    using S = TransferState;
    using I = TransferInput;

    // Universal terminal inputs.
    if (input == I::Remove) {
        m_state = S::Removed;
        return m_state;
    }
    if (input == I::Cancel) {
        if (m_state != S::Complete && m_state != S::Removed) {
            m_state = S::Cancelled;
        }
        return m_state;
    }
    if (input == I::Error) {
        if (m_state != S::Complete && m_state != S::Removed
            && m_state != S::Cancelled) {
            m_state = S::Failed;
        }
        return m_state;
    }
    if (input == I::AllBytesCached) {
        if (m_state == S::Streaming || m_state == S::Downloading
            || m_state == S::IdleCached || m_state == S::Paused
            || m_state == S::Resolving) {
            m_state = S::Complete;
        }
        return m_state;
    }

    switch (m_state) {
    case S::None:
        if (input == I::OpenSession) {
            m_state = S::Queued;
        }
        break;

    case S::Queued:
        if (input == I::OnDemandReady) {
            m_state = S::Streaming;
        } else if (input == I::FullReady) {
            m_state = S::Downloading;
        } else if (input == I::OpenSession) {
            m_state = S::Resolving;
        }
        break;

    case S::Resolving:
        if (input == I::OnDemandReady) {
            m_state = S::Streaming;
        } else if (input == I::FullReady) {
            m_state = S::Downloading;
        }
        break;

    case S::Streaming:
        if (input == I::PlayerDetached) {
            m_state = S::IdleCached;
        } else if (input == I::SaveOffline || input == I::FullReady) {
            m_state = S::Downloading;
        } else if (input == I::Pause) {
            m_state = S::Paused;
        }
        break;

    case S::Downloading:
        if (input == I::Pause) {
            m_state = S::Paused;
        }
        break;

    case S::IdleCached:
        if (input == I::PlayerAttached) {
            m_state = S::Streaming;
        } else if (input == I::SaveOffline || input == I::FullReady) {
            m_state = S::Downloading;
        } else if (input == I::Pause) {
            m_state = S::Paused;
        }
        break;

    case S::Paused:
        if (input == I::Resume || input == I::Retry) {
            // Resume route: we don't know whether the caller wants
            // OnDemand or Full; default to Streaming and let a
            // follow-up `SaveOffline` push it to Downloading.
            m_state = S::Streaming;
        }
        break;

    case S::Complete:
        if (input == I::PlayerAttached) {
            // Re-streaming from a fully-cached file is still "ready"
            // — the source merely serves cached bytes. Leaving as
            // Complete keeps UI accurate.
        }
        break;

    case S::Failed:
        if (input == I::Retry) {
            m_state = S::Queued;
        }
        break;

    case S::Cancelled:
        if (input == I::Retry) {
            m_state = S::Queued;
        }
        break;

    case S::Removed:
        // Terminal; reset() needed before re-use.
        break;
    }
    return m_state;
}

domain::DownloadState TransferStateMachine::toDownloadState(
    bool playerAttached) const noexcept
{
    using S = TransferState;
    switch (m_state) {
    case S::None:
    case S::Queued:
        return domain::DownloadState::Queued;
    case S::Resolving:
        return domain::DownloadState::Resolving;
    case S::Streaming:
    case S::Downloading:
        return domain::DownloadState::Active;
    case S::IdleCached:
        return playerAttached
            ? domain::DownloadState::Active
            : domain::DownloadState::Idle;
    case S::Paused:
        return domain::DownloadState::Paused;
    case S::Complete:
        return domain::DownloadState::Completed;
    case S::Failed:
        return domain::DownloadState::Failed;
    case S::Cancelled:
    case S::Removed:
        return domain::DownloadState::Cancelled;
    }
    return domain::DownloadState::Queued;
}

} // namespace kinema::playback::session

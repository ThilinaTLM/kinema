// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackStateMachine.h"

namespace kinema::playback::session {

QString playbackStateName(PlaybackState s)
{
    switch (s) {
    case PlaybackState::Idle: return QStringLiteral("Idle");
    case PlaybackState::ResolvingSource: return QStringLiteral("ResolvingSource");
    case PlaybackState::PreparingTransfer: return QStringLiteral("PreparingTransfer");
    case PlaybackState::LoadingPlayer: return QStringLiteral("LoadingPlayer");
    case PlaybackState::ShowingResumePrompt: return QStringLiteral("ShowingResumePrompt");
    case PlaybackState::Playing: return QStringLiteral("Playing");
    case PlaybackState::Paused: return QStringLiteral("Paused");
    case PlaybackState::Seeking: return QStringLiteral("Seeking");
    case PlaybackState::Buffering: return QStringLiteral("Buffering");
    case PlaybackState::Stopping: return QStringLiteral("Stopping");
    case PlaybackState::Ending: return QStringLiteral("Ending");
    case PlaybackState::Completed: return QStringLiteral("Completed");
    case PlaybackState::Failed: return QStringLiteral("Failed");
    }
    return QStringLiteral("Unknown");
}

QString playbackInputName(PlaybackInput i)
{
    switch (i) {
    case PlaybackInput::PlayStream: return QStringLiteral("PlayStream");
    case PlaybackInput::SourceResolved: return QStringLiteral("SourceResolved");
    case PlaybackInput::TransferReady: return QStringLiteral("TransferReady");
    case PlaybackInput::PlayableUrlReady: return QStringLiteral("PlayableUrlReady");
    case PlaybackInput::ResumePromptRequired: return QStringLiteral("ResumePromptRequired");
    case PlaybackInput::ResumeAccepted: return QStringLiteral("ResumeAccepted");
    case PlaybackInput::ResumeDeclined: return QStringLiteral("ResumeDeclined");
    case PlaybackInput::PlayerLoaded: return QStringLiteral("PlayerLoaded");
    case PlaybackInput::Pause: return QStringLiteral("Pause");
    case PlaybackInput::Resume: return QStringLiteral("Resume");
    case PlaybackInput::SeekStarted: return QStringLiteral("SeekStarted");
    case PlaybackInput::SeekCompleted: return QStringLiteral("SeekCompleted");
    case PlaybackInput::BufferingStarted: return QStringLiteral("BufferingStarted");
    case PlaybackInput::BufferingEnded: return QStringLiteral("BufferingEnded");
    case PlaybackInput::EndOfFile: return QStringLiteral("EndOfFile");
    case PlaybackInput::Stop: return QStringLiteral("Stop");
    case PlaybackInput::UserClosed: return QStringLiteral("UserClosed");
    case PlaybackInput::Error: return QStringLiteral("Error");
    case PlaybackInput::LoadTimeout: return QStringLiteral("LoadTimeout");
    }
    return QStringLiteral("Unknown");
}

PlaybackState PlaybackStateMachine::handle(PlaybackInput input) noexcept
{
    using S = PlaybackState;
    using I = PlaybackInput;

    // Universal: any error or load timeout from non-terminal states
    // moves us to Failed. From terminal states the input is ignored.
    if (input == I::Error || input == I::LoadTimeout) {
        if (m_state != S::Completed && m_state != S::Failed) {
            m_state = S::Failed;
        }
        return m_state;
    }
    // Universal stop: user closes the window or stops playback.
    if (input == I::Stop || input == I::UserClosed) {
        if (m_state == S::Idle || m_state == S::Completed
            || m_state == S::Failed) {
            return m_state;
        }
        m_state = S::Stopping;
        return m_state;
    }
    // A fresh PlayStream while we already have a session means
    // "replace this with a new attempt"; rewind to ResolvingSource.
    if (input == I::PlayStream) {
        m_state = S::ResolvingSource;
        return m_state;
    }

    switch (m_state) {
    case S::Idle:
        // Nothing else is meaningful in Idle.
        break;

    case S::ResolvingSource:
        if (input == I::SourceResolved) {
            m_state = S::PreparingTransfer;
        }
        break;

    case S::PreparingTransfer:
        if (input == I::TransferReady || input == I::PlayableUrlReady) {
            m_state = S::LoadingPlayer;
        }
        break;

    case S::LoadingPlayer:
        if (input == I::ResumePromptRequired) {
            m_state = S::ShowingResumePrompt;
        } else if (input == I::PlayerLoaded) {
            m_state = S::Playing;
        }
        break;

    case S::ShowingResumePrompt:
        if (input == I::ResumeAccepted || input == I::ResumeDeclined
            || input == I::PlayerLoaded) {
            m_state = S::Playing;
        }
        break;

    case S::Playing:
        if (input == I::Pause) {
            m_state = S::Paused;
        } else if (input == I::SeekStarted) {
            m_state = S::Seeking;
        } else if (input == I::BufferingStarted) {
            m_state = S::Buffering;
        } else if (input == I::EndOfFile) {
            m_state = S::Ending;
        }
        break;

    case S::Paused:
        if (input == I::Resume) {
            m_state = S::Playing;
        } else if (input == I::SeekStarted) {
            m_state = S::Seeking;
        } else if (input == I::EndOfFile) {
            m_state = S::Ending;
        }
        break;

    case S::Seeking:
        if (input == I::SeekCompleted) {
            m_state = S::Playing;
        } else if (input == I::BufferingStarted) {
            m_state = S::Buffering;
        }
        break;

    case S::Buffering:
        if (input == I::BufferingEnded) {
            m_state = S::Playing;
        } else if (input == I::SeekStarted) {
            m_state = S::Seeking;
        }
        break;

    case S::Stopping:
        if (input == I::EndOfFile || input == I::PlayerLoaded) {
            // The underlying player has reported it stopped/loaded
            // — close out the session.
            m_state = S::Completed;
        }
        break;

    case S::Ending:
        // Wait for terminal observation; for now treat any state
        // observation as transition to Completed.
        m_state = S::Completed;
        break;

    case S::Completed:
    case S::Failed:
        // Terminal. Reset() is needed before a new session.
        break;
    }
    return m_state;
}

} // namespace kinema::playback::session

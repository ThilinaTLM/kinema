// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QString>

namespace kinema::playback::session {

/// User-visible lifecycle for one playback attempt. Drives the
/// transitions inside `PlaybackSession` and replaces the
/// scattered `m_phase` / `m_loadfileInFlight` booleans in the
/// legacy `PlaybackController`.
enum class PlaybackState {
    Idle,
    ResolvingSource,
    PreparingTransfer,
    LoadingPlayer,
    ShowingResumePrompt,
    Playing,
    Paused,
    Seeking,
    Buffering,
    Stopping,
    Ending,
    Completed,
    Failed,
};

/// Inputs that may cause a `PlaybackState` transition.
enum class PlaybackInput {
    PlayStream, ///< user issued a fresh `PlayStream` command
    SourceResolved, ///< backend selection + asset id finalized
    TransferReady, ///< local URL is ready for the player
    PlayableUrlReady, ///< player adapter has a URL to load
    ResumePromptRequired, ///< resume position above prompt threshold
    ResumeAccepted, ///< user clicked Resume
    ResumeDeclined, ///< user clicked Start over
    PlayerLoaded, ///< mpv reported `file-loaded`
    Pause, ///< user paused
    Resume, ///< user resumed
    SeekStarted, ///< mpv seek began
    SeekCompleted, ///< mpv seek finished
    BufferingStarted, ///< mpv buffering began
    BufferingEnded, ///< mpv buffering ended
    EndOfFile, ///< mpv natural EOF
    Stop, ///< user issued `StopPlayback`
    UserClosed, ///< user closed the player window
    Error, ///< unrecoverable playback error
    LoadTimeout, ///< load watchdog tripped
};

QString playbackStateName(PlaybackState s);
QString playbackInputName(PlaybackInput i);

/**
 * Pure state machine for `PlaybackSession`. No Qt signals, no I/O.
 *
 * Encodes the legal transitions of one playback attempt. The state
 * machine itself does not own the player; it only decides what the
 * next state is in response to inputs.
 */
class PlaybackStateMachine
{
public:
    PlaybackStateMachine() = default;

    PlaybackState state() const noexcept { return m_state; }
    void reset() noexcept { m_state = PlaybackState::Idle; }

    /// Drive the machine. Returns the new state; equals
    /// `state()` when the input was ignored.
    PlaybackState handle(PlaybackInput input) noexcept;

private:
    PlaybackState m_state = PlaybackState::Idle;
};

} // namespace kinema::playback::session

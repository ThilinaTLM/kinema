// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"
#include "playback/session/PlaybackStateMachine.h"

#include <QObject>
#include <QUuid>

#include <optional>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::session {

/**
 * One user-visible playback attempt.
 *
 * Owns a stable `PlaybackSessionId`, the `PlaybackStateMachine`,
 * and the user-visible context (stream + playback metadata).
 * Publishes typed events to `PlaybackEventStream`.
 *
 * `PlaybackSessionManager` owns orchestration; this object owns the
 * per-attempt id, state transitions, context, and event publication.
 * Player adapters publish terminal events for the same session id;
 * this session observes them and terminates without duplicate events.
 *
 * Tests construct a `PlaybackSession` directly with a stub event
 * stream and assert the event sequence and state transitions
 * without needing any of the legacy controller graph.
 */
class PlaybackSession : public QObject
{
    Q_OBJECT
public:
    explicit PlaybackSession(events::PlaybackEventStream& eventStream,
        QObject* parent = nullptr);
    ~PlaybackSession() override;

    /// Stable id for this attempt. Generated at construction; does
    /// not change.
    PlaybackSessionId id() const noexcept { return m_id; }

    /// Current state of the underlying state machine.
    PlaybackState state() const noexcept { return m_state.state(); }

    /// User-facing context attached to this attempt. Set by
    /// `start(...)`; empty before then.
    const domain::PlaybackContext& currentContext() const noexcept { return m_ctx; }

    /// Stream that triggered the attempt; nullopt before `start(...)`.
    const std::optional<domain::Stream>& currentStream() const noexcept { return m_stream; }

    /// Backend override carried through the request; nullopt when
    /// the user did not override.
    std::optional<domain::DownloadBackendKind> backendOverride() const noexcept
    {
        return m_backendOverride;
    }

    bool isTerminal() const noexcept
    {
        if (m_terminated) {
            return true;
        }
        const auto s = m_state.state();
        return s == PlaybackState::Completed
            || s == PlaybackState::Failed;
    }

    /// Begin the attempt. Publishes `PlaybackRequested` and drives
    /// the state machine to `ResolvingSource`. Must be called
    /// exactly once on a fresh session.
    void start(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        std::optional<domain::DownloadBackendKind> backendOverride = std::nullopt);

    // --------------- event hooks ----------------
    // Called by `PlaybackSessionManager` as orchestration progresses.
    // Each one drives the state machine and publishes the corresponding
    // typed event.

    void markSourceResolved(const domain::AssetRef& asset);
    void markPlayableUrl(const QString& assetId, const QUrl& url);
    void markPlayerLoading();
    void markPlayerLoaded();
    void markPositionTick(double seconds);
    void markDuration(double seconds);
    void markEnded(PlaybackEndReason reason);
    void markFailed(const QString& reason);

    /// Drive the state machine to terminal in response to a
    /// terminal event that was already published by another
    /// component (the player adapter). Does NOT publish a fresh
    /// `PlaybackEnded` event - the caller is the publisher.
    /// Idempotent.
    void markTerminatedExternally(PlaybackEndReason reason);
    void markFailedExternally();

    /// User-initiated stop. Publishes `PlaybackEnded(UserStop)`
    /// (idempotent on terminal sessions). Returns true if the
    /// session transitioned, false if it was already terminal.
    bool stopByUser();

    /// Mark this session as superseded by a fresh attempt.
    /// Publishes `PlaybackEnded(ReplacedByNewSource)` once and
    /// transitions to terminal `Completed`. Idempotent.
    void markReplacedByNewSource();

private Q_SLOTS:
    /// Reactive bridge: when a terminal event for THIS session id
    /// arrives on the stream, drive the state machine to terminal
    /// without re-publishing. Used so the player adapter is the
    /// sole publisher of `PlaybackEnded` / `PlaybackFailed`.
    void onEvent(const events::PlaybackEvent& event);

private:
    events::PlaybackEventStream& m_eventStream;
    PlaybackStateMachine m_state;
    PlaybackSessionId m_id;
    domain::PlaybackContext m_ctx;
    std::optional<domain::Stream> m_stream;
    std::optional<domain::DownloadBackendKind> m_backendOverride;
    bool m_started = false;
    /// True once a terminal `PlaybackEnded` / `PlaybackFailed` has
    /// been published for this attempt. Used so the lifecycle
    /// hooks (`markEnded`, `stopByUser`, `markFailed`,
    /// `markReplacedByNewSource`) are idempotent regardless of
    /// whether the underlying state machine has reached `Completed`
    /// or only `Stopping` / `Ending`.
    bool m_terminated = false;
};

} // namespace kinema::playback::session

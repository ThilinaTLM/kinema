// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackSession.h"

#include "playback/events/PlaybackEventStream.h"

namespace kinema::playback::session {

PlaybackSession::PlaybackSession(events::PlaybackEventStream& eventStream,
    QObject* parent)
    : QObject(parent)
    , m_eventStream(eventStream)
    , m_id(QUuid::createUuid())
{
}

PlaybackSession::~PlaybackSession() = default;

void PlaybackSession::start(const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    m_started = true;
    m_stream = stream;
    m_ctx = ctx;
    m_backendOverride = backendOverride;
    m_state.handle(PlaybackInput::PlayStream);
    m_eventStream.publish(events::PlaybackRequested {
        m_id,
        m_ctx,
    });
}

void PlaybackSession::markSourceResolved(const domain::AssetRef& asset)
{
    m_state.handle(PlaybackInput::SourceResolved);
    m_eventStream.publish(events::SourceResolved { m_id, asset });
}

void PlaybackSession::markPlayableUrl(const QString& assetId, const QUrl& url)
{
    m_state.handle(PlaybackInput::PlayableUrlReady);
    m_eventStream.publish(events::PlayableUrlReady {
        m_id,
        assetId,
        url,
    });
}

void PlaybackSession::markPlayerLoading()
{
    m_eventStream.publish(events::PlayerLoading { m_id });
}

void PlaybackSession::markPlayerLoaded()
{
    m_state.handle(PlaybackInput::PlayerLoaded);
    m_eventStream.publish(events::PlayerLoaded { m_id });
}

void PlaybackSession::markPositionTick(double seconds)
{
    m_eventStream.publish(events::PositionTicked { m_id, seconds });
}

void PlaybackSession::markDuration(double seconds)
{
    m_eventStream.publish(events::DurationChanged { m_id, seconds });
}

void PlaybackSession::markEnded(PlaybackEndReason reason)
{
    if (isTerminal()) {
        return;
    }
    m_state.handle(PlaybackInput::EndOfFile);
    // Advance the machine one more step so we land on Completed
    // rather than Ending — the transitional code expects a single
    // terminal observation per attempt.
    if (m_state.state() == PlaybackState::Ending) {
        m_state.handle(PlaybackInput::EndOfFile);
    }
    m_terminated = true;
    m_eventStream.publish(events::PlaybackEnded { m_id, reason, m_ctx });
}

void PlaybackSession::markFailed(const QString& reason)
{
    if (isTerminal()) {
        return;
    }
    m_state.handle(PlaybackInput::Error);
    m_terminated = true;
    m_eventStream.publish(events::PlaybackFailed {
        m_id,
        reason,
        m_ctx,
    });
}

bool PlaybackSession::stopByUser()
{
    if (isTerminal()) {
        return false;
    }
    m_state.handle(PlaybackInput::Stop);
    m_terminated = true;
    m_eventStream.publish(events::PlaybackEnded {
        m_id,
        PlaybackEndReason::UserStop,
        m_ctx,
    });
    return true;
}

void PlaybackSession::markReplacedByNewSource()
{
    if (isTerminal()) {
        return;
    }
    m_state.handle(PlaybackInput::Stop);
    m_terminated = true;
    m_eventStream.publish(events::PlaybackEnded {
        m_id,
        PlaybackEndReason::ReplacedByNewSource,
        m_ctx,
    });
}

} // namespace kinema::playback::session

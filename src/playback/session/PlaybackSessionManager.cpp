// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackSessionManager.h"

#include "playback/adapters/ExternalPlayerAdapter.h"
#include "playback/events/PlaybackEventStream.h"
#include "playback/session/PlaybackSession.h"
#include "services/StreamActions.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "playback/adapters/EmbeddedMpvPlayerAdapter.h"
#endif

namespace kinema::playback::session {

PlaybackSessionManager::PlaybackSessionManager(
    services::StreamActions& actions,
    events::PlaybackEventStream& eventStream,
    adapters::EmbeddedMpvPlayerAdapter* embeddedAdapter,
    adapters::ExternalPlayerAdapter* externalAdapter,
    QObject* parent)
    : QObject(parent)
    , m_actions(actions)
    , m_eventStream(eventStream)
    , m_embeddedAdapter(embeddedAdapter)
    , m_externalAdapter(externalAdapter)
{
    connect(&m_actions, &services::StreamActions::statusMessage,
        this, &PlaybackSessionManager::statusMessage);
}

void PlaybackSessionManager::stampAdapters(const domain::PlaybackContext& ctx)
{
    if (!m_session) {
        return;
    }
    const auto id = m_session->id();
    if (m_externalAdapter) {
        m_externalAdapter->setActiveSession(id, ctx);
    }
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->setActiveSession(id, ctx);
    }
#endif
}

PlaybackSessionManager::~PlaybackSessionManager() = default;

PlaybackSessionId PlaybackSessionManager::activeSessionId() const noexcept
{
    return m_session ? m_session->id() : PlaybackSessionId();
}

void PlaybackSessionManager::supersedeActiveSession()
{
    if (m_session && !m_session->isTerminal()) {
        m_session->markReplacedByNewSource();
    }
    m_session.reset();
}

void PlaybackSessionManager::play(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    supersedeActiveSession();
    m_session = std::make_unique<PlaybackSession>(m_eventStream);
    m_session->start(stream, ctx);
    stampAdapters(ctx);
    // Transitional: the legacy StreamActions path still performs
    // the actual resolution / mpv handoff. Once Phase 5 + 6 land
    // the session will drive TransferUseCase + PlayerPort
    // directly.
    m_actions.play(stream, ctx);
}

void PlaybackSessionManager::playWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    supersedeActiveSession();
    m_session = std::make_unique<PlaybackSession>(m_eventStream);
    m_session->start(stream, ctx, backend);
    stampAdapters(ctx);
    m_actions.playWithBackend(stream, ctx, backend);
}

void PlaybackSessionManager::download(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    // Save-offline is not a playback attempt; do not touch the
    // active session. Eventually this routes through
    // TransferUseCase::saveOffline.
    m_actions.download(stream, ctx);
}

void PlaybackSessionManager::downloadWithBackend(
    const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    m_actions.downloadWithBackend(stream, ctx, backend);
}

// Transport commands route directly into the embedded adapter
// (PlayerPort). The adapter publishes terminal PlaybackEnded /
// PlaybackFailed events to the stream; PlaybackSession picks
// those up via its own subscription to terminate the state
// machine without re-publishing.

void PlaybackSessionManager::pause()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->pause();
    }
#endif
}

void PlaybackSessionManager::resume()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->resume();
    }
#endif
}

void PlaybackSessionManager::playPause()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->togglePause();
    }
#endif
}

void PlaybackSessionManager::stop()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        // Adapter publishes PlaybackEnded(UserStop); session
        // subscription terminates the state machine without
        // re-publishing.
        m_embeddedAdapter->stop();
    }
#endif
}

void PlaybackSessionManager::seekRelativeSeconds(double seconds)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->seekRelative(seconds);
    }
#else
    Q_UNUSED(seconds);
#endif
}

void PlaybackSessionManager::seekAbsoluteSeconds(double seconds)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->seekAbsolute(seconds);
    }
#else
    Q_UNUSED(seconds);
#endif
}

void PlaybackSessionManager::setVolumePercent(double percent)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->setVolumePercent(percent);
    }
#else
    Q_UNUSED(percent);
#endif
}

void PlaybackSessionManager::setPlaybackRate(double factor)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embeddedAdapter) {
        m_embeddedAdapter->setPlaybackRate(factor);
    }
#else
    Q_UNUSED(factor);
#endif
}

} // namespace kinema::playback::session

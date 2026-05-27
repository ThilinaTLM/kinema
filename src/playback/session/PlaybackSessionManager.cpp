// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/session/PlaybackSessionManager.h"

#include "services/StreamActions.h"

#ifdef KINEMA_HAVE_LIBMPV
#include "controllers/PlaybackController.h"
#endif

namespace kinema::playback::session {

PlaybackSessionManager::PlaybackSessionManager(
    services::StreamActions& actions,
    controllers::PlaybackController* embedded,
    QObject* parent)
    : QObject(parent)
    , m_actions(actions)
    , m_embedded(embedded)
{
    connect(&m_actions, &services::StreamActions::statusMessage,
        this, &PlaybackSessionManager::statusMessage);
}

PlaybackSessionManager::~PlaybackSessionManager() = default;

void PlaybackSessionManager::play(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    m_actions.play(stream, ctx);
}

void PlaybackSessionManager::playWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    m_actions.playWithBackend(stream, ctx, backend);
}

void PlaybackSessionManager::download(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    m_actions.download(stream, ctx);
}

void PlaybackSessionManager::downloadWithBackend(
    const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    m_actions.downloadWithBackend(stream, ctx, backend);
}

void PlaybackSessionManager::pause()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->pause();
    }
#endif
}

void PlaybackSessionManager::resume()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->resume();
    }
#endif
}

void PlaybackSessionManager::playPause()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->playPause();
    }
#endif
}

void PlaybackSessionManager::stop()
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->stop();
    }
#endif
}

void PlaybackSessionManager::seekRelativeSeconds(double seconds)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->seekRelativeSeconds(seconds);
    }
#else
    Q_UNUSED(seconds);
#endif
}

void PlaybackSessionManager::seekAbsoluteSeconds(double seconds)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->seekAbsoluteSeconds(seconds);
    }
#else
    Q_UNUSED(seconds);
#endif
}

void PlaybackSessionManager::setVolumePercent(double percent)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->setVolumePercent(percent);
    }
#else
    Q_UNUSED(percent);
#endif
}

void PlaybackSessionManager::setPlaybackRate(double factor)
{
#ifdef KINEMA_HAVE_LIBMPV
    if (m_embedded) {
        m_embedded->setPlaybackRate(factor);
    }
#else
    Q_UNUSED(factor);
#endif
}

} // namespace kinema::playback::session


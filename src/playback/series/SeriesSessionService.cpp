// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/series/SeriesSessionService.h"

#ifdef KINEMA_HAVE_LIBMPV

#include "controllers/SeriesPlaybackSessionController.h"

namespace kinema::playback::series {

SeriesSessionService::SeriesSessionService(
    controllers::SeriesPlaybackSessionController& inner,
    QObject* parent)
    : QObject(parent)
    , m_inner(inner)
{
    connect(&m_inner,
        &controllers::SeriesPlaybackSessionController::navigationChanged,
        this, &SeriesSessionService::navigationChanged);
    connect(&m_inner,
        &controllers::SeriesPlaybackSessionController::packAdjacencyResolved,
        this, &SeriesSessionService::packAdjacencyResolved);
    connect(&m_inner,
        &controllers::SeriesPlaybackSessionController::currentStreamSizeResolved,
        this, &SeriesSessionService::currentStreamSizeResolved);
    connect(&m_inner,
        &controllers::SeriesPlaybackSessionController::windowCloseRequested,
        this, &SeriesSessionService::windowCloseRequested);
}

SeriesSessionService::~SeriesSessionService() = default;

bool SeriesSessionService::navigationVisible() const noexcept
{
    return m_inner.navigationVisible();
}

bool SeriesSessionService::canGoPrevious() const noexcept
{
    return m_inner.canGoPrevious();
}

bool SeriesSessionService::canGoNext() const noexcept
{
    return m_inner.canGoNext();
}

void SeriesSessionService::playPreviousEpisode()
{
    m_inner.playPreviousEpisode();
}

void SeriesSessionService::playNextEpisode()
{
    m_inner.playNextEpisode();
}

} // namespace kinema::playback::series

#endif // KINEMA_HAVE_LIBMPV

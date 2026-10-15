// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/subtitles/SubtitleSessionService.h"

#include "playback/events/PlaybackEventStream.h"

#include <variant>

namespace kinema::playback::subtitles {

SubtitleSessionService::SubtitleSessionService(ports::SubtitleSessionPort& inner,
                                               events::PlaybackEventStream* events,
                                               QObject* parent)
    : QObject(parent), m_inner(inner), m_events(events)
{
    if (m_events) {
        connect(m_events,
                &events::PlaybackEventStream::eventPublished,
                this,
                &SubtitleSessionService::onEvent);
    }
}

SubtitleSessionService::~SubtitleSessionService() = default;

void SubtitleSessionService::setActiveSubtitlePaths(const QStringList& paths)
{
    m_inner.setActiveSubtitlePaths(paths);
}

void SubtitleSessionService::setMoviehash(const QString& hex)
{
    m_inner.setMoviehash(hex);
}

void SubtitleSessionService::clearMoviehash()
{
    m_inner.clearMoviehash();
}

void SubtitleSessionService::onEvent(const events::PlaybackEvent& event)
{
    std::visit(
        [this](const auto& payload) {
            using T = std::decay_t<decltype(payload)>;
            if constexpr (std::is_same_v<T, events::PlaybackRequested>) {
                // Fresh attempt: any cached hash belongs to the
                // previous title and must not bleed into the new
                // search.
                m_inner.clearMoviehash();
            } else if constexpr (std::is_same_v<T, events::MoviehashComputed>) {
                m_inner.setMoviehash(payload.hex);
            }
        },
        event);
}

} // namespace kinema::playback::subtitles

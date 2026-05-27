// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/subtitles/SubtitleSessionService.h"

#include "controllers/SubtitleController.h"

namespace kinema::playback::subtitles {

SubtitleSessionService::SubtitleSessionService(
    controllers::SubtitleController& inner, QObject* parent)
    : QObject(parent)
    , m_inner(inner)
{
}

SubtitleSessionService::~SubtitleSessionService() = default;

void SubtitleSessionService::setActiveSubtitlePaths(
    const QStringList& paths)
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

} // namespace kinema::playback::subtitles

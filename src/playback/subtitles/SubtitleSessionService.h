// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/SubtitlePort.h"

#include <QObject>

namespace kinema::controllers {
class SubtitleController;
}

namespace kinema::playback::subtitles {

/**
 * Facade over `controllers::SubtitleController` exposing only the
 * `SubtitlePort` slice used by `PlaybackSession`. Lets the session
 * push moviehash / active-subtitle state into the controller
 * without depending on the full search/download surface.
 */
class SubtitleSessionService : public QObject,
    public ports::SubtitlePort
{
    Q_OBJECT
public:
    explicit SubtitleSessionService(
        controllers::SubtitleController& inner,
        QObject* parent = nullptr);
    ~SubtitleSessionService() override;

    void setActiveSubtitlePaths(const QStringList& paths) override;
    void setMoviehash(const QString& hex) override;
    void clearMoviehash() override;

    controllers::SubtitleController& inner() noexcept { return m_inner; }

private:
    controllers::SubtitleController& m_inner;
};

} // namespace kinema::playback::subtitles

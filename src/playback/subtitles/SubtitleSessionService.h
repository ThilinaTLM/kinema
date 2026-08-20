// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"

#include <QObject>

namespace kinema::controllers {
class SubtitleController;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::subtitles {

/**
 * Facade over `controllers::SubtitleController` exposing the
 * subtitle side-effects used by `PlaybackSession`, plus an
 * event-stream subscription that translates moviehash + session
 * lifecycle events into the corresponding controller calls.
 *
 * Subscribes to:
 *   - `PlaybackRequested` -> `clearMoviehash()` so a stale hash
 *     from the previous title cannot bleed into the new search.
 *   - `MoviehashComputed` -> `setMoviehash(hex)`.
 */
class SubtitleSessionService : public QObject
{
    Q_OBJECT
public:
    SubtitleSessionService(controllers::SubtitleController& inner,
        events::PlaybackEventStream* events = nullptr,
        QObject* parent = nullptr);
    ~SubtitleSessionService() override;

    void setActiveSubtitlePaths(const QStringList& paths);
    void setMoviehash(const QString& hex);
    void clearMoviehash();

    controllers::SubtitleController& inner() noexcept { return m_inner; }

private:
    void onEvent(const events::PlaybackEvent& event);

    controllers::SubtitleController& m_inner;
    events::PlaybackEventStream* m_events = nullptr;
};

} // namespace kinema::playback::subtitles

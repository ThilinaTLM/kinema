// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"
#include "playback/ports/SubtitlePort.h"

#include <QObject>

namespace kinema::controllers {
class SubtitleController;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::subtitles {

/**
 * Facade over `controllers::SubtitleController` exposing only the
 * `SubtitlePort` slice used by `PlaybackSession`, plus an
 * event-stream subscription that translates moviehash + session
 * lifecycle events into the corresponding controller calls.
 *
 * Subscribes to:
 *   - `PlaybackRequested` -> `clearMoviehash()` so a stale hash
 *     from the previous title cannot bleed into the new search.
 *   - `MoviehashComputed` -> `setMoviehash(hex)`.
 *
 * The `SubtitlePort` slice remains for callers that still need
 * direct access (e.g. the upcoming `PlaybackSession` subtitle
 * attach path).
 */
class SubtitleSessionService : public QObject,
    public ports::SubtitlePort
{
    Q_OBJECT
public:
    SubtitleSessionService(controllers::SubtitleController& inner,
        events::PlaybackEventStream* events = nullptr,
        QObject* parent = nullptr);
    ~SubtitleSessionService() override;

    void setActiveSubtitlePaths(const QStringList& paths) override;
    void setMoviehash(const QString& hex) override;
    void clearMoviehash() override;

    controllers::SubtitleController& inner() noexcept { return m_inner; }

private:
    void onEvent(const events::PlaybackEvent& event);

    controllers::SubtitleController& m_inner;
    events::PlaybackEventStream* m_events = nullptr;
};

} // namespace kinema::playback::subtitles

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"
#include "playback/ports/SubtitleSessionPort.h"

#include <QObject>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::subtitles {

/**
 * Facade over `ports::SubtitleSessionPort` exposing the
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
    SubtitleSessionService(ports::SubtitleSessionPort& inner,
                           events::PlaybackEventStream* events = nullptr,
                           QObject* parent = nullptr);
    ~SubtitleSessionService() override;

    void setActiveSubtitlePaths(const QStringList& paths);
    void setMoviehash(const QString& hex);
    void clearMoviehash();

    ports::SubtitleSessionPort& inner() noexcept { return m_inner; }

private:
    void onEvent(const events::PlaybackEvent& event);

    ports::SubtitleSessionPort& m_inner;
    events::PlaybackEventStream* m_events = nullptr;
};

} // namespace kinema::playback::subtitles

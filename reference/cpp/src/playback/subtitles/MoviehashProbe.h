// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::core {
class HttpClient;
}

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::subtitles {

/**
 * Event-driven moviehash probe.
 *
 * Subscribes to `PlayableUrlReady` and runs the OpenSubtitles
 * moviehash compute against the HTTPS URL using HEAD +
 * two Range GETs (first and last 64 KiB blocks). On success the
 * computed hex is republished as a `MoviehashComputed` event on
 * the same event stream.
 *
 * Replaces the inline `kickoffMoviehashCompute` coroutine that
 * used to live on `controllers::PlaybackController`.
 *
 * Non-HTTPS URLs (local stream gateway, custom schemes) and
 * media smaller than two block sizes are skipped silently; the
 * subtitle search degrades to filename / IMDb id matching.
 *
 * The probe is per-attempt: a new `PlayableUrlReady` for the same
 * session_id supersedes the previous probe via an internal epoch
 * counter so a late HTTP response from a stale probe is dropped.
 *
 * Lifetime: owned by `ServiceContainer`; depends only on the
 * event stream and `core::HttpClient`. Safe to construct when no
 * HTTP client is available - the probe becomes a no-op.
 */
class MoviehashProbe : public QObject
{
    Q_OBJECT
public:
    MoviehashProbe(events::PlaybackEventStream& events,
        core::HttpClient* http,
        QObject* parent = nullptr);
    ~MoviehashProbe() override;

private:
    void onEvent(const events::PlaybackEvent& event);
    void onPlayableUrl(const events::PlayableUrlReady& payload);
    QCoro::Task<void> kickoff(PlaybackSessionId sessionId,
        QUrl url, quint64 epoch);

    events::PlaybackEventStream& m_events;
    core::HttpClient* m_http = nullptr;
    quint64 m_epoch = 0;
};

} // namespace kinema::playback::subtitles

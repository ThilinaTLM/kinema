// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/mpv/MpvTrackList.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::history {
class HistoryQueryService;
}

namespace kinema::playback::ports {
class PlayerPort;
}

namespace kinema::playback::history {

/**
 * Event-driven applier of remembered audio / subtitle language
 * preferences.
 *
 * Subscribes to:
 *   - `PlaybackRequested` -> resets per-session bookkeeping,
 *     latches the active session id + context;
 *   - `TrackListChanged` -> at most once per session, looks up
 *     the stored `HistoryEntry`, compares against the currently
 *     selected tracks, and calls `PlayerPort::selectAudioTrack` /
 *     `selectSubtitleTrack` for any mismatch.
 *
 * Replaces the `onTrackListChanged` / `m_trackMemoryApplied`
 * logic that used to live on `controllers::PlaybackController`.
 *
 * Lifetime: owned by `ServiceContainer` (embedded-build only).
 * No coupling to the player adapter beyond the abstract
 * `PlayerPort`.
 */
class TrackMemoryService : public QObject
{
    Q_OBJECT
public:
    TrackMemoryService(events::PlaybackEventStream& events,
        HistoryQueryService& history,
        ports::PlayerPort* player,
        QObject* parent = nullptr);
    ~TrackMemoryService() override;

private:
    void onEvent(const events::PlaybackEvent& event);
    void onPlaybackRequested(const events::PlaybackRequested& e);
    void onTrackListChanged(const events::TrackListChanged& e);

    events::PlaybackEventStream& m_events;
    HistoryQueryService& m_history;
    ports::PlayerPort* m_player = nullptr;

    PlaybackSessionId m_sessionId;
    domain::PlaybackContext m_ctx;
    bool m_appliedThisSession = false;
};

} // namespace kinema::playback::history

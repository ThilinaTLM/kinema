// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "core/mpv/MpvChapterList.h"
#include "core/mpv/MpvTrackList.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::ports {
class PlaybackHistoryRepository;
}

namespace kinema::playback::progress {

/**
 * Event-driven projection from `PlaybackEventStream` into the
 * `PlaybackHistoryRepository`. Owns the position/finish write
 * path that used to live in the now-deleted
 * `controllers::HistoryController`.
 *
 * Subscribes to:
 *   - `PlaybackRequested`  — seeds / refreshes the history row
 *     immediately so external launches appear in Continue
 *     Watching from the moment Play is fired.
 *   - `PlayerLoaded`       — resets the persistence cursor for
 *     the freshly loaded file.
 *   - `PositionTicked`     — throttled persistence (every
 *     ~5 s, gated by `kMinProgressFraction`).
 *   - `DurationChanged`    — captures duration for completion
 *     math.
 *   - `TrackListChanged`   — captures the selected audio /
 *     subtitle language hints recorded with the row.
 *   - `ChapterListChanged` — captures chapters so end-of-file
 *     can derive a credits-start hint.
 *   - `PlaybackEnded`      — applies the WatchedPolicy via
 *     `recordSessionEnd`, including the inferred credits-start.
 *   - `PlaybackFailed`     — persists the final position
 *     without flipping `finished`.
 *
 * The projector is GUI-thread affine — `PlaybackEventStream`
 * publishes synchronously and the SQLite store runs on the same
 * thread.
 *
 */
class PlaybackProgressProjector : public QObject
{
    Q_OBJECT
public:
    PlaybackProgressProjector(ports::PlaybackHistoryRepository& repo,
        events::PlaybackEventStream& stream,
        QObject* parent = nullptr);
    ~PlaybackProgressProjector() override;

    /// Override the throttle interval for tests. Production default
    /// is 5 seconds.
    void setPersistIntervalSeconds(double s) noexcept;
    double persistIntervalSeconds() const noexcept { return m_persistIntervalSec; }

    /// Minimum fraction of duration that must elapse before any
    /// ticking persistence. Default 0.5%.
    void setMinProgressFraction(double f) noexcept;

    // Diagnostics for tests and the in-memory resume helper.
    double lastPersistedPosition() const noexcept { return m_lastPersistedPosition; }
    double lastPosition() const noexcept { return m_lastPosition; }
    double duration() const noexcept { return m_duration; }
    bool hasActiveContext() const noexcept { return m_active.has_value(); }
    const std::optional<domain::PlaybackContext>& activeContext() const noexcept
    { return m_active; }

private:
    void onEvent(const events::PlaybackEvent& event);

    void onPlaybackRequested(const events::PlaybackRequested& e);
    void onPlayerLoaded(const events::PlayerLoaded& e);
    void onPositionTicked(const events::PositionTicked& e);
    void onDurationChanged(const events::DurationChanged& e);
    void onTrackListChanged(const events::TrackListChanged& e);
    void onChapterListChanged(const events::ChapterListChanged& e);
    void onPlaybackEnded(const events::PlaybackEnded& e);
    void onPlaybackFailed(const events::PlaybackFailed& e);

    void persistActive(bool force);
    domain::HistoryEntry buildActiveEntry() const;

    ports::PlaybackHistoryRepository& m_repo;

    /// Active playback context (set on first PlaybackRequested,
    /// cleared on terminal events). `m_activeSessionId` is held
    /// separately so we can ignore stale events from a session that
    /// was superseded.
    std::optional<domain::PlaybackContext> m_active;
    PlaybackSessionId m_activeSessionId;

    double m_lastPosition = 0.0;
    double m_duration = 0.0;
    double m_lastPersistedPosition = 0.0;
    core::chapters::ChapterList m_activeChapters;
    QString m_rememberedAudioLang;
    QString m_rememberedSubtitleLang;

    double m_persistIntervalSec = 5.0;
    double m_minProgressFraction = 0.005;
};

} // namespace kinema::playback::progress

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/events/PlaybackEvent.h"

#include <QObject>
#include <QString>

#include <optional>

namespace kinema::playback::events {
class PlaybackEventStream;
}

namespace kinema::playback::ports {
class SessionFileCatalog;
}

namespace kinema::playback::session {
class PlaybackSessionManager;
}

namespace kinema::playback::series {

/**
 * Event-driven season-pack navigation service.
 *
 * Subscribes to `PlaybackEventStream`:
 *
 *   - `PlaybackRequested` resets state for the new attempt and
 *     caches the base context.
 *   - `PlayerLoaded` triggers the file-catalog lookup, computes
 *     adjacency via
 *     `playback::policy::MediaFileSelectionPolicy::adjacentEpisodeFiles`
 *     and surfaces `navigationChanged` /
 *     `packAdjacencyResolved` / `currentStreamSizeResolved`.
 *   - `PlaybackEnded(NaturalEof)` with a `next` adjacency target
 *     dispatches a fresh play via `PlaybackSessionManager::play`.
 *     With no `next`, emits `windowCloseRequested`.
 *   - `PlaybackEnded(UserStop)` or `PlaybackFailed` clears state
 *     without auto-next.
 *
 * Backend-agnostic: any `SessionFileCatalog` (torrent / debrid)
 * works identically. Replaces
 * `controllers::SeriesPlaybackSessionController`.
 */
class SeriesSessionService : public QObject
{
    Q_OBJECT
public:
    SeriesSessionService(events::PlaybackEventStream& events,
        ports::SessionFileCatalog& catalog,
        session::PlaybackSessionManager& sessions,
        QObject* parent = nullptr);
    ~SeriesSessionService() override;

    bool navigationVisible() const noexcept { return m_navigationVisible; }
    bool canGoPrevious() const noexcept
    {
        return m_navigationVisible && m_previous.has_value();
    }
    bool canGoNext() const noexcept
    {
        return m_navigationVisible && m_next.has_value();
    }

public Q_SLOTS:
    void playPreviousEpisode();
    void playNextEpisode();

Q_SIGNALS:
    /// Adjacency state changed (visibility / prev / next changed).
    void navigationChanged();

    /// Fires at most once per `(infoHash, season, episode)` per
    /// playback session, once the catalog has produced an answer
    /// for a multi-file ("pack") torrent. Wired by ShellViewModel
    /// to a `passiveMessage` so the picker's "Season pack" badge
    /// gets paired with a one-shot status confirmation.
    void packAdjacencyResolved(bool nextAvailable,
        int nextSeason, int nextEpisode);

    /// Fires once per `(infoHash, fileIndex)` per playback session
    /// when the catalog surfaces a definitive size for the current
    /// file. Picked up by the detail view-models to patch the
    /// visible picker row whose original parse left the size cell
    /// blank.
    void currentStreamSizeResolved(const QString& infoHash,
        int fileIndex, qint64 sizeBytes);

    /// Forwarded so the shell can close the player window when an
    /// EOF lands on a non-pack stream (no auto-next available).
    void windowCloseRequested();

private:
    struct EpisodeTarget {
        domain::PlaybackKey key;
        int fileIndex = -1;
        QString fileNameHint;
        qint64 sizeBytes = 0;
    };

    void onEvent(const events::PlaybackEvent& e);
    void onPlaybackRequested(const events::PlaybackRequested& e);
    void onPlayerLoaded(const events::PlayerLoaded& e);
    void onPlaybackEnded(const events::PlaybackEnded& e);
    void onPlaybackFailed(const events::PlaybackFailed& e);

    /// Recompute adjacency from the catalog. Idempotent within a
    /// playback session; de-dup keys prevent re-spamming the toast
    /// signals on resume / seek / repeated PlayerLoaded events.
    void resolveAdjacencyFromCatalog();

    void clearState();
    void setState(std::optional<EpisodeTarget> previous,
        std::optional<EpisodeTarget> next);
    void dispatch(const EpisodeTarget& target);

    static QString episodeCode(int season, int episode);
    static QString displayTitle(const domain::PlaybackContext& base,
        int season, int episode);

    events::PlaybackEventStream& m_events;
    ports::SessionFileCatalog& m_catalog;
    session::PlaybackSessionManager& m_sessions;

    /// Identity of the currently tracked attempt. Set by
    /// `PlaybackRequested`; used to ignore stale events from
    /// superseded sessions.
    PlaybackSessionId m_currentSessionId;
    domain::PlaybackContext m_baseContext;

    std::optional<EpisodeTarget> m_previous;
    std::optional<EpisodeTarget> m_next;
    bool m_navigationVisible = false;

    /// De-dup key for `packAdjacencyResolved`, set to
    /// `"<infoHash>:S<n>E<m>"` once the signal has fired for the
    /// current episode.
    QString m_lastAdjacencyKey;
    /// De-dup key for `currentStreamSizeResolved`, formatted
    /// `"<infoHash>:<fileIndex>:<size>"`.
    QString m_lastSizeHydrationKey;
};

} // namespace kinema::playback::series

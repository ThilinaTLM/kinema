// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#ifdef KINEMA_HAVE_LIBMPV

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "torrent/MediaFileSelector.h"

#include <QObject>

#include <optional>

namespace kinema::controllers {
class PlaybackController;
}

namespace kinema::playback::ports {
class SessionFileCatalog;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::controllers {

class SeriesPlaybackSessionController : public QObject
{
    Q_OBJECT
public:
    SeriesPlaybackSessionController(PlaybackController& playback,
        torrent::TorrentStreamingService& torrentStreaming,
        services::StreamActions& actions,
        playback::ports::SessionFileCatalog* sessionFiles,
        QObject* parent = nullptr);

    bool navigationVisible() const noexcept;
    bool canGoPrevious() const noexcept;
    bool canGoNext() const noexcept;

public Q_SLOTS:
    void refreshFromPlayback(bool active);
    void onPlayerEndOfFile(const QString& reason,
        const domain::PlaybackContext& ctx);
    void onPlayerUserClosed(const domain::PlaybackContext& ctx);
    void playPreviousEpisode();
    void playNextEpisode();

Q_SIGNALS:
    void navigationChanged();
    void windowCloseRequested();

    /// Fires at most once per `(infoHash, season, episode)` per
    /// playback session, after `refreshFromPlayback()` has resolved
    /// the adjacency for a multi-file ("pack") torrent. Wired by
    /// `ShellViewModel` to a `passiveMessage` so the picker's
    /// "Season pack" badge gets paired with a one-shot status
    /// confirmation ("Auto-play queued for S01E03" vs "Next episode
    /// is not in this pack"). Not emitted for single-file torrents
    /// where the question doesn't apply.
    void packAdjacencyResolved(bool nextAvailable,
        int nextSeason, int nextEpisode);

    /// Fires once per `(infoHash, fileIndex)` per playback session
    /// when the resolver / engine surfaces a definitive size for the
    /// currently playing file. Picked up by the detail view-models
    /// to patch the visible picker row whose original parse left the
    /// size cell blank (Torrentio packs that omit the \xf0\x9f\x92\xbe
    /// emoji token). No-op when the row has scrolled out of the
    /// model.
    void currentStreamSizeResolved(const QString& infoHash,
        int fileIndex, qint64 sizeBytes);

private:
    struct EpisodeTarget {
        domain::PlaybackKey key;
        int fileIndex = -1;
        QString fileNameHint;
        qint64 sizeBytes = 0;
    };

    void clearState();
    void setState(const domain::PlaybackContext& ctx,
        std::optional<EpisodeTarget> previous,
        std::optional<EpisodeTarget> next);
    void playTarget(const EpisodeTarget& target);
    static QString episodeCode(int season, int episode);
    static QString displayTitle(const domain::PlaybackContext& base,
        int season, int episode);

    PlaybackController& m_playback;
    torrent::TorrentStreamingService& m_torrentStreaming;
    services::StreamActions& m_actions;
    /// Optional in unit-test setups; production wiring always
    /// provides one. When present, it is consulted first for the
    /// magnet's file list so HTTP-backed debrid sessions resolve
    /// adjacency just like libtorrent ones.
    playback::ports::SessionFileCatalog* m_sessionFiles = nullptr;

    domain::PlaybackContext m_baseContext;
    std::optional<EpisodeTarget> m_previous;
    std::optional<EpisodeTarget> m_next;
    bool m_navigationVisible = false;
    bool m_userClosed = false;
    /// De-dup key for `packAdjacencyResolved`. Set to
    /// `"<infoHash>:S<n>E<m>"` once the signal has fired for the
    /// current episode so player-side refreshes (resume / seek /
    /// metadata re-resolve) don't re-spam the status bar.
    QString m_lastAdjacencyKey;
    /// De-dup key for `currentStreamSizeResolved`, formatted
    /// `"<infoHash>:<fileIndex>:<size>"`. Prevents the signal from
    /// firing for every refresh of the same active file.
    QString m_lastSizeHydrationKey;
};

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV

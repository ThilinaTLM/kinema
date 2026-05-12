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

    domain::PlaybackContext m_baseContext;
    std::optional<EpisodeTarget> m_previous;
    std::optional<EpisodeTarget> m_next;
    bool m_navigationVisible = false;
    bool m_userClosed = false;
};

} // namespace kinema::controllers

#endif // KINEMA_HAVE_LIBMPV

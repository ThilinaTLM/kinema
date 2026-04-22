// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Discover.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::ui {
class SeriesDetailPane;
}

namespace kinema::controllers {

class NavigationController;

/**
 * Drives the series-detail surface: Cinemeta series meta + Torrentio
 * streams for a selected episode. Owns two epochs \u2014 one for the
 * series-meta fetch and one for the per-episode stream fetch \u2014 so
 * rapid navigation / episode-switching drops stale responses.
 *
 * Also listens to NavigationController::currentChanged so the pane's
 * internal sub-page (episodes \u2194 streams) is kept in sync when Back
 * walks the user out of SeriesStreams.
 */
class SeriesDetailController : public QObject
{
    Q_OBJECT
public:
    SeriesDetailController(
        api::CinemetaClient* cinemeta,
        api::TorrentioClient* torrentio,
        api::TmdbClient* tmdb,
        ui::SeriesDetailPane* pane,
        NavigationController* nav,
        const config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);

    QString currentImdbId() const noexcept { return m_currentImdbId; }
    const std::optional<api::Episode>& currentEpisode() const noexcept
    {
        return m_currentEpisode;
    }

public Q_SLOTS:
    /// Entry from the search/results grid.
    void openFromSummary(const api::MetaSummary& summary);

    /// Entry from a Discover card (resolves imdb via TMDB then hands
    /// off to openFromSummary).
    void openFromDiscover(const api::DiscoverItem& item);

    /// Entry from a Continue-Watching card. Opens the series pane
    /// and, once episodes load, auto-selects the stored season +
    /// episode so the user lands on the streams sub-page.
    void openFromHistory(const api::HistoryEntry& entry);

    /// User picked an episode inside the pane.
    void selectEpisode(const api::Episode& ep);

    /// Re-run the currently-visible episode's stream fetch, used by
    /// the Torrentio-options-changed debounce.
    void refetchCurrentEpisode();

    /// User backed out of the streams sub-page. Drops the in-flight
    /// episode task and clears the cached episode pointer.
    void onBackToEpisodes();

    /// Clear all cached state (series + episode).
    void clear();

Q_SIGNALS:
    /// Status-bar message; MainWindow forwards to statusBar().
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    QCoro::Task<void> loadSeriesTask(api::MetaSummary summary);
    QCoro::Task<void> loadEpisodeTask(api::Episode ep, QString imdbId);
    QCoro::Task<void> openFromDiscoverTask(api::DiscoverItem item);

    api::CinemetaClient* m_cinemeta;
    api::TorrentioClient* m_torrentio;
    api::TmdbClient* m_tmdb;
    ui::SeriesDetailPane* m_pane;
    NavigationController* m_nav;
    const config::AppSettings& m_settings;
    const QString& m_rdToken;

    quint64 m_seriesEpoch = 0;
    quint64 m_episodeEpoch = 0;

    QString m_currentImdbId;
    std::optional<api::Episode> m_currentEpisode;

    /// Latest series-level display metadata used to build per-episode
    /// PlaybackContext entries. Sourced from the opening summary and
    /// refreshed once Cinemeta returns.
    QString m_currentSeriesTitle;
    QUrl m_currentSeriesPoster;

    /// Pending (season, episode) to auto-select once Cinemeta returns
    /// the series episode list. Set by openFromHistory(); cleared
    /// after successful selection.
    std::optional<int> m_pendingSeason;
    std::optional<int> m_pendingEpisode;
};

} // namespace kinema::controllers

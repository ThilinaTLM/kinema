// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "ui/qml-bridge/DetailViewModelBase.h"
#include "ui/qml-bridge/EpisodesListModel.h"

#include <QList>
#include <QObject>
#include <QString>
#include <QStringList>
#include <QVariantList>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class CinemetaClient;
class IndexerSelector;
class TmdbClient;
}

namespace kinema::config {
class AppSettings;
}

namespace kinema::controllers {
class DownloadController;
class StreamUtilityController;
class LibraryController;
class TokenController;
class WatchedController;
}

namespace kinema::playback::session {
class PlaybackSessionManager;
}

namespace kinema::ui::qml {

/**
 * View-model behind `SeriesDetailPage.qml`. The meta block, stream
 * UI-filters, sort config, debrid chip, "More like this" rail, library
 * state, and every per-row stream action live in `DetailViewModelBase`.
 *
 * This subclass owns the series-shaped concerns: seasons / episodes,
 * the per-episode Torrentio fetch, and series/episode watched-state.
 *
 * Two independent epoch counters guard the coroutines this VM kicks
 * off (the third, `m_similarEpoch`, lives in the base):
 *
 *   * `m_metaEpoch`     \u2014 series meta + episode list
 *   * `m_episodeEpoch`  \u2014 per-episode Torrentio fetch
 *
 * Season picker semantics: seasons are surfaced as a flat
 * `seasonLabels` `QStringList` indexed by `currentSeason`. Specials
 * (season 0) are excluded from the picker but kept in the parsed
 * `SeriesDetail` so a Continue-Watching entry pointing at a special
 * still resolves an episode row.
 *
 * Episode picker semantics: `selectedEpisodeRow` is the row inside the
 * current season's episode list (\u2212 1 = collapsed). Selecting a row
 * builds a `domain::PlaybackContext` and kicks the per-episode stream
 * fetch; clearing collapses the streams region.
 */
class SeriesDetailViewModel : public DetailViewModelBase
{
    Q_OBJECT

    /// `MetaState` enum mirrored as int for cheap QML comparisons.
    /// Declared here (not on the base) so QML's
    /// `SeriesDetailViewModel.Ready` attached-enum lookup resolves on
    /// the registered type.
    Q_PROPERTY(MetaState metaState READ metaState NOTIFY metaStateChanged)
    Q_PROPERTY(QString metaError READ metaError NOTIFY metaStateChanged)

    // ---- season + episode picker ----------------------------------
    Q_PROPERTY(QStringList seasonLabels READ seasonLabels NOTIFY seasonsChanged)
    Q_PROPERTY(QVariantList seasonNumbers READ seasonNumbers NOTIFY seasonsChanged)
    Q_PROPERTY(QVariantList seasonWatchedList READ seasonWatchedList NOTIFY seasonsChanged)
    Q_PROPERTY(int currentSeason READ currentSeason WRITE setCurrentSeason NOTIFY currentSeasonChanged)
    Q_PROPERTY(EpisodesListModel* episodes READ episodes CONSTANT)
    Q_PROPERTY(int selectedEpisodeRow READ selectedEpisodeRow NOTIFY selectedEpisodeChanged)
    Q_PROPERTY(QString selectedEpisodeLabel READ selectedEpisodeLabel NOTIFY selectedEpisodeChanged)

    Q_PROPERTY(bool seriesWatched READ seriesWatched NOTIFY watchedStateChanged)

public:
    enum class MetaState {
        Idle = 0,
        Loading,
        Ready,
        Error,
    };
    Q_ENUM(MetaState)

    SeriesDetailViewModel(api::CinemetaClient* cinemeta,
        api::IndexerSelector* indexers,
        api::TmdbClient* tmdb,
        playback::session::PlaybackSessionManager* playback,
        controllers::StreamUtilityController* streamUtility,
        controllers::LibraryController* library,
        controllers::WatchedController* watched,
        controllers::TokenController* tokens,
        config::AppSettings& settings,
        const QString& rdTokenRef,
        const QString& adApiKeyRef,
        QObject* parent = nullptr);
    /// Slim constructor for tests; equivalent to passing
    /// `library = nullptr, watched = nullptr`.
    SeriesDetailViewModel(api::CinemetaClient* cinemeta,
        api::IndexerSelector* indexers,
        api::TmdbClient* tmdb,
        playback::session::PlaybackSessionManager* playback,
        controllers::StreamUtilityController* streamUtility,
        controllers::TokenController* tokens,
        config::AppSettings& settings,
        const QString& rdTokenRef,
        const QString& adApiKeyRef,
        QObject* parent = nullptr);
    ~SeriesDetailViewModel() override;

    MetaState metaState() const noexcept { return m_metaState; }
    QString metaError() const { return m_metaError; }

    // ---- season + episode accessors -------------------------------
    QStringList seasonLabels() const { return m_seasonLabels; }
    QVariantList seasonNumbers() const;
    QVariantList seasonWatchedList() const { return m_seasonWatchedList; }
    int currentSeason() const noexcept { return m_currentSeasonIdx; }
    void setCurrentSeason(int idx);
    EpisodesListModel* episodes() const noexcept { return m_episodes; }
    int selectedEpisodeRow() const noexcept { return m_selectedEpisodeRow; }
    QString selectedEpisodeLabel() const { return m_selectedEpisodeLabel; }

    bool seriesWatched() const noexcept { return m_seriesWatched; }

public Q_SLOTS:
    /// Open the series detail page for `imdbId`. Optional `season` /
    /// `episode` seed (used by Continue Watching) auto-selects the
    /// matching episode once the meta resolves.
    void load(const QString& imdbId);
    void loadAt(const QString& imdbId, int season, int episode);

    /// Resolve a TMDB id to its IMDB id and load.
    void loadByTmdbId(int tmdbId, const QString& title);

    /// Re-run meta + (optionally) the selected episode's streams.
    void retry();

    /// Drop the loaded title and reset every model.
    void clear();

    /// Picker hooks.
    void selectEpisode(int row);
    /// Collapse the streams region by clearing the selected episode.
    void clearEpisode();
    /// Select the row (kicking off the streams fetch) and immediately
    /// ask the host to push the Streams page.
    void selectEpisodeAndOpenStreams(int row);

    /// Header action: ask the host to push the Streams page for the
    /// currently-selected episode. No-op when no episode is selected.
    void requestStreams();

    /// Re-run only the streams fetch for the currently-selected
    /// episode. No-op when no episode is selected.
    void refreshStreams();
    void addToLibrary();
    void toggleEpisodeWatched(int row);
    void toggleSeriesWatched();
    void markSeasonWatched(int season, bool watched);

Q_SIGNALS:
    void metaStateChanged();
    void seasonsChanged();
    void currentSeasonChanged();
    void selectedEpisodeChanged();

private:
    domain::MediaKind mediaKind() const override
    {
        return domain::MediaKind::Series;
    }
    domain::PlaybackContext currentContext() const override;

    QCoro::Task<void> loadSeriesMetaTask(QString imdbId,
        std::optional<int> pendingSeason,
        std::optional<int> pendingEpisode);
    QCoro::Task<void> loadEpisodeStreamsTask(domain::Episode ep);
    QCoro::Task<void> resolveByTmdbAndLoad(int tmdbId, QString title);

    void applyMeta(const domain::SeriesDetail& sd);
    void resetMeta();
    void refreshEpisodeWatchedState();
    void setMetaState(MetaState s, const QString& error = {});

    /// Re-derive `m_seasonLabels` + per-season episode lists from
    /// `m_allEpisodes`. Specials (season 0) are filtered out of the
    /// picker but kept in the lookup for Continue-Watching seeds.
    void rebuildSeasons();
    /// Push the current season's episode list into `m_episodes`.
    void publishCurrentSeasonEpisodes();

    EpisodesListModel* m_episodes;

    quint64 m_metaEpoch = 0;
    quint64 m_episodeEpoch = 0;

    domain::SeriesDetail m_currentSeries;
    MetaState m_metaState = MetaState::Idle;
    QString m_metaError;

    // All episodes (across all seasons, sorted by (season, number) by
    // the parser). Specials (season 0) included; the picker filters.
    QList<domain::Episode> m_allEpisodes;
    /// Season numbers excluding specials, in ascending order. The
    /// `currentSeason` Q_PROPERTY indexes into this list.
    QList<int> m_seasonNumbers;
    QStringList m_seasonLabels;
    QVariantList m_seasonWatchedList;
    int m_currentSeasonIdx = -1;

    // Selected episode (within the current season).
    int m_selectedEpisodeRow = -1;
    domain::Episode m_selectedEpisode;
    QString m_selectedEpisodeLabel;

    bool m_seriesWatched = false;
};

} // namespace kinema::ui::qml

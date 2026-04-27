// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "ui/qml-bridge/EpisodesListModel.h"
#include "ui/qml-bridge/StreamsListModel.h"

#include <QHash>
#include <QList>
#include <QObject>
#include <QString>
#include <QStringList>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class TorrentioClient;
}

namespace kinema::config {
class AppSettings;
class FilterSettings;
class TorrentioSettings;
}

namespace kinema::controllers {
class TokenController;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui::qml {

class DiscoverSectionModel;

/**
 * View-model behind `SeriesDetailPage.qml`. Replaces the
 * widget-coupled `controllers::SeriesDetailController` +
 * `SeriesDetailPane` glue.
 *
 * Three independent epoch counters guard the three coroutines this
 * VM kicks off so a slow response from one can't clobber a fresher
 * one:
 *
 *   * `m_metaEpoch`     \u2014 series meta + episode list
 *   * `m_episodeEpoch`  \u2014 per-episode Torrentio fetch
 *   * `m_similarEpoch`  \u2014 TMDB recommendations carousel
 *
 * Season picker semantics: seasons are surfaced as a flat
 * `seasonLabels` `QStringList` (e.g. `["Season 1", "Season 2"]`)
 * indexed by `currentSeason`. Specials (season 0) are excluded from
 * the picker but kept in the parsed `SeriesDetail` so a Continue-
 * Watching entry pointing at a special still resolves an episode
 * row.
 *
 * Episode picker semantics: `selectedEpisodeRow` is the row inside
 * the current season's episode list (\u2212 1 = collapsed). Selecting
 * a row builds an `api::PlaybackContext` and kicks the per-episode
 * stream fetch; clearing collapses the streams region.
 *
 * Streams config (sort + cached-only) and per-row action delegation
 * mirror `MovieDetailViewModel` exactly. The streams list itself is
 * a fresh `StreamsListModel` instance; it is NOT shared with the
 * movie VM.
 */
class SeriesDetailViewModel : public QObject
{
    Q_OBJECT

    // ---- meta ------------------------------------------------------
    Q_PROPERTY(QString imdbId READ imdbId NOTIFY metaChanged)
    Q_PROPERTY(QString title READ title NOTIFY metaChanged)
    Q_PROPERTY(int year READ year NOTIFY metaChanged)
    Q_PROPERTY(QString posterUrl READ posterUrl NOTIFY metaChanged)
    Q_PROPERTY(QString backdropUrl READ backdropUrl NOTIFY metaChanged)
    Q_PROPERTY(QString description READ description NOTIFY metaChanged)
    Q_PROPERTY(QStringList genres READ genres NOTIFY metaChanged)
    Q_PROPERTY(QStringList cast READ cast NOTIFY metaChanged)
    Q_PROPERTY(double rating READ rating NOTIFY metaChanged)
    Q_PROPERTY(QString releaseDateText READ releaseDateText NOTIFY metaChanged)
    Q_PROPERTY(MetaState metaState READ metaState NOTIFY metaStateChanged)
    Q_PROPERTY(QString metaError READ metaError NOTIFY metaStateChanged)

    // ---- season + episode picker ----------------------------------
    Q_PROPERTY(QStringList seasonLabels READ seasonLabels NOTIFY seasonsChanged)
    Q_PROPERTY(int currentSeason READ currentSeason WRITE setCurrentSeason NOTIFY currentSeasonChanged)
    Q_PROPERTY(EpisodesListModel* episodes READ episodes CONSTANT)
    Q_PROPERTY(int selectedEpisodeRow READ selectedEpisodeRow NOTIFY selectedEpisodeChanged)
    Q_PROPERTY(QString selectedEpisodeLabel READ selectedEpisodeLabel NOTIFY selectedEpisodeChanged)

    // ---- streams + similar ----------------------------------------
    Q_PROPERTY(StreamsListModel* streams READ streams CONSTANT)
    Q_PROPERTY(DiscoverSectionModel* similar READ similar CONSTANT)
    Q_PROPERTY(bool similarVisible READ similarVisible NOTIFY similarChanged)

    // ---- streams configuration ------------------------------------
    Q_PROPERTY(int sortMode READ sortMode WRITE setSortMode NOTIFY sortChanged)
    Q_PROPERTY(bool sortDescending READ sortDescending WRITE setSortDescending NOTIFY sortChanged)
    Q_PROPERTY(bool cachedOnly READ cachedOnly WRITE setCachedOnly NOTIFY cachedOnlyChanged)
    Q_PROPERTY(bool realDebridConfigured READ realDebridConfigured NOTIFY realDebridConfiguredChanged)
    Q_PROPERTY(int rawStreamsCount READ rawStreamsCount NOTIFY rawStreamsCountChanged)

public:
    enum class MetaState {
        Idle = 0,
        Loading,
        Ready,
        Error,
    };
    Q_ENUM(MetaState)

    SeriesDetailViewModel(api::CinemetaClient* cinemeta,
        api::TorrentioClient* torrentio,
        api::TmdbClient* tmdb,
        services::StreamActions* actions,
        controllers::TokenController* tokens,
        config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);
    ~SeriesDetailViewModel() override;

    // ---- meta accessors -------------------------------------------
    QString imdbId() const { return m_imdbId; }
    QString title() const { return m_title; }
    int year() const noexcept { return m_year; }
    QString posterUrl() const { return m_posterUrl; }
    QString backdropUrl() const { return m_backdropUrl; }
    QString description() const { return m_description; }
    QStringList genres() const { return m_genres; }
    QStringList cast() const { return m_cast; }
    double rating() const noexcept { return m_rating; }
    QString releaseDateText() const { return m_releaseDateText; }
    MetaState metaState() const noexcept { return m_metaState; }
    QString metaError() const { return m_metaError; }

    // ---- season + episode accessors -------------------------------
    QStringList seasonLabels() const { return m_seasonLabels; }
    int currentSeason() const noexcept { return m_currentSeasonIdx; }
    void setCurrentSeason(int idx);
    EpisodesListModel* episodes() const noexcept { return m_episodes; }
    int selectedEpisodeRow() const noexcept { return m_selectedEpisodeRow; }
    QString selectedEpisodeLabel() const { return m_selectedEpisodeLabel; }

    // ---- streams + similar accessors ------------------------------
    StreamsListModel* streams() const noexcept { return m_streams; }
    DiscoverSectionModel* similar() const noexcept { return m_similar; }
    bool similarVisible() const noexcept { return m_similarVisible; }

    int sortMode() const noexcept { return static_cast<int>(m_sortMode); }
    void setSortMode(int mode);
    bool sortDescending() const noexcept { return m_sortDescending; }
    void setSortDescending(bool desc);
    bool cachedOnly() const;
    void setCachedOnly(bool on);
    bool realDebridConfigured() const noexcept { return !m_rdToken.isEmpty(); }
    int rawStreamsCount() const noexcept
    {
        return static_cast<int>(m_rawStreams.size());
    }

public Q_SLOTS:
    /// Open the series detail page for `imdbId`. Optional `season` /
    /// `episode` seed (used by Continue Watching) auto-selects the
    /// matching episode once the meta resolves.
    void load(const QString& imdbId);
    void loadAt(const QString& imdbId, int season, int episode);

    /// Resolve a TMDB id to its IMDB id and load. Used by Browse /
    /// Discover hand-off (which carry TMDB ids only).
    void loadByTmdbId(int tmdbId, const QString& title);

    /// Re-run meta + (optionally) the selected episode's streams.
    void retry();

    /// Drop the loaded title and reset every model. Called when
    /// the page is popped off the stack.
    void clear();

    /// Picker hooks.
    void selectEpisode(int row);
    /// Collapse the streams region by clearing the selected episode.
    void clearEpisode();
    /// Convenience for the QML episode tap handler: select the row
    /// (kicking off the streams fetch as `selectEpisode` does) and
    /// immediately ask the host to push the Streams page.
    void selectEpisodeAndOpenStreams(int row);

    /// Header action: ask the host to push the Streams page for
    /// the currently-selected episode. No-op when no episode is
    /// selected.
    void requestStreams();

    /// Per-row action handlers driven by `StreamCard.qml`'s ⋮ menu.
    void play(int row);
    void copyMagnet(int row);
    void openMagnet(int row);
    void copyDirectUrl(int row);
    void openDirectUrl(int row);

    void requestSubtitles();
    void requestSubtitlesFor(int row);

    void activateSimilar(int row);

Q_SIGNALS:
    void metaChanged();
    void metaStateChanged();
    void seasonsChanged();
    void currentSeasonChanged();
    void selectedEpisodeChanged();
    void similarChanged();
    void sortChanged();
    void cachedOnlyChanged();
    void realDebridConfiguredChanged();
    void rawStreamsCountChanged();

    void statusMessage(const QString& text, int durationMs);

    void openMovieByTmdbRequested(int tmdbId, const QString& title);
    void openSeriesByTmdbRequested(int tmdbId, const QString& title);

    void subtitlesRequested(const api::PlaybackContext& ctx);

    /// Emitted from `requestStreams()` /
    /// `selectEpisodeAndOpenStreams()`. `MainController` connects
    /// to a lambda that forwards as
    /// `showStreamsRequested(QObject* detailVm)`, identifying
    /// `this` so the QML shell can bind the pushed `StreamsPage`
    /// to the right view-model.
    void streamsRequested();

private:
    QCoro::Task<void> loadSeriesMetaTask(QString imdbId,
        std::optional<int> pendingSeason,
        std::optional<int> pendingEpisode);
    QCoro::Task<void> loadEpisodeStreamsTask(api::Episode ep);
    QCoro::Task<void> resolveByTmdbAndLoad(int tmdbId, QString title);
    QCoro::Task<void> loadSimilarFor(QString imdbId);

    void applyMeta(const api::SeriesDetail& sd);
    void resetMeta();
    void setMetaState(MetaState s, const QString& error = {});
    void setSimilarVisible(bool on);

    /// Re-derive `m_seasonLabels` + per-season episode lists from
    /// `m_allEpisodes`. Specials (season 0) are filtered out of the
    /// picker but kept in the lookup for Continue-Watching seeds.
    void rebuildSeasons();
    /// Push the current season's episode list into `m_episodes`.
    void publishCurrentSeasonEpisodes();

    void rebuildVisibleStreams();
    QList<api::Stream> applyFilters() const;
    void sortInPlace(QList<api::Stream>& rows) const;
    api::PlaybackContext currentContext() const;

    api::CinemetaClient* m_cinemeta;
    api::TorrentioClient* m_torrentio;
    api::TmdbClient* m_tmdb;
    services::StreamActions* m_actions;
    controllers::TokenController* m_tokens;
    config::AppSettings& m_settings;
    const QString& m_rdToken;

    StreamsListModel* m_streams;
    EpisodesListModel* m_episodes;
    DiscoverSectionModel* m_similar;
    bool m_similarVisible = false;

    quint64 m_metaEpoch = 0;
    quint64 m_episodeEpoch = 0;
    quint64 m_similarEpoch = 0;

    // Series meta.
    QString m_imdbId;
    QString m_title;
    int m_year = 0;
    QString m_posterUrl;
    QString m_backdropUrl;
    QString m_description;
    QStringList m_genres;
    QStringList m_cast;
    double m_rating = -1.0;
    QString m_releaseDateText;
    MetaState m_metaState = MetaState::Idle;
    QString m_metaError;

    // All episodes (across all seasons, sorted by (season, number) by
    // the parser). Specials (season 0) included; the picker filters.
    QList<api::Episode> m_allEpisodes;
    /// Season numbers excluding specials, in ascending order. The
    /// `currentSeason` Q_PROPERTY indexes into this list.
    QList<int> m_seasonNumbers;
    QStringList m_seasonLabels;
    int m_currentSeasonIdx = -1;

    // Selected episode (within the current season).
    int m_selectedEpisodeRow = -1;
    api::Episode m_selectedEpisode;
    QString m_selectedEpisodeLabel;

    // Streams config.
    QList<api::Stream> m_rawStreams;
    StreamsListModel::SortMode m_sortMode
        = StreamsListModel::SortMode::Seeders;
    bool m_sortDescending = true;
};

} // namespace kinema::ui::qml

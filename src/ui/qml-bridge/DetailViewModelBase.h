// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "ui/qml-bridge/StreamsListModel.h"

#include <QList>
#include <QObject>
#include <QString>
#include <QStringList>

#include <QCoro/QCoroTask>

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

class DiscoverSectionModel;

/**
 * Shared base for `MovieDetailViewModel` and `SeriesDetailViewModel`.
 *
 * The two detail pages were ~95 % identical: the meta block, the
 * stream UI-filter axes, sort config, the debrid-configured chip, the
 * "More like this" rail, library state, and every per-row stream
 * action were copy-pasted between two ~1000-line classes. That logic
 * now lives here once.
 *
 * Subclasses keep only what genuinely differs:
 *   * `MovieDetailViewModel` \u2014 runtime / upcoming flags, movie
 *     watched-state, the movie-shaped meta + stream fetch.
 *   * `SeriesDetailViewModel` \u2014 seasons / episodes, per-episode
 *     stream fetch, series/episode watched-state.
 *
 * The `MetaState` enum + `metaState` / `metaError` properties stay in
 * the subclasses on purpose: QML references them as attached enums
 * (`MovieDetailViewModel.Ready`), and keeping the declaration on the
 * registered type avoids any inherited-enum resolution surprise.
 *
 * This class is abstract (`mediaKind()` + `currentContext()` are pure
 * virtual) and is not registered with QML directly; the concrete
 * subclasses are.
 */
class DetailViewModelBase : public QObject
{
    Q_OBJECT

    // ---- meta (shared fields) --------------------------------------
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

    // ---- streams + similar -----------------------------------------
    Q_PROPERTY(StreamsListModel* streams READ streams CONSTANT)
    Q_PROPERTY(DiscoverSectionModel* similar READ similar CONSTANT)
    Q_PROPERTY(bool similarVisible READ similarVisible NOTIFY similarChanged)

    // ---- streams configuration -------------------------------------
    Q_PROPERTY(int sortMode READ sortMode WRITE setSortMode NOTIFY sortChanged)
    Q_PROPERTY(bool sortDescending READ sortDescending WRITE setSortDescending NOTIFY sortChanged)
    Q_PROPERTY(bool debridConfigured READ debridConfigured NOTIFY debridConfiguredChanged)
    Q_PROPERTY(int rawStreamsCount READ rawStreamsCount NOTIFY rawStreamsCountChanged)
    // Transient UI-only filter axes consumed by the `StreamsPage`
    // header. Reset every time `clear()` runs; never persisted.
    Q_PROPERTY(QString uiResolutionFilter READ uiResolutionFilter
        WRITE setUiResolutionFilter NOTIFY uiFiltersChanged)
    Q_PROPERTY(bool uiHdrOnly READ uiHdrOnly WRITE setUiHdrOnly NOTIFY uiFiltersChanged)
    Q_PROPERTY(bool uiDolbyVisionOnly READ uiDolbyVisionOnly
        WRITE setUiDolbyVisionOnly NOTIFY uiFiltersChanged)
    Q_PROPERTY(bool uiMultiAudioOnly READ uiMultiAudioOnly
        WRITE setUiMultiAudioOnly NOTIFY uiFiltersChanged)
    Q_PROPERTY(bool uiCachedOnly READ uiCachedOnly
        WRITE setUiCachedOnly NOTIFY uiFiltersChanged)
    Q_PROPERTY(bool uiAnyFilterActive READ uiAnyFilterActive NOTIFY uiFiltersChanged)

    Q_PROPERTY(bool inLibrary READ inLibrary NOTIFY libraryStateChanged)
    Q_PROPERTY(QString libraryActionText READ libraryActionText NOTIFY libraryStateChanged)

public:
    ~DetailViewModelBase() override;

    // ---- meta accessors --------------------------------------------
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

    StreamsListModel* streams() const noexcept { return m_streams; }
    DiscoverSectionModel* similar() const noexcept { return m_similar; }
    bool similarVisible() const noexcept { return m_similarVisible; }

    int sortMode() const noexcept { return static_cast<int>(m_sortMode); }
    void setSortMode(int mode);
    bool sortDescending() const noexcept { return m_sortDescending; }
    void setSortDescending(bool desc);
    bool debridConfigured() const noexcept
    {
        return !m_rdToken.isEmpty() || !m_adApiKey.isEmpty();
    }
    int rawStreamsCount() const noexcept
    {
        return static_cast<int>(m_rawStreams.size());
    }

    QString uiResolutionFilter() const { return m_uiResolutionFilter; }
    void setUiResolutionFilter(const QString& res);
    bool uiHdrOnly() const noexcept { return m_uiHdrOnly; }
    void setUiHdrOnly(bool on);
    bool uiDolbyVisionOnly() const noexcept { return m_uiDolbyVisionOnly; }
    void setUiDolbyVisionOnly(bool on);
    bool uiMultiAudioOnly() const noexcept { return m_uiMultiAudioOnly; }
    void setUiMultiAudioOnly(bool on);
    bool uiCachedOnly() const noexcept { return m_uiCachedOnly; }
    void setUiCachedOnly(bool on);
    bool uiAnyFilterActive() const noexcept;
    Q_INVOKABLE void clearUiFilters();

    bool inLibrary() const noexcept { return m_inLibrary; }
    QString libraryActionText() const;

public Q_SLOTS:
    void removeFromLibrary();

    /// Wire the download controller. Same two-phase pattern as the
    /// other injected services. Safe to leave unset for tests;
    /// `download` short-circuits on a null controller.
    void setDownloadController(controllers::DownloadController* dl);

    /// Per-row action handlers driven by `StreamListCard.qml`'s \u22ee menu.
    void playNow(int row);
    void playWithBackend(int row, int backendKind);
    void download(int row);
    void downloadWithBackend(int row, int backendKind);
    void copyMagnet(int row);
    void openMagnet(int row);
    void copyDirectUrl(int row);
    void openDirectUrl(int row);
    void copyReleaseName(int row);

    void requestSubtitles();
    void requestSubtitlesFor(int row);

    /// "More like this" carousel hooks. Shared verbatim by both pages.
    void activateSimilar(int row);
    void addSimilarToLibrary(int row);
    void markSimilarWatched(int row);
    void findSimilarStreams(int row);

Q_SIGNALS:
    void metaChanged();
    void similarChanged();
    void sortChanged();
    void debridConfiguredChanged();
    void rawStreamsCountChanged();
    void uiFiltersChanged();
    void libraryStateChanged();
    void watchedStateChanged();

    /// Forwarded into `MainController::passiveMessage`.
    void statusMessage(const QString& text, int durationMs);

    void openMovieByTmdbRequested(int tmdbId, const QString& title);
    void openSeriesByTmdbRequested(int tmdbId, const QString& title);
    void findMovieStreamsByTmdbRequested(int tmdbId, const QString& title);
    void findSeriesStreamsByTmdbRequested(int tmdbId, const QString& title);

    void subtitlesRequested(const domain::PlaybackContext& ctx);
    void streamsRequested();

protected:
    DetailViewModelBase(api::CinemetaClient* cinemeta,
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
        domain::MediaKind kind,
        const QString& similarRailLabel,
        QObject* parent);

    // ---- subclass contract -----------------------------------------
    /// The media kind this VM represents. Drives library/watched
    /// lookups and the playback context.
    virtual domain::MediaKind mediaKind() const = 0;
    /// The playback context for the currently-loaded title (movie) or
    /// selected episode (series). Used by every per-row action.
    virtual domain::PlaybackContext currentContext() const = 0;

    // ---- shared meta helpers ---------------------------------------
    /// Assign the ten shared meta fields from a `MetaDetail`. Does NOT
    /// emit `metaChanged` or refresh derived state \u2014 the caller owns
    /// signal ordering so subclass-only fields (runtime, episodes) are
    /// in place before the single `metaChanged` emit.
    void assignCommonMeta(const domain::MetaDetail& detail);
    /// Reset the ten shared meta fields to defaults. No emit / refresh.
    void clearCommonMeta();
    void refreshLibraryState();

    // ---- shared streams helpers ------------------------------------
    void setSimilarVisible(bool on);
    /// Re-render `m_streams` from `m_rawStreams` after applying
    /// blocklist + UI filters + sort.
    void rebuildVisibleStreams();
    QList<domain::Stream> applyFilters() const;
    void sortInPlace(QList<domain::Stream>& rows) const;
    /// Drop the streams model + raw list + similar rail + UI filters
    /// back to idle. Shared tail of both subclasses' `clear()`.
    void resetStreamsAndFilters();

    /// Resolve `imdbId` \u2192 TMDB id and populate the similar rail.
    /// Guarded by `m_similarEpoch`.
    QCoro::Task<void> loadSimilarFor(QString imdbId, domain::MediaKind kind);

    /// Forwards a row's stream to a `StreamUtilityController`
    /// pointer-to-member. Keeps the per-row copy/open trampolines as
    /// one-liners.
    template <typename Method>
    void dispatchStreamAction(int row, Method method);

    // ---- injected services -----------------------------------------
    api::CinemetaClient* m_cinemeta;
    api::IndexerSelector* m_indexers;
    api::TmdbClient* m_tmdb;
    playback::session::PlaybackSessionManager* m_playback {};
    controllers::StreamUtilityController* m_streamUtility {};
    controllers::LibraryController* m_library {};
    controllers::WatchedController* m_watched {};
    controllers::DownloadController* m_downloads {};
    controllers::TokenController* m_tokens;
    config::AppSettings& m_settings;
    const QString& m_rdToken;
    const QString& m_adApiKey;

    // ---- shared models ---------------------------------------------
    StreamsListModel* m_streams;
    DiscoverSectionModel* m_similar;
    bool m_similarVisible = false;
    quint64 m_similarEpoch = 0;

    // ---- shared meta fields ----------------------------------------
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

    // ---- shared streams config -------------------------------------
    QList<domain::Stream> m_rawStreams;
    StreamsListModel::SortMode m_sortMode
        = StreamsListModel::SortMode::Smart;
    bool m_sortDescending = true;

    // Transient UI filter state \u2014 not persisted.
    QString m_uiResolutionFilter; ///< "" | "2160p" | "1080p" | "720p" | "sd"
    bool m_uiHdrOnly = false;
    bool m_uiDolbyVisionOnly = false;
    bool m_uiMultiAudioOnly = false;
    bool m_uiCachedOnly = false;

    bool m_inLibrary = false;
};

} // namespace kinema::ui::qml

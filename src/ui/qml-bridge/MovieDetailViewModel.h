// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "ui/qml-bridge/DetailViewModelBase.h"

#include <QObject>
#include <QString>

#include <QCoro/QCoroTask>

#include <optional>

namespace kinema::api {
class CinemetaClient;
class TmdbClient;
class IndexerSelector;
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
 * View-model behind `MovieDetailPage.qml`. The meta block, stream
 * UI-filters, sort config, debrid chip, "More like this" rail, library
 * state, and every per-row stream action live in `DetailViewModelBase`.
 *
 * This subclass owns only the movie-shaped concerns: the
 * Cinemeta movie-meta + Torrentio stream fetch (coroutine + epoch
 * guard, with the future-release skip), runtime / upcoming flags, and
 * single-flag watched state.
 */
class MovieDetailViewModel : public DetailViewModelBase
{
    Q_OBJECT

    Q_PROPERTY(int runtimeMinutes READ runtimeMinutes NOTIFY metaChanged)
    Q_PROPERTY(bool isUpcoming READ isUpcoming NOTIFY metaChanged)

    /// `MetaState` enum mirrored as int for cheap QML comparisons.
    /// Declared here (not on the base) so QML's `MovieDetailViewModel.Ready`
    /// attached-enum lookup resolves on the registered type.
    Q_PROPERTY(MetaState metaState READ metaState NOTIFY metaStateChanged)
    Q_PROPERTY(QString metaError READ metaError NOTIFY metaStateChanged)

    Q_PROPERTY(bool movieWatched READ movieWatched NOTIFY watchedStateChanged)
    Q_PROPERTY(QString watchedActionText READ watchedActionText NOTIFY watchedStateChanged)

public:
    enum class MetaState {
        Idle = 0,
        Loading,
        Ready,
        Error,
    };
    Q_ENUM(MetaState)

    MovieDetailViewModel(api::CinemetaClient* cinemeta,
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
    MovieDetailViewModel(api::CinemetaClient* cinemeta,
        api::IndexerSelector* indexers,
        api::TmdbClient* tmdb,
        playback::session::PlaybackSessionManager* playback,
        controllers::StreamUtilityController* streamUtility,
        controllers::TokenController* tokens,
        config::AppSettings& settings,
        const QString& rdTokenRef,
        const QString& adApiKeyRef,
        QObject* parent = nullptr);
    ~MovieDetailViewModel() override;

    int runtimeMinutes() const noexcept { return m_runtimeMinutes; }
    bool isUpcoming() const noexcept { return m_isUpcoming; }
    MetaState metaState() const noexcept { return m_metaState; }
    QString metaError() const { return m_metaError; }

    bool movieWatched() const noexcept { return m_movieWatched; }
    QString watchedActionText() const;

public Q_SLOTS:
    /// Open the detail surface for `imdbId`. Bumps the epoch and
    /// kicks off the meta + streams + similar fetches; previous
    /// requests in flight are dropped on arrival via the epoch
    /// guard.
    void load(const QString& imdbId);

    /// Resolve a TMDB id to its IMDB id and load. Used by the
    /// Browse page and the Discover similar carousel.
    void loadByTmdbId(int tmdbId, const QString& title);

    /// Re-run meta + streams for the current IMDB id.
    void retry();

    /// Drop the loaded title and reset every model.
    void clear();

    /// Header action: ask the host to push the Streams page.
    void requestStreams();

    /// Re-run only the streams fetch for the current title.
    void refreshStreams();
    void addToLibrary();
    void toggleMovieWatched();

Q_SIGNALS:
    void metaStateChanged();

private:
    domain::MediaKind mediaKind() const override
    {
        return domain::MediaKind::Movie;
    }
    domain::PlaybackContext currentContext() const override;

    QCoro::Task<void> loadMetaAndStreams(QString imdbId);
    QCoro::Task<void> loadStreamsTask(QString imdbId,
        std::optional<QDate> released, quint64 expectedEpoch);
    QCoro::Task<void> resolveByTmdbAndLoad(int tmdbId, QString title);

    void refreshStreamsForCurrentTitle();
    void resetMeta();
    void applyMeta(const domain::MetaDetail& detail);
    void refreshWatchedState();
    void setMetaState(MetaState s, const QString& error = {});

    quint64 m_epoch = 0;

    domain::MetaDetail m_currentMeta;
    int m_runtimeMinutes = 0;
    bool m_isUpcoming = false;
    MetaState m_metaState = MetaState::Idle;
    QString m_metaError;
    bool m_movieWatched = false;
};

} // namespace kinema::ui::qml

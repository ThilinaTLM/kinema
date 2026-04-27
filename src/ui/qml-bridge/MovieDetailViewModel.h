// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "ui/qml-bridge/StreamsListModel.h"

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
 * View-model behind `MovieDetailPage.qml`. Replaces the widget-
 * coupled `controllers::MovieDetailController` + `DetailPane` glue
 * outright: same coroutine + epoch pattern, same future-release
 * skip, same Cinemeta → Torrentio chain, but now exposes the
 * resulting state through `Q_PROPERTY` signals + a
 * `StreamsListModel` + a `DiscoverSectionModel` ("More like this").
 *
 * The "Similar" rail mirrors `ui::SimilarStrip`'s two-step path:
 * resolve `imdbId` → `tmdbId` via TMDB's `/find`, then
 * `recommendations(...)` with a `/similar` fallback. Returned items
 * land in a single `DiscoverSectionModel` reused from the Discover
 * surface.
 *
 * Per-row stream actions (Play / Copy / Open) delegate to the
 * shared `services::StreamActions`. The detail VM is the canonical
 * dispatcher — neither `StreamsListModel` nor the QML delegate
 * tries to reach `StreamActions` directly.
 *
 * Sort order + cached-only / blocklist filters live here: the VM
 * keeps the unfiltered raw list and re-renders the model whenever
 * any of those inputs change. Persisting `cachedOnly` flows
 * through `config::TorrentioSettings::setCachedOnly()` (mirroring
 * the legacy `StreamsPanel` two-way binding).
 */
class MovieDetailViewModel : public QObject
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
    Q_PROPERTY(int runtimeMinutes READ runtimeMinutes NOTIFY metaChanged)
    Q_PROPERTY(double rating READ rating NOTIFY metaChanged)
    Q_PROPERTY(bool isUpcoming READ isUpcoming NOTIFY metaChanged)
    Q_PROPERTY(QString releaseDateText READ releaseDateText NOTIFY metaChanged)

    /// `MetaState` enum mirrored as int for cheap QML comparisons.
    Q_PROPERTY(MetaState metaState READ metaState NOTIFY metaStateChanged)
    Q_PROPERTY(QString metaError READ metaError NOTIFY metaStateChanged)

    // ---- streams + similar -----------------------------------------
    Q_PROPERTY(StreamsListModel* streams READ streams CONSTANT)
    Q_PROPERTY(DiscoverSectionModel* similar READ similar CONSTANT)
    Q_PROPERTY(bool similarVisible READ similarVisible NOTIFY similarChanged)

    // ---- streams configuration -------------------------------------
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

    MovieDetailViewModel(api::CinemetaClient* cinemeta,
        api::TorrentioClient* torrentio,
        api::TmdbClient* tmdb,
        services::StreamActions* actions,
        controllers::TokenController* tokens,
        config::AppSettings& settings,
        const QString& rdTokenRef,
        QObject* parent = nullptr);
    ~MovieDetailViewModel() override;

    // ---- meta accessors --------------------------------------------
    QString imdbId() const { return m_imdbId; }
    QString title() const { return m_title; }
    int year() const noexcept { return m_year; }
    QString posterUrl() const { return m_posterUrl; }
    QString backdropUrl() const { return m_backdropUrl; }
    QString description() const { return m_description; }
    QStringList genres() const { return m_genres; }
    QStringList cast() const { return m_cast; }
    int runtimeMinutes() const noexcept { return m_runtimeMinutes; }
    double rating() const noexcept { return m_rating; }
    bool isUpcoming() const noexcept { return m_isUpcoming; }
    QString releaseDateText() const { return m_releaseDateText; }
    MetaState metaState() const noexcept { return m_metaState; }
    QString metaError() const { return m_metaError; }

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
    /// Open the detail surface for `imdbId`. Bumps the epoch and
    /// kicks off the meta + streams + similar fetches; previous
    /// requests in flight are dropped on arrival via the epoch
    /// guard.
    void load(const QString& imdbId);

    /// Resolve a TMDB id to its IMDB id and load. Used by the
    /// Browse page and the Discover similar carousel — both hand
    /// off TMDB ids rather than IMDB ids. `title` is used for the
    /// in-flight "Looking up …" status message.
    void loadByTmdbId(int tmdbId, const QString& title);

    /// Re-run meta + streams for the current IMDB id. Connected to
    /// the page's "Retry" placeholder action.
    void retry();

    /// Drop the loaded title and reset every model. Called when
    /// the page is popped off the stack so a fresh push lands on
    /// an empty surface.
    void clear();

    /// Header action: ask the host to push the Streams page on
    /// top of the current detail page. Emits `streamsRequested()`;
    /// `MainController` forwards as `showStreamsRequested(this)`.
    void requestStreams();

    /// Per-row action handlers driven by `StreamCard.qml`'s ⋮ menu.
    void play(int row);
    void copyMagnet(int row);
    void openMagnet(int row);
    void copyDirectUrl(int row);
    void openDirectUrl(int row);

    /// Header subtitle action / per-row subtitle action. Phase 05
    /// stubs both with `subtitlesRequested` → passive notification;
    /// phase 06 wires the `SubtitlesPage` push through
    /// `MainController`. The per-row variant carries the chosen
    /// stream's release name so the subtitles dialog has a hint
    /// for moviehash matching once it lands.
    void requestSubtitles();
    void requestSubtitlesFor(int row);

    /// Click on a similar carousel card. Routes through
    /// `openMovieRequested` / `openSeriesRequested` so the page
    /// stack gets a fresh push for the chosen title.
    void activateSimilar(int row);

Q_SIGNALS:
    void metaChanged();
    void metaStateChanged();
    void similarChanged();
    void sortChanged();
    void cachedOnlyChanged();
    void realDebridConfiguredChanged();
    void rawStreamsCountChanged();

    /// Forwarded into `MainController::passiveMessage`.
    void statusMessage(const QString& text, int durationMs);

    /// Emitted when the user activates a card in the "More like
    /// this" carousel. TMDB ids only (the similar endpoint doesn't
    /// carry IMDB ids); `MainController` routes them through a
    /// `loadByTmdbId(...)` push so the page stack gets a fresh
    /// detail page on top.
    void openMovieByTmdbRequested(int tmdbId, const QString& title);
    void openSeriesByTmdbRequested(int tmdbId, const QString& title);

    /// Phase 06 hook: pushes the Subtitles page with the carried
    /// playback context. Phase 05 stubs the connection in
    /// `MainController` with a passive notification.
    void subtitlesRequested(const api::PlaybackContext& ctx);

    /// Emitted from `requestStreams()`. `MainController` connects
    /// to a lambda that forwards as
    /// `showStreamsRequested(QObject* detailVm)`, identifying
    /// `this` so the QML shell can bind the pushed `StreamsPage`
    /// to the right view-model.
    void streamsRequested();

private:
    QCoro::Task<void> loadMetaAndStreams(QString imdbId);
    QCoro::Task<void> resolveByTmdbAndLoad(int tmdbId, QString title);
    QCoro::Task<void> loadSimilarFor(QString imdbId,
        api::MediaKind kind);

    void resetMeta();
    void applyMeta(const api::MetaDetail& detail);
    void setMetaState(MetaState s, const QString& error = {});
    void setSimilarVisible(bool on);

    /// Re-render `m_streams` from `m_rawStreams` after applying
    /// cached-only + blocklist + sort. Called when any of those
    /// inputs change.
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
    DiscoverSectionModel* m_similar;
    bool m_similarVisible = false;

    quint64 m_epoch = 0;
    quint64 m_similarEpoch = 0;

    // Meta fields.
    QString m_imdbId;
    QString m_title;
    int m_year = 0;
    QString m_posterUrl;
    QString m_backdropUrl;
    QString m_description;
    QStringList m_genres;
    QStringList m_cast;
    int m_runtimeMinutes = 0;
    double m_rating = -1.0;
    bool m_isUpcoming = false;
    QString m_releaseDateText;
    MetaState m_metaState = MetaState::Idle;
    QString m_metaError;

    // Streams config.
    QList<api::Stream> m_rawStreams;
    StreamsListModel::SortMode m_sortMode
        = StreamsListModel::SortMode::Seeders;
    bool m_sortDescending = true;
};

} // namespace kinema::ui::qml

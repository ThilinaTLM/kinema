// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MovieDetailViewModel.h"

#include "api/CinemetaClient.h"
#include "domain/Indexer.h"
#include "api/IndexerSelector.h"
#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "controllers/LibraryController.h"
#include "controllers/TokenController.h"
#include "controllers/WatchedController.h"
#include "core/util/DateFormat.h"
#include "core/io/HttpError.h"
#include "core/io/HttpErrorPresenter.h"
#include "kinema_log_ui.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

MovieDetailViewModel::MovieDetailViewModel(api::CinemetaClient* cinemeta,
    api::IndexerSelector* indexers,
    api::TmdbClient* tmdb,
    playback::session::PlaybackSessionManager* playback,
    controllers::StreamUtilityController* streamUtility,
    controllers::TokenController* tokens,
    config::AppSettings& settings,
    const QString& rdTokenRef,
    const QString& adApiKeyRef,
    QObject* parent)
    : MovieDetailViewModel(cinemeta, indexers, tmdb, playback, streamUtility,
          /*library=*/nullptr, /*watched=*/nullptr,
          tokens, settings, rdTokenRef, adApiKeyRef, parent)
{
}

MovieDetailViewModel::MovieDetailViewModel(api::CinemetaClient* cinemeta,
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
    QObject* parent)
    : DetailViewModelBase(cinemeta, indexers, tmdb, playback, streamUtility,
          library, watched, tokens, settings, rdTokenRef, adApiKeyRef,
          domain::MediaKind::Movie,
          i18nc("@label movie detail rail", "More like this"), parent)
{
    if (m_watched) {
        connect(m_watched, &controllers::WatchedController::changed,
            this, &MovieDetailViewModel::refreshWatchedState);
    }

    if (m_tokens) {
        // Either debrid credential's presence drives the
        // `debridConfigured` chip visibility on the streams page
        // and the per-row override menu (Play via torrent /
        // Use torrent for this download).
        const auto onDebridChanged = [this](const QString&) {
            Q_EMIT debridConfiguredChanged();
            refreshStreamsForCurrentTitle();
        };
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, onDebridChanged);
        connect(m_tokens,
            &controllers::TokenController::allDebridApiKeyChanged,
            this, onDebridChanged);
    }
}

MovieDetailViewModel::~MovieDetailViewModel() = default;

void MovieDetailViewModel::clear()
{
    ++m_epoch;
    ++m_similarEpoch;
    resetMeta();
    setMetaState(MetaState::Idle);
    resetStreamsAndFilters();
}

void MovieDetailViewModel::resetMeta()
{
    clearCommonMeta();
    m_runtimeMinutes = 0;
    m_isUpcoming = false;
    m_currentMeta = {};
    refreshLibraryState();
    refreshWatchedState();
    Q_EMIT metaChanged();
}

void MovieDetailViewModel::setMetaState(MetaState s, const QString& error)
{
    bool changed = false;
    if (m_metaState != s) {
        m_metaState = s;
        changed = true;
    }
    if (m_metaError != error) {
        m_metaError = error;
        changed = true;
    }
    if (changed) {
        Q_EMIT metaStateChanged();
    }
}

void MovieDetailViewModel::applyMeta(const domain::MetaDetail& detail)
{
    m_currentMeta = detail;
    assignCommonMeta(detail);
    m_runtimeMinutes = detail.runtimeMinutes.value_or(0);
    m_isUpcoming = core::isFutureRelease(detail.summary.released);
    Q_EMIT metaChanged();
    refreshLibraryState();
    refreshWatchedState();
}

void MovieDetailViewModel::refreshWatchedState()
{
    const bool watched = m_watched && !m_imdbId.isEmpty()
        && m_watched->isMovieWatched(m_imdbId);
    if (m_movieWatched == watched) {
        return;
    }
    m_movieWatched = watched;
    Q_EMIT watchedStateChanged();
}

QString MovieDetailViewModel::watchedActionText() const
{
    return m_movieWatched
        ? i18nc("@action:button", "Mark Unwatched")
        : i18nc("@action:button", "Mark Watched");
}

void MovieDetailViewModel::load(const QString& imdbId)
{
    if (imdbId.isEmpty()) {
        return;
    }
    auto t = loadMetaAndStreams(imdbId);
    Q_UNUSED(t);
}

void MovieDetailViewModel::loadByTmdbId(int tmdbId, const QString& title)
{
    if (tmdbId <= 0) {
        return;
    }
    auto t = resolveByTmdbAndLoad(tmdbId, title);
    Q_UNUSED(t);
}

void MovieDetailViewModel::retry()
{
    if (m_imdbId.isEmpty()) {
        return;
    }
    auto t = loadMetaAndStreams(m_imdbId);
    Q_UNUSED(t);
}

QCoro::Task<void> MovieDetailViewModel::loadMetaAndStreams(QString imdbId)
{
    const auto myEpoch = ++m_epoch;

    // Reset to a clean slate: a fresh load shouldn't paint the
    // previous title's poster while the new meta resolves.
    setMetaState(MetaState::Loading);
    m_streams->setLoading();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    setSimilarVisible(false);
    m_similar->setItems({});

    domain::MetaDetail detail;
    try {
        detail = co_await m_cinemeta->meta(domain::MediaKind::Movie, imdbId);
        if (myEpoch != m_epoch) {
            co_return;
        }
        applyMeta(detail);
        setMetaState(MetaState::Ready);
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        const auto msg = core::describeError(e, "movie detail/meta");
        setMetaState(MetaState::Error, msg);
        m_streams->setError(msg);
        co_return;
    }

    // Kick off the similar fetch in parallel — it doesn't block the
    // streams fetch and its own epoch guard handles cancellation.
    auto similarTask = loadSimilarFor(imdbId, domain::MediaKind::Movie);
    Q_UNUSED(similarTask);

    co_await loadStreamsTask(imdbId, detail.summary.released, myEpoch);
}

QCoro::Task<void> MovieDetailViewModel::loadStreamsTask(
    QString imdbId, std::optional<QDate> released, quint64 expectedEpoch)
{
    // Releases more than a day out produce no useful Torrentio result;
    // surface the release date and stop. Titles within the lookahead
    // window (today / tomorrow) still attempt a fetch — torrents often
    // seed a day early. The header `isUpcoming` badge stays bound to
    // the strict `isFutureRelease` semantics.
    if (released && core::isReleaseTooEarlyForStreams(*released)) {
        m_streams->setUnreleased(*released);
        co_return;
    }

    try {
        // The indexer is discovery-only — RD is no longer in the URL.
        // Per-indexer config (sort, filters) lives inside each
        // concrete Indexer; the view-model just asks for streams.
        auto* indexer = m_indexers ? m_indexers->active() : nullptr;
        if (!indexer) {
            m_streams->setError(i18nc("@info streams empty",
                "No stream indexer is configured."));
            co_return;
        }
        auto streams = co_await indexer->streams(
            domain::MediaKind::Movie, imdbId);
        if (expectedEpoch != m_epoch) {
            co_return;
        }
        m_rawStreams = std::move(streams);
        Q_EMIT rawStreamsCountChanged();
        rebuildVisibleStreams();
    } catch (const std::exception& e) {
        if (expectedEpoch != m_epoch) {
            co_return;
        }
        m_streams->setError(
            core::describeError(e, "movie detail/streams"));
    }
}

void MovieDetailViewModel::refreshStreamsForCurrentTitle()
{
    if (m_metaState != MetaState::Ready || m_imdbId.isEmpty()) {
        rebuildVisibleStreams();
        return;
    }

    const auto myEpoch = ++m_epoch;
    m_streams->setLoading();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    auto t = loadStreamsTask(m_imdbId, m_currentMeta.summary.released,
        myEpoch);
    Q_UNUSED(t);
}

QCoro::Task<void> MovieDetailViewModel::resolveByTmdbAndLoad(
    int tmdbId, QString title)
{
    Q_EMIT statusMessage(
        i18nc("@info:status",
            "Looking up \u201c%1\u201d\u2026", title),
        0);

    QString imdbId;
    try {
        imdbId = co_await m_tmdb->imdbIdForTmdbMovie(tmdbId);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e)) {
            const bool notFound
                = he->kind() == core::HttpError::Kind::HttpStatus
                && he->httpStatus() == 404;
            qCWarning(KINEMA_UI).nospace()
                << "TMDB external_ids lookup failed: movie/"
                << tmdbId << " (\"" << title << "\") \u2014 "
                << he->httpStatus() << " " << he->message();
            if (notFound) {
                Q_EMIT statusMessage(
                    i18nc("@info:status",
                        "\u201c%1\u201d isn't reachable on TMDB \u2014 "
                        "can't fetch streams.",
                        title),
                    6000);
                co_return;
            }
        }
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Could not open \u201c%1\u201d: %2",
                title,
                core::describeError(e,
                    "movie detail/tmdb external_ids")),
            6000);
        co_return;
    }

    if (imdbId.isEmpty()) {
        qCWarning(KINEMA_UI).nospace()
            << "TMDB has no IMDB id for movie/" << tmdbId
            << " (\"" << title << "\")";
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "\u201c%1\u201d has no IMDB id on TMDB \u2014 "
                "can't fetch streams.",
                title),
            6000);
        co_return;
    }

    auto t = loadMetaAndStreams(imdbId);
    Q_UNUSED(t);
    co_return;
}

domain::PlaybackContext MovieDetailViewModel::currentContext() const
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Movie;
    ctx.key.imdbId = m_imdbId;
    ctx.title = m_title;
    ctx.poster = QUrl(m_posterUrl);
    ctx.backdrop = QUrl(m_backdropUrl);
    return ctx;
}

void MovieDetailViewModel::requestStreams()
{
    Q_EMIT streamsRequested();
}

void MovieDetailViewModel::refreshStreams()
{
    refreshStreamsForCurrentTitle();
}

void MovieDetailViewModel::addToLibrary()
{
    if (!m_library || m_currentMeta.summary.imdbId.isEmpty()) {
        return;
    }
    m_library->saveMovie(m_currentMeta);
}

void MovieDetailViewModel::toggleMovieWatched()
{
    if (m_watched && !m_imdbId.isEmpty()) {
        m_watched->setMovieWatched(m_imdbId, !m_movieWatched);
    }
}

} // namespace kinema::ui::qml

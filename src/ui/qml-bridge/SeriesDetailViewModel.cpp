// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SeriesDetailViewModel.h"

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

#include <algorithm>

namespace kinema::ui::qml {

namespace {

QString episodeDisplayLabel(const QString& seriesTitle,
    const domain::Episode& ep)
{
    QString display = seriesTitle;
    if (!display.isEmpty()) {
        display += QStringLiteral(" \u2014 ");
    }
    display += QStringLiteral("S%1E%2")
                   .arg(ep.season, 2, 10, QLatin1Char('0'))
                   .arg(ep.number, 2, 10, QLatin1Char('0'));
    if (!ep.title.isEmpty()) {
        display += QStringLiteral(" \u2014 ") + ep.title;
    }
    return display;
}

} // namespace

SeriesDetailViewModel::SeriesDetailViewModel(
    api::CinemetaClient* cinemeta,
    api::IndexerSelector* indexers,
    api::TmdbClient* tmdb,
    playback::session::PlaybackSessionManager* playback,
    controllers::StreamUtilityController* streamUtility,
    controllers::TokenController* tokens,
    config::AppSettings& settings,
    const QString& rdTokenRef,
    const QString& adApiKeyRef,
    QObject* parent)
    : SeriesDetailViewModel(cinemeta, indexers, tmdb, playback, streamUtility,
          /*library=*/nullptr, /*watched=*/nullptr,
          tokens, settings, rdTokenRef, adApiKeyRef, parent)
{
}

SeriesDetailViewModel::SeriesDetailViewModel(
    api::CinemetaClient* cinemeta,
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
          domain::MediaKind::Series,
          i18nc("@label series detail rail", "More like this"), parent)
    , m_episodes(new EpisodesListModel(this))
{
    if (m_watched) {
        connect(m_watched, &controllers::WatchedController::changed,
            this, &SeriesDetailViewModel::refreshEpisodeWatchedState);
    }

    if (m_tokens) {
        const auto onDebridChanged = [this](const QString&) {
            Q_EMIT debridConfiguredChanged();
            if (m_selectedEpisodeRow >= 0 && !m_imdbId.isEmpty()) {
                auto t = loadEpisodeStreamsTask(m_selectedEpisode);
                Q_UNUSED(t);
                return;
            }
            rebuildVisibleStreams();
        };
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, onDebridChanged);
        connect(m_tokens,
            &controllers::TokenController::allDebridApiKeyChanged,
            this, onDebridChanged);
    }
}

SeriesDetailViewModel::~SeriesDetailViewModel() = default;

void SeriesDetailViewModel::setMetaState(MetaState s, const QString& error)
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

void SeriesDetailViewModel::resetMeta()
{
    clearCommonMeta();
    m_currentSeries = {};
    refreshLibraryState();
    refreshEpisodeWatchedState();
    Q_EMIT metaChanged();

    m_allEpisodes.clear();
    m_seasonNumbers.clear();
    m_seasonLabels.clear();
    m_currentSeasonIdx = -1;
    m_episodes->setEpisodes({});
    Q_EMIT seasonsChanged();
    Q_EMIT currentSeasonChanged();

    if (m_selectedEpisodeRow != -1) {
        m_selectedEpisodeRow = -1;
        m_selectedEpisodeLabel.clear();
        Q_EMIT selectedEpisodeChanged();
    }
}

void SeriesDetailViewModel::clear()
{
    ++m_metaEpoch;
    ++m_episodeEpoch;
    ++m_similarEpoch;
    resetMeta();
    setMetaState(MetaState::Idle);
    resetStreamsAndFilters();
}

void SeriesDetailViewModel::applyMeta(const domain::SeriesDetail& sd)
{
    m_currentSeries = sd;
    assignCommonMeta(sd.meta);
    Q_EMIT metaChanged();
    refreshLibraryState();

    m_allEpisodes = sd.episodes;
    rebuildSeasons();
    refreshEpisodeWatchedState();
}

void SeriesDetailViewModel::rebuildSeasons()
{
    QList<int> seasons;
    for (const auto& ep : m_allEpisodes) {
        if (ep.season <= 0) {
            // Specials excluded from the picker.
            continue;
        }
        if (!seasons.contains(ep.season)) {
            seasons.append(ep.season);
        }
    }
    std::sort(seasons.begin(), seasons.end());

    m_seasonNumbers = seasons;
    m_seasonLabels.clear();
    for (int n : std::as_const(m_seasonNumbers)) {
        m_seasonLabels.append(
            i18nc("@label series detail season tab",
                "Season %1", n));
    }
    Q_EMIT seasonsChanged();

    // Default to the first season if nothing was selected, or clamp
    // the current index to the new season list.
    int newIdx = -1;
    if (!m_seasonNumbers.isEmpty()) {
        newIdx = (m_currentSeasonIdx >= 0
                && m_currentSeasonIdx < m_seasonNumbers.size())
            ? m_currentSeasonIdx
            : 0;
    }
    if (m_currentSeasonIdx != newIdx) {
        m_currentSeasonIdx = newIdx;
        Q_EMIT currentSeasonChanged();
    }
    publishCurrentSeasonEpisodes();
}

void SeriesDetailViewModel::publishCurrentSeasonEpisodes()
{
    if (m_currentSeasonIdx < 0
        || m_currentSeasonIdx >= m_seasonNumbers.size()) {
        m_episodes->setEpisodes({});
        m_episodes->setLibraryState({}, {});
        return;
    }
    const int season = m_seasonNumbers.at(m_currentSeasonIdx);
    QList<domain::Episode> rows;
    rows.reserve(m_allEpisodes.size());
    for (const auto& ep : m_allEpisodes) {
        if (ep.season == season) {
            rows.append(ep);
        }
    }
    m_episodes->setEpisodes(std::move(rows));
    refreshEpisodeWatchedState();
}

void SeriesDetailViewModel::refreshEpisodeWatchedState()
{
    QList<bool> watched;
    QList<double> progress;
    watched.reserve(m_episodes->episodes().size());
    progress.reserve(m_episodes->episodes().size());
    int airedCount = 0;
    int watchedCount = 0;
    for (const auto& ep : m_episodes->episodes()) {
        const bool isWatched = m_watched && !m_imdbId.isEmpty()
            && m_watched->isEpisodeWatched(m_imdbId, ep.season, ep.number);
        if (!core::isFutureRelease(ep.released)) {
            ++airedCount;
            if (isWatched) {
                ++watchedCount;
            }
        }
        watched.append(isWatched);
        progress.append(m_watched && !m_imdbId.isEmpty()
            ? m_watched->episodeProgress(m_imdbId, ep.season, ep.number)
            : -1.0);
    }
    m_episodes->setLibraryState(std::move(watched), std::move(progress));

    const bool allWatched = airedCount > 0 && watchedCount == airedCount;
    if (m_seriesWatched != allWatched) {
        m_seriesWatched = allWatched;
        Q_EMIT watchedStateChanged();
    }

    // Compute per-season watched state for the tab bar badges.
    QVariantList newSeasonWatched;
    newSeasonWatched.reserve(m_seasonNumbers.size());
    for (int seasonNum : m_seasonNumbers) {
        int seasonAired = 0;
        int seasonWatchedCount = 0;
        bool hasUpcomingEpisode = false;
        for (const auto& ep : m_allEpisodes) {
            if (ep.season != seasonNum) {
                continue;
            }
            if (core::isFutureRelease(ep.released)) {
                hasUpcomingEpisode = true;
                continue;
            }
            ++seasonAired;
            if (m_watched && !m_imdbId.isEmpty()
                && m_watched->isEpisodeWatched(m_imdbId, ep.season, ep.number)) {
                ++seasonWatchedCount;
            }
        }
        newSeasonWatched.append(seasonAired > 0
            && seasonWatchedCount == seasonAired
            && !hasUpcomingEpisode);
    }
    if (m_seasonWatchedList != newSeasonWatched) {
        m_seasonWatchedList = std::move(newSeasonWatched);
        Q_EMIT seasonsChanged();
    }
}

QVariantList SeriesDetailViewModel::seasonNumbers() const
{
    QVariantList out;
    out.reserve(m_seasonNumbers.size());
    for (int n : m_seasonNumbers) {
        out.append(n);
    }
    return out;
}

void SeriesDetailViewModel::setCurrentSeason(int idx)
{
    if (idx < 0 || idx >= m_seasonNumbers.size()) {
        return;
    }
    if (m_currentSeasonIdx == idx) {
        return;
    }
    m_currentSeasonIdx = idx;
    Q_EMIT currentSeasonChanged();
    // Switching seasons collapses the streams region: the previously
    // selected episode belongs to a different season's row indices.
    clearEpisode();
    publishCurrentSeasonEpisodes();
}

void SeriesDetailViewModel::load(const QString& imdbId)
{
    if (imdbId.isEmpty()) {
        return;
    }
    auto t = loadSeriesMetaTask(imdbId, std::nullopt, std::nullopt);
    Q_UNUSED(t);
}

void SeriesDetailViewModel::loadAt(const QString& imdbId, int season,
    int episode)
{
    if (imdbId.isEmpty()) {
        return;
    }
    auto t = loadSeriesMetaTask(imdbId,
        season > 0 ? std::optional<int>(season) : std::nullopt,
        episode > 0 ? std::optional<int>(episode) : std::nullopt);
    Q_UNUSED(t);
}

void SeriesDetailViewModel::loadByTmdbId(int tmdbId, const QString& title)
{
    if (tmdbId <= 0) {
        return;
    }
    auto t = resolveByTmdbAndLoad(tmdbId, title);
    Q_UNUSED(t);
}

void SeriesDetailViewModel::retry()
{
    if (m_imdbId.isEmpty()) {
        return;
    }
    // If an episode was selected, retry both the meta and the
    // episode-stream fetch so a transient failure on either side
    // recovers from one user action.
    auto pendingSeason = m_selectedEpisodeRow >= 0
        ? std::optional<int>(m_selectedEpisode.season)
        : std::nullopt;
    auto pendingEpisode = m_selectedEpisodeRow >= 0
        ? std::optional<int>(m_selectedEpisode.number)
        : std::nullopt;
    auto t = loadSeriesMetaTask(m_imdbId, pendingSeason, pendingEpisode);
    Q_UNUSED(t);
}

QCoro::Task<void> SeriesDetailViewModel::loadSeriesMetaTask(
    QString imdbId,
    std::optional<int> pendingSeason,
    std::optional<int> pendingEpisode)
{
    const auto myEpoch = ++m_metaEpoch;
    ++m_episodeEpoch; // any in-flight episode fetch is now stale

    setMetaState(MetaState::Loading);
    m_streams->setIdle();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    setSimilarVisible(false);
    m_similar->setItems({});

    domain::SeriesDetail sd;
    try {
        sd = co_await m_cinemeta->seriesMeta(imdbId);
        if (myEpoch != m_metaEpoch) {
            co_return;
        }
        applyMeta(sd);
        setMetaState(MetaState::Ready);
    } catch (const std::exception& e) {
        if (myEpoch != m_metaEpoch) {
            co_return;
        }
        const auto msg = core::describeError(e, "series detail/meta");
        setMetaState(MetaState::Error, msg);
        co_return;
    }

    // Kick off similar in parallel \u2014 its own epoch handles cancellation.
    auto similarTask = loadSimilarFor(imdbId, domain::MediaKind::Series);
    Q_UNUSED(similarTask);

    // Auto-select pending (season, episode) seed (Continue Watching).
    if (pendingSeason && pendingEpisode) {
        const int wantS = *pendingSeason;
        const int wantE = *pendingEpisode;
        // Move the season picker first so the published episode list
        // matches what we're indexing into.
        const int idx = m_seasonNumbers.indexOf(wantS);
        if (idx >= 0) {
            if (m_currentSeasonIdx != idx) {
                m_currentSeasonIdx = idx;
                Q_EMIT currentSeasonChanged();
                publishCurrentSeasonEpisodes();
            }
            const int row = m_episodes->rowFor(wantS, wantE);
            if (row >= 0) {
                selectEpisode(row);
            }
        }
    }
    co_return;
}

void SeriesDetailViewModel::selectEpisode(int row)
{
    const auto* ep = m_episodes->at(row);
    if (!ep) {
        return;
    }
    m_selectedEpisodeRow = row;
    m_selectedEpisode = *ep;
    m_selectedEpisodeLabel = episodeDisplayLabel(m_title, *ep);
    Q_EMIT selectedEpisodeChanged();

    auto t = loadEpisodeStreamsTask(*ep);
    Q_UNUSED(t);
}

void SeriesDetailViewModel::clearEpisode()
{
    if (m_selectedEpisodeRow == -1) {
        return;
    }
    ++m_episodeEpoch; // any in-flight stream fetch is now stale
    m_selectedEpisodeRow = -1;
    m_selectedEpisode = {};
    m_selectedEpisodeLabel.clear();
    Q_EMIT selectedEpisodeChanged();
    m_streams->setIdle();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
}

void SeriesDetailViewModel::selectEpisodeAndOpenStreams(int row)
{
    selectEpisode(row);
    if (m_selectedEpisodeRow < 0) {
        // selectEpisode bails when the row index is bad; do not
        // navigate without a selection.
        return;
    }
    Q_EMIT streamsRequested();
}

void SeriesDetailViewModel::requestStreams()
{
    if (m_selectedEpisodeRow < 0) {
        return;
    }
    Q_EMIT streamsRequested();
}

void SeriesDetailViewModel::refreshStreams()
{
    if (m_selectedEpisodeRow < 0 || m_imdbId.isEmpty()) {
        return;
    }
    auto t = loadEpisodeStreamsTask(m_selectedEpisode);
    Q_UNUSED(t);
}

void SeriesDetailViewModel::addToLibrary()
{
    if (!m_library || m_currentSeries.meta.summary.imdbId.isEmpty()) {
        return;
    }
    m_library->saveSeries(m_currentSeries);
}

void SeriesDetailViewModel::toggleEpisodeWatched(int row)
{
    const auto* ep = m_episodes->at(row);
    if (!m_watched || !ep || m_imdbId.isEmpty()
        || core::isFutureRelease(ep->released)) {
        return;
    }
    const bool watched = m_watched->isEpisodeWatched(
        m_imdbId, ep->season, ep->number);
    m_watched->setEpisodeWatched(m_imdbId, ep->season, ep->number,
        !watched);
}

void SeriesDetailViewModel::toggleSeriesWatched()
{
    if (!m_watched || m_imdbId.isEmpty()) {
        return;
    }
    QList<QPair<int, int>> pairs;
    pairs.reserve(m_allEpisodes.size());
    for (const auto& ep : m_allEpisodes) {
        // Skip specials so the action's intent ("finish the series")
        // matches what the season picker actually exposes.
        if (ep.season <= 0 || core::isFutureRelease(ep.released)) {
            continue;
        }
        pairs.append({ ep.season, ep.number });
    }
    m_watched->setEpisodesWatched(m_imdbId, pairs, !m_seriesWatched);
}

void SeriesDetailViewModel::markSeasonWatched(int season, bool watched)
{
    if (!m_watched || m_imdbId.isEmpty()) {
        return;
    }
    QList<QPair<int, int>> pairs;
    for (const auto& ep : m_allEpisodes) {
        if (ep.season == season && !core::isFutureRelease(ep.released)) {
            pairs.append({ ep.season, ep.number });
        }
    }
    m_watched->setEpisodesWatched(m_imdbId, pairs, watched);
}

QCoro::Task<void> SeriesDetailViewModel::loadEpisodeStreamsTask(
    domain::Episode ep)
{
    const auto myEpoch = ++m_episodeEpoch;

    m_streams->setLoading();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();

    // Skip the network call only for episodes still more than a day
    // out \u2014 torrents commonly seed ~1 day before the official air
    // date. Strict "upcoming" semantics still drive episode-row badges
    // and watched aggregation via `core::isFutureRelease`.
    if (ep.released
        && core::isReleaseTooEarlyForStreams(ep.released)) {
        m_streams->setUnreleased(*ep.released);
        co_return;
    }

    try {
        auto* indexer = m_indexers ? m_indexers->active() : nullptr;
        if (!indexer) {
            m_streams->setError(i18nc("@info streams empty",
                "No stream indexer is configured."));
            co_return;
        }
        auto streams = co_await indexer->streams(
            domain::MediaKind::Series, ep.streamId(m_imdbId));
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_rawStreams = std::move(streams);
        Q_EMIT rawStreamsCountChanged();
        rebuildVisibleStreams();
    } catch (const std::exception& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_streams->setError(
            core::describeError(e, "episode streams"));
    }
}

QCoro::Task<void> SeriesDetailViewModel::resolveByTmdbAndLoad(
    int tmdbId, QString title)
{
    Q_EMIT statusMessage(
        i18nc("@info:status",
            "Looking up \u201c%1\u201d\u2026", title),
        0);

    QString imdbId;
    try {
        imdbId = co_await m_tmdb->imdbIdForTmdbSeries(tmdbId);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e)) {
            const bool notFound
                = he->kind() == core::HttpError::Kind::HttpStatus
                && he->httpStatus() == 404;
            qCWarning(KINEMA_UI).nospace()
                << "TMDB external_ids lookup failed: tv/"
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
                    "series detail/tmdb external_ids")),
            6000);
        co_return;
    }

    if (imdbId.isEmpty()) {
        qCWarning(KINEMA_UI).nospace()
            << "TMDB has no IMDB id for tv/" << tmdbId
            << " (\"" << title << "\")";
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "\u201c%1\u201d has no IMDB id on TMDB \u2014 "
                "can't fetch streams.",
                title),
            6000);
        co_return;
    }

    auto t = loadSeriesMetaTask(imdbId, std::nullopt, std::nullopt);
    Q_UNUSED(t);
    co_return;
}

domain::PlaybackContext SeriesDetailViewModel::currentContext() const
{
    domain::PlaybackContext ctx;
    ctx.key.kind = domain::MediaKind::Series;
    ctx.key.imdbId = m_imdbId;
    if (m_selectedEpisodeRow >= 0) {
        ctx.key.season = m_selectedEpisode.season;
        ctx.key.episode = m_selectedEpisode.number;
        ctx.episodeTitle = m_selectedEpisode.title;
    }
    ctx.seriesTitle = m_title;
    ctx.title = m_selectedEpisodeRow >= 0
        ? m_selectedEpisodeLabel
        : m_title;
    ctx.poster = QUrl(m_posterUrl);
    ctx.backdrop = QUrl(m_backdropUrl);
    return ctx;
}

} // namespace kinema::ui::qml

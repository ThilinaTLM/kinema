// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SeriesDetailViewModel.h"

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#include "controllers/TokenController.h"
#include "controllers/WatchedController.h"
#include "core/DateFormat.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/StreamFilter.h"
#include "kinema_log_app.h"
#include "controllers/PlayQueueController.h"
#include "services/StreamActions.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/StreamSorting.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {

using SortMode = StreamsListModel::SortMode;

QString episodeDisplayLabel(const QString& seriesTitle,
    const api::Episode& ep)
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
    api::TorrentioClient* torrentio,
    api::TmdbClient* tmdb,
    services::StreamActions* actions,
    controllers::TokenController* tokens,
    config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : SeriesDetailViewModel(cinemeta, torrentio, tmdb, actions,
          /*library=*/nullptr, /*watched=*/nullptr,
          tokens, settings, rdTokenRef, parent)
{
}

SeriesDetailViewModel::SeriesDetailViewModel(
    api::CinemetaClient* cinemeta,
    api::TorrentioClient* torrentio,
    api::TmdbClient* tmdb,
    services::StreamActions* actions,
    controllers::LibraryController* library,
    controllers::WatchedController* watched,
    controllers::TokenController* tokens,
    config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_torrentio(torrentio)
    , m_tmdb(tmdb)
    , m_actions(actions)
    , m_library(library)
    , m_watched(watched)
    , m_tokens(tokens)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
    , m_streams(new StreamsListModel(this))
    , m_episodes(new EpisodesListModel(this))
    , m_similar(new DiscoverSectionModel(
          i18nc("@label series detail rail", "More like this"), this))
{
    connect(&m_settings.filter(),
        &config::FilterSettings::keywordBlocklistChanged, this,
        [this](const QStringList&) { rebuildVisibleStreams(); });

    if (m_library) {
        connect(m_library, &controllers::LibraryController::changed,
            this, &SeriesDetailViewModel::refreshLibraryState);
    }
    if (m_watched) {
        connect(m_watched, &controllers::WatchedController::changed,
            this, &SeriesDetailViewModel::refreshEpisodeWatchedState);
    }

    if (m_tokens) {
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, [this](const QString&) {
                Q_EMIT realDebridConfiguredChanged();
                if (m_selectedEpisodeRow >= 0 && !m_imdbId.isEmpty()) {
                    auto t = loadEpisodeStreamsTask(m_selectedEpisode);
                    Q_UNUSED(t);
                    return;
                }
                rebuildVisibleStreams();
            });
    }
}

SeriesDetailViewModel::~SeriesDetailViewModel() = default;

void SeriesDetailViewModel::setSortMode(int mode)
{
    const auto m = static_cast<SortMode>(mode);
    if (m_sortMode == m) {
        return;
    }
    m_sortMode = m;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void SeriesDetailViewModel::setSortDescending(bool desc)
{
    if (m_sortDescending == desc) {
        return;
    }
    m_sortDescending = desc;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void SeriesDetailViewModel::setUiResolutionFilter(const QString& res)
{
    if (m_uiResolutionFilter == res) {
        return;
    }
    m_uiResolutionFilter = res;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void SeriesDetailViewModel::setUiHdrOnly(bool on)
{
    if (m_uiHdrOnly == on) {
        return;
    }
    m_uiHdrOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void SeriesDetailViewModel::setUiDolbyVisionOnly(bool on)
{
    if (m_uiDolbyVisionOnly == on) {
        return;
    }
    m_uiDolbyVisionOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void SeriesDetailViewModel::setUiMultiAudioOnly(bool on)
{
    if (m_uiMultiAudioOnly == on) {
        return;
    }
    m_uiMultiAudioOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

bool SeriesDetailViewModel::uiAnyFilterActive() const noexcept
{
    return !m_uiResolutionFilter.isEmpty()
        || m_uiHdrOnly || m_uiDolbyVisionOnly || m_uiMultiAudioOnly;
}

void SeriesDetailViewModel::clearUiFilters()
{
    if (!uiAnyFilterActive()) {
        return;
    }
    m_uiResolutionFilter.clear();
    m_uiHdrOnly = false;
    m_uiDolbyVisionOnly = false;
    m_uiMultiAudioOnly = false;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

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

void SeriesDetailViewModel::setSimilarVisible(bool on)
{
    if (m_similarVisible == on) {
        return;
    }
    m_similarVisible = on;
    Q_EMIT similarChanged();
}

void SeriesDetailViewModel::resetMeta()
{
    m_imdbId.clear();
    m_title.clear();
    m_year = 0;
    m_posterUrl.clear();
    m_backdropUrl.clear();
    m_description.clear();
    m_genres.clear();
    m_cast.clear();
    m_rating = -1.0;
    m_releaseDateText.clear();
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
    m_streams->setIdle();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    m_similar->setItems({});
    setSimilarVisible(false);
    if (uiAnyFilterActive()) {
        m_uiResolutionFilter.clear();
        m_uiHdrOnly = false;
        m_uiDolbyVisionOnly = false;
        m_uiMultiAudioOnly = false;
        Q_EMIT uiFiltersChanged();
    }
}

void SeriesDetailViewModel::applyMeta(const api::SeriesDetail& sd)
{
    m_currentSeries = sd;
    const auto& s = sd.meta.summary;
    m_imdbId = s.imdbId;
    m_title = s.title;
    m_year = s.year.value_or(0);
    m_posterUrl = s.poster.toString();
    m_backdropUrl = sd.meta.background.toString();
    m_description = s.description;
    m_genres = sd.meta.genres;
    m_cast = sd.meta.cast;
    m_rating = s.imdbRating.value_or(-1.0);
    m_releaseDateText = (s.released && s.released->isValid())
        ? core::formatReleaseDate(*s.released)
        : QString();
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
    QList<api::Episode> rows;
    rows.reserve(m_allEpisodes.size());
    for (const auto& ep : m_allEpisodes) {
        if (ep.season == season) {
            rows.append(ep);
        }
    }
    m_episodes->setEpisodes(std::move(rows));
    refreshEpisodeWatchedState();
}

void SeriesDetailViewModel::refreshLibraryState()
{
    const bool inLibrary = m_library && !m_imdbId.isEmpty()
        && m_library->isInLibrary(api::MediaKind::Series, m_imdbId);
    if (m_inLibrary != inLibrary) {
        m_inLibrary = inLibrary;
        Q_EMIT libraryStateChanged();
    }
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

QString SeriesDetailViewModel::libraryActionText() const
{
    return m_inLibrary
        ? i18nc("@action:button", "Remove from Library")
        : i18nc("@action:button", "Add to Library");
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

    api::SeriesDetail sd;
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
    auto similarTask = loadSimilarFor(imdbId);
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

void SeriesDetailViewModel::addToLibrary()
{
    if (!m_library || m_currentSeries.meta.summary.imdbId.isEmpty()) {
        return;
    }
    m_library->saveSeries(m_currentSeries);
}

void SeriesDetailViewModel::removeFromLibrary()
{
    if (m_library && !m_imdbId.isEmpty()) {
        m_library->removeFromLibrary(api::MediaKind::Series, m_imdbId);
    }
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
    api::Episode ep)
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
        auto opts = m_settings.torrentioOptions();
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Series, ep.streamId(m_imdbId), opts);
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
            qCWarning(KINEMA_APP).nospace()
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
        qCWarning(KINEMA_APP).nospace()
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

QCoro::Task<void> SeriesDetailViewModel::loadSimilarFor(QString imdbId)
{
    const auto myEpoch = ++m_similarEpoch;
    auto kind = api::MediaKind::Series;

    if (imdbId.isEmpty() || !m_tmdb || !m_tmdb->hasToken()) {
        m_similar->setItems({});
        setSimilarVisible(false);
        co_return;
    }

    m_similar->setLoading();

    int tmdbId = 0;
    try {
        const auto [id, found] = co_await m_tmdb->findByImdb(imdbId, kind);
        if (myEpoch != m_similarEpoch) {
            co_return;
        }
        tmdbId = id;
        kind = found;
        if (tmdbId == 0) {
            m_similar->setItems({});
            setSimilarVisible(false);
            co_return;
        }
    } catch (const std::exception& e) {
        if (myEpoch != m_similarEpoch) {
            co_return;
        }
        Q_UNUSED(core::describeError(e, "series detail/similar/find"));
        m_similar->setItems({});
        setSimilarVisible(false);
        co_return;
    }

    QList<api::DiscoverItem> items;
    try {
        items = co_await m_tmdb->recommendations(kind, tmdbId);
        if (myEpoch != m_similarEpoch) {
            co_return;
        }
        if (items.isEmpty()) {
            items = co_await m_tmdb->similar(kind, tmdbId);
            if (myEpoch != m_similarEpoch) {
                co_return;
            }
        }
    } catch (const std::exception& e) {
        if (myEpoch != m_similarEpoch) {
            co_return;
        }
        Q_UNUSED(core::describeError(e,
            "series detail/similar/recommendations"));
        m_similar->setItems({});
        setSimilarVisible(false);
        co_return;
    }

    if (items.isEmpty()) {
        m_similar->setItems({});
        setSimilarVisible(false);
        co_return;
    }

    m_similar->setItems(std::move(items));
    setSimilarVisible(true);
}

void SeriesDetailViewModel::rebuildVisibleStreams()
{
    if (m_rawStreams.isEmpty()) {
        return;
    }
    auto visible = applyFilters();
    sortInPlace(visible);

    QString emptyExplanation;
    if (visible.isEmpty()) {
        emptyExplanation = i18nc("@info streams empty",
            "Loosen the exclusions or keyword blocklist in "
            "Settings.");
    }
    m_streams->setItems(std::move(visible), emptyExplanation);
}

QList<api::Stream> SeriesDetailViewModel::applyFilters() const
{
    if (m_rawStreams.isEmpty()) {
        return {};
    }
    core::stream_filter::ClientFilters f;
    f.keywordBlocklist = m_settings.filter().keywordBlocklist();
    auto rows = core::stream_filter::apply(m_rawStreams, f);

    return stream_sorting::applyUiFilters(std::move(rows),
        { m_uiResolutionFilter, m_uiHdrOnly,
            m_uiDolbyVisionOnly, m_uiMultiAudioOnly });
}

void SeriesDetailViewModel::sortInPlace(QList<api::Stream>& rows) const
{
    stream_sorting::sortInPlace(rows, m_sortMode, m_sortDescending);
}

api::PlaybackContext SeriesDetailViewModel::currentContext() const
{
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Series;
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
    return ctx;
}

void SeriesDetailViewModel::setPlayQueue(controllers::PlayQueueController* queue)
{
    m_queue = queue;
}

void SeriesDetailViewModel::setDownloadController(
    controllers::DownloadController* dl)
{
    m_downloads = dl;
}

void SeriesDetailViewModel::playNow(int row)
{
    const auto* s = m_streams->at(row);
    if (!s) {
        return;
    }
    if (s->directUrl.isEmpty() && s->infoHash.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "This stream has no playable URL or magnet."),
            4000);
        return;
    }
    if (!m_queue) {
        return;
    }
    m_queue->playNow(*s, currentContext());
}

void SeriesDetailViewModel::playNext(int row)
{
    const auto* s = m_streams->at(row);
    if (!s || (s->directUrl.isEmpty() && s->infoHash.isEmpty()) || !m_queue) {
        return;
    }
    m_queue->playNext(*s, currentContext());
}

void SeriesDetailViewModel::enqueue(int row)
{
    const auto* s = m_streams->at(row);
    if (!s || (s->directUrl.isEmpty() && s->infoHash.isEmpty()) || !m_queue) {
        return;
    }
    m_queue->enqueue(*s, currentContext());
}

void SeriesDetailViewModel::download(int row)
{
    const auto* s = m_streams->at(row);
    if (!s) {
        return;
    }
    if (s->infoHash.isEmpty() && s->directUrl.isEmpty()) {
        Q_EMIT statusMessage(i18nc("@info:status",
            "This stream has no playable URL or magnet."), 4000);
        return;
    }
    if (!m_downloads) {
        return;
    }
    m_downloads->download(*s, currentContext());
    Q_EMIT statusMessage(
        i18nc("@info:status starting a background episode download",
            "Downloading \u201c%1\u201d\u2026", currentContext().title),
        3500);
}

void SeriesDetailViewModel::downloadWithBackend(int row, int backendKind)
{
    const auto* s = m_streams->at(row);
    if (!s || !m_downloads) {
        return;
    }
    m_downloads->downloadWithBackend(*s, currentContext(),
        static_cast<api::DownloadBackendKind>(backendKind));
}

template <typename Method>
void SeriesDetailViewModel::dispatchStreamAction(int row, Method method)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        (m_actions->*method)(*s);
    }
}

void SeriesDetailViewModel::copyMagnet(int row)
{
    dispatchStreamAction(row, &services::StreamActions::copyMagnet);
}

void SeriesDetailViewModel::openMagnet(int row)
{
    dispatchStreamAction(row, &services::StreamActions::openMagnet);
}

void SeriesDetailViewModel::copyDirectUrl(int row)
{
    dispatchStreamAction(row, &services::StreamActions::copyDirectUrl);
}

void SeriesDetailViewModel::openDirectUrl(int row)
{
    dispatchStreamAction(row, &services::StreamActions::openDirectUrl);
}

void SeriesDetailViewModel::requestSubtitles()
{
    Q_EMIT subtitlesRequested(currentContext());
}

void SeriesDetailViewModel::requestSubtitlesFor(int row)
{
    auto ctx = currentContext();
    if (const auto* s = m_streams->at(row)) {
        ctx.streamRef = api::HistoryStreamRef::fromStream(*s);
    }
    Q_EMIT subtitlesRequested(ctx);
}

void SeriesDetailViewModel::activateSimilar(int row)
{
    const auto* item = m_similar->itemAt(row);
    if (!item) {
        return;
    }
    if (item->kind == api::MediaKind::Series) {
        Q_EMIT openSeriesByTmdbRequested(item->tmdbId, item->title);
    } else {
        Q_EMIT openMovieByTmdbRequested(item->tmdbId, item->title);
    }
}

} // namespace kinema::ui::qml

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/SeriesDetailViewModel.h"

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "config/FilterSettings.h"
#include "config/TorrentioSettings.h"
#include "controllers/TokenController.h"
#include "core/DateFormat.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "core/StreamFilter.h"
#include "kinema_debug.h"
#include "services/StreamActions.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"

#include <KLocalizedString>

#include <algorithm>

namespace kinema::ui::qml {

namespace {

using SortMode = StreamsListModel::SortMode;

int resolutionRank(const QString& res)
{
    if (res == QLatin1String("2160p")) return 2160;
    if (res == QLatin1String("1440p")) return 1440;
    if (res == QLatin1String("1080p")) return 1080;
    if (res == QLatin1String("720p"))  return 720;
    if (res == QLatin1String("480p"))  return 480;
    if (res == QLatin1String("360p"))  return 360;
    return 0;
}

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
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_torrentio(torrentio)
    , m_tmdb(tmdb)
    , m_actions(actions)
    , m_tokens(tokens)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
    , m_streams(new StreamsListModel(this))
    , m_episodes(new EpisodesListModel(this))
    , m_similar(new DiscoverSectionModel(
          i18nc("@label series detail rail", "More like this"), this))
{
    connect(&m_settings.torrentio(),
        &config::TorrentioSettings::cachedOnlyChanged, this,
        [this](bool) {
            Q_EMIT cachedOnlyChanged();
            rebuildVisibleStreams();
        });
    connect(&m_settings.filter(),
        &config::FilterSettings::keywordBlocklistChanged, this,
        [this](const QStringList&) { rebuildVisibleStreams(); });

    if (m_tokens) {
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, [this](const QString&) {
                Q_EMIT realDebridConfiguredChanged();
                rebuildVisibleStreams();
            });
    }
}

SeriesDetailViewModel::~SeriesDetailViewModel() = default;

bool SeriesDetailViewModel::cachedOnly() const
{
    return m_settings.torrentio().cachedOnly();
}

void SeriesDetailViewModel::setCachedOnly(bool on)
{
    if (m_settings.torrentio().cachedOnly() == on) {
        return;
    }
    m_settings.torrentio().setCachedOnly(on);
}

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
}

void SeriesDetailViewModel::applyMeta(const api::SeriesDetail& sd)
{
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

    m_allEpisodes = sd.episodes;
    rebuildSeasons();
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

QCoro::Task<void> SeriesDetailViewModel::loadEpisodeStreamsTask(
    api::Episode ep)
{
    const auto myEpoch = ++m_episodeEpoch;

    m_streams->setLoading();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();

    // Unaired episodes \u2014 skip the network call.
    if (core::isFutureRelease(ep.released) && ep.released) {
        m_streams->setUnreleased(*ep.released);
        co_return;
    }

    try {
        auto opts = m_settings.torrentioOptions();
        opts.realDebridToken = m_rdToken;
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
            qCWarning(KINEMA).nospace()
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
        qCWarning(KINEMA).nospace()
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
        const bool cachedFiltered
            = realDebridConfigured() && cachedOnly();
        emptyExplanation = cachedFiltered
            ? i18nc("@info streams empty",
                "Uncheck \u201cCached on Real-Debrid only\u201d or "
                "widen your filters in Settings.")
            : i18nc("@info streams empty",
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
    f.cachedOnly = realDebridConfigured() && cachedOnly();
    f.keywordBlocklist = m_settings.filter().keywordBlocklist();
    return core::stream_filter::apply(m_rawStreams, f);
}

void SeriesDetailViewModel::sortInPlace(QList<api::Stream>& rows) const
{
    auto cmp = [this](const api::Stream& a, const api::Stream& b) {
        switch (m_sortMode) {
        case SortMode::Seeders:
            return a.seeders.value_or(-1) < b.seeders.value_or(-1);
        case SortMode::Size:
            return a.sizeBytes.value_or(-1) < b.sizeBytes.value_or(-1);
        case SortMode::Quality:
            return resolutionRank(a.resolution)
                < resolutionRank(b.resolution);
        case SortMode::Provider:
            return a.provider.localeAwareCompare(b.provider) < 0;
        case SortMode::ReleaseName:
            return a.releaseName.localeAwareCompare(b.releaseName) < 0;
        }
        return false;
    };
    if (m_sortDescending) {
        std::stable_sort(rows.begin(), rows.end(),
            [cmp](const api::Stream& a, const api::Stream& b) {
                return cmp(b, a);
            });
    } else {
        std::stable_sort(rows.begin(), rows.end(), cmp);
    }
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

void SeriesDetailViewModel::play(int row)
{
    if (!m_actions) {
        return;
    }
    const auto* s = m_streams->at(row);
    if (!s) {
        return;
    }
    if (s->directUrl.isEmpty()) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "Direct playback needs a Real-Debrid cached stream."),
            4000);
        return;
    }
    m_actions->play(*s, currentContext());
}

void SeriesDetailViewModel::copyMagnet(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->copyMagnet(*s);
    }
}

void SeriesDetailViewModel::openMagnet(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->openMagnet(*s);
    }
}

void SeriesDetailViewModel::copyDirectUrl(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->copyDirectUrl(*s);
    }
}

void SeriesDetailViewModel::openDirectUrl(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->openDirectUrl(*s);
    }
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

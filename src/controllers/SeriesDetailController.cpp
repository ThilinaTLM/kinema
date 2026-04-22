// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/SeriesDetailController.h"

#include "api/CinemetaClient.h"
#include "api/TmdbClient.h"
#include "api/TorrentioClient.h"
#include "config/AppSettings.h"
#include "controllers/NavigationController.h"
#include "controllers/Page.h"
#include "core/DateFormat.h"
#include "core/HttpError.h"
#include "core/HttpErrorPresenter.h"
#include "kinema_debug.h"
#include "ui/SeriesDetailPane.h"

#include <KLocalizedString>

namespace kinema::controllers {

SeriesDetailController::SeriesDetailController(
    api::CinemetaClient* cinemeta,
    api::TorrentioClient* torrentio,
    api::TmdbClient* tmdb,
    ui::SeriesDetailPane* pane,
    NavigationController* nav,
    const config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_torrentio(torrentio)
    , m_tmdb(tmdb)
    , m_pane(pane)
    , m_nav(nav)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
{
    // Keep the pane's sub-page in sync with nav transitions. When
    // Back walks us out of SeriesStreams the nav controller emits
    // currentChanged(SeriesEpisodes); flip the pane so the episode
    // list reappears.
    connect(m_nav, &NavigationController::currentChanged,
        this, [this](Page page) {
            if (page == Page::SeriesEpisodes) {
                m_pane->showEpisodesPage();
            }
        });
}

void SeriesDetailController::openFromSummary(const api::MetaSummary& summary)
{
    m_nav->goTo(Page::SeriesEpisodes);
    auto t = loadSeriesTask(summary);
    Q_UNUSED(t);
}

void SeriesDetailController::openFromDiscover(const api::DiscoverItem& item)
{
    auto t = openFromDiscoverTask(item);
    Q_UNUSED(t);
}

void SeriesDetailController::openFromHistory(const api::HistoryEntry& entry)
{
    if (entry.key.imdbId.isEmpty()) {
        return;
    }
    // Stash the season/episode to select once Cinemeta returns the
    // full episode list (see loadSeriesTask).
    m_pendingSeason = entry.key.season;
    m_pendingEpisode = entry.key.episode;

    api::MetaSummary s;
    s.imdbId = entry.key.imdbId;
    s.kind = api::MediaKind::Series;
    s.title = entry.seriesTitle.isEmpty() ? entry.title : entry.seriesTitle;
    s.poster = entry.poster;
    openFromSummary(s);
}

void SeriesDetailController::selectEpisode(const api::Episode& ep)
{
    if (m_currentImdbId.isEmpty()) {
        return;
    }
    // Flip to the streams sub-page in both the pane (for user feedback
    // before the fetch completes) and the nav controller (so Back /
    // the toolbar tooltip are correct).
    m_pane->showEpisodeStreamsPage(ep);
    m_nav->goTo(Page::SeriesStreams);

    // Feed a per-episode playback context into the panel so Play
    // carries the right identity / display metadata.
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Series;
    ctx.key.imdbId = m_currentImdbId;
    ctx.key.season = ep.season;
    ctx.key.episode = ep.number;
    ctx.seriesTitle = m_currentSeriesTitle;
    ctx.episodeTitle = ep.title;
    // Display title: "Series — S01E03 — Pilot" is the cleanest
    // notification line; panes use it verbatim.
    QString display = m_currentSeriesTitle;
    if (!display.isEmpty()) {
        display += QStringLiteral(" — ");
    }
    display += QStringLiteral("S%1E%2")
                   .arg(ep.season, 2, 10, QLatin1Char('0'))
                   .arg(ep.number, 2, 10, QLatin1Char('0'));
    if (!ep.title.isEmpty()) {
        display += QStringLiteral(" — ") + ep.title;
    }
    ctx.title = display;
    ctx.poster = m_currentSeriesPoster.isValid()
        ? m_currentSeriesPoster
        : ep.thumbnail;
    m_pane->setPlaybackContext(ctx);

    auto t = loadEpisodeTask(ep, m_currentImdbId);
    Q_UNUSED(t);
}

void SeriesDetailController::refetchCurrentEpisode()
{
    if (!m_currentEpisode || m_currentImdbId.isEmpty()) {
        return;
    }
    auto t = loadEpisodeTask(*m_currentEpisode, m_currentImdbId);
    Q_UNUSED(t);
}

void SeriesDetailController::onBackToEpisodes()
{
    ++m_episodeEpoch;
    m_currentEpisode.reset();
    m_pane->showEpisodesPage();
    if (m_nav->current() == Page::SeriesStreams) {
        m_nav->goTo(Page::SeriesEpisodes);
    }
}

void SeriesDetailController::clear()
{
    ++m_seriesEpoch;
    ++m_episodeEpoch;
    m_currentImdbId.clear();
    m_currentEpisode.reset();
    m_pendingSeason.reset();
    m_pendingEpisode.reset();
}

QCoro::Task<void> SeriesDetailController::loadSeriesTask(
    api::MetaSummary summary)
{
    const auto myEpoch = ++m_seriesEpoch;
    m_currentImdbId = summary.imdbId;
    m_currentEpisode.reset();
    m_currentSeriesTitle = summary.title;
    m_currentSeriesPoster = summary.poster;

    m_pane->showMetaLoading();

    try {
        auto sd = co_await m_cinemeta->seriesMeta(summary.imdbId);
        if (myEpoch != m_seriesEpoch) {
            co_return;
        }
        if (!sd.meta.summary.title.isEmpty()) {
            m_currentSeriesTitle = sd.meta.summary.title;
        }
        if (sd.meta.summary.poster.isValid()) {
            m_currentSeriesPoster = sd.meta.summary.poster;
        }
        m_pane->setSeries(sd);
        m_pane->setSimilarContext(summary.imdbId);
        m_pane->focusEpisodeList();

        // Auto-select a pending episode (from openFromHistory) now
        // that the episode list is populated. One-shot: clear the
        // pending values so a later Cinemeta refetch doesn't
        // re-trigger the jump.
        if (m_pendingSeason && m_pendingEpisode) {
            const int wantS = *m_pendingSeason;
            const int wantE = *m_pendingEpisode;
            m_pendingSeason.reset();
            m_pendingEpisode.reset();
            for (const auto& ep : sd.episodes) {
                if (ep.season == wantS && ep.number == wantE) {
                    selectEpisode(ep);
                    break;
                }
            }
        }

    } catch (const std::exception& e) {
        if (myEpoch != m_seriesEpoch) {
            co_return;
        }
        m_pane->showMetaError(core::describeError(e, "series detail"));
    }
}

QCoro::Task<void> SeriesDetailController::loadEpisodeTask(
    api::Episode ep, QString imdbId)
{
    const auto myEpoch = ++m_episodeEpoch;
    m_currentEpisode = ep;

    // Unaired episodes \u2014 skip the network call.
    if (core::isFutureRelease(ep.released)) {
        m_pane->showTorrentsUnreleased(*ep.released);
        co_return;
    }

    m_pane->showTorrentsLoading();

    auto opts = m_settings.torrentioOptions();
    opts.realDebridToken = m_rdToken;

    try {
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Series, ep.streamId(imdbId), opts);
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_pane->setTorrents(std::move(streams));
    } catch (const std::exception& e) {
        if (myEpoch != m_episodeEpoch) {
            co_return;
        }
        m_pane->showTorrentsError(
            core::describeError(e, "episode streams"));
    }
}

QCoro::Task<void> SeriesDetailController::openFromDiscoverTask(
    api::DiscoverItem item)
{
    Q_EMIT statusMessage(
        i18nc("@info:status", "Looking up \u201c%1\u201d\u2026", item.title), 0);

    QString imdbId;
    try {
        imdbId = co_await m_tmdb->imdbIdForTmdbSeries(item.tmdbId);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e)) {
            const bool notFound = he->kind() == core::HttpError::Kind::HttpStatus
                && he->httpStatus() == 404;
            qCWarning(KINEMA).nospace()
                << "TMDB external_ids lookup failed: tv/"
                << item.tmdbId << " (\"" << item.title << "\") \u2014 "
                << he->httpStatus() << " " << he->message();
            if (notFound) {
                Q_EMIT statusMessage(
                    i18nc("@info:status",
                        "\u201c%1\u201d isn't reachable on TMDB \u2014 "
                        "can't fetch streams.",
                        item.title),
                    6000);
                co_return;
            }
        }

        Q_EMIT statusMessage(
            i18nc("@info:status", "Could not open \u201c%1\u201d: %2",
                item.title,
                core::describeError(e, "series detail/tmdb external_ids")),
            6000);
        co_return;
    }

    if (imdbId.isEmpty()) {
        qCWarning(KINEMA).nospace()
            << "TMDB has no IMDB id for tv/"
            << item.tmdbId << " (\"" << item.title << "\")";
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "\u201c%1\u201d has no IMDB id on TMDB \u2014 "
                "can't fetch streams.",
                item.title),
            6000);
        co_return;
    }

    api::MetaSummary s;
    s.imdbId = imdbId;
    s.kind = api::MediaKind::Series;
    s.title = item.title;
    s.year = item.year;
    s.poster = item.poster;
    s.description = item.overview;
    s.imdbRating = item.voteAverage;

    openFromSummary(s);
}

} // namespace kinema::controllers

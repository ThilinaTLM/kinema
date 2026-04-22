// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/MovieDetailController.h"

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
#include "ui/DetailPane.h"

#include <KLocalizedString>

namespace kinema::controllers {

MovieDetailController::MovieDetailController(
    api::CinemetaClient* cinemeta,
    api::TorrentioClient* torrentio,
    api::TmdbClient* tmdb,
    ui::DetailPane* pane,
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
}

void MovieDetailController::openFromSummary(const api::MetaSummary& summary)
{
    m_nav->goTo(Page::MovieDetail);
    auto t = loadTask(summary);
    Q_UNUSED(t);
}

void MovieDetailController::openFromDiscover(const api::DiscoverItem& item)
{
    auto t = openFromDiscoverTask(item);
    Q_UNUSED(t);
}

void MovieDetailController::openFromHistory(const api::HistoryEntry& entry)
{
    if (entry.key.imdbId.isEmpty()) {
        return;
    }
    // Synthesise a MetaSummary from the history row so the pane has
    // something to render immediately. The real Cinemeta meta will
    // overwrite the poster / description once it lands.
    api::MetaSummary s;
    s.imdbId = entry.key.imdbId;
    s.kind = api::MediaKind::Movie;
    s.title = entry.title;
    s.poster = entry.poster;
    openFromSummary(s);
}

void MovieDetailController::refetchCurrent()
{
    if (!m_current) {
        return;
    }
    auto t = loadTask(*m_current);
    Q_UNUSED(t);
}

void MovieDetailController::clear()
{
    ++m_epoch; // supersede any in-flight task
    m_current.reset();
}

QCoro::Task<void> MovieDetailController::loadTask(api::MetaSummary summary)
{
    const auto myEpoch = ++m_epoch;
    m_current = summary;

    m_pane->showMetaLoading();
    m_pane->showTorrentsLoading();

    try {
        auto detail = co_await m_cinemeta->meta(
            api::MediaKind::Movie, summary.imdbId);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_pane->setMeta(detail);
        m_pane->setSimilarContext(
            api::MediaKind::Movie, summary.imdbId);

        // Feed the history-aware playback context into the streams
        // panel so any Play action carries a well-formed key.
        api::PlaybackContext ctx;
        ctx.key.kind = api::MediaKind::Movie;
        ctx.key.imdbId = summary.imdbId;
        ctx.title = detail.summary.title.isEmpty()
            ? summary.title
            : detail.summary.title;
        ctx.poster = detail.summary.poster.isValid()
            ? detail.summary.poster
            : summary.poster;
        m_pane->setPlaybackContext(ctx);

        // Unreleased movies produce no useful Torrentio result.
        if (core::isFutureRelease(detail.summary.released)) {
            m_pane->showTorrentsUnreleased(*detail.summary.released);
            co_return;
        }

        auto opts = m_settings.torrentioOptions();
        opts.realDebridToken = m_rdToken;
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Movie, summary.imdbId, opts);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_pane->setTorrents(std::move(streams));

    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_pane->showTorrentsError(
            core::describeError(e, "movie detail/streams"));
    }
}

QCoro::Task<void> MovieDetailController::openFromDiscoverTask(
    api::DiscoverItem item)
{
    Q_EMIT statusMessage(
        i18nc("@info:status", "Looking up \u201c%1\u201d\u2026", item.title), 0);

    QString imdbId;
    try {
        imdbId = co_await m_tmdb->imdbIdForTmdbMovie(item.tmdbId);
    } catch (const std::exception& e) {
        if (const auto* he = core::asHttpError(e)) {
            const bool notFound = he->kind() == core::HttpError::Kind::HttpStatus
                && he->httpStatus() == 404;
            qCWarning(KINEMA).nospace()
                << "TMDB external_ids lookup failed: movie/"
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
                core::describeError(e, "movie detail/tmdb external_ids")),
            6000);
        co_return;
    }

    if (imdbId.isEmpty()) {
        qCWarning(KINEMA).nospace()
            << "TMDB has no IMDB id for movie/"
            << item.tmdbId << " (\"" << item.title << "\")";
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "\u201c%1\u201d has no IMDB id on TMDB \u2014 "
                "can't fetch streams.",
                item.title),
            6000);
        co_return;
    }

    // Build a MetaSummary mirroring what a search would have produced.
    api::MetaSummary s;
    s.imdbId = imdbId;
    s.kind = api::MediaKind::Movie;
    s.title = item.title;
    s.year = item.year;
    s.poster = item.poster;
    s.description = item.overview;
    s.imdbRating = item.voteAverage;

    openFromSummary(s);
}

} // namespace kinema::controllers

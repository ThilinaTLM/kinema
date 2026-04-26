// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MovieDetailViewModel.h"

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

#include <QPointer>

#include <algorithm>

namespace kinema::ui::qml {

namespace {

using SortMode = StreamsListModel::SortMode;

/// Resolution string ("1080p", "2160p", "—") to numeric height.
/// Larger sorts higher; missing/unknown becomes 0.
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

} // namespace

MovieDetailViewModel::MovieDetailViewModel(api::CinemetaClient* cinemeta,
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
    , m_similar(new DiscoverSectionModel(
          i18nc("@label movie detail rail", "More like this"), this))
{
    // Persisted filter inputs feed the same rebuild path the user-
    // facing "Cached on RD only" toggle does, so a settings change
    // during another phase reflows the visible list immediately.
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
        // RD token presence drives the cachedOnly checkbox visibility
        // (`realDebridConfigured`) and gates RD direct-URL columns.
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, [this](const QString&) {
                Q_EMIT realDebridConfiguredChanged();
                // The cached-only filter is meaningless without RD,
                // so re-render so it goes back to "everything".
                rebuildVisibleStreams();
            });
    }
}

MovieDetailViewModel::~MovieDetailViewModel() = default;

bool MovieDetailViewModel::cachedOnly() const
{
    return m_settings.torrentio().cachedOnly();
}

void MovieDetailViewModel::setCachedOnly(bool on)
{
    if (m_settings.torrentio().cachedOnly() == on) {
        return;
    }
    m_settings.torrentio().setCachedOnly(on);
    // The KConfig setter fires `cachedOnlyChanged`, which re-renders
    // through the connection above. We don't manually emit here.
}

void MovieDetailViewModel::setSortMode(int mode)
{
    const auto m = static_cast<SortMode>(mode);
    if (m_sortMode == m) {
        return;
    }
    m_sortMode = m;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void MovieDetailViewModel::setSortDescending(bool desc)
{
    if (m_sortDescending == desc) {
        return;
    }
    m_sortDescending = desc;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void MovieDetailViewModel::clear()
{
    ++m_epoch;
    ++m_similarEpoch;
    resetMeta();
    setMetaState(MetaState::Idle);
    m_streams->setIdle();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    m_similar->setItems({});
    setSimilarVisible(false);
}

void MovieDetailViewModel::resetMeta()
{
    m_imdbId.clear();
    m_title.clear();
    m_year = 0;
    m_posterUrl.clear();
    m_backdropUrl.clear();
    m_description.clear();
    m_genres.clear();
    m_cast.clear();
    m_runtimeMinutes = 0;
    m_rating = -1.0;
    m_isUpcoming = false;
    m_releaseDateText.clear();
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

void MovieDetailViewModel::setSimilarVisible(bool on)
{
    if (m_similarVisible == on) {
        return;
    }
    m_similarVisible = on;
    Q_EMIT similarChanged();
}

void MovieDetailViewModel::applyMeta(const api::MetaDetail& detail)
{
    const auto& s = detail.summary;
    m_imdbId = s.imdbId;
    m_title = s.title;
    m_year = s.year.value_or(0);
    m_posterUrl = s.poster.toString();
    m_backdropUrl = detail.background.toString();
    m_description = s.description;
    m_genres = detail.genres;
    m_cast = detail.cast;
    m_runtimeMinutes = detail.runtimeMinutes.value_or(0);
    m_rating = s.imdbRating.value_or(-1.0);
    m_isUpcoming = core::isFutureRelease(s.released);
    m_releaseDateText = (s.released && s.released->isValid())
        ? core::formatReleaseDate(*s.released)
        : QString();
    Q_EMIT metaChanged();
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

    api::MetaDetail detail;
    try {
        detail = co_await m_cinemeta->meta(api::MediaKind::Movie, imdbId);
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
    auto similarTask = loadSimilarFor(imdbId, api::MediaKind::Movie);
    Q_UNUSED(similarTask);

    // Future releases produce no useful Torrentio result; surface the
    // release date and stop.
    if (m_isUpcoming && detail.summary.released) {
        m_streams->setUnreleased(*detail.summary.released);
        co_return;
    }

    try {
        auto opts = m_settings.torrentioOptions();
        opts.realDebridToken = m_rdToken;
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Movie, imdbId, opts);
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_rawStreams = std::move(streams);
        Q_EMIT rawStreamsCountChanged();
        rebuildVisibleStreams();
    } catch (const std::exception& e) {
        if (myEpoch != m_epoch) {
            co_return;
        }
        m_streams->setError(
            core::describeError(e, "movie detail/streams"));
    }
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
            qCWarning(KINEMA).nospace()
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
        qCWarning(KINEMA).nospace()
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

QCoro::Task<void> MovieDetailViewModel::loadSimilarFor(QString imdbId,
    api::MediaKind kind)
{
    const auto myEpoch = ++m_similarEpoch;

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
        // Failure to resolve the TMDB id is silent: hide the rail
        // rather than painting an error box for a secondary surface.
        Q_UNUSED(core::describeError(e, "movie detail/similar/find"));
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
            "movie detail/similar/recommendations"));
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

void MovieDetailViewModel::rebuildVisibleStreams()
{
    // Filter / sort / settings changes are meaningful only when we
    // already have raw rows to operate on. With an empty raw list,
    // hold whatever placeholder the model is showing (Loading,
    // Unreleased, Error, or Idle) instead of clobbering it with an
    // unsolicited Empty.
    if (m_rawStreams.isEmpty()) {
        return;
    }

    auto visible = applyFilters();
    sortInPlace(visible);

    QString emptyExplanation;
    if (visible.isEmpty() && !m_rawStreams.isEmpty()) {
        const bool cachedFiltered
            = realDebridConfigured() && cachedOnly();
        emptyExplanation = cachedFiltered
            ? i18nc("@info streams empty",
                "Uncheck \u201cCached on Real-Debrid only\u201d or "
                "widen your filters in Settings.")
            : i18nc("@info streams empty",
                "Loosen the exclusions or keyword blocklist in "
                "Settings.");
    } else if (visible.isEmpty()) {
        emptyExplanation = i18nc("@info streams empty",
            "Try a different release or widen your filters.");
    }

    m_streams->setItems(std::move(visible), emptyExplanation);
}

QList<api::Stream> MovieDetailViewModel::applyFilters() const
{
    if (m_rawStreams.isEmpty()) {
        return {};
    }
    core::stream_filter::ClientFilters f;
    f.cachedOnly = realDebridConfigured() && cachedOnly();
    f.keywordBlocklist = m_settings.filter().keywordBlocklist();
    return core::stream_filter::apply(m_rawStreams, f);
}

void MovieDetailViewModel::sortInPlace(QList<api::Stream>& rows) const
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

api::PlaybackContext MovieDetailViewModel::currentContext() const
{
    api::PlaybackContext ctx;
    ctx.key.kind = api::MediaKind::Movie;
    ctx.key.imdbId = m_imdbId;
    ctx.title = m_title;
    ctx.poster = QUrl(m_posterUrl);
    return ctx;
}

void MovieDetailViewModel::playBest()
{
    if (m_streams->rowCount() == 0) {
        Q_EMIT statusMessage(
            i18nc("@info:status",
                "No streams available to play."),
            4000);
        return;
    }
    play(0);
}

void MovieDetailViewModel::play(int row)
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

void MovieDetailViewModel::copyMagnet(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->copyMagnet(*s);
    }
}

void MovieDetailViewModel::openMagnet(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->openMagnet(*s);
    }
}

void MovieDetailViewModel::copyDirectUrl(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->copyDirectUrl(*s);
    }
}

void MovieDetailViewModel::openDirectUrl(int row)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        m_actions->openDirectUrl(*s);
    }
}

void MovieDetailViewModel::requestSubtitles()
{
    Q_EMIT subtitlesRequested(currentContext());
}

void MovieDetailViewModel::requestSubtitlesFor(int row)
{
    auto ctx = currentContext();
    if (const auto* s = m_streams->at(row)) {
        ctx.streamRef = api::HistoryStreamRef::fromStream(*s);
    }
    Q_EMIT subtitlesRequested(ctx);
}

void MovieDetailViewModel::activateSimilar(int row)
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

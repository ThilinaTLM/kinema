// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/MovieDetailViewModel.h"

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
#include "services/StreamActions.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/StreamSorting.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {
using SortMode = StreamsListModel::SortMode;
} // namespace

MovieDetailViewModel::MovieDetailViewModel(api::CinemetaClient* cinemeta,
    api::TorrentioClient* torrentio,
    api::TmdbClient* tmdb,
    services::StreamActions* actions,
    controllers::TokenController* tokens,
    config::AppSettings& settings,
    const QString& rdTokenRef,
    QObject* parent)
    : MovieDetailViewModel(cinemeta, torrentio, tmdb, actions,
          /*library=*/nullptr, /*watched=*/nullptr,
          tokens, settings, rdTokenRef, parent)
{
}

MovieDetailViewModel::MovieDetailViewModel(api::CinemetaClient* cinemeta,
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
    , m_similar(new DiscoverSectionModel(
          i18nc("@label movie detail rail", "More like this"), this))
{
    connect(&m_settings.filter(),
        &config::FilterSettings::keywordBlocklistChanged, this,
        [this](const QStringList&) { rebuildVisibleStreams(); });

    if (m_library) {
        connect(m_library, &controllers::LibraryController::changed,
            this, &MovieDetailViewModel::refreshLibraryState);
    }
    if (m_watched) {
        connect(m_watched, &controllers::WatchedController::changed,
            this, &MovieDetailViewModel::refreshWatchedState);
    }

    if (m_tokens) {
        // RD token presence drives the `realDebridConfigured` chip
        // visibility on the streams page and routing inside the
        // download manager (RD-first when configured).
        connect(m_tokens,
            &controllers::TokenController::realDebridTokenChanged,
            this, [this](const QString&) {
                Q_EMIT realDebridConfiguredChanged();
                refreshStreamsForCurrentTitle();
            });
    }
}

MovieDetailViewModel::~MovieDetailViewModel() = default;

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

void MovieDetailViewModel::setUiResolutionFilter(const QString& res)
{
    if (m_uiResolutionFilter == res) {
        return;
    }
    m_uiResolutionFilter = res;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void MovieDetailViewModel::setUiHdrOnly(bool on)
{
    if (m_uiHdrOnly == on) {
        return;
    }
    m_uiHdrOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void MovieDetailViewModel::setUiDolbyVisionOnly(bool on)
{
    if (m_uiDolbyVisionOnly == on) {
        return;
    }
    m_uiDolbyVisionOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void MovieDetailViewModel::setUiMultiAudioOnly(bool on)
{
    if (m_uiMultiAudioOnly == on) {
        return;
    }
    m_uiMultiAudioOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

bool MovieDetailViewModel::uiAnyFilterActive() const noexcept
{
    return !m_uiResolutionFilter.isEmpty()
        || m_uiHdrOnly || m_uiDolbyVisionOnly || m_uiMultiAudioOnly;
}

void MovieDetailViewModel::clearUiFilters()
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
    // Transient UI filters are page-scoped: a fresh title starts
    // with no chips active.
    if (uiAnyFilterActive()) {
        m_uiResolutionFilter.clear();
        m_uiHdrOnly = false;
        m_uiDolbyVisionOnly = false;
        m_uiMultiAudioOnly = false;
        Q_EMIT uiFiltersChanged();
    }
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
    m_currentMeta = detail;
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
    refreshLibraryState();
    refreshWatchedState();
}

void MovieDetailViewModel::refreshLibraryState()
{
    const bool inLibrary = m_library && !m_imdbId.isEmpty()
        && m_library->isInLibrary(api::MediaKind::Movie, m_imdbId);
    if (m_inLibrary == inLibrary) {
        return;
    }
    m_inLibrary = inLibrary;
    Q_EMIT libraryStateChanged();
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

QString MovieDetailViewModel::libraryActionText() const
{
    return m_inLibrary
        ? i18nc("@action:button", "Remove from Library")
        : i18nc("@action:button", "Add to Library");
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
        // Torrentio is discovery-only — RD is no longer in the URL.
        auto opts = m_settings.torrentioOptions();
        auto streams = co_await m_torrentio->streams(
            api::MediaKind::Movie, imdbId, opts);
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
            qCWarning(KINEMA_APP).nospace()
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
        qCWarning(KINEMA_APP).nospace()
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
        emptyExplanation = i18nc("@info streams empty",
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
    f.keywordBlocklist = m_settings.filter().keywordBlocklist();
    auto rows = core::stream_filter::apply(m_rawStreams, f);

    return stream_sorting::applyUiFilters(std::move(rows),
        { m_uiResolutionFilter, m_uiHdrOnly,
            m_uiDolbyVisionOnly, m_uiMultiAudioOnly });
}

void MovieDetailViewModel::sortInPlace(QList<api::Stream>& rows) const
{
    stream_sorting::sortInPlace(rows, m_sortMode, m_sortDescending);
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

void MovieDetailViewModel::requestStreams()
{
    Q_EMIT streamsRequested();
}

void MovieDetailViewModel::addToLibrary()
{
    if (!m_library || m_currentMeta.summary.imdbId.isEmpty()) {
        return;
    }
    m_library->saveMovie(m_currentMeta);
}

void MovieDetailViewModel::removeFromLibrary()
{
    if (m_library && !m_imdbId.isEmpty()) {
        m_library->removeFromLibrary(api::MediaKind::Movie, m_imdbId);
    }
}

void MovieDetailViewModel::toggleMovieWatched()
{
    if (m_watched && !m_imdbId.isEmpty()) {
        m_watched->setMovieWatched(m_imdbId, !m_movieWatched);
    }
}

void MovieDetailViewModel::playNow(int row)
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
    if (!m_actions) {
        return;
    }
    m_actions->play(*s, currentContext());
}

void MovieDetailViewModel::download(int row)
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
        i18nc("@info:status starting a background download",
            "Downloading \u201c%1\u201d\u2026", m_title),
        3500);
}

void MovieDetailViewModel::downloadWithBackend(int row, int backendKind)
{
    const auto* s = m_streams->at(row);
    if (!s || !m_downloads) {
        return;
    }
    m_downloads->downloadWithBackend(*s, currentContext(),
        static_cast<api::DownloadBackendKind>(backendKind));
}

void MovieDetailViewModel::setDownloadController(
    controllers::DownloadController* dl)
{
    m_downloads = dl;
}

template <typename Method>
void MovieDetailViewModel::dispatchStreamAction(int row, Method method)
{
    if (!m_actions) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        (m_actions->*method)(*s);
    }
}

void MovieDetailViewModel::copyMagnet(int row)
{
    dispatchStreamAction(row, &services::StreamActions::copyMagnet);
}

void MovieDetailViewModel::openMagnet(int row)
{
    dispatchStreamAction(row, &services::StreamActions::openMagnet);
}

void MovieDetailViewModel::copyDirectUrl(int row)
{
    dispatchStreamAction(row, &services::StreamActions::copyDirectUrl);
}

void MovieDetailViewModel::openDirectUrl(int row)
{
    dispatchStreamAction(row, &services::StreamActions::openDirectUrl);
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

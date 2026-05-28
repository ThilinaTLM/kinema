// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DetailViewModelBase.h"

#include "api/TmdbClient.h"
#include "config/AppSettings.h"
#include "config/FilterSettings.h"
#include "controllers/DownloadController.h"
#include "controllers/LibraryController.h"
#include "controllers/StreamUtilityController.h"
#include "controllers/WatchedController.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/util/DateFormat.h"
#include "core/util/StreamFilter.h"
#include "playback/session/PlaybackSessionManager.h"
#include "ui/qml-bridge/DiscoverSectionModel.h"
#include "ui/qml-bridge/StreamSorting.h"
#include "ui/qml-bridge/TitleActions.h"

#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {
using SortMode = StreamsListModel::SortMode;
} // namespace

DetailViewModelBase::DetailViewModelBase(api::CinemetaClient* cinemeta,
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
    domain::MediaKind kind,
    const QString& similarRailLabel,
    QObject* parent)
    : QObject(parent)
    , m_cinemeta(cinemeta)
    , m_indexers(indexers)
    , m_tmdb(tmdb)
    , m_playback(playback)
    , m_streamUtility(streamUtility)
    , m_library(library)
    , m_watched(watched)
    , m_tokens(tokens)
    , m_settings(settings)
    , m_rdToken(rdTokenRef)
    , m_adApiKey(adApiKeyRef)
    , m_streams(new StreamsListModel(this))
    , m_similar(new DiscoverSectionModel(similarRailLabel, this))
{
    m_streams->setMediaKind(kind);
    connect(&m_settings.filter(),
        &config::FilterSettings::keywordBlocklistChanged, this,
        [this](const QStringList&) { rebuildVisibleStreams(); });
    connect(&m_settings.filter(),
        &config::FilterSettings::exclusionsChanged, this,
        [this]() { rebuildVisibleStreams(); });

    if (m_library) {
        connect(m_library, &controllers::LibraryController::changed,
            this, &DetailViewModelBase::refreshLibraryState);
    }
}

DetailViewModelBase::~DetailViewModelBase() = default;

void DetailViewModelBase::setSortMode(int mode)
{
    const auto m = static_cast<SortMode>(mode);
    if (m_sortMode == m) {
        return;
    }
    m_sortMode = m;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void DetailViewModelBase::setSortDescending(bool desc)
{
    if (m_sortDescending == desc) {
        return;
    }
    m_sortDescending = desc;
    Q_EMIT sortChanged();
    rebuildVisibleStreams();
}

void DetailViewModelBase::setUiResolutionFilter(const QString& res)
{
    if (m_uiResolutionFilter == res) {
        return;
    }
    m_uiResolutionFilter = res;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void DetailViewModelBase::setUiHdrOnly(bool on)
{
    if (m_uiHdrOnly == on) {
        return;
    }
    m_uiHdrOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void DetailViewModelBase::setUiDolbyVisionOnly(bool on)
{
    if (m_uiDolbyVisionOnly == on) {
        return;
    }
    m_uiDolbyVisionOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

void DetailViewModelBase::setUiMultiAudioOnly(bool on)
{
    if (m_uiMultiAudioOnly == on) {
        return;
    }
    m_uiMultiAudioOnly = on;
    Q_EMIT uiFiltersChanged();
    rebuildVisibleStreams();
}

bool DetailViewModelBase::uiAnyFilterActive() const noexcept
{
    return !m_uiResolutionFilter.isEmpty()
        || m_uiHdrOnly || m_uiDolbyVisionOnly || m_uiMultiAudioOnly;
}

void DetailViewModelBase::clearUiFilters()
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

void DetailViewModelBase::assignCommonMeta(const domain::MetaDetail& detail)
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
    m_rating = s.imdbRating.value_or(-1.0);
    m_releaseDateText = (s.released && s.released->isValid())
        ? core::formatReleaseDate(*s.released)
        : QString();
}

void DetailViewModelBase::clearCommonMeta()
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
}

void DetailViewModelBase::refreshLibraryState()
{
    const bool inLibrary = m_library && !m_imdbId.isEmpty()
        && m_library->isInLibrary(mediaKind(), m_imdbId);
    if (m_inLibrary == inLibrary) {
        return;
    }
    m_inLibrary = inLibrary;
    Q_EMIT libraryStateChanged();
}

QString DetailViewModelBase::libraryActionText() const
{
    return m_inLibrary
        ? i18nc("@action:button", "Remove from Library")
        : i18nc("@action:button", "Add to Library");
}

void DetailViewModelBase::removeFromLibrary()
{
    if (m_library && !m_imdbId.isEmpty()) {
        m_library->removeFromLibrary(mediaKind(), m_imdbId);
    }
}

void DetailViewModelBase::setSimilarVisible(bool on)
{
    if (m_similarVisible == on) {
        return;
    }
    m_similarVisible = on;
    Q_EMIT similarChanged();
}

void DetailViewModelBase::resetStreamsAndFilters()
{
    m_streams->setIdle();
    m_rawStreams.clear();
    Q_EMIT rawStreamsCountChanged();
    m_similar->setItems({});
    setSimilarVisible(false);
    // Transient UI filters are page-scoped: a fresh title starts with
    // no chips active.
    if (uiAnyFilterActive()) {
        m_uiResolutionFilter.clear();
        m_uiHdrOnly = false;
        m_uiDolbyVisionOnly = false;
        m_uiMultiAudioOnly = false;
        Q_EMIT uiFiltersChanged();
    }
}

void DetailViewModelBase::rebuildVisibleStreams()
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
    if (visible.isEmpty()) {
        emptyExplanation = i18nc("@info streams empty",
            "Loosen the exclusions or keyword blocklist in "
            "Settings.");
    }

    m_streams->setItems(std::move(visible), emptyExplanation);
}

QList<domain::Stream> DetailViewModelBase::applyFilters() const
{
    if (m_rawStreams.isEmpty()) {
        return {};
    }
    core::stream_filter::ClientFilters f;
    f.keywordBlocklist = m_settings.filter().keywordBlocklist();
    f.excludedResolutions = m_settings.filter().excludedResolutions();
    f.excludedCategories = m_settings.filter().excludedCategories();
    auto rows = core::stream_filter::apply(m_rawStreams, f);

    return stream_sorting::applyUiFilters(std::move(rows),
        { m_uiResolutionFilter, m_uiHdrOnly,
            m_uiDolbyVisionOnly, m_uiMultiAudioOnly });
}

void DetailViewModelBase::sortInPlace(QList<domain::Stream>& rows) const
{
    stream_sorting::sortInPlace(rows, m_sortMode, m_sortDescending);
}

QCoro::Task<void> DetailViewModelBase::loadSimilarFor(QString imdbId,
    domain::MediaKind kind)
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
        Q_UNUSED(core::describeError(e, "detail/similar/find"));
        m_similar->setItems({});
        setSimilarVisible(false);
        co_return;
    }

    QList<domain::DiscoverItem> items;
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
            "detail/similar/recommendations"));
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

void DetailViewModelBase::setDownloadController(
    controllers::DownloadController* dl)
{
    m_downloads = dl;
}

void DetailViewModelBase::playNow(int row)
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
    if (!m_playback) {
        return;
    }
    m_playback->play(*s, currentContext());
}

void DetailViewModelBase::playWithBackend(int row, int backendKind)
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
    if (!m_playback) {
        return;
    }
    m_playback->playWithBackend(*s, currentContext(),
        static_cast<domain::DownloadBackendKind>(backendKind));
}

void DetailViewModelBase::download(int row)
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
            "Downloading \u201c%1\u201d\u2026", currentContext().title),
        3500);
}

void DetailViewModelBase::downloadWithBackend(int row, int backendKind)
{
    const auto* s = m_streams->at(row);
    if (!s || !m_downloads) {
        return;
    }
    m_downloads->downloadWithBackend(*s, currentContext(),
        static_cast<domain::DownloadBackendKind>(backendKind));
}

template <typename Method>
void DetailViewModelBase::dispatchStreamAction(int row, Method method)
{
    if (!m_streamUtility) {
        return;
    }
    if (const auto* s = m_streams->at(row)) {
        (m_streamUtility->*method)(*s);
    }
}

void DetailViewModelBase::copyMagnet(int row)
{
    dispatchStreamAction(row, &controllers::StreamUtilityController::copyMagnet);
}

void DetailViewModelBase::openMagnet(int row)
{
    dispatchStreamAction(row, &controllers::StreamUtilityController::openMagnet);
}

void DetailViewModelBase::copyDirectUrl(int row)
{
    dispatchStreamAction(row, &controllers::StreamUtilityController::copyDirectUrl);
}

void DetailViewModelBase::openDirectUrl(int row)
{
    dispatchStreamAction(row, &controllers::StreamUtilityController::openDirectUrl);
}

void DetailViewModelBase::copyReleaseName(int row)
{
    dispatchStreamAction(row, &controllers::StreamUtilityController::copyReleaseName);
}

void DetailViewModelBase::requestSubtitles()
{
    Q_EMIT subtitlesRequested(currentContext());
}

void DetailViewModelBase::requestSubtitlesFor(int row)
{
    auto ctx = currentContext();
    if (const auto* s = m_streams->at(row)) {
        ctx.streamRef = domain::HistoryStreamRef::fromStream(*s);
    }
    Q_EMIT subtitlesRequested(ctx);
}

void DetailViewModelBase::activateSimilar(int row)
{
    const auto* item = m_similar->itemAt(row);
    if (!item) {
        return;
    }
    if (item->kind == domain::MediaKind::Series) {
        Q_EMIT openSeriesByTmdbRequested(item->tmdbId, item->title);
    } else {
        Q_EMIT openMovieByTmdbRequested(item->tmdbId, item->title);
    }
}

void DetailViewModelBase::addSimilarToLibrary(int row)
{
    const auto* item = m_similar->itemAt(row);
    if (!item) {
        return;
    }
    auto task = title_actions::addToLibraryByTmdb(m_tmdb,
        m_library, this, item->tmdbId, item->kind, item->title);
    Q_UNUSED(task);
}

void DetailViewModelBase::markSimilarWatched(int row)
{
    const auto* item = m_similar->itemAt(row);
    if (!item) {
        return;
    }
    auto task = title_actions::markWatchedByTmdb(m_tmdb,
        m_watched, this, item->tmdbId, item->kind, item->title);
    Q_UNUSED(task);
}

void DetailViewModelBase::findSimilarStreams(int row)
{
    const auto* item = m_similar->itemAt(row);
    if (!item || item->tmdbId <= 0) {
        return;
    }
    if (item->kind == domain::MediaKind::Series) {
        Q_EMIT findSeriesStreamsByTmdbRequested(
            item->tmdbId, item->title);
    } else {
        Q_EMIT findMovieStreamsByTmdbRequested(
            item->tmdbId, item->title);
    }
}

} // namespace kinema::ui::qml

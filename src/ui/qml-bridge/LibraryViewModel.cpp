// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/LibraryViewModel.h"

#include "controllers/LibraryController.h"
#include "controllers/WatchedController.h"
#include "core/util/DateFormat.h"

#include <KLocalizedString>

#include <QDate>
#include <QVariantMap>

#include <algorithm>

namespace kinema::ui::qml {

namespace {

QString posterString(const domain::LibraryTitle& t)
{
    return t.poster.isValid() ? t.poster.toString() : QString();
}

/// 16:9 hero artwork URL of the parent title — same image used
/// behind the detail-page header. Used by the smart rails as the
/// preferred fallback for episode cards whose own thumbnail is
/// missing (typical for unaired Airing Soon entries) so the card
/// stays at frame aspect instead of letterboxing a 2:3 poster.
QString backdropString(const domain::LibraryTitle& t)
{
    return t.backdrop.isValid() ? t.backdrop.toString() : QString();
}

/// Returns the episode's own thumbnail URL, or an empty string
/// when the source didn't carry one. The QML side then walks
/// `backdropUrl → posterUrl → fallback icon` so card heights
/// stay uniform across the rail.
QString thumbnailString(const domain::LibraryEpisode& ep)
{
    return ep.thumbnail.isValid() ? ep.thumbnail.toString() : QString();
}

QString episodeCode(int season, int episode)
{
    return QStringLiteral("S%1E%2")
        .arg(season, 2, 10, QLatin1Char('0'))
        .arg(episode, 2, 10, QLatin1Char('0'));
}

bool isFuture(const std::optional<QDate>& date)
{
    return core::isFutureRelease(date);
}

QString releaseText(const std::optional<QDate>& date)
{
    return date && date->isValid() ? core::formatReleaseDate(*date) : QString();
}

/// Resolve the Browse-style date-window index to a lower-bound date.
/// Returns std::nullopt when the window is "Any time" \u2014 callers
/// should skip the filter.
std::optional<QDate> dateWindowFloor(int idx)
{
    const auto today = QDate::currentDate();
    switch (idx) {
    case LibraryViewModel::DateWindowPastMonth:
        return today.addMonths(-1);
    case LibraryViewModel::DateWindowPast3Months:
        return today.addMonths(-3);
    case LibraryViewModel::DateWindowThisYear:
        return QDate(today.year(), 1, 1);
    case LibraryViewModel::DateWindowPast3Years:
        return today.addYears(-3);
    case LibraryViewModel::DateWindowAny:
    default:
        return std::nullopt;
    }
}

constexpr int kAiringSoonEpisodeHorizonDays = 30;
constexpr int kRecentlyAddedLimit = 20;

} // namespace

LibraryViewModel::LibraryViewModel(
    controllers::LibraryController* library,
    controllers::WatchedController* watched,
    QObject* parent)
    : QObject(parent)
    , m_library(library)
    , m_watched(watched)
    , m_model(new LibraryListModel(this))
    , m_upNextModel(new LibraryRailModel(this))
    , m_airingSoonModel(new LibraryRailModel(this))
    , m_recentlyAddedModel(new LibraryRailModel(this))
{
    if (m_library) {
        connect(m_library, &controllers::LibraryController::changed,
            this, &LibraryViewModel::refresh);
        connect(m_library, &controllers::LibraryController::statusMessage,
            this, &LibraryViewModel::statusMessage);
    }
    if (m_watched) {
        // Watched-state flips elsewhere (a detail-page mark, a finished
        // playback) feed back into the grid badges, the Continue
        // bucketing for sort, and the Up Next rail.
        connect(m_watched, &controllers::WatchedController::changed,
            this, &LibraryViewModel::refresh);
    }
    refresh();
}

LibraryViewModel::~LibraryViewModel() = default;

// -- properties --------------------------------------------------------

bool LibraryViewModel::filtersActive() const noexcept
{
    return m_kind != KindFilter::All
        || m_status != StatusFilter::All
        || !m_genreIds.isEmpty()
        || m_sort != SortMode::RecentlyAdded
        || m_dateWindow != DateWindowAny
        || m_minRatingPct > 0
        || m_hideWatched;
}

void LibraryViewModel::setKind(KindFilter v)
{
    if (m_kind == v) return;
    m_kind = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setStatus(StatusFilter v)
{
    if (m_status == v) return;
    m_status = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setGenreIds(const QList<int>& v)
{
    if (m_genreIds == v) return;
    m_genreIds = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setSort(SortMode v)
{
    if (m_sort == v) return;
    m_sort = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setDateWindow(int v)
{
    if (m_dateWindow == v) return;
    m_dateWindow = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setMinRatingPct(int v)
{
    if (m_minRatingPct == v) return;
    m_minRatingPct = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

void LibraryViewModel::setHideWatched(bool v)
{
    if (m_hideWatched == v) return;
    m_hideWatched = v;
    Q_EMIT filtersChanged();
    rebuildGrid();
}

// -- public slots ------------------------------------------------------

void LibraryViewModel::refresh()
{
    buildEntries();
    rebuildAvailableGenres();
    rebuildRails();
    rebuildGrid();
}

void LibraryViewModel::resetFilters()
{
    bool changed = false;
    if (m_kind != KindFilter::All) { m_kind = KindFilter::All; changed = true; }
    if (m_status != StatusFilter::All) { m_status = StatusFilter::All; changed = true; }
    if (!m_genreIds.isEmpty()) { m_genreIds.clear(); changed = true; }
    if (m_sort != SortMode::RecentlyAdded) { m_sort = SortMode::RecentlyAdded; changed = true; }
    if (m_dateWindow != DateWindowAny) { m_dateWindow = DateWindowAny; changed = true; }
    if (m_minRatingPct != 0) { m_minRatingPct = 0; changed = true; }
    if (m_hideWatched) { m_hideWatched = false; changed = true; }
    if (changed) {
        Q_EMIT filtersChanged();
        rebuildGrid();
    }
}

void LibraryViewModel::activate(int row)
{
    const auto* r = m_model->at(row);
    if (!r) return;
    if (r->kind == domain::MediaKind::Movie) {
        Q_EMIT openMovieRequested(r->imdbId, r->title);
        return;
    }
    if (r->season && r->episode) {
        Q_EMIT openSeriesEpisodeRequested(
            r->imdbId, r->title, *r->season, *r->episode);
        return;
    }
    Q_EMIT openSeriesRequested(r->imdbId, r->title);
}

void LibraryViewModel::resume(int row)
{
    const auto* r = m_model->at(row);
    if (!r || !r->resumeEntry) {
        activate(row);
        return;
    }
    Q_EMIT resumeRequested(*r->resumeEntry);
}

void LibraryViewModel::removeFromLibrary(int row)
{
    const auto* r = m_model->at(row);
    if (!r || !m_library) return;
    m_library->removeFromLibrary(r->kind, r->imdbId);
}

void LibraryViewModel::toggleWatched(int row)
{
    const auto* r = m_model->at(row);
    if (!r || !m_watched) return;
    if (r->kind == domain::MediaKind::Movie) {
        const bool watched = m_watched->isMovieWatched(r->imdbId);
        m_watched->setMovieWatched(r->imdbId, !watched);
        return;
    }
    QList<QPair<int, int>> pairs;
    if (m_library) {
        const auto eps = m_library->episodesForSeries(r->imdbId);
        pairs.reserve(eps.size());
        for (const auto& ep : eps) {
            if (ep.season <= 0) continue;
            pairs.append({ ep.season, ep.episode });
        }
    }
    m_watched->setEpisodesWatched(r->imdbId, pairs, !r->watched);
}

void LibraryViewModel::activateRail(const QString& railId, int row)
{
    LibraryRailModel* m = nullptr;
    if (railId == QLatin1String("upNext")) m = m_upNextModel;
    else if (railId == QLatin1String("airingSoon")) m = m_airingSoonModel;
    else if (railId == QLatin1String("recentlyAdded")) m = m_recentlyAddedModel;
    if (!m) return;
    const auto* r = m->at(row);
    if (!r) return;
    const bool isUpNext = railId == QLatin1String("upNext");
    if (r->kind == domain::MediaKind::Movie) {
        // Resume rail: skip the detail page and go straight to
        // streams. Airing Soon / Recently Added open the detail
        // page as before.
        if (isUpNext) {
            Q_EMIT openMovieStreamsRequested(r->imdbId, r->title);
        } else {
            Q_EMIT openMovieRequested(r->imdbId, r->title);
        }
        return;
    }
    if (r->season && r->episode && isUpNext) {
        // Up Next episodes have aired -- jump straight into streams.
        Q_EMIT openSeriesEpisodeStreamsRequested(
            r->imdbId, r->title, *r->season, *r->episode);
        return;
    }
    // Airing Soon rows reference unaired episodes whose streams will
    // be empty; recentlyAdded is just "open the title". In both
    // cases the safe action is to open the series page.
    Q_EMIT openSeriesRequested(r->imdbId, r->title);
}

// -- entry construction -----------------------------------------------

void LibraryViewModel::buildEntries()
{
    m_entries.clear();
    if (!m_library) {
        m_totalCount = 0;
        Q_EMIT libraryEmptyChanged();
        return;
    }

    const auto titles = m_library->titles();
    m_entries.reserve(titles.size());

    for (const auto& t : titles) {
        Entry e;
        e.title = t;

        if (t.kind == domain::MediaKind::Movie) {
            const bool watched = m_watched
                && m_watched->isMovieWatched(t.imdbId);
            const auto resume = m_watched
                ? m_watched->resumeEntryForMovie(t.imdbId)
                : std::optional<domain::HistoryEntry>{};
            const double progress = m_watched
                ? m_watched->movieProgress(t.imdbId) : -1.0;
            e.resume = resume;
            e.resumeProgress = progress;

            if (resume) {
                e.status = ResolvedStatus::Continue;
            } else if (isFuture(t.releaseDate)) {
                e.status = ResolvedStatus::Upcoming;
            } else if (watched) {
                e.status = ResolvedStatus::Watched;
            } else {
                e.status = ResolvedStatus::ToWatch;
            }
            m_entries.append(std::move(e));
            continue;
        }

        // Series: scan episode list once to derive every signal.
        e.episodes = m_library->episodesForSeries(t.imdbId);
        for (const auto& ep : e.episodes) {
            if (ep.season <= 0) continue; // skip specials
            if (isFuture(ep.releaseDate)) {
                ++e.upcomingCount;
                if (!e.firstUpcoming
                    || (ep.releaseDate && e.firstUpcoming->releaseDate
                        && *ep.releaseDate < *e.firstUpcoming->releaseDate)) {
                    e.firstUpcoming = &ep;
                }
                continue;
            }
            ++e.totalAired;
            const bool epWatched = m_watched
                && m_watched->isEpisodeWatched(
                    t.imdbId, ep.season, ep.episode);
            if (epWatched) {
                ++e.watchedAired;
            } else if (!e.nextToWatch) {
                e.nextToWatch = &ep;
            }
            const auto resume = m_watched
                ? m_watched->resumeEntryForEpisode(
                    t.imdbId, ep.season, ep.episode)
                : std::optional<domain::HistoryEntry>{};
            if (resume && (!e.resume
                    || resume->lastWatchedAt > e.resume->lastWatchedAt)) {
                e.resume = resume;
                e.resumeProgress = resume->progressFraction();
            }
        }

        // Status precedence: Continue (any active resume) >
        // Upcoming (zero aired episodes, only future) > Watched (all
        // aired watched) > ToWatch.
        if (e.resume) {
            e.status = ResolvedStatus::Continue;
        } else if (e.totalAired == 0 && e.upcomingCount > 0) {
            e.status = ResolvedStatus::Upcoming;
        } else if (e.totalAired > 0 && e.watchedAired == e.totalAired) {
            e.status = ResolvedStatus::Watched;
        } else {
            e.status = ResolvedStatus::ToWatch;
        }
        m_entries.append(std::move(e));
    }

    const int prev = m_totalCount;
    m_totalCount = static_cast<int>(m_entries.size());
    if (prev != m_totalCount) {
        Q_EMIT libraryEmptyChanged();
    }
}

void LibraryViewModel::rebuildAvailableGenres()
{
    QHash<QString, int> byName;
    QStringList ordered;
    for (const auto& e : m_entries) {
        for (const auto& g : e.title.genres) {
            const auto trimmed = g.trimmed();
            if (trimmed.isEmpty() || byName.contains(trimmed)) continue;
            byName.insert(trimmed, 0); // placeholder, fixed below
            ordered.append(trimmed);
        }
    }
    std::sort(ordered.begin(), ordered.end(),
        [](const QString& a, const QString& b) {
            return QString::localeAwareCompare(a, b) < 0;
        });

    QVariantList list;
    list.reserve(ordered.size());
    int next = 0;
    for (const auto& name : ordered) {
        QVariantMap m;
        m.insert(QStringLiteral("id"), next);
        m.insert(QStringLiteral("name"), name);
        list.append(m);
        byName.insert(name, next);
        ++next;
    }

    // Drop selected genre IDs that no longer correspond to any
    // surviving genre name; otherwise the chip strip would carry
    // stale selections.
    if (!m_genreIds.isEmpty()) {
        QList<int> kept;
        kept.reserve(m_genreIds.size());
        for (const int id : m_genreIds) {
            if (id >= 0 && id < next) kept.append(id);
        }
        if (kept != m_genreIds) {
            m_genreIds = kept;
            // No filtersChanged emit here \u2014 the grid is about to
            // rebuild anyway in refresh().
        }
    }

    if (m_availableGenres != list) {
        m_availableGenres = list;
        m_genreIdByName = byName;
        Q_EMIT availableGenresChanged();
    } else {
        m_genreIdByName = byName;
    }
}

// -- rails ------------------------------------------------------------

void LibraryViewModel::rebuildRails()
{
    QList<LibraryRailRow> upNext;
    QList<LibraryRailRow> airingOrComing;
    QList<LibraryRailRow> recentlyAdded;

    const auto today = QDate::currentDate();
    const auto episodeHorizon
        = today.addDays(kAiringSoonEpisodeHorizonDays);

    for (const auto& e : m_entries) {
        const auto& t = e.title;

        // "Airing soon" merges upcoming episodes (within the
        // ~30-day horizon) and saved movies with any future
        // release date. Sort is ascending by release date so a
        // single chronological strip surfaces what's next.
        if (t.kind == domain::MediaKind::Movie) {
            if (t.releaseDate && *t.releaseDate > today) {
                LibraryRailRow row;
                row.kind = domain::MediaKind::Movie;
                row.imdbId = t.imdbId;
                row.title = t.title;
                row.posterUrl = posterString(t);
                row.backdropUrl = backdropString(t);
                row.primaryLine = t.title;
                row.tertiaryLine = i18nc(
                    "@info library airing-soon rail, movie release date",
                    "Releases %1", releaseText(t.releaseDate));
                row.releaseDate = t.releaseDate;
                airingOrComing.append(std::move(row));
            }
        } else if (e.firstUpcoming
            && e.firstUpcoming->releaseDate
            && *e.firstUpcoming->releaseDate <= episodeHorizon) {
            const auto& ep = *e.firstUpcoming;
            LibraryRailRow row;
            row.kind = domain::MediaKind::Series;
            row.imdbId = t.imdbId;
            row.title = t.title;
            row.season = ep.season;
            row.episode = ep.episode;
            row.posterUrl = posterString(t);
            row.backdropUrl = backdropString(t);
            row.thumbnailUrl = thumbnailString(ep);
            const auto code = episodeCode(ep.season, ep.episode);
            row.primaryLine = t.title;
            row.secondaryLine = ep.title.isEmpty()
                ? code
                : i18nc(
                    "@info library rail, episode code dot title",
                    "%1 \u00b7 %2", code, ep.title);
            row.tertiaryLine = i18nc(
                "@info library airing-soon rail, episode air date",
                "Airs %1", releaseText(ep.releaseDate));
            row.releaseDate = ep.releaseDate;
            airingOrComing.append(std::move(row));
        }

        // "Ready to Watch" surfaces the next unwatched aired episode
        // per series only; movies don't have a meaningful "next".
        // The rail is dedup'd against Continue Watching: when the
        // next-up episode already has an in-progress history entry
        // (`e.resume` is computed against that exact episode in
        // `buildEntries`), Continue Watching covers the row and we
        // skip it here so the two rails never duplicate the same
        // title.
        if (t.kind == domain::MediaKind::Series && e.nextToWatch
            && !e.resume.has_value()) {
            const auto& ep = *e.nextToWatch;
            LibraryRailRow row;
            row.kind = domain::MediaKind::Series;
            row.imdbId = t.imdbId;
            row.title = t.title;
            row.season = ep.season;
            row.episode = ep.episode;
            row.posterUrl = posterString(t);
            row.backdropUrl = backdropString(t);
            row.thumbnailUrl = thumbnailString(ep);
            const auto code = episodeCode(ep.season, ep.episode);
            row.primaryLine = t.title;
            row.secondaryLine = ep.title.isEmpty()
                ? code
                : i18nc(
                    "@info library rail, episode code dot title",
                    "%1 \u00b7 %2", code, ep.title);
            // No resume bar / tertiary line by construction: the
            // !e.resume.has_value() guard above guarantees there is
            // no in-progress entry for this episode (anything with
            // progress lives in Continue Watching instead).
            upNext.append(std::move(row));
        }
    }

    // Recently added: every saved title, sorted by addedAt desc
    // and capped so the rail stays visually sane on large libraries.
    QList<const Entry*> byAdded;
    byAdded.reserve(m_entries.size());
    for (const auto& e : m_entries) {
        byAdded.append(&e);
    }
    std::sort(byAdded.begin(), byAdded.end(),
        [](const Entry* a, const Entry* b) {
            return a->title.addedAt > b->title.addedAt;
        });
    if (byAdded.size() > kRecentlyAddedLimit) {
        byAdded.resize(kRecentlyAddedLimit);
    }
    for (const auto* e : byAdded) {
        const auto& t = e->title;
        LibraryRailRow row;
        row.kind = t.kind;
        row.imdbId = t.imdbId;
        row.title = t.title;
        row.posterUrl = posterString(t);
        // Recently added intentionally uses poster artwork only:
        // both `thumbnailUrl` and `backdropUrl` stay empty so the
        // card's fallback chain skips straight to the 2:3 poster.
        // (Up Next and Airing Soon use 16:9 frames and want the
        // show backdrop as their thumbnail fallback; Recently
        // Added is a 2:3 poster strip and must keep that look.)
        row.primaryLine = t.title;
        if (t.year) {
            row.secondaryLine = QString::number(*t.year);
        }
        row.releaseDate = t.releaseDate;
        recentlyAdded.append(std::move(row));
    }

    std::sort(upNext.begin(), upNext.end(),
        [](const auto& a, const auto& b) {
            return QString::localeAwareCompare(a.title, b.title) < 0;
        });
    const auto byReleaseAsc = [](const LibraryRailRow& a,
                                  const LibraryRailRow& b) {
        if (a.releaseDate && b.releaseDate) {
            return *a.releaseDate < *b.releaseDate;
        }
        return a.releaseDate.has_value() > b.releaseDate.has_value();
    };
    std::sort(airingOrComing.begin(), airingOrComing.end(),
        byReleaseAsc);

    m_upNextModel->setRows(std::move(upNext));
    m_airingSoonModel->setRows(std::move(airingOrComing));
    m_recentlyAddedModel->setRows(std::move(recentlyAdded));
}

// -- grid -------------------------------------------------------------

bool LibraryViewModel::entryMatchesFilters(const Entry& e) const
{
    if (m_kind == KindFilter::Movies && e.title.kind != domain::MediaKind::Movie) {
        return false;
    }
    if (m_kind == KindFilter::Series && e.title.kind != domain::MediaKind::Series) {
        return false;
    }

    if (m_status == StatusFilter::Continue && e.status != ResolvedStatus::Continue) {
        return false;
    }
    if (m_status == StatusFilter::ToWatch && e.status != ResolvedStatus::ToWatch) {
        return false;
    }
    if (m_status == StatusFilter::Watched && e.status != ResolvedStatus::Watched) {
        return false;
    }
    if (m_status == StatusFilter::Upcoming && e.status != ResolvedStatus::Upcoming) {
        return false;
    }

    if (m_hideWatched && e.status == ResolvedStatus::Watched) {
        return false;
    }

    if (!m_genreIds.isEmpty()) {
        bool any = false;
        for (const auto& g : e.title.genres) {
            const auto it = m_genreIdByName.constFind(g.trimmed());
            if (it != m_genreIdByName.constEnd()
                && m_genreIds.contains(it.value())) {
                any = true;
                break;
            }
        }
        if (!any) return false;
    }

    if (m_minRatingPct > 0) {
        if (!e.title.imdbRating) return false;
        if (*e.title.imdbRating * 10.0 < m_minRatingPct) return false;
    }

    if (const auto floor = dateWindowFloor(m_dateWindow)) {
        if (!e.title.releaseDate) return false;
        if (*e.title.releaseDate < *floor) return false;
    }

    return true;
}

LibraryListRow LibraryViewModel::toGridRow(const Entry& e) const
{
    LibraryListRow row;
    row.kind = e.title.kind;
    row.imdbId = e.title.imdbId;
    row.title = e.title.title;
    row.posterUrl = posterString(e.title);
    row.releaseDateText = releaseText(e.title.releaseDate);
    row.rating = e.title.imdbRating;
    row.runtimeMinutes = e.title.runtimeMinutes;

    switch (e.status) {
    case ResolvedStatus::Continue: {
        row.resumeEntry = e.resume;
        row.progress = e.resumeProgress;
        if (e.title.kind == domain::MediaKind::Movie) {
            row.subtitle = i18nc("@info library card subtitle",
                "Resume from %1%",
                qRound(qBound(0.0, e.resumeProgress, 1.0) * 100.0));
        } else if (e.resume && e.resume->key.season
            && e.resume->key.episode) {
            row.season = e.resume->key.season;
            row.episode = e.resume->key.episode;
            row.subtitle = i18nc("@info library card subtitle",
                "Resume %1",
                episodeCode(*e.resume->key.season, *e.resume->key.episode));
        } else {
            row.subtitle = i18nc("@info library card subtitle", "Resume");
        }
        break;
    }
    case ResolvedStatus::ToWatch: {
        if (e.title.kind == domain::MediaKind::Movie) {
            row.subtitle = i18nc("@info library card subtitle", "To watch");
        } else if (e.nextToWatch) {
            row.season = e.nextToWatch->season;
            row.episode = e.nextToWatch->episode;
            const auto code = episodeCode(
                e.nextToWatch->season, e.nextToWatch->episode);
            row.subtitle = e.nextToWatch->title.isEmpty()
                ? i18nc("@info library card subtitle, episode code",
                    "Next: %1", code)
                : i18nc("@info library card subtitle, episode code and title",
                    "Next: %1 \u2014 %2", code, e.nextToWatch->title);
        } else {
            row.subtitle = i18nc("@info library card subtitle", "To watch");
        }
        break;
    }
    case ResolvedStatus::Watched: {
        row.watched = true;
        if (e.title.kind == domain::MediaKind::Movie) {
            row.subtitle = i18nc("@info library card subtitle", "Watched");
        } else {
            row.progress = e.totalAired > 0
                ? static_cast<double>(e.watchedAired) / e.totalAired
                : 1.0;
            row.subtitle = i18nc(
                "@info library card subtitle, watched count over total episodes",
                "%1 / %2 episodes watched",
                e.watchedAired, e.totalAired);
        }
        break;
    }
    case ResolvedStatus::Upcoming: {
        row.upcoming = true;
        if (e.title.kind == domain::MediaKind::Movie) {
            row.subtitle = row.releaseDateText.isEmpty()
                ? i18nc("@info library card subtitle", "Upcoming")
                : i18nc("@info library card subtitle, release date",
                    "Releases %1", row.releaseDateText);
        } else if (e.firstUpcoming) {
            row.season = e.firstUpcoming->season;
            row.episode = e.firstUpcoming->episode;
            const auto epDate = releaseText(e.firstUpcoming->releaseDate);
            row.subtitle = e.upcomingCount == 1
                ? i18nc("@info library card subtitle",
                    "%1 airs %2",
                    episodeCode(e.firstUpcoming->season,
                        e.firstUpcoming->episode),
                    epDate)
                : i18ncp("@info library card subtitle",
                    "%1 upcoming episode",
                    "%1 upcoming episodes",
                    e.upcomingCount);
        } else {
            row.subtitle = i18nc("@info library card subtitle", "Upcoming");
        }
        break;
    }
    }
    return row;
}

void LibraryViewModel::rebuildGrid()
{
    QList<LibraryListRow> rows;
    rows.reserve(m_entries.size());
    QList<const Entry*> matched;
    matched.reserve(m_entries.size());
    for (const auto& e : m_entries) {
        if (entryMatchesFilters(e)) {
            matched.append(&e);
        }
    }

    // Sort the matched entries.
    auto byTitle = [](const Entry* a, const Entry* b) {
        return QString::localeAwareCompare(a->title.title, b->title.title) < 0;
    };
    auto byAddedDesc = [](const Entry* a, const Entry* b) {
        return a->title.addedAt > b->title.addedAt;
    };
    auto byReleaseDesc = [](const Entry* a, const Entry* b) {
        const bool ah = a->title.releaseDate.has_value();
        const bool bh = b->title.releaseDate.has_value();
        if (ah != bh) return ah > bh;
        if (ah && bh && *a->title.releaseDate != *b->title.releaseDate) {
            return *a->title.releaseDate > *b->title.releaseDate;
        }
        return QString::localeAwareCompare(a->title.title, b->title.title) < 0;
    };
    auto byRatingDesc = [](const Entry* a, const Entry* b) {
        const bool ah = a->title.imdbRating.has_value();
        const bool bh = b->title.imdbRating.has_value();
        if (ah != bh) return ah > bh;
        if (ah && bh && *a->title.imdbRating != *b->title.imdbRating) {
            return *a->title.imdbRating > *b->title.imdbRating;
        }
        return QString::localeAwareCompare(a->title.title, b->title.title) < 0;
    };
    switch (m_sort) {
    case SortMode::Title:
        std::sort(matched.begin(), matched.end(), byTitle); break;
    case SortMode::ReleaseDate:
        std::sort(matched.begin(), matched.end(), byReleaseDesc); break;
    case SortMode::Rating:
        std::sort(matched.begin(), matched.end(), byRatingDesc); break;
    case SortMode::RecentlyAdded:
    default:
        std::sort(matched.begin(), matched.end(), byAddedDesc); break;
    }

    for (const auto* e : matched) {
        rows.append(toGridRow(*e));
    }
    m_model->setRows(std::move(rows));
    Q_EMIT modelChanged();
}

} // namespace kinema::ui::qml

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/LibraryViewModel.h"

#include "controllers/LibraryController.h"
#include "core/DateFormat.h"

#include <KLocalizedString>

#include <QDate>

#include <algorithm>

namespace kinema::ui::qml {

namespace {

QString posterString(const api::LibraryTitle& t)
{
    return t.poster.isValid() ? t.poster.toString() : QString();
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

bool isReleased(const std::optional<QDate>& date)
{
    return !isFuture(date);
}

QString releaseText(const std::optional<QDate>& date)
{
    return date && date->isValid() ? core::formatReleaseDate(*date) : QString();
}

void sortByTitle(QList<LibraryListRow>& rows)
{
    std::sort(rows.begin(), rows.end(), [](const auto& a, const auto& b) {
        return QString::localeAwareCompare(a.title, b.title) < 0;
    });
}

} // namespace

LibraryViewModel::LibraryViewModel(
    controllers::LibraryController* library, QObject* parent)
    : QObject(parent)
    , m_library(library)
    , m_model(new LibraryListModel(this))
{
    if (m_library) {
        connect(m_library, &controllers::LibraryController::changed,
            this, &LibraryViewModel::refresh);
        connect(m_library, &controllers::LibraryController::statusMessage,
            this, &LibraryViewModel::statusMessage);
    }
    refresh();
}

LibraryViewModel::~LibraryViewModel() = default;

void LibraryViewModel::refresh()
{
    rebuildRows();
    publishSection();
}

void LibraryViewModel::setSection(Section section)
{
    if (m_section == section) {
        return;
    }
    m_section = section;
    m_userSelectedSection = true;
    Q_EMIT sectionChanged();
    publishSection();
}

void LibraryViewModel::activate(int row)
{
    const auto* r = m_model->at(row);
    if (!r) {
        return;
    }
    if (m_section == Section::Continue && r->resumeEntry) {
        Q_EMIT resumeRequested(*r->resumeEntry);
        return;
    }
    if (r->kind == api::MediaKind::Movie) {
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

void LibraryViewModel::rebuildRows()
{
    m_continueRows.clear();
    m_toWatchRows.clear();
    m_watchedRows.clear();
    m_upcomingRows.clear();

    if (!m_library) {
        Q_EMIT countsChanged();
        return;
    }

    const auto titles = m_library->activeTitles();
    for (const auto& t : titles) {
        if (t.kind == api::MediaKind::Movie) {
            const bool watched = m_library->isMovieWatched(t.imdbId);
            const double progress = m_library->movieProgress(t.imdbId);
            const auto resume = m_library->resumeEntryForMovie(t.imdbId);
            const bool upcoming = isFuture(t.releaseDate);

            LibraryListRow row;
            row.kind = t.kind;
            row.imdbId = t.imdbId;
            row.title = t.title;
            row.posterUrl = posterString(t);
            row.progress = progress;
            row.watched = watched;
            row.upcoming = upcoming;
            row.releaseDateText = releaseText(t.releaseDate);
            row.resumeEntry = resume;

            if (resume) {
                row.subtitle = i18nc("@info library card subtitle",
                    "Resume from %1%",
                    qRound(qBound(0.0, progress, 1.0) * 100.0));
                m_continueRows.append(row);
            } else if (upcoming) {
                row.subtitle = row.releaseDateText.isEmpty()
                    ? i18nc("@info library card subtitle", "Upcoming")
                    : i18nc("@info library card subtitle, release date",
                        "Releases %1", row.releaseDateText);
                m_upcomingRows.append(row);
            } else if (watched) {
                row.subtitle = i18nc("@info library card subtitle", "Watched");
                m_watchedRows.append(row);
            } else if (isReleased(t.releaseDate)) {
                row.subtitle = i18nc("@info library card subtitle", "To watch");
                m_toWatchRows.append(row);
            }
            continue;
        }

        const auto episodes = m_library->episodesForSeries(t.imdbId);
        int watchedCount = 0;
        int totalAired = 0;
        int upcomingCount = 0;
        const api::LibraryEpisode* nextToWatch = nullptr;
        const api::LibraryEpisode* firstUpcoming = nullptr;
        std::optional<api::HistoryEntry> latestResume;
        double latestProgress = -1.0;

        for (const auto& ep : episodes) {
            const bool future = isFuture(ep.releaseDate);
            if (future) {
                ++upcomingCount;
                if (!firstUpcoming
                    || (ep.releaseDate && firstUpcoming->releaseDate
                        && *ep.releaseDate < *firstUpcoming->releaseDate)) {
                    firstUpcoming = &ep;
                }
                continue;
            }
            ++totalAired;
            const bool epWatched = m_library->isEpisodeWatched(
                t.imdbId, ep.season, ep.episode);
            if (epWatched) {
                ++watchedCount;
            } else if (!nextToWatch) {
                nextToWatch = &ep;
            }
            const auto resume = m_library->resumeEntryForEpisode(
                t.imdbId, ep.season, ep.episode);
            if (resume && (!latestResume
                    || resume->lastWatchedAt > latestResume->lastWatchedAt)) {
                latestProgress = resume->progressFraction();
                latestResume = resume;
            }
        }

        if (latestResume) {
            LibraryListRow row;
            row.kind = t.kind;
            row.imdbId = t.imdbId;
            row.season = latestResume->key.season;
            row.episode = latestResume->key.episode;
            row.title = t.title;
            row.posterUrl = posterString(t);
            row.progress = latestProgress;
            row.resumeEntry = latestResume;
            if (row.season && row.episode) {
                row.subtitle = i18nc("@info library card subtitle",
                    "Resume %1", episodeCode(*row.season, *row.episode));
            } else {
                row.subtitle = i18nc("@info library card subtitle", "Resume");
            }
            m_continueRows.append(row);
        }

        if (nextToWatch) {
            LibraryListRow row;
            row.kind = t.kind;
            row.imdbId = t.imdbId;
            row.season = nextToWatch->season;
            row.episode = nextToWatch->episode;
            row.title = t.title;
            row.posterUrl = posterString(t);
            row.subtitle = nextToWatch->title.isEmpty()
                ? i18nc("@info library card subtitle, episode code",
                    "Next: %1", episodeCode(nextToWatch->season, nextToWatch->episode))
                : i18nc("@info library card subtitle, episode code and title",
                    "Next: %1 — %2",
                    episodeCode(nextToWatch->season, nextToWatch->episode),
                    nextToWatch->title);
            m_toWatchRows.append(row);
        }

        if (watchedCount > 0 || (totalAired > 0 && watchedCount == totalAired)) {
            LibraryListRow row;
            row.kind = t.kind;
            row.imdbId = t.imdbId;
            row.title = t.title;
            row.posterUrl = posterString(t);
            row.watched = totalAired > 0 && watchedCount == totalAired;
            row.progress = totalAired > 0
                ? static_cast<double>(watchedCount) / totalAired
                : -1.0;
            row.subtitle = i18nc("@info library card subtitle, watched count over total episodes",
                "%1 / %2 episodes watched", watchedCount, totalAired);
            m_watchedRows.append(row);
        }

        if (upcomingCount > 0 && firstUpcoming) {
            LibraryListRow row;
            row.kind = t.kind;
            row.imdbId = t.imdbId;
            row.season = firstUpcoming->season;
            row.episode = firstUpcoming->episode;
            row.title = t.title;
            row.posterUrl = posterString(t);
            row.upcoming = true;
            row.releaseDateText = releaseText(firstUpcoming->releaseDate);
            row.subtitle = upcomingCount == 1
                ? i18nc("@info library card subtitle",
                    "%1 airs %2",
                    episodeCode(firstUpcoming->season, firstUpcoming->episode),
                    row.releaseDateText)
                : i18ncp("@info library card subtitle",
                    "%1 upcoming episode",
                    "%1 upcoming episodes",
                    upcomingCount);
            m_upcomingRows.append(row);
        }
    }

    sortByTitle(m_toWatchRows);
    sortByTitle(m_watchedRows);
    sortByTitle(m_upcomingRows);
    std::sort(m_continueRows.begin(), m_continueRows.end(), [](const auto& a, const auto& b) {
        if (a.resumeEntry && b.resumeEntry) {
            return a.resumeEntry->lastWatchedAt > b.resumeEntry->lastWatchedAt;
        }
        return a.resumeEntry.has_value() > b.resumeEntry.has_value();
    });

    if (!m_userSelectedSection && !m_sectionInitialized) {
        m_section = m_continueRows.isEmpty() ? Section::ToWatch : Section::Continue;
        m_sectionInitialized = true;
        Q_EMIT sectionChanged();
    }
    Q_EMIT countsChanged();
}

void LibraryViewModel::publishSection()
{
    m_model->setRows(rowsFor(m_section));
    Q_EMIT modelChanged();
}

QList<LibraryListRow>& LibraryViewModel::rowsFor(Section section)
{
    switch (section) {
    case Section::Continue: return m_continueRows;
    case Section::Watched: return m_watchedRows;
    case Section::Upcoming: return m_upcomingRows;
    case Section::ToWatch: break;
    }
    return m_toWatchRows;
}

const QList<LibraryListRow>& LibraryViewModel::rowsFor(Section section) const
{
    switch (section) {
    case Section::Continue: return m_continueRows;
    case Section::Watched: return m_watchedRows;
    case Section::Upcoming: return m_upcomingRows;
    case Section::ToWatch: break;
    }
    return m_toWatchRows;
}

} // namespace kinema::ui::qml

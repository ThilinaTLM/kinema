// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Library.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QList>
#include <QObject>
#include <QString>

#include <optional>

namespace kinema::api {
struct MetaDetail;
struct SeriesDetail;
}

namespace kinema::core {
class LibraryStore;
}

namespace kinema::controllers {

class HistoryController;

class LibraryController : public QObject
{
    Q_OBJECT
public:
    LibraryController(core::LibraryStore& store,
        HistoryController* history,
        QObject* parent = nullptr);
    ~LibraryController() override;

    bool isInLibrary(api::MediaKind kind, const QString& imdbId) const;
    std::optional<api::LibraryTitle> title(api::MediaKind kind,
        const QString& imdbId) const;
    QList<api::LibraryTitle> activeTitles() const;
    QList<api::LibraryEpisode> episodesForSeries(
        const QString& imdbId) const;

    void saveMovie(const api::MetaDetail& meta);
    void saveSeries(const api::SeriesDetail& detail);
    void softRemove(api::MediaKind kind, const QString& imdbId);
    void hardDelete(api::MediaKind kind, const QString& imdbId);

    void setMovieWatched(const QString& imdbId, bool watched);
    void setEpisodeWatched(const QString& imdbId,
        int season, int episode, bool watched);
    void setSeriesWatched(const QString& imdbId, bool watched);
    void setSeasonWatched(const QString& imdbId,
        int season, bool watched);
    void clearWatchOverride(const api::PlaybackKey& key);

    bool isMovieWatched(const QString& imdbId) const;
    bool isEpisodeWatched(const QString& imdbId,
        int season, int episode) const;
    double movieProgress(const QString& imdbId) const;
    double episodeProgress(const QString& imdbId,
        int season, int episode) const;

    std::optional<api::HistoryEntry> resumeEntryForMovie(
        const QString& imdbId) const;
    std::optional<api::HistoryEntry> resumeEntryForEpisode(
        const QString& imdbId, int season, int episode) const;

Q_SIGNALS:
    void changed();
    void statusMessage(const QString& text, int timeoutMs = 3000);

private:
    api::PlaybackKey movieKey(const QString& imdbId) const;
    api::PlaybackKey episodeKey(const QString& imdbId,
        int season, int episode) const;
    bool isWatched(const api::PlaybackKey& key) const;
    double progressFor(const api::PlaybackKey& key) const;

    core::LibraryStore& m_store;
    HistoryController* m_history {};
};

} // namespace kinema::controllers

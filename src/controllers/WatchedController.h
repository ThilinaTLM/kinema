// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QList>
#include <QObject>
#include <QPair>
#include <QString>

#include <optional>

namespace kinema::core {
class WatchedStore;
}

namespace kinema::controllers {

class HistoryController;

/**
 * Watched-state mediator. Composes `core::WatchedStore` (manual user
 * override) with `controllers::HistoryController` (playback finished
 * flag) into a single API the UI consumes.
 *
 * Resolution order, identical for movies and episodes:
 *   1. Manual override = `Watched`   \u2192 watched.
 *   2. Manual override = `Unwatched` \u2192 not watched.
 *   3. No override \u2192 `history.finished` for the same `PlaybackKey`.
 *
 * The controller is intentionally library-agnostic: it works for any
 * `imdbId` the user has interacted with, whether or not the title sits
 * in `core::LibraryStore`. Bulk operations (`setEpisodesWatched`)
 * accept the episode list from the caller so the series detail page
 * can mark a non-library series watched using the in-memory Cinemeta
 * meta it already loaded.
 */
class WatchedController : public QObject
{
    Q_OBJECT
public:
    WatchedController(core::WatchedStore& store,
        HistoryController* history, QObject* parent = nullptr);
    ~WatchedController() override;

    // ---- queries --------------------------------------------------
    bool isWatched(const domain::PlaybackKey& key) const;
    bool isMovieWatched(const QString& imdbId) const;
    bool isEpisodeWatched(const QString& imdbId,
        int season, int episode) const;

    /// Progress fraction in [0, 1] for an in-progress play, or `-1.0`
    /// when no progress is recorded (or the entry is finished).
    double progressFor(const domain::PlaybackKey& key) const;
    double movieProgress(const QString& imdbId) const;
    double episodeProgress(const QString& imdbId,
        int season, int episode) const;

    /// Resume entry: in-progress history row for the given key, or
    /// `std::nullopt` when nothing is to resume.
    std::optional<domain::HistoryEntry> resumeEntryFor(
        const domain::PlaybackKey& key) const;
    std::optional<domain::HistoryEntry> resumeEntryForMovie(
        const QString& imdbId) const;
    std::optional<domain::HistoryEntry> resumeEntryForEpisode(
        const QString& imdbId, int season, int episode) const;

    // ---- mutations ------------------------------------------------
    void setMovieWatched(const QString& imdbId, bool watched);
    void setEpisodeWatched(const QString& imdbId,
        int season, int episode, bool watched);
    /// Bulk mark for a series / season. `seasonEpisode` is a list of
    /// `(season, episode)` pairs; the controller writes one override
    /// row per pair. Coalesces a single `changed()` per event-loop
    /// tick downstream of `WatchedStore`.
    void setEpisodesWatched(const QString& imdbId,
        const QList<QPair<int, int>>& seasonEpisode, bool watched);
    void clearOverride(const domain::PlaybackKey& key);

Q_SIGNALS:
    /// Fan-in of the underlying store and history controller signals.
    /// View-models subscribe here for "the watched picture might have
    /// shifted" without needing to track both sources.
    void changed();

private:
    domain::PlaybackKey movieKey(const QString& imdbId) const;
    domain::PlaybackKey episodeKey(const QString& imdbId,
        int season, int episode) const;

    core::WatchedStore& m_store;
    HistoryController* m_history {};
};

} // namespace kinema::controllers

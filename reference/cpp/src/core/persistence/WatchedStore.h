// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/PlaybackContext.h"

#include <QHash>
#include <QObject>
#include <QString>

namespace kinema::core {

class Database;

/**
 * Manual watched / unwatched override per `domain::PlaybackKey`. Owns the
 * `watched_overrides` SQLite table.
 *
 * The override is a *user intent* — independent of the playback
 * `history` table. `controllers::WatchedController` composes this DAO
 * with `controllers::HistoryController` to answer the unified
 * "is this watched?" question:
 *   - `Watched`   override wins \u2192 watched.
 *   - `Unwatched` override wins \u2192 not watched (even when
 *     `history.finished = 1`).
 *   - `None` (no row) \u2192 fall through to `history.finished`.
 *
 * The store is intentionally library-agnostic: it stores overrides
 * keyed only by `PlaybackKey` and never asks whether the title is in
 * the user's Library. Pre-1.0 the table sat next to `library_titles`
 * inside `LibraryStore`; migration v6 renamed it to make the data
 * boundary obvious.
 */
class WatchedStore : public QObject
{
    Q_OBJECT
public:
    enum class Override {
        None = 0,
        Watched = 1,
        Unwatched = 2,
    };
    Q_ENUM(Override)

    explicit WatchedStore(Database& db, QObject* parent = nullptr);
    ~WatchedStore() override;

    WatchedStore(const WatchedStore&) = delete;
    WatchedStore& operator=(const WatchedStore&) = delete;

    Override override(const domain::PlaybackKey& key) const;
    QHash<QString, Override> overridesForImdb(const QString& imdbId) const;

    /// Set or clear (`Override::None`) the manual override for `key`.
    void setOverride(const domain::PlaybackKey& key, Override state);
    void clearOverride(const domain::PlaybackKey& key);

    /// Wipe every override row for `imdbId` regardless of season /
    /// episode. Used by destructive flows that want to forget
    /// everything for a title.
    void clearAllForImdb(const QString& imdbId);

Q_SIGNALS:
    /// Coalesced to one emit per event-loop tick.
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    bool m_changePending = false;
};

} // namespace kinema::core

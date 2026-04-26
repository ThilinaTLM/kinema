// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QList>
#include <QObject>

#include <optional>

namespace kinema::core {

class Database;

/**
 * DAO over the `history` table. The public API is deliberately shaped
 * as if backed by an in-memory store so consumers
 * (`controllers::HistoryController`, tests) remain oblivious to the
 * SQLite substrate.
 *
 * Semantics:
 *   - `record()` is an upsert on `key`. Progress-only ticks (empty
 *     `lastStream`) preserve the previously persisted stream ref so
 *     the resume pointer is never clobbered by a metadata-free
 *     update.
 *   - `finished` flips automatically when
 *     `position_sec / duration_sec >= finishedThreshold` (default
 *     0.9); this drives the Continue-Watching filter.
 *   - `changed()` is coalesced to once per event-loop tick.
 *   - Writes are synchronous. With WAL + synchronous=NORMAL each
 *     write is sub-millisecond; no debouncing needed.
 *   - A closed or unopened `Database` turns every mutating call into
 *     a no-op and every read into an empty result \u2014 the UI stays
 *     usable when the DB layer is broken.
 */
class HistoryStore : public QObject
{
    Q_OBJECT
public:
    explicit HistoryStore(Database& db, QObject* parent = nullptr);
    ~HistoryStore() override;

    HistoryStore(const HistoryStore&) = delete;
    HistoryStore& operator=(const HistoryStore&) = delete;

    /// Drop `finished = 1 AND last_watched_at < now()-365 days` rows.
    /// Idempotent; safe to call once at startup.
    void runRetentionPass();

    /// Override the default 365-day retention window. Primarily for
    /// tests.
    void setRetentionDays(int days);

    /// Override the finished-at threshold (fraction of duration).
    /// Primarily for tests. Default 0.9.
    void setFinishedThreshold(double fraction);
    double finishedThreshold() const noexcept { return m_finishedThreshold; }

    std::optional<api::HistoryEntry> find(const api::PlaybackKey& key) const;

    /// For movies: returns the movie entry if any. For series:
    /// returns the most-recent entry for that imdbId across all
    /// seasons/episodes. Finished flag is respected \u2014 a finished
    /// entry still resolves if it's the most recent.
    std::optional<api::HistoryEntry> findLatestForMedia(
        api::MediaKind kind, const QString& imdbId) const;

    /// Most-recent in-progress entries for the Continue-Watching row.
    /// Series are deduplicated to the latest-watched episode per
    /// imdbId. `finished` entries are excluded.
    QList<api::HistoryEntry> continueWatching(int limit = 30) const;

    /// Upsert one entry. Empty `lastStream` on an existing row
    /// preserves the previously stored ref.
    void record(const api::HistoryEntry& entry);

    void remove(const api::PlaybackKey& key);
    void clear();

    /// Update only the remembered subtitle language for `key`.
    /// No-op when no row exists; emits `changed()` on success.
    /// Used by the subtitle download flow so the next visit pre-ticks
    /// the language filter on the picker.
    void setRememberedSubtitleLang(const api::PlaybackKey& key,
        const QString& lang);

Q_SIGNALS:
    /// Fired once per event-loop tick regardless of how many
    /// record/remove calls happened. Drives UI refresh.
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    double m_finishedThreshold = 0.9;
    int m_retentionDays = 365;
    bool m_changePending = false;
};

} // namespace kinema::core

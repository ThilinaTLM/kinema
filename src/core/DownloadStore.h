// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"

#include <QList>
#include <QObject>

#include <optional>

namespace kinema::core {

class Database;

/**
 * DAO over the `download_items` table.
 *
 * Mirrors the shape of `HistoryStore`: synchronous calls, coalesced
 * `changed()` signal, no-op behaviour when the underlying database
 * is closed. The table itself is created via
 * `Database::ensureSupplementalTables()` (idempotent CREATE TABLE
 * IF NOT EXISTS, no migration version bump).
 */
class DownloadStore : public QObject
{
    Q_OBJECT
public:
    explicit DownloadStore(Database& db, QObject* parent = nullptr);
    ~DownloadStore() override;

    DownloadStore(const DownloadStore&) = delete;
    DownloadStore& operator=(const DownloadStore&) = delete;

    /// Load every persisted download row, ordered by `updated_at DESC`
    /// (most recently changed first).
    QList<api::DownloadItem> loadAll() const;

    /// Find a single row by primary key. Returns nullopt when missing.
    std::optional<api::DownloadItem> find(const QString& assetId) const;

    /// Find the most-recent download row for a given playback key.
    /// Useful for "is there already a local asset for this title?"
    /// lookups during queue/history replay.
    std::optional<api::DownloadItem> findForKey(const api::PlaybackKey& key) const;

    /// Insert or update a row. Primary key is `assetId`; the upsert
    /// preserves `added_at` on existing rows but always refreshes
    /// `updated_at`.
    void upsert(const api::DownloadItem& item);

    /// Mutate progress fields without rewriting the entire row.
    /// `lastUsedAt` is set to now on every successful update.
    void updateProgress(const QString& assetId,
        qint64 cachedSizeBytes, bool complete);

    /// Mutate the lifecycle state. `lastError` may be empty.
    void updateState(const QString& assetId,
        api::DownloadState state, const QString& lastError = {});

    /// Toggle the cache disposition (pinned <-> ephemeral).
    void setDisposition(const QString& assetId, api::CacheDisposition d);

    /// Touch `last_used_at` to now without changing other fields.
    void touch(const QString& assetId);

    /// Delete a row.
    void remove(const QString& assetId);

    /// Drop every row.
    void clear();

Q_SIGNALS:
    void changed();

private:
    void scheduleChanged();

    Database& m_db;
    bool m_changePending = false;
};

} // namespace kinema::core

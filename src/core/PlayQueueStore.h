// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/QueueItem.h"

#include <QList>
#include <QObject>

namespace kinema::core {

class Database;

/**
 * DAO over the `play_queue` table. Sibling of `HistoryStore` \u2014
 * shape and conventions match it deliberately: Database & wired in
 * the constructor, public API independent of SQLite, mutating calls
 * become no-ops on a closed database.
 *
 * Persistence model:
 *   - `directUrl` is NEVER persisted. Real-Debrid unrestrict URLs
 *     expire; the controller always re-resolves at pop time
 *     (matching by infoHash via the existing `HistoryStreamRef`
 *     resume pattern).
 *   - `status` is also not persisted; rows always reload as
 *     `Status::Pending`. `Active` and `Failed` describe in-flight
 *     state during a single session.
 *   - Reordering is done via `replaceAll(items)` inside one
 *     transaction. With the small queues users actually build
 *     (realistically &lt; 20 items), renumbering everything on every
 *     mutation is cheaper than maintaining sparse ord values.
 *
 * No `changed()` signal: the controller keeps the in-memory model
 * in sync with the store and emits the granular row signals
 * itself. The store is a pure DAO.
 */
class PlayQueueStore : public QObject
{
    Q_OBJECT
public:
    explicit PlayQueueStore(Database& db, QObject* parent = nullptr);
    ~PlayQueueStore() override;

    PlayQueueStore(const PlayQueueStore&) = delete;
    PlayQueueStore& operator=(const PlayQueueStore&) = delete;

    /// Load every queue item, ordered by `ord` ascending. Returns
    /// an empty list when the database is closed.
    QList<api::QueueItem> loadAll() const;

    /// Insert one item. Caller is expected to have already set
    /// `item.order` to the desired position; the store does NOT
    /// renumber other rows here. Use `replaceAll` for bulk inserts
    /// or reorders.
    /// Returns the newly assigned `id` (&gt; 0) or 0 on failure.
    qint64 insert(const api::QueueItem& item);

    /// Delete one row by id. No-op when not present.
    void remove(qint64 id);

    /// Delete every row.
    void clear();

    /// Atomic replace: clear the table and re-insert `items` with
    /// `ord` set from each item's `order`. New items
    /// (`id == 0`) get a freshly-allocated id; existing ones keep
    /// their id.
    /// Returns the input list with ids backfilled, or an empty list
    /// on failure.
    QList<api::QueueItem> replaceAll(const QList<api::QueueItem>& items);

private:
    Database& m_db;
};

} // namespace kinema::core

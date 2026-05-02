// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/PlaybackContext.h"

#include <QDateTime>
#include <QMetaType>
#include <QString>
#include <QUrl>

namespace kinema::api {

/**
 * One row in the play queue. POD shape, ferried by value through
 * the store / controller / view-model boundaries.
 *
 * Lifecycle:
 *   - At enqueue time the controller fills `key`, `title`,
 *     `seriesTitle`, `episodeTitle`, `poster`, `streamRef`, and
 *     `addedAt`. `cachedDirectUrl` is the optional URL captured
 *     from the live `api::Stream` the user just clicked on \u2014 we
 *     keep it in memory only so the very next play after enqueue
 *     can skip a Torrentio round-trip.
 *   - The store assigns `id` on first insert and persists every
 *     field except `cachedDirectUrl` and `status` (which are
 *     in-memory-only by design \u2014 RD direct URLs expire and
 *     status is per-session state).
 *   - At pop time the controller re-resolves a fresh `directUrl`
 *     by either using `cachedDirectUrl` (if still set) or
 *     re-fetching Torrentio and matching by `streamRef.infoHash`.
 *
 * `order` is a 0-based index that mirrors the row's position in
 * the queue list. The store renumbers everything in a single
 * transaction on every mutation to avoid collision bugs.
 */
struct QueueItem {
    /**
     * Per-row session state. Persisted rows always re-load as
     * `Pending` \u2014 `Active` and `Failed` describe in-flight
     * playback / a recent resolution failure and don't survive
     * an app restart.
     */
    enum class Status {
        Pending,    ///< Sitting in the queue, ready to be played.
        Active,     ///< Currently the focus of the player.
        Failed,     ///< Last resolve attempt could not produce a URL.
        Played,     ///< Watched to completion in this session. Session-only;
                    ///< not persisted, capped, and rendered above the
                    ///< active row so `Previous` can replay it.
    };

    qint64 id = 0;          ///< DB primary key. 0 = not yet persisted.
    int order = 0;          ///< 0..N-1 queue position.
    PlaybackKey key;
    QString title;          ///< Display title (movie title or "Show \u2014 S\u00d7E").
    QString seriesTitle;    ///< Empty for movies.
    QString episodeTitle;   ///< Empty for movies.
    QUrl poster;
    HistoryStreamRef streamRef; ///< infoHash + release info (no URL).
    QDateTime addedAt;      ///< UTC; assigned at enqueue time.

    Status status = Status::Pending;
    QUrl cachedDirectUrl;   ///< Not persisted. Cleared once consumed.

    bool isValid() const { return key.isValid() && !streamRef.isEmpty(); }
};

} // namespace kinema::api

Q_DECLARE_METATYPE(kinema::api::QueueItem)
Q_DECLARE_METATYPE(kinema::api::QueueItem::Status)

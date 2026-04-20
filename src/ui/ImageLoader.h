// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#pragma once

#include <QHash>
#include <QObject>
#include <QPixmap>
#include <QPixmapCache>
#include <QPointer>
#include <QPromise>
#include <QSharedPointer>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

#include <memory>

namespace kinema::core {
class HttpClient;
}

namespace kinema::ui {

/**
 * Async poster fetcher with a two-level cache:
 *
 *   1. QPixmapCache (process-wide in-memory, defaults to 10 MB)
 *   2. On-disk cache under QStandardPaths::CacheLocation/posters/
 *
 * Concurrent requests for the same URL share a single in-flight download.
 *
 * Usage:
 *     auto pm = co_await imageLoader.requestPoster(url);
 *     if (!pm.isNull()) { ... }
 *
 * An empty QPixmap is returned on any failure — image load is best-effort
 * and never throws.
 */
class ImageLoader : public QObject
{
    Q_OBJECT
public:
    explicit ImageLoader(core::HttpClient* http, QObject* parent = nullptr);

    QCoro::Task<QPixmap> requestPoster(QUrl url);

    /// Clear the memory cache (not the disk cache).
    void clearMemoryCache();

Q_SIGNALS:
    /// Emitted after a poster is fetched (from any source), so views can refresh.
    void posterReady(const QUrl& url);

private:
    QString diskPathFor(const QUrl& url) const;
    QString cacheKeyFor(const QUrl& url) const;

    core::HttpClient* m_http;
    QString m_diskDir;

    // De-dupe in-flight requests: each URL maps to a shared future-like
    // handle. We stash a small shared record so multiple awaiters get the
    // same pixmap without triggering multiple downloads.
    struct Pending {
        QCoro::Task<QPixmap> task;
    };
    // We can't easily "share" a QCoro::Task<T>; instead we store a
    // QSharedPointer<QPromise<QPixmap>>-style shim: the original requester
    // does the work, other awaiters poll an ordinary Qt signal. Simpler
    // approach: keep a map of QUrl→QSharedPointer<QPromise<QPixmap>>; the
    // first caller does the fetch and fulfils the promise, others co_await
    // a task that watches the promise's future.
    struct InFlight {
        QSharedPointer<QPromise<QPixmap>> promise;
    };
    QHash<QUrl, InFlight> m_inFlight;
};

} // namespace kinema::ui

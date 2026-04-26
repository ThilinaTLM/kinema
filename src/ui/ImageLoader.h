// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QCache>
#include <QHash>
#include <QImage>
#include <QObject>
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
 *   1. In-process QCache<QUrl, QImage> owned by this loader (cost-based
 *      LRU, ~256 MB budget). Unlike QPixmapCache this cache has no idle
 *      timer, so entries stay put until actual cost pressure evicts them.
 *   2. On-disk bytes under QStandardPaths::CacheLocation/posters/.
 *
 * Concurrent requests for the same URL share a single in-flight download.
 *
 * Usage (async):
 *     auto image = co_await imageLoader.requestPoster(url);
 *     if (!image.isNull()) { ... }
 *
 * Usage (sync probe, e.g. from paintEvent):
 *     QImage image = imageLoader.cached(url);
 *
 * An empty QImage is returned on any failure — image load is best-effort
 * and never throws.
 *
 * posterReady(url) is emitted whenever a fetch that went beyond the
 * in-memory cache completes — i.e. on disk-cache hits and HTTP fetches,
 * but NOT on plain memory-cache hits (no fetch actually ran, and
 * synchronous callers that respond to posterReady by calling
 * requestPoster again would recurse unbounded). In-flight dedup awaiters
 * rely on the original initiator's single emit.
 */
class ImageLoader : public QObject
{
    Q_OBJECT
public:
    explicit ImageLoader(core::HttpClient* http, QObject* parent = nullptr);

    QCoro::Task<QImage> requestPoster(QUrl url);

    /// Return a cached image for @p url without triggering a fetch. Null if
    /// absent from the in-memory cache.
    QImage cached(const QUrl& url) const;

    /// Clear the memory cache (not the disk cache).
    void clearMemoryCache();

Q_SIGNALS:
    /// Emitted after a poster is fetched (from any source), so views can refresh.
    void posterReady(const QUrl& url);

private:
    QString diskPathFor(const QUrl& url) const;

    core::HttpClient* m_http;
    QString m_diskDir;

    // In-memory decoded cache. Cost is in KB (see costKB in the .cpp).
    // mutable so cached() can be const.
    mutable QCache<QUrl, QImage> m_memCache;

    // De-dupe in-flight requests: each URL maps to a shared future-like
    // handle. We stash a small shared record so multiple awaiters get the
    // same image without triggering multiple downloads.
    struct Pending {
        QCoro::Task<QImage> task;
    };
    // We can't easily "share" a QCoro::Task<T>; instead we store a
    // QSharedPointer<QPromise<QImage>>-style shim: the original requester
    // does the work, other awaiters poll an ordinary Qt signal. Simpler
    // approach: keep a map of QUrl→QSharedPointer<QPromise<QImage>>; the
    // first caller does the fetch and fulfils the promise, others co_await
    // a task that watches the promise's future.
    struct InFlight {
        QSharedPointer<QPromise<QImage>> promise;
    };
    QHash<QUrl, InFlight> m_inFlight;
};

} // namespace kinema::ui

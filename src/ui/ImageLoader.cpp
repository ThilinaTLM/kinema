// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/ImageLoader.h"

#include "core/HttpClient.h"
#include "kinema_debug.h"

#include <QCoro/QCoroFuture>

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QStandardPaths>

#include <algorithm>

namespace kinema::ui {

namespace {
// Rough KB estimate used as QCache cost. A null/zero-depth pixmap still
// costs 1 so it can be inserted (QCache ignores items with cost > maxCost).
int costKB(const QPixmap& pm)
{
    const int depth = pm.depth() > 0 ? pm.depth() : 32;
    const qint64 bytes = static_cast<qint64>(pm.width())
        * static_cast<qint64>(pm.height()) * depth / 8;
    return static_cast<int>(std::max<qint64>(1, bytes / 1024));
}
} // namespace

ImageLoader::ImageLoader(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
{
    const auto base = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    m_diskDir = base + QStringLiteral("/posters");
    QDir().mkpath(m_diskDir);

    // 256 MB KB budget. Big enough to hold the full Discover page plus
    // detail-pane posters and episode thumbnails without churn; small
    // enough that a wild scroll session can't balloon RSS unbounded.
    m_memCache.setMaxCost(256 * 1024);
}

QString ImageLoader::diskPathFor(const QUrl& url) const
{
    const auto hash = QCryptographicHash::hash(
        url.toString(QUrl::FullyEncoded).toUtf8(), QCryptographicHash::Sha1);
    return m_diskDir + QLatin1Char('/') + QString::fromLatin1(hash.toHex()) + QStringLiteral(".img");
}

QPixmap ImageLoader::cached(const QUrl& url) const
{
    if (auto* pm = m_memCache.object(url)) {
        return *pm;
    }
    return {};
}

void ImageLoader::clearMemoryCache()
{
    m_memCache.clear();
}

QCoro::Task<QPixmap> ImageLoader::requestPoster(QUrl url)
{
    if (!url.isValid() || url.isEmpty()) {
        co_return QPixmap();
    }

    // 1. Memory cache. We deliberately do NOT emit posterReady here:
    // the caller either co_awaits (and gets the pixmap via co_return)
    // or probes synchronously via cached() — no fetch actually ran, and
    // emitting here would re-enter handlers like DetailPane's that call
    // updatePoster() → requestPoster() in response, causing unbounded
    // recursion when the first real emission (disk hit / HTTP) has
    // already populated the memory cache.
    if (const auto* pm = m_memCache.object(url)) {
        // Return a deliberate copy rather than `co_return *pm;`. QCoro's
        // TaskPromise<T>::return_value<U>(U&&) template picks up the lvalue
        // `*pm` with U = QPixmap& and then calls `T(std::move(value))`,
        // move-constructing from the cache entry. Since QPixmap is
        // implicitly shared, the move leaves the cached QPixmap null —
        // any later cached()/object() probe for this URL returns an empty
        // pixmap, even though the cache slot still exists at the same
        // pointer. That breaks DetailPane::updatePoster, which probes
        // the memory cache synchronously after requestPoster() and finds
        // a null pixmap it just "saw" moments earlier.
        const QPixmap copy = *pm;
        co_return copy;
    }

    // 2. Disk cache
    const auto diskPath = diskPathFor(url);
    if (QFileInfo::exists(diskPath)) {
        QPixmap pm;
        if (pm.load(diskPath) && !pm.isNull()) {
            m_memCache.insert(url, new QPixmap(pm), costKB(pm));
            Q_EMIT posterReady(url);
            co_return pm;
        }
        // Corrupt file — remove and fall through to refetch.
        QFile::remove(diskPath);
    }

    // 3. In-flight de-dup. Secondary awaiters don't emit posterReady —
    // the original initiator will do so exactly once when it finishes.
    if (auto it = m_inFlight.find(url); it != m_inFlight.end()) {
        auto promise = it.value().promise;
        auto fut = promise->future();
        co_await qCoro(fut).waitForFinished();
        co_return fut.isValid() && !fut.isCanceled() ? fut.result() : QPixmap();
    }

    auto promise = QSharedPointer<QPromise<QPixmap>>::create();
    promise->start();
    m_inFlight.insert(url, InFlight { promise });

    QPixmap result;
    try {
        const auto bytes = co_await m_http->get(url);
        QImage img;
        if (img.loadFromData(bytes)) {
            result = QPixmap::fromImage(img);
            if (!result.isNull()) {
                m_memCache.insert(url, new QPixmap(result), costKB(result));
                QFile f(diskPath);
                if (f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
                    f.write(bytes);
                    f.close();
                }
            }
        } else {
            qCDebug(KINEMA) << "poster decode failed for" << url;
        }
    } catch (const std::exception& e) {
        qCDebug(KINEMA) << "poster fetch failed:" << e.what();
    }

    promise->addResult(result);
    promise->finish();
    m_inFlight.remove(url);

    Q_EMIT posterReady(url);
    co_return result;
}

} // namespace kinema::ui

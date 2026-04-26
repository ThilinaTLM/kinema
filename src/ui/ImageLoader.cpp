// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/ImageLoader.h"

#include "core/HttpClient.h"
#include "core/UrlRedactor.h"
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
// Rough KB estimate used as QCache cost. A null/zero-size image still costs
// 1 so it can be inserted (QCache ignores items with cost > maxCost).
int costKB(const QImage& image)
{
    const qint64 bytes = image.isNull()
        ? 1
        : std::max<qsizetype>(1, image.sizeInBytes());
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

QImage ImageLoader::cached(const QUrl& url) const
{
    if (auto* image = m_memCache.object(url)) {
        return *image;
    }
    return {};
}

void ImageLoader::clearMemoryCache()
{
    m_memCache.clear();
}

QCoro::Task<QImage> ImageLoader::requestPoster(QUrl url)
{
    if (!url.isValid() || url.isEmpty()) {
        co_return QImage();
    }

    // 1. Memory cache. We deliberately do NOT emit posterReady here:
    // the caller either co_awaits (and gets the image via co_return)
    // or probes synchronously via cached() — no fetch actually ran, and
    // emitting here would re-enter handlers that call requestPoster() in
    // response, causing unbounded recursion when the first real emission
    // (disk hit / HTTP) has already populated the memory cache.
    if (const auto* image = m_memCache.object(url)) {
        const QImage copy = *image;
        co_return copy;
    }

    // 2. Disk cache
    const auto diskPath = diskPathFor(url);
    if (QFileInfo::exists(diskPath)) {
        QImage image;
        if (image.load(diskPath) && !image.isNull()) {
            m_memCache.insert(url, new QImage(image), costKB(image));
            Q_EMIT posterReady(url);
            co_return image;
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
        co_return fut.isValid() && !fut.isCanceled() ? fut.result() : QImage();
    }

    auto promise = QSharedPointer<QPromise<QImage>>::create();
    promise->start();
    m_inFlight.insert(url, InFlight { promise });

    QImage result;
    try {
        const auto bytes = co_await m_http->get(url);
        if (result.loadFromData(bytes)) {
            if (!result.isNull()) {
                m_memCache.insert(url, new QImage(result), costKB(result));
                QFile f(diskPath);
                if (f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
                    f.write(bytes);
                    f.close();
                }
            }
        } else {
            qCDebug(KINEMA) << "poster decode failed for"
                            << core::redactUrlForLog(url);
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

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

namespace kinema::ui {

ImageLoader::ImageLoader(core::HttpClient* http, QObject* parent)
    : QObject(parent)
    , m_http(http)
{
    const auto base = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
    m_diskDir = base + QStringLiteral("/posters");
    QDir().mkpath(m_diskDir);

    // 50 MB feels generous for posters but is cheap.
    QPixmapCache::setCacheLimit(50 * 1024);
}

QString ImageLoader::cacheKeyFor(const QUrl& url) const
{
    return QStringLiteral("kinema:poster:") + url.toString(QUrl::FullyEncoded);
}

QString ImageLoader::diskPathFor(const QUrl& url) const
{
    const auto hash = QCryptographicHash::hash(
        url.toString(QUrl::FullyEncoded).toUtf8(), QCryptographicHash::Sha1);
    return m_diskDir + QLatin1Char('/') + QString::fromLatin1(hash.toHex()) + QStringLiteral(".img");
}

void ImageLoader::clearMemoryCache()
{
    QPixmapCache::clear();
}

QCoro::Task<QPixmap> ImageLoader::requestPoster(QUrl url)
{
    if (!url.isValid() || url.isEmpty()) {
        co_return QPixmap();
    }

    const auto key = cacheKeyFor(url);

    // 1. Memory cache
    QPixmap cached;
    if (QPixmapCache::find(key, &cached) && !cached.isNull()) {
        co_return cached;
    }

    // 2. Disk cache
    const auto diskPath = diskPathFor(url);
    if (QFileInfo::exists(diskPath)) {
        QPixmap pm;
        if (pm.load(diskPath) && !pm.isNull()) {
            QPixmapCache::insert(key, pm);
            co_return pm;
        }
        // Corrupt file — remove and fall through to refetch.
        QFile::remove(diskPath);
    }

    // 3. In-flight de-dup
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
                QPixmapCache::insert(key, result);
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

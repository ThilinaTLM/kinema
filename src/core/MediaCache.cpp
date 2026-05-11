// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/MediaCache.h"

#include "config/DownloadSettings.h"
#include "core/CachePaths.h"
#include "kinema_log_download.h"

#include <QDateTime>
#include <QDir>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>

#include <algorithm>

namespace kinema::core {

namespace {
constexpr auto kAccessMarker = ".last-used";
constexpr auto kPinnedMarker = ".pinned";

bool removeRecursively(const QString& path)
{
    QDir d(path);
    return !d.exists() || d.removeRecursively();
}
}

MediaCache::MediaCache(const config::DownloadSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QString MediaCache::normalizedAssetId(QString id)
{
    id = id.trimmed();
    id.remove(QLatin1Char('/'));
    id.remove(QLatin1Char('\\'));
    return id;
}

QDir MediaCache::rootDir() const
{
    return cache::mediaDir();
}

QDir MediaCache::assetDir(const QString& assetId) const
{
    QDir root = rootDir();
    const auto normalized = normalizedAssetId(assetId);
    if (normalized.isEmpty()) {
        return root;
    }
    const auto path = root.absoluteFilePath(normalized);
    QDir d(path);
    if (!d.exists()) {
        root.mkpath(normalized);
    }
    return d;
}

QString MediaCache::accessMarkerPath(const QString& assetId) const
{
    return assetDir(assetId).absoluteFilePath(
        QString::fromLatin1(kAccessMarker));
}

QString MediaCache::pinnedMarkerPath(const QString& assetId) const
{
    return assetDir(assetId).absoluteFilePath(
        QString::fromLatin1(kPinnedMarker));
}

void MediaCache::markActive(const QString& assetId)
{
    const auto normalized = normalizedAssetId(assetId);
    if (!normalized.isEmpty()) {
        m_active.insert(normalized);
        touch(normalized);
    }
}

void MediaCache::markInactive(const QString& assetId)
{
    m_active.remove(normalizedAssetId(assetId));
}

bool MediaCache::isActive(const QString& assetId) const
{
    return m_active.contains(normalizedAssetId(assetId));
}

bool MediaCache::hasPinnedMarker(const QString& assetId) const
{
    return QFileInfo::exists(pinnedMarkerPath(assetId));
}

void MediaCache::setPinned(const QString& assetId, bool on)
{
    const auto path = pinnedMarkerPath(assetId);
    if (on) {
        QFile f(path);
        if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
            qCWarning(KINEMA_DOWNLOAD) << "MediaCache: could not pin" << path;
            return;
        }
        f.write(QDateTime::currentDateTimeUtc()
                .toString(Qt::ISODateWithMs).toUtf8());
    } else {
        QFile::remove(path);
    }
}

bool MediaCache::isPinned(const QString& assetId) const
{
    return hasPinnedMarker(assetId);
}

void MediaCache::touch(const QString& assetId) const
{
    const auto marker = accessMarkerPath(assetId);
    QFile f(marker);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        // Either the asset dir doesn't exist yet (which is fine \u2014
        // there's nothing to evict) or the disk is read-only.
        return;
    }
    f.write(QDateTime::currentDateTimeUtc()
            .toString(Qt::ISODateWithMs).toUtf8());
}

bool MediaCache::removeAsset(const QString& assetId)
{
    const auto normalized = normalizedAssetId(assetId);
    if (normalized.isEmpty()) {
        return false;
    }
    m_active.remove(normalized);
    return removeRecursively(rootDir().absoluteFilePath(normalized));
}

qint64 MediaCache::directorySize(const QString& path) const
{
    return cache::dirSizeBytes(QDir(path));
}

qint64 MediaCache::sizeBytes() const
{
    return cache::dirSizeBytes(rootDir());
}

qint64 MediaCache::ephemeralSizeBytes() const
{
    QDir root = rootDir();
    qint64 total = 0;
    const auto dirs = root.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot,
        QDir::Name);
    for (const auto& info : dirs) {
        if (hasPinnedMarker(info.fileName())) {
            continue;
        }
        total += directorySize(info.absoluteFilePath());
    }
    return total;
}

qint64 MediaCache::budgetBytes() const
{
    return static_cast<qint64>(m_settings.cacheBudgetGb())
        * 1024LL * 1024LL * 1024LL;
}

void MediaCache::enforceBudget()
{
    const auto budget = budgetBytes();
    if (budget <= 0) {
        return;
    }

    QDir root = rootDir();
    const auto dirs = root.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot,
        QDir::Name);

    struct Candidate {
        QString assetId;
        QString path;
        QDateTime lastUsed;
        qint64 size = 0;
    };

    QList<Candidate> candidates;
    qint64 ephemeralTotal = 0;
    for (const auto& info : dirs) {
        const auto assetId = info.fileName();
        if (hasPinnedMarker(assetId)) {
            continue; // never auto-evict pinned assets
        }
        const auto size = directorySize(info.absoluteFilePath());
        ephemeralTotal += size;
        if (isActive(assetId)) {
            continue; // never evict active assets
        }
        const QFileInfo marker(info.absoluteFilePath()
            + QLatin1Char('/') + QString::fromLatin1(kAccessMarker));
        candidates.append({ assetId, info.absoluteFilePath(),
            marker.exists() ? marker.lastModified() : info.lastModified(),
            size });
    }

    if (ephemeralTotal <= budget) {
        return;
    }

    std::sort(candidates.begin(), candidates.end(),
        [](const Candidate& a, const Candidate& b) {
            return a.lastUsed < b.lastUsed;
        });

    for (const auto& c : candidates) {
        if (ephemeralTotal <= budget) {
            break;
        }
        if (!removeRecursively(c.path)) {
            qCWarning(KINEMA_DOWNLOAD) << "MediaCache: failed to prune" << c.path;
            continue;
        }
        ephemeralTotal -= c.size;
        qCInfo(KINEMA_DOWNLOAD) << "MediaCache: pruned" << c.assetId;
    }
}

} // namespace kinema::core

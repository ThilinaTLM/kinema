// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/TorrentCache.h"

#include "config/TorrentStreamingSettings.h"
#include "core/CachePaths.h"
#include "kinema_debug.h"

#include <QDateTime>
#include <QDirIterator>
#include <QFile>
#include <QFileInfo>

#include <algorithm>

namespace kinema::core {

namespace {
constexpr auto kResumeFile = "resume.dat";
constexpr auto kAccessMarker = ".last-used";

bool removeRecursively(const QString& path)
{
    QDir d(path);
    return !d.exists() || d.removeRecursively();
}

qint64 directorySize(const QString& path)
{
    return cache::dirSizeBytes(QDir(path));
}
}

TorrentCache::TorrentCache(
    const config::TorrentStreamingSettings& settings, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
{
}

QString TorrentCache::normalizedHash(QString infoHash)
{
    infoHash = infoHash.trimmed().toLower();
    infoHash.remove(QLatin1Char('/'));
    infoHash.remove(QLatin1Char('\\'));
    return infoHash;
}

QDir TorrentCache::rootDir() const
{
    return cache::torrentsDir();
}

QDir TorrentCache::torrentDir(const QString& infoHash) const
{
    QDir root = rootDir();
    const auto normalized = normalizedHash(infoHash);
    const auto path = root.absoluteFilePath(normalized);
    QDir d(path);
    if (!d.exists()) {
        root.mkpath(normalized);
    }
    return d;
}

QString TorrentCache::resumeDataPath(const QString& infoHash) const
{
    return torrentDir(infoHash).absoluteFilePath(
        QString::fromLatin1(kResumeFile));
}

QString TorrentCache::accessMarkerPath(const QString& infoHash) const
{
    return torrentDir(infoHash).absoluteFilePath(
        QString::fromLatin1(kAccessMarker));
}

void TorrentCache::markActive(const QString& infoHash)
{
    const auto normalized = normalizedHash(infoHash);
    if (!normalized.isEmpty()) {
        m_active.insert(normalized);
        touch(normalized);
    }
}

void TorrentCache::markInactive(const QString& infoHash)
{
    m_active.remove(normalizedHash(infoHash));
    touch(infoHash);
}

bool TorrentCache::isActive(const QString& infoHash) const
{
    return m_active.contains(normalizedHash(infoHash));
}

void TorrentCache::touch(const QString& infoHash) const
{
    const auto marker = accessMarkerPath(infoHash);
    QFile f(marker);
    if (!f.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
        qCWarning(KINEMA) << "TorrentCache: could not touch" << marker;
        return;
    }
    f.write(QDateTime::currentDateTimeUtc()
            .toString(Qt::ISODateWithMs).toUtf8());
}

qint64 TorrentCache::sizeBytes() const
{
    return cache::dirSizeBytes(rootDir());
}

qint64 TorrentCache::budgetBytes() const
{
    return static_cast<qint64>(m_settings.cacheBudgetGb())
        * 1024LL * 1024LL * 1024LL;
}

void TorrentCache::enforceBudget()
{
    const auto budget = budgetBytes();
    if (budget <= 0) {
        return;
    }

    QDir root = rootDir();
    const auto dirs = root.entryInfoList(QDir::Dirs | QDir::NoDotAndDotDot,
        QDir::Name);

    struct Candidate {
        QString hash;
        QString path;
        QDateTime lastUsed;
        qint64 size = 0;
    };

    QList<Candidate> candidates;
    qint64 total = 0;
    for (const auto& info : dirs) {
        const auto hash = info.fileName().toLower();
        const auto size = directorySize(info.absoluteFilePath());
        total += size;
        if (isActive(hash)) {
            continue;
        }
        const QFileInfo marker(info.absoluteFilePath()
            + QLatin1Char('/') + QString::fromLatin1(kAccessMarker));
        candidates.append({ hash, info.absoluteFilePath(),
            marker.exists() ? marker.lastModified() : info.lastModified(),
            size });
    }

    if (total <= budget) {
        return;
    }

    std::sort(candidates.begin(), candidates.end(),
        [](const Candidate& a, const Candidate& b) {
            return a.lastUsed < b.lastUsed;
        });

    for (const auto& c : candidates) {
        if (total <= budget) {
            break;
        }
        if (!removeRecursively(c.path)) {
            qCWarning(KINEMA) << "TorrentCache: failed to prune" << c.path;
            continue;
        }
        total -= c.size;
        qCInfo(KINEMA) << "TorrentCache: pruned" << c.hash;
    }
}

} // namespace kinema::core

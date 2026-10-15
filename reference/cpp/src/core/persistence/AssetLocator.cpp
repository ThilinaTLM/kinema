// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "core/persistence/AssetLocator.h"

#include "core/io/CachePaths.h"

#include <QDirIterator>
#include <QFileInfo>

namespace kinema::core {

namespace {

/// Mirrors `TorrentCache::normalizedHash`.
QString normalizedHash(QString infoHash)
{
    infoHash = infoHash.trimmed().toLower();
    infoHash.remove(QLatin1Char('/'));
    infoHash.remove(QLatin1Char('\\'));
    return infoHash;
}

/// Mirrors `MediaCache::normalizedAssetId`.
QString normalizedAssetId(QString id)
{
    id = id.trimmed();
    id.remove(QLatin1Char('/'));
    id.remove(QLatin1Char('\\'));
    return id;
}

bool isTorrentBacked(const domain::DownloadItem& item)
{
    return item.backendKind == domain::DownloadBackendKind::Torrent;
}

/// Every payload candidate under `dir`, skipping the dotfiles both
/// caches scatter around (`.last-used`, `.pinned`, libtorrent's
/// `.<hash>.parts` and resume data).
QList<QFileInfo> payloadCandidates(const QString& dir)
{
    QList<QFileInfo> out;
    QDirIterator it(dir, QDir::Files | QDir::NoDotAndDotDot,
        QDirIterator::Subdirectories);
    while (it.hasNext()) {
        it.next();
        const QFileInfo fi = it.fileInfo();
        if (fi.fileName().startsWith(QLatin1Char('.'))) {
            continue;
        }
        out.append(fi);
    }
    return out;
}

/// Pick the row's own file out of `candidates`. A season pack shares
/// one torrent directory across many assets, so an ambiguous result
/// resolves to nothing rather than to a confidently wrong episode.
QString pickPayload(const QList<QFileInfo>& candidates,
    const domain::DownloadItem& item)
{
    if (candidates.isEmpty()) {
        return {};
    }
    if (candidates.size() == 1) {
        return candidates.first().absoluteFilePath();
    }

    // 1. Exact file-name match on the hint the indexer gave us.
    if (!item.fileNameHint.isEmpty()) {
        for (const auto& fi : candidates) {
            if (fi.fileName().compare(item.fileNameHint,
                    Qt::CaseInsensitive)
                == 0) {
                return fi.absoluteFilePath();
            }
        }
    }

    // 2. Exact byte size. Precise inside a pack, where every member
    //    sits in the same directory but no two share a size.
    if (item.expectedSizeBytes && *item.expectedSizeBytes > 0) {
        QString match;
        int hits = 0;
        for (const auto& fi : candidates) {
            if (fi.size() == *item.expectedSizeBytes) {
                match = fi.absoluteFilePath();
                ++hits;
            }
        }
        if (hits == 1) {
            return match;
        }
    }

    // 3. Substring on the hint — release names often carry extra
    //    tags the on-disk name drops, or vice versa.
    if (!item.fileNameHint.isEmpty()) {
        QString match;
        int hits = 0;
        for (const auto& fi : candidates) {
            if (fi.fileName().contains(item.fileNameHint,
                    Qt::CaseInsensitive)) {
                match = fi.absoluteFilePath();
                ++hits;
            }
        }
        if (hits == 1) {
            return match;
        }
    }

    return {};
}

} // namespace

QString locateAssetDir(const domain::DownloadItem& item)
{
    if (isTorrentBacked(item)) {
        const auto hash = normalizedHash(item.infoHash);
        if (!hash.isEmpty()) {
            return cache::torrentsDir().absoluteFilePath(hash);
        }
        // No hash to key on — the stored column is the best we have.
        return item.localDir;
    }

    const auto assetId = normalizedAssetId(item.assetId);
    if (!assetId.isEmpty()) {
        return cache::mediaDir().absoluteFilePath(assetId);
    }
    return item.localDir;
}

AssetLocation locateAsset(const domain::DownloadItem& item)
{
    AssetLocation out;
    out.dir = locateAssetDir(item);
    if (out.dir.isEmpty()) {
        return out;
    }
    out.exists = QFileInfo(out.dir).isDir();
    if (!out.exists) {
        return out;
    }
    out.file = pickPayload(payloadCandidates(out.dir), item);
    return out;
}

} // namespace kinema::core

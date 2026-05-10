// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "api/RealDebrid.h"

#include <QObject>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::download {

/// Outcome of the multi-step RD resolution workflow.
struct ResolvedRdLink {
    /// Hoster URL the `HttpAssetSession` will fetch from. Only valid
    /// for ~24 hours \u2014 callers should re-resolve on 401 / 403.
    QUrl downloadUrl;
    /// File size announced by RD's unrestrict response. Authoritative
    /// upper bound for the local sparse file.
    qint64 fileSize = 0;
    /// File name announced by RD. Used as a fallback display name.
    QString fileName;
    /// RD's torrent id, kept around so callers can `deleteTorrent()`
    /// on cleanup.
    QString rdTorrentId;
};

/**
 * Encapsulates the RD workflow needed to obtain a fresh hoster URL
 * for a given asset:
 *
 *   1. addMagnet(magnet)
 *   2. torrentInfo(rdTorrentId) until the file list is populated
 *   3. choose the best file by `fileIndex` / `fileNameHint`
 *   4. selectFiles(rdTorrentId, chosenIds)
 *   5. torrentInfo(rdTorrentId) again to retrieve produced links
 *   6. unrestrictLink(link)
 *
 * RD's `/torrents/instantAvailability/` cache probe was deprecated
 * upstream (returns 403 / empty objects in practice), so it's no
 * longer part of the flow. When RD already has the bytes the
 * `addMagnet` -> `torrentInfo` cycle resolves in one tick anyway.
 */
class RealDebridResolver : public QObject
{
    Q_OBJECT
public:
    RealDebridResolver(api::RealDebridClient& rd, QObject* parent = nullptr);

    /// Run the full pipeline. Throws on failure.
    QCoro::Task<ResolvedRdLink> resolve(api::AssetRef ref);

    /// Best-effort cleanup of an RD torrent id.
    QCoro::Task<void> cleanup(QString rdTorrentId);

private:
    api::RealDebridClient& m_rd;
};

} // namespace kinema::download

Q_DECLARE_METATYPE(kinema::download::ResolvedRdLink)

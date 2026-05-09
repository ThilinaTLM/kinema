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
 * Encapsulates the seven-step RD workflow needed to obtain a fresh
 * hoster URL for a given asset:
 *
 *   1. instantAvailability(infoHash)
 *   2. choose the best matching variant by `fileIndex` / `fileNameHint`
 *   3. addMagnet(magnet)
 *   4. torrentInfo(rdTorrentId)
 *   5. selectFiles(rdTorrentId, chosenIds)
 *   6. torrentInfo(rdTorrentId) again to retrieve produced links
 *   7. unrestrictLink(link)
 *
 * Steps 1+3 are unconditional; the resolver short-circuits when RD
 * already has a fully cached variant.
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

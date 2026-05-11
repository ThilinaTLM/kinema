// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/RealDebrid.h"
#include "download/DebridResolver.h"

#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::download {

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
class RealDebridResolver : public DebridResolver
{
    Q_OBJECT
public:
    RealDebridResolver(api::RealDebridClient& rd, QObject* parent = nullptr);

    QCoro::Task<ResolvedDebridLink> resolve(domain::AssetRef ref) override;
    QCoro::Task<void> cleanup(QString providerTorrentId) override;

private:
    api::RealDebridClient& m_rd;
};

} // namespace kinema::download

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "download/AssetSession.h"

#include <QString>

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::download {

/**
 * `AssetSession` adapter over the libtorrent-backed
 * `torrent::TorrentStreamingService`. The session represents a
 * single (info hash, selected file) tuple already prepared via
 * `TorrentStreamingService::prepareSession(...)`.
 *
 * Lifetime is owned by the `DownloadManager`. The session must be
 * unregistered from the `LocalMediaServer` before destruction.
 */
class TorrentAssetSession : public AssetSession
{
    Q_OBJECT
public:
    TorrentAssetSession(torrent::TorrentStreamingService& engine,
        QString assetId,
        QString token,
        QString fileName,
        qint64 fileSize,
        QString infoHash,
        QObject* parent = nullptr);
    ~TorrentAssetSession() override;

    QString token() const override { return m_token; }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }

    QCoro::Task<bool> ensureRange(ByteRange range) override;
    QByteArray readRange(ByteRange range) const override;
    void touch() override;

    domain::DownloadMode mode() const override { return m_mode; }
    void setMode(domain::DownloadMode m) override { m_mode = m; }

    void pause() override;
    void resume() override;

    /// Info hash this session was registered against. Used by the
    /// manager when stopping by hash.
    const QString& infoHash() const noexcept { return m_infoHash; }

private:
    torrent::TorrentStreamingService& m_engine;
    QString m_assetId;
    QString m_token;
    QString m_fileName;
    qint64 m_fileSize;
    QString m_infoHash;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

} // namespace kinema::download

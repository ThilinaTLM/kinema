// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "download/AssetSession.h"

#include <QString>
#include <QVector>

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::playback::sources {

/**
 * `AssetSession` adapter over the libtorrent-backed
 * `torrent::TorrentStreamingService`. The session represents a
 * single (info hash, selected file) tuple already prepared via
 * `TorrentStreamingService::prepareSession(...)`.
 *
 * Implements `playback::ports::ByteRangeSource` via the legacy
 * `download::AssetSession` base; the base goes away with the rest
 * of the legacy download/ directory at the end of this refactor
 * step.
 */
class TorrentAssetSession : public kinema::download::AssetSession
{
    Q_OBJECT
public:
    TorrentAssetSession(kinema::torrent::TorrentStreamingService& engine,
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

    QCoro::Task<bool> ensureRange(kinema::torrent::ByteRange range) override;
    QByteArray readRange(kinema::torrent::ByteRange range) const override;
    void touch() override;

    /// Forwards to `TorrentStreamingService::filesForInfoHash`
    /// so series adjacency can resolve through the unified
    /// `AssetSession::files()` API.
    QVector<kinema::torrent::TorrentFileEntry> files() const override;

    domain::DownloadMode mode() const override { return m_mode; }
    void setMode(domain::DownloadMode m) override { m_mode = m; }

    void pause() override;
    void resume() override;

    /// Info hash this session was registered against. Used by the
    /// manager when stopping by hash.
    const QString& infoHash() const noexcept { return m_infoHash; }

private:
    kinema::torrent::TorrentStreamingService& m_engine;
    QString m_assetId;
    QString m_token;
    QString m_fileName;
    qint64 m_fileSize;
    QString m_infoHash;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

} // namespace kinema::playback::sources

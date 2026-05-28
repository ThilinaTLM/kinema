// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/sources/AssetSession.h"

#include <QString>
#include <QVector>

namespace kinema::playback::torrent {
class LibtorrentClient;
}

namespace kinema::playback::sources {

/**
 * `playback::sources::AssetSession` adapter over the
 * libtorrent-backed `playback::torrent::LibtorrentClient`. The
 * session represents a single (info hash, selected file) tuple
 * already prepared via `LibtorrentClient::prepareSession(...)`.
 *
 * Implements `playback::ports::ByteRangeSource` through the
 * `AssetSession` QObject base that lives alongside in
 * `playback::sources`.
 */
class TorrentAssetSession : public AssetSession
{
    Q_OBJECT
public:
    TorrentAssetSession(playback::torrent::LibtorrentClient& engine,
        QString assetId,
        QString token,
        QString fileName,
        qint64 fileSize,
        QString infoHash,
        QObject* parent = nullptr);
    ~TorrentAssetSession() override;

    /// Opaque session token retained from the engine handshake.
    /// Used historically by the legacy `LocalMediaServer` URL
    /// generation; the new gateway keys by `assetId()` directly, so
    /// this accessor only survives for diagnostics and tests.
    QString token() const { return m_token; }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }

    QCoro::Task<bool> ensureRange(kinema::torrent::ByteRange range) override;
    QByteArray readRange(kinema::torrent::ByteRange range) const override;
    void touch() override;

    /// Forwards to `LibtorrentClient::filesForInfoHash` so series
    /// adjacency can resolve through the unified
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
    playback::torrent::LibtorrentClient& m_engine;
    QString m_assetId;
    QString m_token;
    QString m_fileName;
    qint64 m_fileSize;
    QString m_infoHash;
    domain::DownloadMode m_mode = domain::DownloadMode::OnDemand;
};

} // namespace kinema::playback::sources

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "download/TorrentAssetSession.h"

#include "torrent/TorrentStreamingService.h"

namespace kinema::download {

TorrentAssetSession::TorrentAssetSession(
    torrent::TorrentStreamingService& engine,
    QString assetId,
    QString token,
    QString fileName,
    qint64 fileSize,
    QString infoHash,
    QObject* parent)
    : AssetSession(parent)
    , m_engine(engine)
    , m_assetId(std::move(assetId))
    , m_token(std::move(token))
    , m_fileName(std::move(fileName))
    , m_fileSize(fileSize)
    , m_infoHash(std::move(infoHash))
{
}

TorrentAssetSession::~TorrentAssetSession() = default;

QCoro::Task<bool> TorrentAssetSession::ensureRange(ByteRange range)
{
    co_return co_await m_engine.ensureRange(m_token, range);
}

QByteArray TorrentAssetSession::readRange(ByteRange range) const
{
    return m_engine.readRange(m_token, range);
}

void TorrentAssetSession::touch()
{
    m_engine.touchToken(m_token);
}

} // namespace kinema::download

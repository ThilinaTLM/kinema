// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/sources/TorrentAssetSession.h"

#include "playback/torrent/LibtorrentClient.h"

namespace kinema::playback::sources {

TorrentAssetSession::TorrentAssetSession(playback::torrent::LibtorrentClient& engine,
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
    // Filter the engine's per-hash signals down to *our* hash.
    connect(&m_engine,
            &playback::torrent::LibtorrentClient::statsUpdated,
            this,
            [this](const QString& hash,
                   qint64 doneBytes,
                   qint64 ratePayloadBps,
                   int peers,
                   int seeds,
                   int etaSeconds,
                   bool /*finished*/) {
                if (hash != m_infoHash) {
                    return;
                }
                Q_EMIT cachedBytesChanged(doneBytes);
                Q_EMIT liveStatsChanged(ratePayloadBps, peers, seeds, etaSeconds);
            });
    connect(&m_engine,
            &playback::torrent::LibtorrentClient::torrentFinished,
            this,
            [this](const QString& hash) {
                if (hash == m_infoHash) {
                    Q_EMIT completed();
                }
            });
    connect(&m_engine,
            &playback::torrent::LibtorrentClient::torrentFailed,
            this,
            [this](const QString& hash, const QString& reason) {
                if (hash == m_infoHash) {
                    Q_EMIT failed(reason);
                }
            });
}

TorrentAssetSession::~TorrentAssetSession() = default;

QCoro::Task<bool> TorrentAssetSession::ensureRange(kinema::core::ByteRange range)
{
    co_return co_await m_engine.ensureRange(m_token, range);
}

QByteArray TorrentAssetSession::readRange(kinema::core::ByteRange range) const
{
    return m_engine.readRange(m_token, range);
}

void TorrentAssetSession::touch()
{
    m_engine.touchToken(m_token);
}

QVector<kinema::torrent::TorrentFileEntry> TorrentAssetSession::files() const
{
    return m_engine.filesForInfoHash(m_infoHash);
}

void TorrentAssetSession::pause()
{
    m_engine.pauseInfoHash(m_infoHash);
}

void TorrentAssetSession::resume()
{
    m_engine.resumeInfoHash(m_infoHash);
}

} // namespace kinema::playback::sources

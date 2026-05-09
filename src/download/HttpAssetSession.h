// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "download/AssetSession.h"
#include "download/RealDebridResolver.h"

#include <QFile>
#include <QHash>
#include <QMutex>
#include <QQueue>
#include <QString>
#include <QUrl>

#include <memory>
#include <vector>

namespace kinema::core {
class HttpClient;
}

namespace kinema::config {
class DownloadSettings;
}

namespace kinema::download {

/**
 * RD-backed `AssetSession` that streams an upstream hoster URL into a
 * local sparse file in fixed-size chunks. Each chunk is a chunk of
 * 4 MiB by default; availability is tracked in a side-car bitmap so
 * the local cache survives restarts.
 *
 * Foreground `ensureRange()` requests preempt background prefetch
 * jobs by jumping to the head of the work queue.
 *
 * On 401/403 from the upstream URL the session re-runs the RD
 * resolution pipeline once and retries the request. URLs are not
 * persisted across sessions because RD URLs expire.
 */
class HttpAssetSession : public AssetSession
{
    Q_OBJECT
public:
    HttpAssetSession(core::HttpClient& http,
        RealDebridResolver& resolver,
        const config::DownloadSettings& settings,
        api::AssetRef ref,
        QString assetId,
        QString localDir,
        QObject* parent = nullptr);
    ~HttpAssetSession() override;

    QString token() const override { return m_token; }
    QString assetId() const override { return m_assetId; }
    QString fileName() const override { return m_fileName; }
    qint64 fileSize() const override { return m_fileSize; }
    qint64 cachedBytes() const override;

    QCoro::Task<bool> ensureRange(ByteRange range) override;
    QByteArray readRange(ByteRange range) const override;
    void touch() override;

    /// Resolve the upstream hoster URL (idempotent). Throws on failure.
    QCoro::Task<void> ensureResolved();

    /// Sequential prefetch of the entire file in the background.
    /// Foreground `ensureRange` calls preempt by lifting their range
    /// to the head of the queue. Safe to call multiple times.
    QCoro::Task<void> prefetchAll();

    const api::AssetRef& ref() const noexcept { return m_ref; }
    const QString& localDir() const noexcept { return m_localDir; }

    /// Chunk size in bytes. Test hook \u2014 prefer the constructor for
    /// production code.
    void setChunkSize(qint64 bytes);
    qint64 chunkSize() const noexcept { return m_chunkSize; }

private:
    QCoro::Task<bool> ensureChunk(int chunkIndex);
    QCoro::Task<bool> fetchChunk(int chunkIndex);
    void loadChunkMap();
    void saveChunkMap() const;
    QString chunkMapPath() const;
    QString payloadPath() const;
    int chunkIndexForByte(qint64 byte) const noexcept;
    qint64 chunkStart(int idx) const noexcept;
    qint64 chunkEndExclusive(int idx) const noexcept;
    bool isChunkAvailable(int idx) const;
    void markChunkAvailable(int idx);
    void ensureFileSizedToTotal();

    core::HttpClient& m_http;
    RealDebridResolver& m_resolver;
    const config::DownloadSettings& m_settings;

    api::AssetRef m_ref;
    QString m_assetId;
    QString m_token;
    QString m_localDir;
    QString m_fileName;
    qint64 m_fileSize = -1;
    qint64 m_chunkSize = 4LL * 1024LL * 1024LL;

    QUrl m_upstream;
    QString m_rdTorrentId;
    bool m_resolveInFlight = false;

    /// `m_chunkAvailable[i] == true` means chunk i is fully on disk.
    std::vector<bool> m_chunkAvailable;
    int m_totalChunks = 0;
};

} // namespace kinema::download

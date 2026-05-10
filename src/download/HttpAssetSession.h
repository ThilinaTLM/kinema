// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "download/AssetSession.h"
#include "download/DebridResolver.h"

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
 * Debrid-backed `AssetSession` that streams an upstream hoster URL
 * into a local sparse file in fixed-size chunks. Each chunk is
 * 4 MiB by default; availability is tracked in a side-car bitmap so
 * the local cache survives restarts.
 *
 * Foreground `ensureRange()` requests preempt background prefetch
 * jobs by jumping to the head of the work queue.
 *
 * On 401/403 from the upstream URL the session re-runs the debrid
 * provider's resolution pipeline once and retries the request. URLs
 * are not persisted across sessions because hoster URLs expire.
 */
class HttpAssetSession : public AssetSession
{
    Q_OBJECT
public:
    HttpAssetSession(core::HttpClient& http,
        DebridResolver& resolver,
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

    api::DownloadMode mode() const override { return m_mode; }
    void setMode(api::DownloadMode m) override { m_mode = m; }

    void pause() override;
    void resume() override;
    bool isPaused() const noexcept { return m_paused; }

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
    DebridResolver& m_resolver;
    const config::DownloadSettings& m_settings;

    api::AssetRef m_ref;
    QString m_assetId;
    QString m_token;
    QString m_localDir;
    QString m_fileName;
    qint64 m_fileSize = -1;
    qint64 m_chunkSize = 4LL * 1024LL * 1024LL;

    QUrl m_upstream;
    QString m_providerTorrentId;
    bool m_resolveInFlight = false;

    /// User-visible mode. Defaults to OnDemand; promoted to Full
    /// when the manager triggers `prefetchAll()` via `changeMode`.
    api::DownloadMode m_mode = api::DownloadMode::OnDemand;

    /// User-paused flag honoured by `prefetchAll()` between chunk
    /// boundaries. Range fetches driven by the player ignore it so
    /// playback of already-cached bytes keeps working.
    bool m_paused = false;

    /// `m_chunkAvailable[i] == true` means chunk i is fully on disk.
    std::vector<bool> m_chunkAvailable;
    int m_totalChunks = 0;

    /// EWMA bookkeeping for the synthesised download rate. Updated
    /// every time a chunk lands so the UI sees a live MiB/s figure
    /// for HTTP-backed assets too.
    qint64 m_emaRateBps = 0;
    qint64 m_lastSampleBytes = 0;
    qint64 m_lastSampleAtMsec = 0;
};

} // namespace kinema::download

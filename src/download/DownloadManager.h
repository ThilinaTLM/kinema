// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"
#include "api/Media.h"
#include "api/PlaybackContext.h"

#include <QHash>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QUrl>

#include <QCoro/QCoroTask>

#include <map>
#include <memory>

namespace kinema::api {
class RealDebridClient;
}

namespace kinema::config {
class DownloadSettings;
}

namespace kinema::core {
class DownloadStore;
class HttpClient;
class MediaCache;
}

namespace kinema::torrent {
class TorrentStreamingService;
}

namespace kinema::download {

class AssetSession;
class HttpAssetSession;
class LocalMediaServer;
class RealDebridResolver;
class TorrentAssetSession;

/**
 * Long-lived orchestrator for the unified downloader.
 *
 * Owns: the localhost media server, every active asset session, the
 * RD resolver, and the bridge to the persistent `DownloadStore`. UI
 * code reaches the manager via `controllers::DownloadController`.
 *
 * Responsibilities:
 *  - Choose a backend for an asset (RD-cached / RD-magnet / torrent).
 *  - Create / reuse asset sessions.
 *  - Hand out localhost playback URLs.
 *  - Persist `DownloadItem` rows for both ephemeral and pinned
 *    downloads.
 *  - Run cache eviction.
 */
class DownloadManager : public QObject
{
    Q_OBJECT
public:
    DownloadManager(core::HttpClient& http,
        api::RealDebridClient& rd,
        torrent::TorrentStreamingService& torrentEngine,
        core::DownloadStore& store,
        core::MediaCache& cache,
        const config::DownloadSettings& settings,
        QObject* parent = nullptr);
    ~DownloadManager() override;

    /// Return a localhost URL the player can stream from. Selects
    /// the appropriate backend and starts buffering as needed.
    QCoro::Task<QUrl> prepareForPlayback(api::Stream stream,
        api::PlaybackContext ctx);

    /// Enqueue a download (with the given disposition) and start
    /// background prefetch.
    void enqueueDownload(const api::Stream& stream,
        const api::PlaybackContext& ctx,
        api::CacheDisposition disposition);

    /// Look up the persisted item by playback key, if any.
    std::optional<api::DownloadItem> findForKey(const api::PlaybackKey& key) const;

    /// Toggle a pin marker. Updates both the cache marker and the
    /// store row. No-op when no row exists yet.
    void pin(const QString& assetId, bool on);

    /// Delete the persisted row plus any cached files. Stops the
    /// session if active.
    void remove(const QString& assetId, bool deleteFiles = true);

    /// Stop work for the given assetId without removing files.
    void cancel(const QString& assetId);

public Q_SLOTS:
    void retry(const QString& assetId);
    void runEviction();

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void itemChanged(const QString& assetId);

private:
    QCoro::Task<QUrl> prepareTorrent(api::AssetRef ref,
        api::Stream stream, api::PlaybackContext ctx);
    QCoro::Task<QUrl> prepareHttp(api::AssetRef ref,
        api::Stream stream, api::PlaybackContext ctx);

    api::DownloadBackendKind chooseBackend(const api::Stream& s) const;
    api::DownloadItem buildItem(const api::AssetRef& ref,
        const api::Stream& s, const api::PlaybackContext& ctx,
        api::DownloadBackendKind backend,
        api::CacheDisposition disposition) const;

    core::HttpClient& m_http;
    api::RealDebridClient& m_rd;
    torrent::TorrentStreamingService& m_torrentEngine;
    core::DownloadStore& m_store;
    core::MediaCache& m_cache;
    const config::DownloadSettings& m_settings;

    std::unique_ptr<RealDebridResolver> m_resolver;
    std::unique_ptr<LocalMediaServer> m_server;

    /// Active sessions keyed by `assetId`. Owned via std::unique_ptr.
    std::map<QString, std::unique_ptr<AssetSession>> m_sessions;
};

} // namespace kinema::download

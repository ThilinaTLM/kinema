// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/ByteRangeSource.h"
#include "playback/ports/SessionFileCatalog.h"
#include "playback/transfer/LiveAssetStats.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QSet>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::core {
class MediaCache;
}

namespace kinema::playback::ports {
class DownloadRepository;
}

namespace kinema::playback::streaming {
class LocalHttpStreamGateway;
}

namespace kinema::playback::torrent {
class LibtorrentClient;
}

namespace kinema::playback::transfer {

class BackendRegistry;
class SessionRegistry;
class TransferSession;
class TransferSupervisor;

/**
 * Application-level entry point for the transfer subsystem.
 *
 * Composes the registries / supervisor / gateway / repository the
 * `DownloadManager` previously owned inline. Every play /
 * save-offline / lifecycle path runs through here; no consumer
 * reaches into `DownloadManager` after sub-commit 10.
 *
 * Public surface intentionally mirrors the `DownloadManager` API
 * the controllers depended on (`prepareForPlayback` becomes
 * `ensurePlayable`, `enqueueDownload` becomes `saveOffline`).
 */
class TransferUseCase : public QObject,
    public ports::SessionFileCatalog
{
    Q_OBJECT
public:
    TransferUseCase(BackendRegistry& backends,
        SessionRegistry& sessions,
        TransferSupervisor& supervisor,
        streaming::LocalHttpStreamGateway& gateway,
        ports::DownloadRepository& repo,
        core::MediaCache& cache,
        playback::torrent::LibtorrentClient& torrentEngine,
        QObject* parent = nullptr);
    ~TransferUseCase() override;

    /// Open or reuse a session for the asset and return the
    /// localhost URL the player should load. Defaults to
    /// `OnDemand + Ephemeral` for a fresh session; never downgrades
    /// an existing Full session. Also attaches the player.
    QCoro::Task<QUrl> ensurePlayable(domain::Stream stream,
        domain::PlaybackContext ctx,
        std::optional<domain::DownloadBackendKind> backendOverride
            = std::nullopt);

    /// Open a Full/Pinned session in the background; upgrades an
    /// existing OnDemand session in place rather than spawning a
    /// duplicate.
    void saveOffline(domain::Stream stream,
        domain::PlaybackContext ctx,
        std::optional<domain::DownloadBackendKind> backendOverride
            = std::nullopt);

    /// Promote an OnDemand session in place to Full/Pinned.
    void upgradeToFull(const QString& assetId);

    void attachPlayer(const QString& assetId);
    void detachPlayer(const QString& assetId);

    void pause(const QString& assetId);
    void resumeTransfer(const QString& assetId);
    void retry(const QString& assetId);
    void cancel(const QString& assetId);
    void remove(const QString& assetId, bool deleteFiles = true);
    void pin(const QString& assetId, bool on);

    /// Re-attach every persisted Full row that is not in a terminal
    /// state. Called once from `ServiceContainer` after the
    /// use-case is wired up. OnDemand rows are intentionally
    /// skipped because they are consumer-driven.
    void resumePersisted();

    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const;
    std::optional<LiveAssetStats> liveStatsFor(
        const QString& assetId) const;
    QSet<QString> attachedPlayerAssetIds() const;

    /// Cold-recovery hook used by `LocalHttpStreamGateway` to
    /// materialise a session for an `assetId` that has no live
    /// entry yet. Walks the repository for a persisted row and
    /// opens it via the backend; returns nullptr when recovery is
    /// impossible. The returned pointer is borrowed from the
    /// `SessionRegistry`; do not delete.
    QCoro::Task<ports::ByteRangeSource*> ensureSessionForAssetId(
        const QString& assetId);

    // SessionFileCatalog
    QVector<domain::MediaFileEntry> filesForStreamRef(
        const domain::HistoryStreamRef& streamRef) const override;
    QVector<domain::MediaFileEntry> filesForAssetId(
        const QString& assetId) const override;

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void itemChanged(const QString& assetId);

private:
    /// Open (or reuse) a session and persist its row. Returns the
    /// localhost URL. Throws on backend failure.
    QCoro::Task<QUrl> openSession(domain::AssetRef ref,
        domain::Stream stream,
        domain::PlaybackContext ctx,
        domain::DownloadMode mode,
        domain::CacheDisposition disposition,
        std::optional<domain::DownloadBackendKind> backendOverride);

    /// Fire-and-forget wrapper around `openSession` for background
    /// downloads. Translates failure into a `Failed` repository
    /// row + `itemChanged`.
    QCoro::Task<void> startBackground(domain::AssetRef ref,
        domain::Stream stream,
        domain::PlaybackContext ctx,
        domain::DownloadMode mode,
        domain::CacheDisposition disposition,
        std::optional<domain::DownloadBackendKind> backendOverride);

    /// Compose the persisted row from the inputs. Mirrors the
    /// legacy `DownloadManager::buildItem` shape.
    domain::DownloadItem buildItem(const domain::AssetRef& ref,
        const domain::Stream& s, const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend,
        domain::DownloadMode mode,
        domain::CacheDisposition disposition) const;

    /// Walk the registry for live sessions sharing `infoHash`
    /// (except `keepAssetId`) and tear them down so the new
    /// session can take over. Used on series-pack episode swap
    /// where the torrent backend can only serve one file per hash.
    void supersedeSameHashSessions(const QString& infoHash,
        const QString& keepAssetId);

    BackendRegistry& m_backends;
    SessionRegistry& m_sessions;
    TransferSupervisor& m_supervisor;
    streaming::LocalHttpStreamGateway& m_gateway;
    ports::DownloadRepository& m_repo;
    core::MediaCache& m_cache;
    playback::torrent::LibtorrentClient& m_torrentEngine;
};

} // namespace kinema::playback::transfer

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "domain/Media.h"
#include "domain/MediaFile.h"
#include "domain/PlaybackContext.h"
#include "playback/ports/SessionFileCatalog.h"

#include <QCoro/QCoroTask>

#include <QObject>
#include <QSet>
#include <QString>
#include <QUrl>

#include <optional>

namespace kinema::download {
class DownloadManager;
struct LiveAssetStats;
}

namespace kinema::playback::transfer {

/**
 * Application-level entry point for the transfer subsystem.
 *
 * This is the long-term replacement for the play/save-offline
 * surface of `services::StreamActions` and the public surface of
 * `download::DownloadManager`. During the refactor it currently
 * delegates to a wrapped `DownloadManager`; once each backend has
 * been ported to `MediaSourcePort`, the inner pieces (session
 * registry, supervisor, backend registry) will be composed here
 * directly and the wrapped manager removed.
 */
class TransferUseCase : public QObject,
    public ports::SessionFileCatalog
{
    Q_OBJECT
public:
    explicit TransferUseCase(download::DownloadManager& manager,
        QObject* parent = nullptr);
    ~TransferUseCase() override;

    /// Open or reuse an OnDemand/Ephemeral session and return the
    /// localhost URL the player should load.
    QCoro::Task<QUrl> ensurePlayable(domain::Stream stream,
        domain::PlaybackContext ctx,
        std::optional<domain::DownloadBackendKind> backendOverride
            = std::nullopt);

    /// Open a Full/Pinned session in the background; upgrades the
    /// existing session in place when one already exists.
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

    /// Background resume of every persisted Full/Pinned row.
    void resumePersisted();

    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const;
    std::optional<download::LiveAssetStats> liveStatsFor(
        const QString& assetId) const;
    QSet<QString> attachedPlayerAssetIds() const;

    // SessionFileCatalog
    QVector<domain::MediaFileEntry> filesForStreamRef(
        const domain::HistoryStreamRef& streamRef) const override;
    QVector<domain::MediaFileEntry> filesForAssetId(
        const QString& assetId) const override;

    /// Read-only access to the wrapped manager. Kept while the
    /// refactor still has direct consumers of `DownloadManager`
    /// (Phase 4 will gradually empty this).
    download::DownloadManager& manager() noexcept { return m_manager; }

Q_SIGNALS:
    void statusMessage(const QString& text, int timeoutMs = 3000);
    void itemChanged(const QString& assetId);

private:
    download::DownloadManager& m_manager;
};

} // namespace kinema::playback::transfer

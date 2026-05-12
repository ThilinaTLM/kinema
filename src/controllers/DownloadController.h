// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QObject>
#include <QSet>
#include <QString>

#include <optional>

namespace kinema::core {
class DownloadStore;
}

namespace kinema::download {
class DownloadManager;
}

namespace kinema::controllers {

/**
 * QObject facade for the unified downloader. Exposes the manager's
 * actions through invokable slots and re-emits the store's
 * `changed()` signal so QML view-models can refresh.
 *
 * The split between Play (`OnDemand`) and Download (`Full`) lives
 * here so view-models don't have to think about lifecycle policies:
 *
 *   - `play()` / `playWithBackend()`   -> manager `prepareForPlayback`
 *   - `download()` / `downloadWithBackend()` -> manager `enqueueDownload`
 *
 * `upgradeToFull/pause/resume/attachPlayer/detachPlayer` are forwarders
 * to the matching manager methods.
 */
class DownloadController : public QObject
{
    Q_OBJECT
public:
    DownloadController(download::DownloadManager& manager,
        core::DownloadStore& store,
        QObject* parent = nullptr);

    QList<domain::DownloadItem> items() const;
    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const;

    /// Snapshot of asset ids that currently have a player attached;
    /// used by the view-model to compute `hasPlayerAttached` per row.
    QSet<QString> attachedPlayerAssetIds() const;

public Q_SLOTS:
    /// Background full-file download with `Pinned` disposition.
    void download(const domain::Stream& stream,
        const domain::PlaybackContext& ctx);
    void downloadWithBackend(const domain::Stream& stream,
        const domain::PlaybackContext& ctx,
        domain::DownloadBackendKind backend);

    /// Promote an existing OnDemand session to Full + Pinned.
    void upgradeToFull(const QString& assetId);

    /// User pause/resume + player attach/detach plumbing.
    void pause(const QString& assetId);
    void resume(const QString& assetId);
    void attachPlayer(const QString& assetId);
    void detachPlayer(const QString& assetId);

    void retry(const QString& assetId);
    void cancel(const QString& assetId);
    void remove(const QString& assetId, bool deleteFiles);
    void pin(const QString& assetId, bool on);

Q_SIGNALS:
    void changed();
    void statusMessage(const QString& text, int timeoutMs);

private:
    download::DownloadManager& m_manager;
    core::DownloadStore& m_store;
};

} // namespace kinema::controllers

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"
#include "playback/transfer/LiveAssetStats.h"

#include <QObject>
#include <QSet>
#include <QString>

#include <optional>

namespace kinema::core {
class DownloadStore;
}

namespace kinema::playback::transfer {
class TransferUseCase;
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
 *   - `play()` / `playWithBackend()`   -> use-case `ensurePlayable`
 *   - `download()` / `downloadWithBackend()` -> use-case `saveOffline`
 *
 * `upgradeToFull/pause/resume/attachPlayer/detachPlayer` are forwarders
 * to the matching `TransferUseCase` methods.
 */
class DownloadController : public QObject
{
    Q_OBJECT
public:
    DownloadController(playback::transfer::TransferUseCase& useCase,
        core::DownloadStore& store,
        QObject* parent = nullptr);

    QList<domain::DownloadItem> items() const;
    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const;

    /// Single-row fetch for the hot path: view-models bind to
    /// `DownloadController::itemChanged(assetId)` and re-read just
    /// that one row instead of doing a full `loadAll()` per tick.
    std::optional<domain::DownloadItem> find(const QString& assetId) const;

    /// Snapshot of asset ids that currently have a player attached;
    /// used by the view-model to compute `hasPlayerAttached` per row.
    QSet<QString> attachedPlayerAssetIds() const;

    /// Live transient telemetry (rate / peers / seeds / ETA) for
    /// the asset's currently-active transfer session. Returns
    /// nullopt when no session is active. Exposed here so
    /// `DownloadsViewModel` can read per-row stats through the
    /// controller boundary instead of holding its own
    /// `TransferUseCase&`.
    std::optional<playback::transfer::LiveAssetStats> liveStatsFor(
        const QString& assetId) const;

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
    /// Structural change to the list (insert / remove / reorder).
    /// Forwarded from `DownloadStore::changed`, which is already
    /// coalesced via the store's single-shot scheduler.
    void changed();

    /// In-place mutation of a single row — progress, live stats,
    /// state, error, etc. View-models translate this into a
    /// `dataChanged` on the affected row instead of a full reset.
    void itemChanged(const QString& assetId);

    void statusMessage(const QString& text, int timeoutMs);

private:
    playback::transfer::TransferUseCase& m_useCase;
    core::DownloadStore& m_store;
};

} // namespace kinema::controllers

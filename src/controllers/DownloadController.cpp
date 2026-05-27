// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/DownloadController.h"

#include "core/persistence/DownloadStore.h"
#include "playback/transfer/TransferUseCase.h"

namespace kinema::controllers {

DownloadController::DownloadController(
    playback::transfer::TransferUseCase& useCase,
    core::DownloadStore& store, QObject* parent)
    : QObject(parent)
    , m_useCase(useCase)
    , m_store(store)
{
    // Structural store changes (insert / remove / coalesced bursts)
    // map to the list-shape signal.
    connect(&m_store, &core::DownloadStore::changed,
        this, &DownloadController::changed);
    connect(&m_useCase,
        &playback::transfer::TransferUseCase::statusMessage,
        this, &DownloadController::statusMessage);
    // In-place per-row mutations stay separate so view-models can
    // route them to a `dataChanged`-only path and keep delegates
    // (and any open popups) alive across ticks.
    connect(&m_useCase,
        &playback::transfer::TransferUseCase::itemChanged,
        this, &DownloadController::itemChanged);
}

QList<domain::DownloadItem> DownloadController::items() const
{
    return m_store.loadAll();
}

std::optional<domain::DownloadItem> DownloadController::findForKey(
    const domain::PlaybackKey& key) const
{
    return m_store.findForKey(key);
}

std::optional<domain::DownloadItem> DownloadController::find(
    const QString& assetId) const
{
    return m_store.find(assetId);
}

QSet<QString> DownloadController::attachedPlayerAssetIds() const
{
    return m_useCase.attachedPlayerAssetIds();
}

void DownloadController::download(const domain::Stream& stream,
    const domain::PlaybackContext& ctx)
{
    m_useCase.saveOffline(stream, ctx);
}

void DownloadController::downloadWithBackend(const domain::Stream& stream,
    const domain::PlaybackContext& ctx,
    domain::DownloadBackendKind backend)
{
    m_useCase.saveOffline(stream, ctx, backend);
}

void DownloadController::upgradeToFull(const QString& assetId)
{
    m_useCase.upgradeToFull(assetId);
}

void DownloadController::pause(const QString& assetId)
{
    m_useCase.pause(assetId);
}

void DownloadController::resume(const QString& assetId)
{
    m_useCase.resumeTransfer(assetId);
}

void DownloadController::attachPlayer(const QString& assetId)
{
    m_useCase.attachPlayer(assetId);
}

void DownloadController::detachPlayer(const QString& assetId)
{
    m_useCase.detachPlayer(assetId);
}

void DownloadController::retry(const QString& assetId)
{
    m_useCase.retry(assetId);
}

void DownloadController::cancel(const QString& assetId)
{
    m_useCase.cancel(assetId);
}

void DownloadController::remove(const QString& assetId, bool deleteFiles)
{
    m_useCase.remove(assetId, deleteFiles);
}

void DownloadController::pin(const QString& assetId, bool on)
{
    m_useCase.pin(assetId, on);
}

} // namespace kinema::controllers

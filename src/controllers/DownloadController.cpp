// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "controllers/DownloadController.h"

#include "core/DownloadStore.h"
#include "download/DownloadManager.h"

namespace kinema::controllers {

DownloadController::DownloadController(download::DownloadManager& manager,
    core::DownloadStore& store, QObject* parent)
    : QObject(parent)
    , m_manager(manager)
    , m_store(store)
{
    connect(&m_store, &core::DownloadStore::changed,
        this, &DownloadController::changed);
    connect(&m_manager, &download::DownloadManager::statusMessage,
        this, &DownloadController::statusMessage);
    connect(&m_manager, &download::DownloadManager::itemChanged,
        this, &DownloadController::changed);
}

QList<api::DownloadItem> DownloadController::items() const
{
    return m_store.loadAll();
}

std::optional<api::DownloadItem> DownloadController::findForKey(
    const api::PlaybackKey& key) const
{
    return m_store.findForKey(key);
}

QSet<QString> DownloadController::attachedPlayerAssetIds() const
{
    return m_manager.attachedPlayerAssetIds();
}

void DownloadController::download(const api::Stream& stream,
    const api::PlaybackContext& ctx)
{
    m_manager.enqueueDownload(stream, ctx);
}

void DownloadController::downloadWithBackend(const api::Stream& stream,
    const api::PlaybackContext& ctx,
    api::DownloadBackendKind backend)
{
    m_manager.enqueueDownload(stream, ctx, backend);
}

void DownloadController::upgradeToFull(const QString& assetId)
{
    m_manager.upgradeToFull(assetId);
}

void DownloadController::pause(const QString& assetId)
{
    m_manager.pause(assetId);
}

void DownloadController::resume(const QString& assetId)
{
    m_manager.resume(assetId);
}

void DownloadController::attachPlayer(const QString& assetId)
{
    m_manager.attachPlayer(assetId);
}

void DownloadController::detachPlayer(const QString& assetId)
{
    m_manager.detachPlayer(assetId);
}

void DownloadController::retry(const QString& assetId)
{
    m_manager.retry(assetId);
}

void DownloadController::cancel(const QString& assetId)
{
    m_manager.cancel(assetId);
}

void DownloadController::remove(const QString& assetId, bool deleteFiles)
{
    m_manager.remove(assetId, deleteFiles);
}

void DownloadController::pin(const QString& assetId, bool on)
{
    m_manager.pin(assetId, on);
}

} // namespace kinema::controllers

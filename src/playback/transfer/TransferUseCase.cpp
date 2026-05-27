// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/transfer/TransferUseCase.h"

#include "download/DownloadManager.h"

#include <algorithm>

namespace kinema::playback::transfer {

namespace {

QVector<domain::MediaFileEntry> lift(
    const QVector<kinema::torrent::TorrentFileEntry>& src)
{
    QVector<domain::MediaFileEntry> out;
    out.reserve(src.size());
    for (const auto& f : src) {
        domain::MediaFileEntry e;
        e.index = f.index;
        e.path = f.path;
        e.size = f.size;
        e.playable = true;
        out.append(e);
    }
    return out;
}

} // namespace

TransferUseCase::TransferUseCase(download::DownloadManager& manager,
    QObject* parent)
    : QObject(parent)
    , m_manager(manager)
{
    connect(&m_manager, &download::DownloadManager::statusMessage,
        this, &TransferUseCase::statusMessage);
    connect(&m_manager, &download::DownloadManager::itemChanged,
        this, &TransferUseCase::itemChanged);
}

TransferUseCase::~TransferUseCase() = default;

QCoro::Task<QUrl> TransferUseCase::ensurePlayable(domain::Stream stream,
    domain::PlaybackContext ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    co_return co_await m_manager.prepareForPlayback(std::move(stream),
        std::move(ctx), backendOverride);
}

void TransferUseCase::saveOffline(domain::Stream stream,
    domain::PlaybackContext ctx,
    std::optional<domain::DownloadBackendKind> backendOverride)
{
    m_manager.enqueueDownload(std::move(stream), std::move(ctx),
        backendOverride);
}

void TransferUseCase::upgradeToFull(const QString& assetId)
{
    m_manager.upgradeToFull(assetId);
}

void TransferUseCase::attachPlayer(const QString& assetId)
{
    m_manager.attachPlayer(assetId);
}

void TransferUseCase::detachPlayer(const QString& assetId)
{
    m_manager.detachPlayer(assetId);
}

void TransferUseCase::pause(const QString& assetId)
{
    m_manager.pause(assetId);
}

void TransferUseCase::resumeTransfer(const QString& assetId)
{
    m_manager.resume(assetId);
}

void TransferUseCase::retry(const QString& assetId)
{
    m_manager.retry(assetId);
}

void TransferUseCase::cancel(const QString& assetId)
{
    m_manager.cancel(assetId);
}

void TransferUseCase::remove(const QString& assetId, bool deleteFiles)
{
    m_manager.remove(assetId, deleteFiles);
}

void TransferUseCase::pin(const QString& assetId, bool on)
{
    m_manager.pin(assetId, on);
}

void TransferUseCase::resumePersisted()
{
    m_manager.resumePersisted();
}

std::optional<domain::DownloadItem> TransferUseCase::findForKey(
    const domain::PlaybackKey& key) const
{
    return m_manager.findForKey(key);
}

std::optional<download::LiveAssetStats> TransferUseCase::liveStatsFor(
    const QString& assetId) const
{
    return m_manager.liveStatsFor(assetId);
}

QSet<QString> TransferUseCase::attachedPlayerAssetIds() const
{
    return m_manager.attachedPlayerAssetIds();
}

QVector<domain::MediaFileEntry> TransferUseCase::filesForStreamRef(
    const domain::HistoryStreamRef& streamRef) const
{
    if (streamRef.infoHash.isEmpty()) {
        return {};
    }
    return lift(m_manager.filesForInfoHash(streamRef.infoHash));
}

QVector<domain::MediaFileEntry> TransferUseCase::filesForAssetId(
    const QString& /*assetId*/) const
{
    // Not currently exposed by `DownloadManager`; would require
    // walking `m_sessions`. Leave empty for now; consumers that
    // need it (series adjacency) use `filesForStreamRef`.
    return {};
}

} // namespace kinema::playback::transfer

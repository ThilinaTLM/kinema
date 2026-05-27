// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "playback/downloads/SqliteDownloadRepository.h"

#include "core/persistence/DownloadStore.h"

namespace kinema::playback::downloads {

SqliteDownloadRepository::SqliteDownloadRepository(
    core::DownloadStore& store)
    : m_store(store)
{
}

void SqliteDownloadRepository::upsert(const domain::DownloadItem& item)
{
    m_store.upsert(item);
}

void SqliteDownloadRepository::updateState(const QString& assetId,
    domain::DownloadState state)
{
    m_store.updateState(assetId, state);
}

void SqliteDownloadRepository::updateCachedBytes(const QString& assetId,
    qint64 cachedBytes,
    std::optional<qint64> /*expectedBytes*/,
    bool complete)
{
    // `core::DownloadStore::updateProgress` already merges these
    // through the same SQL UPDATE; `expectedBytes` is set via the
    // initial `upsert`. We mirror that here.
    m_store.updateProgress(assetId, cachedBytes, complete);
}

void SqliteDownloadRepository::setLastError(const QString& assetId,
    const QString& error)
{
    m_store.updateState(assetId, domain::DownloadState::Failed, error);
}

void SqliteDownloadRepository::remove(const QString& assetId)
{
    m_store.remove(assetId);
}

std::optional<domain::DownloadItem> SqliteDownloadRepository::find(
    const QString& assetId) const
{
    return m_store.find(assetId);
}

QVector<domain::DownloadItem> SqliteDownloadRepository::all() const
{
    return m_store.loadAll().toVector();
}

} // namespace kinema::playback::downloads

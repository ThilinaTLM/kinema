// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QObject>
#include <QString>
#include <QVector>

#include <optional>

namespace kinema::playback::ports {

/**
 * Storage port for downloadable assets. The SQLite adapter wraps
 * `core::DownloadStore`; tests can substitute an in-memory fake.
 */
class DownloadRepository
{
public:
    virtual ~DownloadRepository() = default;

    virtual void upsert(const domain::DownloadItem& item) = 0;
    virtual void updateState(const QString& assetId,
        domain::DownloadState state)
        = 0;
    virtual void updateCachedBytes(const QString& assetId,
        qint64 cachedBytes,
        std::optional<qint64> expectedBytes,
        bool complete)
        = 0;
    virtual void setLastError(const QString& assetId,
        const QString& error)
        = 0;
    /// Mutate the user-visible download mode (OnDemand <-> Full).
    /// Used by `TransferUseCase::upgradeToFull` when promoting an
    /// OnDemand session in place.
    virtual void updateMode(const QString& assetId,
        domain::DownloadMode mode)
        = 0;
    /// Toggle the cache disposition (pinned <-> ephemeral).
    virtual void setDisposition(const QString& assetId,
        domain::CacheDisposition disposition)
        = 0;
    virtual void remove(const QString& assetId) = 0;

    virtual std::optional<domain::DownloadItem> find(
        const QString& assetId) const
        = 0;
    /// Most-recent persisted row matching `key`, when any. Used by
    /// `TransferUseCase::findForKey` so view-models can answer "is
    /// there already a local asset for this title?".
    virtual std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const
        = 0;
    virtual QVector<domain::DownloadItem> all() const = 0;
};

} // namespace kinema::playback::ports

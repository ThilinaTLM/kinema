// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "playback/ports/DownloadRepository.h"

namespace kinema::core {
class DownloadStore;
}

namespace kinema::playback::downloads {

/// SQLite-backed adapter for `DownloadRepository`. Forwards to
/// `core::DownloadStore`.
class SqliteDownloadRepository final : public ports::DownloadRepository
{
public:
    explicit SqliteDownloadRepository(core::DownloadStore& store);
    ~SqliteDownloadRepository() override = default;

    void upsert(const domain::DownloadItem& item) override;
    void updateState(const QString& assetId,
        domain::DownloadState state) override;
    void updateCachedBytes(const QString& assetId,
        qint64 cachedBytes,
        std::optional<qint64> expectedBytes,
        bool complete) override;
    void setLastError(const QString& assetId,
        const QString& error) override;
    void updateMode(const QString& assetId,
        domain::DownloadMode mode) override;
    void setDisposition(const QString& assetId,
        domain::CacheDisposition disposition) override;
    void remove(const QString& assetId) override;

    std::optional<domain::DownloadItem> find(
        const QString& assetId) const override;
    std::optional<domain::DownloadItem> findForKey(
        const domain::PlaybackKey& key) const override;
    QVector<domain::DownloadItem> all() const override;

    core::DownloadStore& store() noexcept { return m_store; }

private:
    core::DownloadStore& m_store;
};

} // namespace kinema::playback::downloads

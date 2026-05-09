// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"

#include <QObject>
#include <QString>

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
 */
class DownloadController : public QObject
{
    Q_OBJECT
public:
    DownloadController(download::DownloadManager& manager,
        core::DownloadStore& store,
        QObject* parent = nullptr);

    QList<api::DownloadItem> items() const;
    std::optional<api::DownloadItem> findForKey(
        const api::PlaybackKey& key) const;

public Q_SLOTS:
    void enqueue(const api::Stream& stream,
        const api::PlaybackContext& ctx,
        bool pinned);
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

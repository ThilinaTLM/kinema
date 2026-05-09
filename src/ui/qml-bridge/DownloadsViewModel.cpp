// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsViewModel.h"

#include "controllers/DownloadController.h"
#include "core/MediaCache.h"

#include <KFormat>

namespace kinema::ui::qml {

DownloadsViewModel::DownloadsViewModel(
    controllers::DownloadController& controller,
    core::MediaCache& cache,
    QObject* parent)
    : QObject(parent)
    , m_controller(controller)
    , m_cache(cache)
    , m_items(new DownloadsListModel(this))
{
    connect(&m_controller, &controllers::DownloadController::changed,
        this, &DownloadsViewModel::refresh);
    refresh();
}

void DownloadsViewModel::refresh()
{
    auto rows = m_controller.items();
    m_items->setItems(rows);
    rebuildCounts();
    Q_EMIT cacheChanged();
}

void DownloadsViewModel::rebuildCounts()
{
    m_activeCount = 0;
    m_completedCount = 0;
    m_failedCount = 0;
    for (const auto& it : m_items->items()) {
        switch (it.state) {
        case api::DownloadState::Queued:
        case api::DownloadState::Preparing:
        case api::DownloadState::Downloading:
        case api::DownloadState::Streaming:
            ++m_activeCount;
            break;
        case api::DownloadState::Completed:
            ++m_completedCount;
            break;
        case api::DownloadState::Failed:
            ++m_failedCount;
            break;
        case api::DownloadState::Paused:
        case api::DownloadState::Cancelled:
            break;
        }
    }
    Q_EMIT countsChanged();
}

qint64 DownloadsViewModel::cacheSizeBytes() const
{
    return m_cache.sizeBytes();
}

qint64 DownloadsViewModel::ephemeralSizeBytes() const
{
    return m_cache.ephemeralSizeBytes();
}

QString DownloadsViewModel::cacheSizeText() const
{
    return KFormat().formatByteSize(cacheSizeBytes());
}

QString DownloadsViewModel::ephemeralSizeText() const
{
    return KFormat().formatByteSize(ephemeralSizeBytes());
}

void DownloadsViewModel::retry(const QString& assetId)
{
    m_controller.retry(assetId);
}

void DownloadsViewModel::cancel(const QString& assetId)
{
    m_controller.cancel(assetId);
}

void DownloadsViewModel::remove(const QString& assetId, bool deleteFiles)
{
    m_controller.remove(assetId, deleteFiles);
}

void DownloadsViewModel::pin(const QString& assetId, bool on)
{
    m_controller.pin(assetId, on);
}

} // namespace kinema::ui::qml

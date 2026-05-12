// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/DownloadsListModel.h"

#include <QObject>
#include <QString>

namespace kinema::controllers {
class DownloadController;
}

namespace kinema::download {
class DownloadManager;
}

namespace kinema::services {
class StreamActions;
}

namespace kinema::ui::qml {

/**
 * Per-page view-model for `DownloadsPage.qml`. Owns a
 * `DownloadsListModel` and forwards row-level actions to the
 * `DownloadController`.
 *
 * Cache budget / usage / eviction live on
 * `TorrentStreamingSettingsViewModel` (settings tab "Downloads"); this
 * VM is purely the runtime ops console.
 */
class DownloadsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(DownloadsListModel* items READ items CONSTANT)
    Q_PROPERTY(int activeCount READ activeCount NOTIFY countsChanged)
    Q_PROPERTY(int pausedCount READ pausedCount NOTIFY countsChanged)
    Q_PROPERTY(int completedCount READ completedCount NOTIFY countsChanged)
    Q_PROPERTY(int failedCount READ failedCount NOTIFY countsChanged)
    Q_PROPERTY(int totalCount READ totalCount NOTIFY countsChanged)
    Q_PROPERTY(qint64 totalDownloadRateBps READ totalDownloadRateBps
        NOTIFY countsChanged)
    Q_PROPERTY(QString totalDownloadRateText READ totalDownloadRateText
        NOTIFY countsChanged)
    Q_PROPERTY(int filter READ filter WRITE setFilter NOTIFY filterChanged)

public:
    /// Pre-computed view filters surfaced to QML through bare ints
    /// to avoid spelling enum types in QML bindings.
    enum Filter {
        FilterAll = 0,
        FilterActive = 1,
        FilterCompleted = 2,
        FilterFailed = 3,
    };
    Q_ENUM(Filter)

    DownloadsViewModel(controllers::DownloadController& controller,
        download::DownloadManager& manager,
        services::StreamActions* streamActions,
        QObject* parent = nullptr);

    DownloadsListModel* items() const { return m_items; }

    int activeCount() const noexcept { return m_activeCount; }
    int pausedCount() const noexcept { return m_pausedCount; }
    int completedCount() const noexcept { return m_completedCount; }
    int failedCount() const noexcept { return m_failedCount; }
    int totalCount() const noexcept { return m_totalCount; }
    qint64 totalDownloadRateBps() const noexcept { return m_totalRateBps; }
    QString totalDownloadRateText() const;

    int filter() const noexcept { return m_filter; }
    void setFilter(int f);

public Q_SLOTS:
    void refresh();

    void retry(const QString& assetId);
    void cancel(const QString& assetId);
    void remove(const QString& assetId, bool deleteFiles);
    void pin(const QString& assetId, bool on);

    /// Promote an OnDemand session to Full + Pinned in place. Bound
    /// to the row's `Keep file` action.
    void upgradeToFull(const QString& assetId);

    /// User-initiated pause / resume.
    void pauseDownload(const QString& assetId);
    void resumeDownload(const QString& assetId);

    /// Play the cached asset directly. Synthesises an `domain::Stream` +
    /// `PlaybackContext` from the persisted `DownloadItem` and hands
    /// off to `services::StreamActions::play`. The download manager's
    /// `prepareForPlayback` short-circuits to the local cache file
    /// when the asset is already complete, so this is a no-network
    /// path for finished downloads.
    void playDownload(const QString& assetId);

    /// Bulk equivalents bound to the page-level `Pause all` /
    /// `Resume all` toolbar actions. Iterate the in-memory rows
    /// snapshot and forward per-asset to the controller; redundant
    /// pause/resume calls are tolerated by the manager.
    void pauseAll();
    void resumeAll();

    /// Reveal the asset's local cache directory in the file manager.
    void openLocalDir(const QString& assetId);

Q_SIGNALS:
    void countsChanged();
    void filterChanged();

private:
    bool rowMatchesFilter(const domain::DownloadItem& it) const;
    void rebuildCountsFrom(const QList<domain::DownloadItem>& rows,
        const QHash<QString, DownloadsListModel::LiveRow>& live);

    controllers::DownloadController& m_controller;
    download::DownloadManager& m_manager;
    services::StreamActions* m_streamActions {};
    DownloadsListModel* m_items {};
    int m_activeCount = 0;
    int m_pausedCount = 0;
    int m_completedCount = 0;
    int m_failedCount = 0;
    int m_totalCount = 0;
    qint64 m_totalRateBps = 0;
    int m_filter = FilterAll;
};

} // namespace kinema::ui::qml

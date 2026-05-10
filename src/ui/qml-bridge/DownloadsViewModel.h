// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "ui/qml-bridge/DownloadsListModel.h"

#include <QObject>
#include <QString>

namespace kinema::controllers {
class DownloadController;
}

namespace kinema::core {
class MediaCache;
}

namespace kinema::download {
class DownloadManager;
}

namespace kinema::ui::qml {

/**
 * Per-page view-model for `DownloadsPage.qml`. Owns a
 * `DownloadsListModel` and forwards row-level actions to the
 * `DownloadController`.
 */
class DownloadsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(DownloadsListModel* items READ items CONSTANT)
    Q_PROPERTY(int activeCount READ activeCount NOTIFY countsChanged)
    Q_PROPERTY(int completedCount READ completedCount NOTIFY countsChanged)
    Q_PROPERTY(int failedCount READ failedCount NOTIFY countsChanged)
    Q_PROPERTY(int pinnedCount READ pinnedCount NOTIFY countsChanged)
    Q_PROPERTY(int totalCount READ totalCount NOTIFY countsChanged)
    Q_PROPERTY(qint64 totalDownloadRateBps READ totalDownloadRateBps
        NOTIFY countsChanged)
    Q_PROPERTY(QString totalDownloadRateText READ totalDownloadRateText
        NOTIFY countsChanged)
    Q_PROPERTY(qint64 cacheSizeBytes READ cacheSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(qint64 cacheBudgetBytes READ cacheBudgetBytes NOTIFY cacheChanged)
    Q_PROPERTY(double cacheUsageFraction READ cacheUsageFraction NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheSizeText READ cacheSizeText NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheBudgetText READ cacheBudgetText NOTIFY cacheChanged)
    Q_PROPERTY(qint64 ephemeralSizeBytes READ ephemeralSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(QString ephemeralSizeText READ ephemeralSizeText NOTIFY cacheChanged)
    Q_PROPERTY(int filter READ filter WRITE setFilter NOTIFY filterChanged)

public:
    /// Pre-computed view filters surfaced to QML through bare ints
    /// to avoid spelling enum types in QML bindings.
    enum Filter {
        FilterAll = 0,
        FilterActive = 1,
        FilterCompleted = 2,
        FilterFailed = 3,
        FilterPinned = 4,
    };
    Q_ENUM(Filter)

    DownloadsViewModel(controllers::DownloadController& controller,
        download::DownloadManager& manager,
        core::MediaCache& cache,
        QObject* parent = nullptr);

    DownloadsListModel* items() const { return m_items; }

    int activeCount() const noexcept { return m_activeCount; }
    int completedCount() const noexcept { return m_completedCount; }
    int failedCount() const noexcept { return m_failedCount; }
    int pinnedCount() const noexcept { return m_pinnedCount; }
    int totalCount() const noexcept { return m_totalCount; }
    qint64 totalDownloadRateBps() const noexcept { return m_totalRateBps; }
    QString totalDownloadRateText() const;

    qint64 cacheSizeBytes() const;
    qint64 ephemeralSizeBytes() const;
    qint64 cacheBudgetBytes() const;
    double cacheUsageFraction() const;
    QString cacheSizeText() const;
    QString cacheBudgetText() const;
    QString ephemeralSizeText() const;

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

    /// Run the cache eviction pass now (instead of waiting for the
    /// periodic timer). Bound to a header button.
    void runEvictionNow();

    /// Reveal the asset's local cache directory in the file manager.
    void openLocalDir(const QString& assetId);

    /// Drop persisted rows for completed (and not pinned) downloads.
    /// Files are kept on disk; eviction handles them later.
    void clearFinished();

    /// Drop persisted rows for failed downloads. Files are kept.
    void clearFailed();

Q_SIGNALS:
    void countsChanged();
    void cacheChanged();
    void filterChanged();

private:
    bool rowMatchesFilter(const api::DownloadItem& it) const;
    void rebuildCountsFrom(const QList<api::DownloadItem>& rows,
        const QHash<QString, DownloadsListModel::LiveRow>& live);

    controllers::DownloadController& m_controller;
    download::DownloadManager& m_manager;
    core::MediaCache& m_cache;
    DownloadsListModel* m_items {};
    int m_activeCount = 0;
    int m_completedCount = 0;
    int m_failedCount = 0;
    int m_pinnedCount = 0;
    int m_totalCount = 0;
    qint64 m_totalRateBps = 0;
    int m_filter = FilterAll;
};

} // namespace kinema::ui::qml

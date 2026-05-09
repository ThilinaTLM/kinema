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
    Q_PROPERTY(qint64 cacheSizeBytes READ cacheSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheSizeText READ cacheSizeText NOTIFY cacheChanged)
    Q_PROPERTY(qint64 ephemeralSizeBytes READ ephemeralSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(QString ephemeralSizeText READ ephemeralSizeText NOTIFY cacheChanged)

public:
    DownloadsViewModel(controllers::DownloadController& controller,
        core::MediaCache& cache,
        QObject* parent = nullptr);

    DownloadsListModel* items() const { return m_items; }

    int activeCount() const noexcept { return m_activeCount; }
    int completedCount() const noexcept { return m_completedCount; }
    int failedCount() const noexcept { return m_failedCount; }

    qint64 cacheSizeBytes() const;
    qint64 ephemeralSizeBytes() const;
    QString cacheSizeText() const;
    QString ephemeralSizeText() const;

public Q_SLOTS:
    void refresh();

    void retry(const QString& assetId);
    void cancel(const QString& assetId);
    void remove(const QString& assetId, bool deleteFiles);
    void pin(const QString& assetId, bool on);

Q_SIGNALS:
    void countsChanged();
    void cacheChanged();

private:
    void rebuildCounts();

    controllers::DownloadController& m_controller;
    core::MediaCache& m_cache;
    DownloadsListModel* m_items {};
    int m_activeCount = 0;
    int m_completedCount = 0;
    int m_failedCount = 0;
};

} // namespace kinema::ui::qml

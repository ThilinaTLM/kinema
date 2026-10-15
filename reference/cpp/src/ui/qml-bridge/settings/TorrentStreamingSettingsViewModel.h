// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include <QObject>
#include <QString>

namespace kinema::config {
class TorrentStreamingSettings;
}

namespace kinema::controllers {
class DownloadController;
}

namespace kinema::core {
class MediaCache;
class SubtitleCacheStore;
class TorrentCache;
}

namespace kinema::ui {
class ImageLoader;
}

namespace kinema::ui::qml::settings {

class TorrentStreamingSettingsViewModel : public QObject
{
    Q_OBJECT
    Q_PROPERTY(int cacheBudgetGb READ cacheBudgetGb WRITE setCacheBudgetGb NOTIFY cacheBudgetGbChanged)
    Q_PROPERTY(int startupBufferMiB READ startupBufferMiB WRITE setStartupBufferMiB NOTIFY startupBufferMiBChanged)
    Q_PROPERTY(int readaheadMiB READ readaheadMiB WRITE setReadaheadMiB NOTIFY readaheadMiBChanged)
    Q_PROPERTY(int tailBufferMiB READ tailBufferMiB WRITE setTailBufferMiB NOTIFY tailBufferMiBChanged)
    Q_PROPERTY(int maxDownloadRateKiB READ maxDownloadRateKiB WRITE setMaxDownloadRateKiB NOTIFY maxDownloadRateKiBChanged)
    Q_PROPERTY(int maxUploadRateKiB READ maxUploadRateKiB WRITE setMaxUploadRateKiB NOTIFY maxUploadRateKiBChanged)
    Q_PROPERTY(int idleStopMinutes READ idleStopMinutes WRITE setIdleStopMinutes NOTIFY idleStopMinutesChanged)

    // Live cache state, refreshed by an internal poll timer. Pinned
    // bytes are excluded from the budget; `ephemeralSizeBytes` is
    // what eviction is allowed to drop.
    Q_PROPERTY(qint64 cacheSizeBytes READ cacheSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(qint64 cacheBudgetBytes READ cacheBudgetBytes NOTIFY cacheChanged)
    Q_PROPERTY(qint64 ephemeralSizeBytes READ ephemeralSizeBytes NOTIFY cacheChanged)
    Q_PROPERTY(double cacheUsageFraction READ cacheUsageFraction NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheSizeText READ cacheSizeText NOTIFY cacheChanged)
    Q_PROPERTY(QString cacheBudgetText READ cacheBudgetText NOTIFY cacheChanged)
    Q_PROPERTY(QString ephemeralSizeText READ ephemeralSizeText NOTIFY cacheChanged)

    Q_PROPERTY(QString statusMessage READ statusMessage NOTIFY statusChanged)
    Q_PROPERTY(int statusKind READ statusKind NOTIFY statusChanged)
    Q_PROPERTY(bool busy READ busy NOTIFY busyChanged)

public:
    TorrentStreamingSettingsViewModel(config::TorrentStreamingSettings& settings,
        core::MediaCache& cache,
        controllers::DownloadController* downloads,
        core::TorrentCache* torrentCache,
        core::SubtitleCacheStore* subtitleCache,
        kinema::ui::ImageLoader* imageLoader,
        QObject* parent = nullptr);

    int cacheBudgetGb() const;
    int startupBufferMiB() const;
    int readaheadMiB() const;
    int tailBufferMiB() const;
    int maxDownloadRateKiB() const;
    int maxUploadRateKiB() const;
    int idleStopMinutes() const;

    void setCacheBudgetGb(int v);
    void setStartupBufferMiB(int v);
    void setReadaheadMiB(int v);
    void setTailBufferMiB(int v);
    void setMaxDownloadRateKiB(int v);
    void setMaxUploadRateKiB(int v);
    void setIdleStopMinutes(int v);

    qint64 cacheSizeBytes() const;
    qint64 cacheBudgetBytes() const;
    qint64 ephemeralSizeBytes() const;
    double cacheUsageFraction() const;
    QString cacheSizeText() const;
    QString cacheBudgetText() const;
    QString ephemeralSizeText() const;
    QString statusMessage() const { return m_statusMessage; }
    int statusKind() const { return m_statusKind; }
    bool busy() const { return m_busy; }

public Q_SLOTS:
    /// Drop ephemeral cache entries until the budget is satisfied,
    /// then refresh the cached counters.
    void runEvictionNow();

    /// Destructive cleanup used by Settings → Downloads: remove all
    /// unpinned download rows/files and local caches while preserving
    /// pinned/offline downloads.
    void cleanupDownloadsAndCache();

Q_SIGNALS:
    void cacheBudgetGbChanged();
    void startupBufferMiBChanged();
    void readaheadMiBChanged();
    void tailBufferMiBChanged();
    void maxDownloadRateKiBChanged();
    void maxUploadRateKiBChanged();
    void idleStopMinutesChanged();
    void cacheChanged();
    void statusChanged();
    void busyChanged();

private:
    void setStatus(const QString& message, int kind);
    void setBusy(bool on);

    config::TorrentStreamingSettings& m_settings;
    core::MediaCache& m_cache;
    controllers::DownloadController* m_downloads {};
    core::TorrentCache* m_torrentCache {};
    core::SubtitleCacheStore* m_subtitleCache {};
    kinema::ui::ImageLoader* m_imageLoader {};
    QString m_statusMessage;
    int m_statusKind = 0;
    bool m_busy = false;
};

} // namespace kinema::ui::qml::settings

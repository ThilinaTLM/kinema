// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/TorrentStreamingSettingsViewModel.h"
#include "ui/qml-bridge/settings/SettingsStatus.h"
#include "config/TorrentStreamingSettings.h"
#include "controllers/DownloadController.h"
#include "core/io/CachePaths.h"
#include "core/io/HttpErrorPresenter.h"
#include "core/persistence/MediaCache.h"
#include "core/persistence/SubtitleCacheStore.h"
#include "core/persistence/TorrentCache.h"
#include "core/util/DateFormat.h"
#include "domain/Download.h"
#include "kinema_log_ui.h"
#include "ui/ImageLoader.h"
#include <KFormat>
#include <KLocalizedString>
#include <QDir>
#include <QFile>
#include <QTimer>

#include <exception>

namespace kinema::ui::qml::settings {

// ========================== Torrent streaming =============================

TorrentStreamingSettingsViewModel::TorrentStreamingSettingsViewModel(
    config::TorrentStreamingSettings& settings,
    core::MediaCache& cache,
    controllers::DownloadController* downloads,
    core::TorrentCache* torrentCache,
    core::SubtitleCacheStore* subtitleCache,
    kinema::ui::ImageLoader* imageLoader,
    QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_cache(cache)
    , m_downloads(downloads)
    , m_torrentCache(torrentCache)
    , m_subtitleCache(subtitleCache)
    , m_imageLoader(imageLoader)
{
    // Cache stats are read on demand from `MediaCache` (which walks
    // the asset dirs). A 5 s poll keeps the settings page's usage
    // bar live without making per-property reads expensive.
    auto* poll = new QTimer(this);
    poll->setInterval(5000);
    poll->setTimerType(Qt::CoarseTimer);
    connect(poll, &QTimer::timeout, this,
        [this] { Q_EMIT cacheChanged(); });
    poll->start();
}

int TorrentStreamingSettingsViewModel::cacheBudgetGb() const { return m_settings.cacheBudgetGb(); }
int TorrentStreamingSettingsViewModel::startupBufferMiB() const { return m_settings.startupBufferMiB(); }
int TorrentStreamingSettingsViewModel::readaheadMiB() const { return m_settings.readaheadMiB(); }
int TorrentStreamingSettingsViewModel::tailBufferMiB() const { return m_settings.tailBufferMiB(); }
int TorrentStreamingSettingsViewModel::maxDownloadRateKiB() const { return m_settings.maxDownloadRateKiB(); }
int TorrentStreamingSettingsViewModel::maxUploadRateKiB() const { return m_settings.maxUploadRateKiB(); }
int TorrentStreamingSettingsViewModel::idleStopMinutes() const { return m_settings.idleStopMinutes(); }

void TorrentStreamingSettingsViewModel::setCacheBudgetGb(int v)
{
    if (cacheBudgetGb() == v) return;
    m_settings.setCacheBudgetGb(v);
    Q_EMIT cacheBudgetGbChanged();
    // Budget changed → the usage bar's denominator changed too.
    Q_EMIT cacheChanged();
}
void TorrentStreamingSettingsViewModel::setStartupBufferMiB(int v)
{
    if (startupBufferMiB() == v) return;
    m_settings.setStartupBufferMiB(v);
    Q_EMIT startupBufferMiBChanged();
}
void TorrentStreamingSettingsViewModel::setReadaheadMiB(int v)
{
    if (readaheadMiB() == v) return;
    m_settings.setReadaheadMiB(v);
    Q_EMIT readaheadMiBChanged();
}
void TorrentStreamingSettingsViewModel::setTailBufferMiB(int v)
{
    if (tailBufferMiB() == v) return;
    m_settings.setTailBufferMiB(v);
    Q_EMIT tailBufferMiBChanged();
}
void TorrentStreamingSettingsViewModel::setMaxDownloadRateKiB(int v)
{
    if (maxDownloadRateKiB() == v) return;
    m_settings.setMaxDownloadRateKiB(v);
    Q_EMIT maxDownloadRateKiBChanged();
}
void TorrentStreamingSettingsViewModel::setMaxUploadRateKiB(int v)
{
    if (maxUploadRateKiB() == v) return;
    m_settings.setMaxUploadRateKiB(v);
    Q_EMIT maxUploadRateKiBChanged();
}
void TorrentStreamingSettingsViewModel::setIdleStopMinutes(int v)
{
    if (idleStopMinutes() == v) return;
    m_settings.setIdleStopMinutes(v);
    Q_EMIT idleStopMinutesChanged();
}

qint64 TorrentStreamingSettingsViewModel::cacheSizeBytes() const
{
    return m_cache.sizeBytes();
}

qint64 TorrentStreamingSettingsViewModel::cacheBudgetBytes() const
{
    return m_cache.budgetBytes();
}

qint64 TorrentStreamingSettingsViewModel::ephemeralSizeBytes() const
{
    return m_cache.ephemeralSizeBytes();
}

double TorrentStreamingSettingsViewModel::cacheUsageFraction() const
{
    const qint64 budget = cacheBudgetBytes();
    if (budget <= 0) {
        return 0.0;
    }
    return qBound(0.0,
        static_cast<double>(cacheSizeBytes())
            / static_cast<double>(budget),
        1.0);
}

QString TorrentStreamingSettingsViewModel::cacheSizeText() const
{
    return KFormat().formatByteSize(cacheSizeBytes());
}

QString TorrentStreamingSettingsViewModel::cacheBudgetText() const
{
    return KFormat().formatByteSize(cacheBudgetBytes());
}

QString TorrentStreamingSettingsViewModel::ephemeralSizeText() const
{
    return KFormat().formatByteSize(ephemeralSizeBytes());
}

void TorrentStreamingSettingsViewModel::runEvictionNow()
{
    qCInfo(KINEMA_UI)
        << "Downloads settings: manual cache eviction triggered";
    m_cache.enforceBudget();
    Q_EMIT cacheChanged();
}

void TorrentStreamingSettingsViewModel::cleanupDownloadsAndCache()
{
    if (m_busy) {
        return;
    }

    setBusy(true);
    try {
        qCInfo(KINEMA_UI)
            << "Downloads settings: cleanup downloads/cache triggered";

        QSet<QString> protectedAssetIds;
        QSet<QString> protectedInfoHashes;
        int removedRows = 0;
        if (m_downloads) {
            const auto rows = m_downloads->items();
            for (const auto& row : rows) {
                if (row.disposition == domain::CacheDisposition::Pinned) {
                    protectedAssetIds.insert(row.assetId);
                    if (row.backendKind == domain::DownloadBackendKind::Torrent
                        && !row.infoHash.isEmpty()) {
                        protectedInfoHashes.insert(row.infoHash);
                    }
                }
            }
            for (const auto& row : rows) {
                if (row.disposition == domain::CacheDisposition::Pinned) {
                    continue;
                }
                m_downloads->remove(row.assetId, true);
                ++removedRows;
            }
        }

        const auto mediaCleanup
            = m_cache.removeUnpinnedExcept(protectedAssetIds);

        core::TorrentCache::CleanupResult torrentCleanup;
        if (m_torrentCache) {
            torrentCleanup = m_torrentCache->removeAllExcept(
                protectedInfoHashes);
        }

        int subtitleFiles = 0;
        if (m_subtitleCache) {
            const auto entries = m_subtitleCache->all();
            for (const auto& entry : entries) {
                if (!entry.localPath.isEmpty()) {
                    QFile::remove(entry.localPath);
                }
            }
            subtitleFiles = entries.size();
            m_subtitleCache->clearAll();

            auto subtitleDir = core::cache::subtitlesDir();
            subtitleDir.removeRecursively();
            QDir().mkpath(subtitleDir.absolutePath());
        }

        bool imageCacheOk = true;
        if (m_imageLoader) {
            imageCacheOk = m_imageLoader->clearDiskCache();
        }

        const int failures = mediaCleanup.failedAssets
            + torrentCleanup.failedTorrents
            + (imageCacheOk ? 0 : 1);

        qCInfo(KINEMA_UI)
            << "Downloads settings cleanup removed rows=" << removedRows
            << "mediaDirs=" << mediaCleanup.removedAssets
            << "torrentDirs=" << torrentCleanup.removedTorrents
            << "subtitleFiles=" << subtitleFiles
            << "failures=" << failures;

        Q_EMIT cacheChanged();
        if (failures > 0) {
            setStatus(i18ncp("@info downloads cache cleanup partial failure",
                          "Cleanup finished, but %1 cache item could not be deleted.",
                          "Cleanup finished, but %1 cache items could not be deleted.",
                          failures),
                kStatusError);
        } else {
            setStatus(i18nc("@info downloads cache cleanup complete",
                          "Cleanup complete. Removed %1 unpinned download(s) and cleared local caches; pinned downloads were kept.",
                          removedRows),
                kStatusPositive);
        }
    } catch (const std::exception& e) {
        setStatus(core::describeError(e, "clean downloads and cache"),
            kStatusError);
    }
    setBusy(false);
}

void TorrentStreamingSettingsViewModel::setStatus(
    const QString& message, int kind)
{
    if (m_statusMessage == message && m_statusKind == kind) {
        return;
    }
    m_statusMessage = message;
    m_statusKind = kind;
    Q_EMIT statusChanged();
}

void TorrentStreamingSettingsViewModel::setBusy(bool on)
{
    if (m_busy == on) {
        return;
    }
    m_busy = on;
    Q_EMIT busyChanged();
}

} // namespace kinema::ui::qml::settings

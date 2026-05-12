// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/settings/TorrentStreamingSettingsViewModel.h"
#include "config/TorrentStreamingSettings.h"
#include "core/persistence/MediaCache.h"
#include "core/util/DateFormat.h"
#include "kinema_log_ui.h"
#include <KFormat>
#include <QTimer>

namespace kinema::ui::qml::settings {

// ========================== Torrent streaming =============================

TorrentStreamingSettingsViewModel::TorrentStreamingSettingsViewModel(
    config::TorrentStreamingSettings& settings,
    core::MediaCache& cache, QObject* parent)
    : QObject(parent)
    , m_settings(settings)
    , m_cache(cache)
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

} // namespace kinema::ui::qml::settings

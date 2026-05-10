// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsViewModel.h"

#include "controllers/DownloadController.h"
#include "core/MediaCache.h"
#include "download/DownloadManager.h"
#include "kinema_log_ui.h"

#include <KFormat>
#include <KIO/OpenUrlJob>
#include <KLocalizedString>

#include <QUrl>

namespace kinema::ui::qml {

namespace {

bool isActiveState(api::DownloadState s)
{
    switch (s) {
    case api::DownloadState::Queued:
    case api::DownloadState::Resolving:
    case api::DownloadState::Active:
    case api::DownloadState::Idle:
        return true;
    default:
        return false;
    }
}

} // namespace

DownloadsViewModel::DownloadsViewModel(
    controllers::DownloadController& controller,
    download::DownloadManager& manager,
    core::MediaCache& cache,
    QObject* parent)
    : QObject(parent)
    , m_controller(controller)
    , m_manager(manager)
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

    // Pull live stats once per refresh and join them into the
    // model's transient map. This keeps DownloadsListModel free of
    // any DownloadManager dependency while still letting QML bind
    // to per-row rate / peers / ETA roles.
    QHash<QString, DownloadsListModel::LiveRow> live;
    for (const auto& it : rows) {
        if (auto stats = m_manager.liveStatsFor(it.assetId)) {
            DownloadsListModel::LiveRow lr;
            lr.ratePayloadBps = stats->ratePayloadBps;
            lr.peers = stats->peers;
            lr.seeds = stats->seeds;
            lr.etaSeconds = stats->etaSeconds;
            live.insert(it.assetId, lr);
        }
    }

    rebuildCountsFrom(rows, live);

    QList<api::DownloadItem> visible;
    if (m_filter == FilterAll) {
        visible = rows;
    } else {
        for (auto& it : rows) {
            if (rowMatchesFilter(it)) {
                visible.append(it);
            }
        }
    }
    auto attached = m_controller.attachedPlayerAssetIds();
    m_items->setItems(visible, std::move(live), std::move(attached));

    Q_EMIT countsChanged();
    Q_EMIT cacheChanged();
}

bool DownloadsViewModel::rowMatchesFilter(const api::DownloadItem& it) const
{
    switch (m_filter) {
    case FilterActive:
        return isActiveState(it.state);
    case FilterCompleted:
        return it.state == api::DownloadState::Completed;
    case FilterFailed:
        return it.state == api::DownloadState::Failed;
    case FilterPinned:
        return it.isPinned();
    default:
        return true;
    }
}

void DownloadsViewModel::rebuildCountsFrom(
    const QList<api::DownloadItem>& rows,
    const QHash<QString, DownloadsListModel::LiveRow>& live)
{
    m_activeCount = 0;
    m_completedCount = 0;
    m_failedCount = 0;
    m_pinnedCount = 0;
    m_totalCount = rows.size();
    m_totalRateBps = 0;
    for (const auto& it : rows) {
        if (it.isPinned()) {
            ++m_pinnedCount;
        }
        switch (it.state) {
        case api::DownloadState::Queued:
        case api::DownloadState::Resolving:
        case api::DownloadState::Active:
        case api::DownloadState::Idle:
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
        if (const auto liveIt = live.constFind(it.assetId);
            liveIt != live.constEnd() && isActiveState(it.state)) {
            m_totalRateBps += liveIt->ratePayloadBps;
        }
    }
}

void DownloadsViewModel::setFilter(int f)
{
    if (f < FilterAll || f > FilterPinned) {
        f = FilterAll;
    }
    if (m_filter == f) {
        return;
    }
    m_filter = f;
    Q_EMIT filterChanged();
    refresh();
}

qint64 DownloadsViewModel::cacheSizeBytes() const
{
    return m_cache.sizeBytes();
}

qint64 DownloadsViewModel::ephemeralSizeBytes() const
{
    return m_cache.ephemeralSizeBytes();
}

qint64 DownloadsViewModel::cacheBudgetBytes() const
{
    return m_cache.budgetBytes();
}

double DownloadsViewModel::cacheUsageFraction() const
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

QString DownloadsViewModel::cacheSizeText() const
{
    return KFormat().formatByteSize(cacheSizeBytes());
}

QString DownloadsViewModel::cacheBudgetText() const
{
    return KFormat().formatByteSize(cacheBudgetBytes());
}

QString DownloadsViewModel::ephemeralSizeText() const
{
    return KFormat().formatByteSize(ephemeralSizeBytes());
}

QString DownloadsViewModel::totalDownloadRateText() const
{
    if (m_totalRateBps <= 0) {
        return QString();
    }
    return i18nc("@label aggregate download rate per second",
        "%1/s", KFormat().formatByteSize(m_totalRateBps));
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

void DownloadsViewModel::upgradeToFull(const QString& assetId)
{
    m_controller.upgradeToFull(assetId);
}

void DownloadsViewModel::pauseDownload(const QString& assetId)
{
    m_controller.pause(assetId);
}

void DownloadsViewModel::resumeDownload(const QString& assetId)
{
    m_controller.resume(assetId);
}

void DownloadsViewModel::runEvictionNow()
{
    qCInfo(KINEMA_UI) << "DownloadsViewModel: manual eviction triggered";
    // The controller doesn't expose runEviction; we tunnel through
    // the manager directly because it's only the cache pass.
    m_cache.enforceBudget();
    refresh();
}

void DownloadsViewModel::openLocalDir(const QString& assetId)
{
    const auto rows = m_controller.items();
    QString dir;
    for (const auto& it : rows) {
        if (it.assetId == assetId) {
            dir = it.localDir;
            break;
        }
    }
    if (dir.isEmpty()) {
        return;
    }
    auto* job = new KIO::OpenUrlJob(QUrl::fromLocalFile(dir), this);
    job->setRunExecutables(false);
    job->start();
}

void DownloadsViewModel::clearFinished()
{
    const auto rows = m_controller.items();
    int removed = 0;
    for (const auto& it : rows) {
        if (it.state == api::DownloadState::Completed && !it.isPinned()) {
            m_controller.remove(it.assetId, /*deleteFiles=*/false);
            ++removed;
        }
    }
    qCInfo(KINEMA_UI) << "clearFinished removed" << removed << "rows";
}

void DownloadsViewModel::clearFailed()
{
    const auto rows = m_controller.items();
    int removed = 0;
    for (const auto& it : rows) {
        if (it.state == api::DownloadState::Failed) {
            m_controller.remove(it.assetId, /*deleteFiles=*/false);
            ++removed;
        }
    }
    qCInfo(KINEMA_UI) << "clearFailed removed" << removed << "rows";
}

} // namespace kinema::ui::qml

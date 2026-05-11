// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsViewModel.h"

#include "api/Media.h"
#include "api/PlaybackContext.h"
#include "controllers/DownloadController.h"
#include "core/HttpErrorPresenter.h"
#include "download/DownloadManager.h"
#include "kinema_log_ui.h"
#include "services/StreamActions.h"

#include <KFormat>
#include <KIO/OpenUrlJob>
#include <KLocalizedString>

#include <QUrl>

#include <algorithm>
#include <exception>

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
    services::StreamActions* streamActions,
    QObject* parent)
    : QObject(parent)
    , m_controller(controller)
    , m_manager(manager)
    , m_streamActions(streamActions)
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
    default:
        return true;
    }
}

void DownloadsViewModel::rebuildCountsFrom(
    const QList<api::DownloadItem>& rows,
    const QHash<QString, DownloadsListModel::LiveRow>& live)
{
    m_activeCount = 0;
    m_pausedCount = 0;
    m_completedCount = 0;
    m_failedCount = 0;
    m_totalCount = rows.size();
    m_totalRateBps = 0;
    for (const auto& it : rows) {
        switch (it.state) {
        case api::DownloadState::Queued:
        case api::DownloadState::Resolving:
        case api::DownloadState::Active:
        case api::DownloadState::Idle:
            ++m_activeCount;
            break;
        case api::DownloadState::Paused:
            ++m_pausedCount;
            break;
        case api::DownloadState::Completed:
            ++m_completedCount;
            break;
        case api::DownloadState::Failed:
            ++m_failedCount;
            break;
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
    if (f < FilterAll || f > FilterFailed) {
        f = FilterAll;
    }
    if (m_filter == f) {
        return;
    }
    m_filter = f;
    Q_EMIT filterChanged();
    refresh();
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

void DownloadsViewModel::playDownload(const QString& assetId)
{
    if (!m_streamActions) {
        qCWarning(KINEMA_UI)
            << "playDownload: StreamActions not wired";
        return;
    }
    try {
        const auto rows = m_controller.items();
        const auto it = std::find_if(rows.begin(), rows.end(),
            [&](const api::DownloadItem& r) {
                return r.assetId == assetId;
            });
        if (it == rows.end()) {
            qCWarning(KINEMA_UI)
                << "playDownload: no row for assetId" << assetId;
            return;
        }

        // Synthesise an api::Stream from the persisted DownloadItem
        // so the existing StreamActions::play -> DownloadManager
        // pipeline can short-circuit to the local cached file.
        api::Stream stream;
        stream.qualityLabel = it->qualityLabel;
        stream.resolution = it->resolution;
        stream.releaseName = it->releaseName;
        stream.provider = it->provider;
        stream.infoHash = it->infoHash;
        stream.fileIndex = it->fileIndex;
        stream.fileNameHint = it->fileNameHint;
        if (it->expectedSizeBytes) {
            stream.sizeBytes = *it->expectedSizeBytes;
        }

        api::PlaybackContext ctx;
        ctx.key = it->key;
        ctx.title = it->title;
        ctx.seriesTitle = it->seriesTitle;
        ctx.episodeTitle = it->episodeTitle;
        ctx.poster = it->poster;

        m_streamActions->play(stream, ctx);
    } catch (const std::exception& e) {
        qCWarning(KINEMA_UI)
            << "playDownload failed:"
            << core::describeError(e, "play download");
    }
}

void DownloadsViewModel::pauseAll()
{
    const auto rows = m_controller.items();
    int paused = 0;
    for (const auto& it : rows) {
        if (isActiveState(it.state)) {
            m_controller.pause(it.assetId);
            ++paused;
        }
    }
    qCInfo(KINEMA_UI) << "pauseAll paused" << paused << "rows";
}

void DownloadsViewModel::resumeAll()
{
    const auto rows = m_controller.items();
    int resumed = 0;
    for (const auto& it : rows) {
        if (it.state == api::DownloadState::Paused) {
            m_controller.resume(it.assetId);
            ++resumed;
        }
    }
    qCInfo(KINEMA_UI) << "resumeAll resumed" << resumed << "rows";
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

} // namespace kinema::ui::qml

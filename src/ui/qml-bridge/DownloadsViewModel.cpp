// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsViewModel.h"

#include "domain/Media.h"
#include "domain/PlaybackContext.h"
#include "controllers/DownloadController.h"
#include "core/io/HttpErrorPresenter.h"
#include "download/DownloadManager.h"
#include "kinema_log_ui.h"
#include "services/StreamActions.h"

#include <KFormat>
#include <KIO/OpenUrlJob>
#include <KLocalizedString>

#include <QTimer>
#include <QUrl>

#include <algorithm>
#include <exception>

namespace kinema::ui::qml {

namespace {

bool isActiveState(domain::DownloadState s)
{
    switch (s) {
    case domain::DownloadState::Queued:
    case domain::DownloadState::Resolving:
    case domain::DownloadState::Active:
    case domain::DownloadState::Idle:
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
    // Structural changes — list shape may have changed. Full
    // reload + reset is the right call here; it's also coalesced
    // upstream in `DownloadStore::scheduleChanged`.
    connect(&m_controller, &controllers::DownloadController::changed,
        this, &DownloadsViewModel::refresh);
    // Per-row mutations — route through the differential update
    // path so delegates (and any open popups) survive ticks.
    connect(&m_controller, &controllers::DownloadController::itemChanged,
        this, &DownloadsViewModel::onItemChanged);
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

    QList<domain::DownloadItem> visible;
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

    // Structural rebuild supersedes any pending differential
    // flushes — the rows we just loaded are already the latest.
    m_dirtyAssetIds.clear();

    Q_EMIT countsChanged();
}

void DownloadsViewModel::onItemChanged(const QString& assetId)
{
    if (assetId.isEmpty()) {
        return;
    }
    m_dirtyAssetIds.insert(assetId);
    if (m_flushScheduled) {
        return;
    }
    m_flushScheduled = true;
    // 0-ms single shot — coalesces back-to-back emissions from
    // the same stats tick (cachedBytesChanged + liveStatsChanged)
    // into a single update on the next event-loop iteration.
    QTimer::singleShot(0, this, &DownloadsViewModel::flushDirtyItems);
}

void DownloadsViewModel::flushDirtyItems()
{
    m_flushScheduled = false;
    if (m_dirtyAssetIds.isEmpty()) {
        return;
    }
    const auto dirty = std::exchange(m_dirtyAssetIds, {});
    bool needsStructuralRefresh = false;
    for (const QString& assetId : dirty) {
        const int row = m_items->rowForAssetId(assetId);
        const auto fresh = m_controller.find(assetId);
        if (!fresh) {
            // Row is gone from the store — fall back to a
            // structural refresh once, after the loop.
            needsStructuralRefresh = true;
            continue;
        }
        if (row < 0) {
            // Row exists in the store but not in the model (e.g.
            // filtered out, or a brand-new insert). Defer to a
            // structural refresh; it'll pick the row up if the
            // filter matches.
            needsStructuralRefresh = true;
            continue;
        }
        if (m_filter != FilterAll && !rowMatchesFilter(*fresh)) {
            // The row's state moved out of the active filter
            // bucket. Structural change.
            needsStructuralRefresh = true;
            continue;
        }
        // Push live stats first so the row's persistent update
        // emission lands with the freshest rate / peers visible.
        DownloadsListModel::LiveRow lr;
        if (const auto stats = m_manager.liveStatsFor(assetId)) {
            lr.ratePayloadBps = stats->ratePayloadBps;
            lr.peers = stats->peers;
            lr.seeds = stats->seeds;
            lr.etaSeconds = stats->etaSeconds;
        }
        m_items->updateLiveStatsFor(assetId, lr);
        m_items->updateRow(assetId, *fresh);
    }

    // The attached-player set is a single read; diff it against
    // the model and emit role-scoped dataChanged for whatever
    // flipped. Cheap when nothing changed.
    m_items->updateAttachedPlayers(m_controller.attachedPlayerAssetIds());

    if (needsStructuralRefresh) {
        refresh();
        return;
    }

    recomputeAggregatesFromModel();
}

void DownloadsViewModel::recomputeAggregatesFromModel()
{
    const int prevActive = m_activeCount;
    const int prevPaused = m_pausedCount;
    const int prevCompleted = m_completedCount;
    const int prevFailed = m_failedCount;
    const int prevTotal = m_totalCount;
    const qint64 prevRate = m_totalRateBps;

    rebuildCountsFrom(m_items->items(), m_items->liveStats());

    if (prevActive != m_activeCount
        || prevPaused != m_pausedCount
        || prevCompleted != m_completedCount
        || prevFailed != m_failedCount
        || prevTotal != m_totalCount
        || prevRate != m_totalRateBps) {
        Q_EMIT countsChanged();
    }
}

bool DownloadsViewModel::rowMatchesFilter(const domain::DownloadItem& it) const
{
    switch (m_filter) {
    case FilterActive:
        return isActiveState(it.state);
    case FilterCompleted:
        return it.state == domain::DownloadState::Completed;
    case FilterFailed:
        return it.state == domain::DownloadState::Failed;
    default:
        return true;
    }
}

void DownloadsViewModel::rebuildCountsFrom(
    const QList<domain::DownloadItem>& rows,
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
        case domain::DownloadState::Queued:
        case domain::DownloadState::Resolving:
        case domain::DownloadState::Active:
        case domain::DownloadState::Idle:
            ++m_activeCount;
            break;
        case domain::DownloadState::Paused:
            ++m_pausedCount;
            break;
        case domain::DownloadState::Completed:
            ++m_completedCount;
            break;
        case domain::DownloadState::Failed:
            ++m_failedCount;
            break;
        case domain::DownloadState::Cancelled:
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
            [&](const domain::DownloadItem& r) {
                return r.assetId == assetId;
            });
        if (it == rows.end()) {
            qCWarning(KINEMA_UI)
                << "playDownload: no row for assetId" << assetId;
            return;
        }

        // Synthesise an domain::Stream from the persisted DownloadItem
        // so the existing StreamActions::play -> DownloadManager
        // pipeline can short-circuit to the local cached file.
        domain::Stream stream;
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

        domain::PlaybackContext ctx;
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
        if (it.state == domain::DownloadState::Paused) {
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

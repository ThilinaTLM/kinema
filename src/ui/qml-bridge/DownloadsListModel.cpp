// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsListModel.h"

#include <KFormat>
#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {

QString stateText(api::DownloadState s, api::DownloadMode mode,
    bool hasPlayer, bool complete)
{
    using S = api::DownloadState;
    using M = api::DownloadMode;
    switch (s) {
    case S::Queued:
        return i18nc("@label download state", "Queued");
    case S::Resolving:
        return i18nc("@label download state", "Preparing\u2026");
    case S::Active:
        if (mode == M::OnDemand) {
            return hasPlayer
                ? i18nc("@label download state", "Streaming")
                : i18nc("@label download state", "Caching");
        }
        return hasPlayer
            ? i18nc("@label download state",
                  "Downloading + Playing")
            : i18nc("@label download state", "Downloading");
    case S::Idle:
        return i18nc("@label download state", "Idle");
    case S::Completed:
        Q_UNUSED(complete);
        return i18nc("@label download state", "Completed");
    case S::Paused:
        return i18nc("@label download state", "Paused");
    case S::Cancelled:
        return i18nc("@label download state", "Cancelled");
    case S::Failed:
        return i18nc("@label download state", "Failed");
    }
    return QString();
}

/// One of {"positive","neutral","warn","negative"}. Used by QML to
/// pick a chip background colour without leaking enum values out.
QString stateTone(api::DownloadState s)
{
    switch (s) {
    case api::DownloadState::Completed:
        return QStringLiteral("positive");
    case api::DownloadState::Active:
    case api::DownloadState::Resolving:
        return QStringLiteral("neutral");
    case api::DownloadState::Queued:
    case api::DownloadState::Idle:
    case api::DownloadState::Paused:
    case api::DownloadState::Cancelled:
        return QStringLiteral("warn");
    case api::DownloadState::Failed:
        return QStringLiteral("negative");
    }
    return QStringLiteral("neutral");
}

QString modeLabel(api::DownloadMode m)
{
    switch (m) {
    case api::DownloadMode::OnDemand:
        return i18nc("@label download mode", "Stream");
    case api::DownloadMode::Full:
        return i18nc("@label download mode", "Full");
    }
    return QString();
}

QString backendIcon(api::DownloadBackendKind k)
{
    switch (k) {
    case api::DownloadBackendKind::Torrent:
        return QStringLiteral("network-server-database");
    case api::DownloadBackendKind::RealDebridHttp:
        return QStringLiteral("folder-cloud");
    }
    return QStringLiteral("download");
}

QString backendLabel(api::DownloadBackendKind k)
{
    switch (k) {
    case api::DownloadBackendKind::Torrent:
        return i18nc("@label download backend", "Torrent");
    case api::DownloadBackendKind::RealDebridHttp:
        return i18nc("@label download backend", "Real-Debrid");
    }
    return QString();
}

QString rateText(qint64 bps)
{
    if (bps <= 0) {
        return QString();
    }
    return i18nc("@label download rate per second",
        "%1/s", KFormat().formatByteSize(bps));
}

QString etaText(int seconds)
{
    if (seconds <= 0) {
        return QString();
    }
    return KFormat().formatDuration(
        static_cast<quint64>(seconds) * 1000ULL,
        KFormat::AbbreviatedDuration);
}

QString sizeText(const api::DownloadItem& it)
{
    KFormat fmt;
    if (it.expectedSizeBytes && *it.expectedSizeBytes > 0) {
        if (it.cachedSizeBytes >= *it.expectedSizeBytes) {
            return fmt.formatByteSize(*it.expectedSizeBytes);
        }
        return i18nc("@label cached / total size",
            "%1 / %2",
            fmt.formatByteSize(it.cachedSizeBytes),
            fmt.formatByteSize(*it.expectedSizeBytes));
    }
    if (it.cachedSizeBytes > 0) {
        return fmt.formatByteSize(it.cachedSizeBytes);
    }
    return QStringLiteral("\u2014");
}

QString progressText(const api::DownloadItem& it)
{
    const double f = it.progressFraction();
    if (f < 0.0) {
        return QString();
    }
    return QStringLiteral("%1%").arg(static_cast<int>(f * 100.0));
}

QString subtitleText(const api::DownloadItem& it)
{
    QStringList parts;
    if (!it.qualityLabel.isEmpty()) {
        parts.append(it.qualityLabel);
    } else if (!it.resolution.isEmpty()) {
        parts.append(it.resolution);
    }
    if (!it.releaseName.isEmpty()) {
        parts.append(it.releaseName);
    }
    if (!it.provider.isEmpty()) {
        parts.append(it.provider);
    }
    return parts.join(QStringLiteral(" \u00b7 "));
}

} // namespace

DownloadsListModel::DownloadsListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int DownloadsListModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return m_items.size();
}

QVariant DownloadsListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_items.size()) {
        return {};
    }
    const auto& it = m_items.at(index.row());
    const auto liveIt = m_liveStats.constFind(it.assetId);
    const LiveRow live = liveIt == m_liveStats.constEnd() ? LiveRow {} : *liveIt;
    switch (role) {
    case AssetIdRole:
        return it.assetId;
    case TitleRole:
        return it.title;
    case SubtitleRole:
        return subtitleText(it);
    case PosterUrlRole:
        return it.poster.toString();
    case BackendKindRole:
        return static_cast<int>(it.backendKind);
    case BackendIconRole:
        return backendIcon(it.backendKind);
    case BackendLabelRole:
        return backendLabel(it.backendKind);
    case StateRole:
        return static_cast<int>(it.state);
    case StateTextRole: {
        const bool hasPlayer = m_attachedPlayers.contains(it.assetId);
        return stateText(it.state, it.mode, hasPlayer, it.complete);
    }
    case StateToneRole:
        return stateTone(it.state);
    case ModeRole:
        return static_cast<int>(it.mode);
    case ModeLabelRole:
        return modeLabel(it.mode);
    case CanUpgradeRole:
        return it.mode == api::DownloadMode::OnDemand
            && it.state != api::DownloadState::Completed
            && it.state != api::DownloadState::Failed
            && it.state != api::DownloadState::Cancelled;
    case CanPauseRole:
        return it.state == api::DownloadState::Active
            || it.state == api::DownloadState::Resolving
            || it.state == api::DownloadState::Idle;
    case CanResumeRole:
        return it.state == api::DownloadState::Paused;
    case HasPlayerAttachedRole:
        return m_attachedPlayers.contains(it.assetId);
    case DispositionRole:
        return static_cast<int>(it.disposition);
    case IsPinnedRole:
        return it.isPinned();
    case IsCompleteRole:
        return it.complete;
    case ProgressFractionRole: {
        const double f = it.progressFraction();
        return f < 0.0 ? QVariant() : QVariant(f);
    }
    case ProgressTextRole:
        return progressText(it);
    case CachedSizeBytesRole:
        return it.cachedSizeBytes;
    case ExpectedSizeBytesRole:
        return it.expectedSizeBytes
            ? QVariant(*it.expectedSizeBytes)
            : QVariant();
    case SizeTextRole:
        return sizeText(it);
    case QualityLabelRole:
        return it.qualityLabel;
    case ResolutionRole:
        return it.resolution;
    case ProviderRole:
        return it.provider;
    case ReleaseNameRole:
        return it.releaseName;
    case ErrorTextRole:
        return it.lastError;
    case LocalDirRole:
        return it.localDir;
    case DownloadRateBpsRole:
        return live.ratePayloadBps;
    case DownloadRateTextRole:
        return rateText(live.ratePayloadBps);
    case PeersRole:
        return live.peers;
    case SeedsRole:
        return live.seeds;
    case EtaSecondsRole:
        return live.etaSeconds;
    case EtaTextRole:
        return etaText(live.etaSeconds);
    }
    return {};
}

QHash<int, QByteArray> DownloadsListModel::roleNames() const
{
    return {
        { AssetIdRole, "assetId" },
        { TitleRole, "title" },
        { SubtitleRole, "subtitle" },
        { PosterUrlRole, "posterUrl" },
        { BackendKindRole, "backendKind" },
        { BackendIconRole, "backendIcon" },
        { BackendLabelRole, "backendLabel" },
        { StateRole, "state" },
        { StateTextRole, "stateText" },
        { StateToneRole, "stateTone" },
        { ModeRole, "mode" },
        { ModeLabelRole, "modeLabel" },
        { CanUpgradeRole, "canUpgrade" },
        { CanPauseRole, "canPause" },
        { CanResumeRole, "canResume" },
        { HasPlayerAttachedRole, "hasPlayerAttached" },
        { DispositionRole, "disposition" },
        { IsPinnedRole, "pinned" },
        { IsCompleteRole, "complete" },
        { ProgressFractionRole, "progressFraction" },
        { ProgressTextRole, "progressText" },
        { CachedSizeBytesRole, "cachedSizeBytes" },
        { ExpectedSizeBytesRole, "expectedSizeBytes" },
        { SizeTextRole, "sizeText" },
        { QualityLabelRole, "qualityLabel" },
        { ResolutionRole, "resolution" },
        { ProviderRole, "provider" },
        { ReleaseNameRole, "releaseName" },
        { ErrorTextRole, "errorText" },
        { LocalDirRole, "localDir" },
        { DownloadRateBpsRole, "downloadRateBps" },
        { DownloadRateTextRole, "downloadRateText" },
        { PeersRole, "peers" },
        { SeedsRole, "seeds" },
        { EtaSecondsRole, "etaSeconds" },
        { EtaTextRole, "etaText" },
    };
}

void DownloadsListModel::setItems(QList<api::DownloadItem> items,
    QHash<QString, LiveRow> liveStats,
    QSet<QString> attachedPlayers)
{
    beginResetModel();
    m_items = std::move(items);
    m_attachedPlayers = std::move(attachedPlayers);
    m_liveStats = std::move(liveStats);
    endResetModel();
    Q_EMIT countChanged();
}

} // namespace kinema::ui::qml

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/DownloadsListModel.h"

#include <KFormat>
#include <KLocalizedString>

namespace kinema::ui::qml {

namespace {

QString stateText(api::DownloadState s)
{
    switch (s) {
    case api::DownloadState::Queued:
        return i18nc("@label download state", "Queued");
    case api::DownloadState::Preparing:
        return i18nc("@label download state", "Preparing\u2026");
    case api::DownloadState::Downloading:
        return i18nc("@label download state", "Downloading");
    case api::DownloadState::Streaming:
        return i18nc("@label download state", "Streaming");
    case api::DownloadState::Completed:
        return i18nc("@label download state", "Completed");
    case api::DownloadState::Paused:
        return i18nc("@label download state", "Paused");
    case api::DownloadState::Cancelled:
        return i18nc("@label download state", "Cancelled");
    case api::DownloadState::Failed:
        return i18nc("@label download state", "Failed");
    }
    return QString();
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
    case StateRole:
        return static_cast<int>(it.state);
    case StateTextRole:
        return stateText(it.state);
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
        { StateRole, "state" },
        { StateTextRole, "stateText" },
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
    };
}

void DownloadsListModel::setItems(QList<api::DownloadItem> items)
{
    beginResetModel();
    m_items = std::move(items);
    endResetModel();
    Q_EMIT countChanged();
}

} // namespace kinema::ui::qml

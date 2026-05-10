// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Download.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>

namespace kinema::ui::qml {

/// QAbstractListModel exposing `api::DownloadItem` rows to QML.
class DownloadsListModel : public QAbstractListModel
{
    Q_OBJECT
    Q_PROPERTY(int count READ rowCount NOTIFY countChanged)
public:
    enum Roles {
        AssetIdRole = Qt::UserRole + 1,
        TitleRole,
        SubtitleRole,
        PosterUrlRole,
        BackendKindRole,
        BackendIconRole,
        BackendLabelRole,
        StateRole,
        StateTextRole,
        StateToneRole,
        DispositionRole,
        IsPinnedRole,
        IsCompleteRole,
        ProgressFractionRole,
        ProgressTextRole,
        CachedSizeBytesRole,
        ExpectedSizeBytesRole,
        SizeTextRole,
        QualityLabelRole,
        ResolutionRole,
        ProviderRole,
        ReleaseNameRole,
        ErrorTextRole,
        LocalDirRole,
        DownloadRateBpsRole,
        DownloadRateTextRole,
        PeersRole,
        SeedsRole,
        EtaSecondsRole,
        EtaTextRole,
    };

    /// Transient telemetry sourced from `DownloadManager::liveStatsFor`.
    /// The view-model populates this map before `setItems` runs.
    struct LiveRow {
        qint64 ratePayloadBps = 0;
        int    peers = 0;
        int    seeds = 0;
        int    etaSeconds = -1;
    };

    explicit DownloadsListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void setItems(QList<api::DownloadItem> items,
        QHash<QString, LiveRow> liveStats = {});
    const QList<api::DownloadItem>& items() const noexcept { return m_items; }

Q_SIGNALS:
    void countChanged();

private:
    QList<api::DownloadItem> m_items;
    QHash<QString, LiveRow> m_liveStats;
};

} // namespace kinema::ui::qml

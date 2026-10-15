// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "domain/Download.h"

#include <QAbstractListModel>
#include <QHash>
#include <QList>
#include <QSet>

namespace kinema::ui::qml {

/// QAbstractListModel exposing `domain::DownloadItem` rows to QML.
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
        ModeRole,
        ModeLabelRole,
        CanUpgradeRole,
        CanPauseRole,
        CanResumeRole,
        HasPlayerAttachedRole,
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
        SeriesTitleRole,
        EpisodeTitleRole,
        ErrorTextRole,
        LocalDirRole,
        DownloadRateBpsRole,
        DownloadRateTextRole,
        PeersRole,
        SeedsRole,
        EtaSecondsRole,
        EtaTextRole,
        // Source-title identity, sourced from `DownloadItem::key`
        // (`PlaybackKey`). Used by QML to navigate from a download
        // row to the originating Movie / Series detail page.
        ImdbIdRole,
        KindRole,    ///< 0 = movie, 1 = series
        SeasonRole,  ///< -1 if not a series episode
        EpisodeRole, ///< -1 if not a series episode
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

    /// Structural replacement. Rebuilds the row vector and the
    /// `assetId -> row` index, then fires beginResetModel /
    /// endResetModel. Use this for initial load, filter changes,
    /// and insert/remove fan-out from the store.
    void setItems(QList<domain::DownloadItem> items,
        QHash<QString, LiveRow> liveStats = {},
        QSet<QString> attachedPlayers = {});

    /// Per-row update for the persistent fields (state, progress,
    /// error, etc.). Emits `dataChanged` for the affected row with
    /// role hints scoped to the persistent role set. No reset, so
    /// open popups parented to delegates survive.
    void updateRow(const QString& assetId,
        const domain::DownloadItem& item);

    /// Per-row update for the transient live stats. Emits
    /// `dataChanged` for the affected row with role hints scoped to
    /// the live-stat role set. No reset.
    void updateLiveStatsFor(const QString& assetId, const LiveRow& live);

    /// Replace the attached-player set. Emits `dataChanged` for
    /// every row whose attached-state actually flipped. No reset.
    void updateAttachedPlayers(QSet<QString> attached);

    const QList<domain::DownloadItem>& items() const noexcept { return m_items; }
    const QHash<QString, LiveRow>& liveStats() const noexcept { return m_liveStats; }
    const QSet<QString>& attachedPlayers() const noexcept { return m_attachedPlayers; }

    /// Returns the row index for `assetId`, or -1 if not in the
    /// model.
    int rowForAssetId(const QString& assetId) const;

Q_SIGNALS:
    void countChanged();

private:
    void rebuildAssetIndex();

    QList<domain::DownloadItem> m_items;
    QHash<QString, LiveRow> m_liveStats;
    QSet<QString> m_attachedPlayers;
    QHash<QString, int> m_assetIndex;
};

} // namespace kinema::ui::qml

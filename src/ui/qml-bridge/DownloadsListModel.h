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
        StateRole,
        StateTextRole,
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
    };

    explicit DownloadsListModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role) const override;
    QHash<int, QByteArray> roleNames() const override;

    void setItems(QList<api::DownloadItem> items);
    const QList<api::DownloadItem>& items() const noexcept { return m_items; }

Q_SIGNALS:
    void countChanged();

private:
    QList<api::DownloadItem> m_items;
};

} // namespace kinema::ui::qml

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#pragma once

#include "api/Media.h"

#include <QAbstractTableModel>
#include <QList>

namespace kinema::ui {

/**
 * Table model for the torrent list. Columns map to what a user needs to
 * judge a pick at a glance:
 *
 *   Quality | Release | Size | Seeders | Provider
 *
 * The full Stream struct is exposed via StreamRole for context-menu actions
 * (copy magnet, open magnet).
 */
class TorrentsModel : public QAbstractTableModel
{
    Q_OBJECT
public:
    enum Column {
        ColQuality = 0,
        ColRelease,
        ColSize,
        ColSeeders,
        ColProvider,
        ColCount,
    };

    enum Roles {
        StreamRole = Qt::UserRole + 1,
        SortKeyRole, ///< numeric key for QSortFilterProxyModel-friendly sorting
    };
    Q_ENUM(Roles)

    explicit TorrentsModel(QObject* parent = nullptr);

    int rowCount(const QModelIndex& parent = {}) const override;
    int columnCount(const QModelIndex& parent = {}) const override;
    QVariant data(const QModelIndex& index, int role = Qt::DisplayRole) const override;
    QVariant headerData(int section, Qt::Orientation o, int role = Qt::DisplayRole) const override;

    void reset(QList<api::Stream> rows);
    const QList<api::Stream>& rows() const noexcept { return m_rows; }
    const api::Stream* at(int row) const;

    /// Human readable "1.2 GB", "640 MB", "—".
    static QString formatSize(std::optional<qint64> bytes);

private:
    QList<api::Stream> m_rows;
};

} // namespace kinema::ui

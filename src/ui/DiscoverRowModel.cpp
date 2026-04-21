// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/DiscoverRowModel.h"

#include <QVariant>

namespace kinema::ui {

DiscoverRowModel::DiscoverRowModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int DiscoverRowModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant DiscoverRowModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& r = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return r.year ? QStringLiteral("%1 (%2)").arg(r.title).arg(*r.year) : r.title;
    case TmdbIdRole:
        return r.tmdbId;
    case YearRole:
        return r.year ? QVariant(*r.year) : QVariant();
    case PosterUrlRole:
        return r.poster;
    case OverviewRole:
        return r.overview;
    case VoteAverageRole:
        return r.voteAverage ? QVariant(*r.voteAverage) : QVariant();
    case KindRole:
        return QVariant::fromValue(r.kind);
    case ItemRole:
        return QVariant::fromValue(r);
    default:
        return {};
    }
}

QHash<int, QByteArray> DiscoverRowModel::roleNames() const
{
    auto names = QAbstractListModel::roleNames();
    names.insert(TmdbIdRole, "tmdbId");
    names.insert(TitleRole, "title");
    names.insert(YearRole, "year");
    names.insert(PosterUrlRole, "posterUrl");
    names.insert(OverviewRole, "overview");
    names.insert(VoteAverageRole, "voteAverage");
    names.insert(KindRole, "kind");
    names.insert(ItemRole, "item");
    return names;
}

void DiscoverRowModel::reset(QList<api::DiscoverItem> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
}

const api::DiscoverItem* DiscoverRowModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui

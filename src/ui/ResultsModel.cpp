// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/ResultsModel.h"

#include <QVariant>

namespace kinema::ui {

ResultsModel::ResultsModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int ResultsModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant ResultsModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& r = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return r.year ? QStringLiteral("%1 (%2)").arg(r.title).arg(*r.year) : r.title;
    case ImdbIdRole:
        return r.imdbId;
    case YearRole:
        return r.year ? QVariant(*r.year) : QVariant();
    case PosterUrlRole:
        return r.poster;
    case DescriptionRole:
        return r.description;
    case RatingRole:
        return r.imdbRating ? QVariant(*r.imdbRating) : QVariant();
    case KindRole:
        return QVariant::fromValue(r.kind);
    case SummaryRole:
        return QVariant::fromValue(r);
    default:
        return {};
    }
}

QHash<int, QByteArray> ResultsModel::roleNames() const
{
    auto names = QAbstractListModel::roleNames();
    names.insert(ImdbIdRole, "imdbId");
    names.insert(TitleRole, "title");
    names.insert(YearRole, "year");
    names.insert(PosterUrlRole, "posterUrl");
    names.insert(DescriptionRole, "description");
    names.insert(RatingRole, "rating");
    names.insert(KindRole, "kind");
    names.insert(SummaryRole, "summary");
    return names;
}

void ResultsModel::reset(QList<api::MetaSummary> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
}

const api::MetaSummary* ResultsModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui

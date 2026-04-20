// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilina@tlmtech.dev>
// SPDX-License-Identifier: GPL-2.0-or-later

#include "ui/EpisodesModel.h"

#include <KLocalizedString>

#include <QLocale>

namespace kinema::ui {

EpisodesModel::EpisodesModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int EpisodesModel::rowCount(const QModelIndex& parent) const
{
    return parent.isValid() ? 0 : static_cast<int>(m_rows.size());
}

QVariant EpisodesModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& e = m_rows.at(index.row());

    switch (role) {
    case NumberRole:
        return e.number;
    case TitleRole:
    case Qt::DisplayRole:
        return e.title;
    case DescriptionRole:
        return e.description;
    case ReleasedRole:
        return e.released ? QLocale::system().toString(*e.released, QLocale::ShortFormat)
                          : QString {};
    case ThumbnailUrlRole:
        return e.thumbnail;
    case EpisodeRole:
        return QVariant::fromValue(e);
    case Qt::ToolTipRole:
        return e.description.isEmpty() ? e.title : e.description;
    default:
        return {};
    }
}

void EpisodesModel::reset(QList<api::Episode> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
}

const api::Episode* EpisodesModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui

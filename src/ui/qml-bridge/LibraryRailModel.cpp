// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/LibraryRailModel.h"

namespace kinema::ui::qml {

LibraryRailModel::LibraryRailModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int LibraryRailModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant LibraryRailModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& r = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return r.title;
    case KindRole:
        return static_cast<int>(r.kind);
    case ImdbIdRole:
        return r.imdbId;
    case SeasonRole:
        return r.season ? QVariant(*r.season) : QVariant();
    case EpisodeRole:
        return r.episode ? QVariant(*r.episode) : QVariant();
    case PosterUrlRole:
        return r.posterUrl;
    case ThumbnailUrlRole:
        return r.thumbnailUrl;
    case PrimaryLineRole:
        return r.primaryLine;
    case SecondaryLineRole:
        return r.secondaryLine;
    case ProgressRole:
        return r.progress;
    default:
        return {};
    }
}

QHash<int, QByteArray> LibraryRailModel::roleNames() const
{
    return {
        { KindRole, "kind" },
        { ImdbIdRole, "imdbId" },
        { SeasonRole, "season" },
        { EpisodeRole, "episode" },
        { TitleRole, "title" },
        { PosterUrlRole, "posterUrl" },
        { ThumbnailUrlRole, "thumbnailUrl" },
        { PrimaryLineRole, "primaryLine" },
        { SecondaryLineRole, "secondaryLine" },
        { ProgressRole, "progress" },
    };
}

void LibraryRailModel::setRows(QList<LibraryRailRow> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
    Q_EMIT countChanged();
}

const LibraryRailRow* LibraryRailModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui::qml

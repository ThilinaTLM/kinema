// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/LibraryListModel.h"

namespace kinema::ui::qml {

LibraryListModel::LibraryListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int LibraryListModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant LibraryListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0 || index.row() >= m_rows.size()) {
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
    case SubtitleRole:
        return r.subtitle;
    case PosterUrlRole:
        return r.posterUrl;
    case ProgressRole:
        return r.progress;
    case WatchedRole:
        return r.watched;
    case UpcomingRole:
        return r.upcoming;
    case ReleaseDateTextRole:
        return r.releaseDateText;
    case YearRole:
        return r.year ? QVariant(*r.year) : QVariant();
    case RatingRole:
        return r.rating ? QVariant(*r.rating) : QVariant();
    case RuntimeMinutesRole:
        return r.runtimeMinutes ? QVariant(*r.runtimeMinutes) : QVariant();
    default:
        return {};
    }
}

QHash<int, QByteArray> LibraryListModel::roleNames() const
{
    return {
        { KindRole, "kind" },
        { ImdbIdRole, "imdbId" },
        { SeasonRole, "season" },
        { EpisodeRole, "episode" },
        { TitleRole, "title" },
        { SubtitleRole, "subtitle" },
        { PosterUrlRole, "posterUrl" },
        { ProgressRole, "progress" },
        { WatchedRole, "watched" },
        { UpcomingRole, "upcoming" },
        { ReleaseDateTextRole, "releaseDateText" },
        { YearRole, "year" },
        { RatingRole, "rating" },
        { RuntimeMinutesRole, "runtimeMinutes" },
    };
}

void LibraryListModel::setRows(QList<LibraryListRow> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
    Q_EMIT countChanged();
}

const LibraryListRow* LibraryListModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

} // namespace kinema::ui::qml

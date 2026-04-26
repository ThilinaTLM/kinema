// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#include "ui/qml-bridge/EpisodesListModel.h"

#include "core/DateFormat.h"

namespace kinema::ui::qml {

EpisodesListModel::EpisodesListModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int EpisodesListModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant EpisodesListModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& e = m_rows.at(index.row());
    switch (role) {
    case Qt::DisplayRole:
    case TitleRole:
        return e.title;
    case NumberRole:
        return e.number;
    case DescriptionRole:
        return e.description;
    case ReleasedTextRole:
        return e.released ? core::formatReleaseDate(*e.released)
                          : QString();
    case IsUpcomingRole:
        return core::isFutureRelease(e.released);
    case ThumbnailUrlRole:
        return e.thumbnail.toString();
    case EpisodeRole:
        return QVariant::fromValue(e);
    default:
        return {};
    }
}

QHash<int, QByteArray> EpisodesListModel::roleNames() const
{
    return {
        { NumberRole, "number" },
        { TitleRole, "title" },
        { DescriptionRole, "description" },
        { ReleasedTextRole, "releasedText" },
        { IsUpcomingRole, "isUpcoming" },
        { ThumbnailUrlRole, "thumbnailUrl" },
        { EpisodeRole, "episode" },
    };
}

void EpisodesListModel::setEpisodes(QList<api::Episode> rows)
{
    beginResetModel();
    m_rows = std::move(rows);
    endResetModel();
    Q_EMIT countChanged();
}

const api::Episode* EpisodesListModel::at(int row) const
{
    if (row < 0 || row >= m_rows.size()) {
        return nullptr;
    }
    return &m_rows.at(row);
}

int EpisodesListModel::rowFor(int season, int episodeNumber) const
{
    for (int i = 0; i < m_rows.size(); ++i) {
        const auto& e = m_rows.at(i);
        if (e.season == season && e.number == episodeNumber) {
            return i;
        }
    }
    return -1;
}

} // namespace kinema::ui::qml

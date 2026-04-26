// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/SubtitleTracksModel.h"

namespace kinema::ui::player {

SubtitleTracksModel::SubtitleTracksModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int SubtitleTracksModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant SubtitleTracksModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& e = m_rows.at(index.row());
    switch (role) {
    case IdRole:        return e.id;
    case TitleRole:     return core::tracks::formatLabel(e);
    case LangRole:      return e.lang;
    case CodecRole:     return e.codec;
    case ForcedRole:    return e.forced;
    case ExternalRole:  return e.external;
    case SelectedRole:  return e.selected;
    case Qt::DisplayRole: return core::tracks::formatLabel(e);
    default:            return {};
    }
}

QHash<int, QByteArray> SubtitleTracksModel::roleNames() const
{
    return {
        { IdRole,       QByteArrayLiteral("trackId") },
        { TitleRole,    QByteArrayLiteral("title") },
        { LangRole,     QByteArrayLiteral("lang") },
        { CodecRole,    QByteArrayLiteral("codec") },
        { ForcedRole,   QByteArrayLiteral("forced") },
        { ExternalRole, QByteArrayLiteral("external") },
        { SelectedRole, QByteArrayLiteral("selected") },
    };
}

void SubtitleTracksModel::reset(const core::tracks::TrackList& tracks)
{
    beginResetModel();
    const int oldSize = static_cast<int>(m_rows.size());
    m_rows.clear();
    m_selectedId = -1;
    for (const auto& t : tracks) {
        if (t.type == QLatin1String("sub")) {
            m_rows.append(t);
            if (t.selected && t.id > 0) {
                m_selectedId = t.id;
            }
        }
    }
    endResetModel();
    if (oldSize != static_cast<int>(m_rows.size())) {
        Q_EMIT countChanged();
    }
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV

// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/ChaptersModel.h"

namespace kinema::ui::player {

ChaptersModel::ChaptersModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int ChaptersModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant ChaptersModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& c = m_rows.at(index.row());
    switch (role) {
    case TimeRole:        return c.time;
    case TitleRole:       return c.title;
    case Qt::DisplayRole: return c.title;
    default:              return {};
    }
}

QHash<int, QByteArray> ChaptersModel::roleNames() const
{
    return {
        { TimeRole,  QByteArrayLiteral("time") },
        { TitleRole, QByteArrayLiteral("title") },
    };
}

void ChaptersModel::reset(const core::chapters::ChapterList& chapters)
{
    beginResetModel();
    const int oldSize = static_cast<int>(m_rows.size());
    m_rows = chapters;
    endResetModel();
    if (oldSize != static_cast<int>(m_rows.size())) {
        Q_EMIT countChanged();
    }
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV

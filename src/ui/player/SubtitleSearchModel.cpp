// SPDX-FileCopyrightText: 2026 Thilina Lakshan <thilinalakshanmail@gmail.com>
// SPDX-License-Identifier: Apache-2.0

#ifdef KINEMA_HAVE_LIBMPV

#include "ui/player/SubtitleSearchModel.h"

namespace kinema::ui::player {

SubtitleSearchModel::SubtitleSearchModel(QObject* parent)
    : QAbstractListModel(parent)
{
}

int SubtitleSearchModel::rowCount(const QModelIndex& parent) const
{
    if (parent.isValid()) {
        return 0;
    }
    return static_cast<int>(m_rows.size());
}

QVariant SubtitleSearchModel::data(const QModelIndex& index, int role) const
{
    if (!index.isValid() || index.row() < 0
        || index.row() >= m_rows.size()) {
        return {};
    }
    const auto& h = m_rows.at(index.row());
    switch (role) {
    case FileIdRole:           return h.fileId;
    case ReleaseRole:          return h.releaseName;
    case FileNameRole:         return h.fileName;
    case LanguageRole:         return h.language;
    case LanguageNameRole:     return h.languageName;
    case DownloadCountRole:    return h.downloadCount;
    case RatingRole:           return h.rating;
    case FormatRole:           return h.format;
    case HearingImpairedRole:  return h.hearingImpaired;
    case ForeignPartsOnlyRole: return h.foreignPartsOnly;
    case MoviehashMatchRole:   return h.moviehashMatch;
    case CachedRole:           return m_cached.contains(h.fileId);
    case ActiveRole:           return m_active.contains(h.fileId);
    case UploaderRole:         return h.uploader;
    case FpsRole:              return h.fps;
    case Qt::DisplayRole:
        return h.releaseName.isEmpty() ? h.fileName : h.releaseName;
    default:
        return {};
    }
}

QHash<int, QByteArray> SubtitleSearchModel::roleNames() const
{
    return {
        { FileIdRole,           QByteArrayLiteral("fileId") },
        { ReleaseRole,          QByteArrayLiteral("release") },
        { FileNameRole,         QByteArrayLiteral("fileName") },
        { LanguageRole,         QByteArrayLiteral("language") },
        { LanguageNameRole,     QByteArrayLiteral("languageName") },
        { DownloadCountRole,    QByteArrayLiteral("downloadCount") },
        { RatingRole,           QByteArrayLiteral("rating") },
        { FormatRole,           QByteArrayLiteral("format") },
        { HearingImpairedRole,  QByteArrayLiteral("hearingImpaired") },
        { ForeignPartsOnlyRole, QByteArrayLiteral("foreignPartsOnly") },
        { MoviehashMatchRole,   QByteArrayLiteral("moviehashMatch") },
        { CachedRole,           QByteArrayLiteral("cached") },
        { ActiveRole,           QByteArrayLiteral("active") },
        { UploaderRole,         QByteArrayLiteral("uploader") },
        { FpsRole,              QByteArrayLiteral("fps") },
    };
}

void SubtitleSearchModel::setHits(const QList<api::SubtitleHit>& hits)
{
    beginResetModel();
    const int oldSize = static_cast<int>(m_rows.size());
    m_rows = hits;
    endResetModel();
    if (oldSize != static_cast<int>(m_rows.size())) {
        Q_EMIT countChanged();
    }
}

void SubtitleSearchModel::setCachedFileIds(const QSet<QString>& ids)
{
    if (m_cached == ids) {
        return;
    }
    m_cached = ids;
    if (!m_rows.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(static_cast<int>(m_rows.size()) - 1),
            { CachedRole });
    }
}

void SubtitleSearchModel::setActiveFileIds(const QSet<QString>& ids)
{
    if (m_active == ids) {
        return;
    }
    m_active = ids;
    if (!m_rows.isEmpty()) {
        Q_EMIT dataChanged(index(0), index(static_cast<int>(m_rows.size()) - 1),
            { ActiveRole });
    }
}

} // namespace kinema::ui::player

#endif // KINEMA_HAVE_LIBMPV
